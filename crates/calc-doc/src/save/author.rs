//! What an authored save changes in the package: the parts it rewrites, removes and adds, all
//! serialized here, before any byte is written.

use super::diff::{SheetChanges, sheet_changes};
use super::parts::{self, NewRelationship};
use super::reader::Reader;
use super::sheet::{self, Job};
use super::strings::{self, SharedStrings, read_part};
use super::{Edited, SaveRefusal};
use crate::edit::{FormulaCheck, check_formula};
use crate::load::sheet::collapsed;
use crate::load::{self, LoadError, SHARED_STRINGS};
use crate::opc::{Package, TargetMode};
use crate::xml::XmlElement;
use calc_doc_model::CellInput;
use calc_doc_model::value::CellKey;
use std::collections::BTreeMap;

const CALC_CHAIN: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain";
const TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";

/// The parts an authored save writes differently from the source.
#[derive(Default)]
pub(super) struct Changes {
    /// The new content of each part rewritten, by part name.
    pub(super) rewritten: BTreeMap<String, Vec<u8>>,
    /// The new content of `[Content_Types].xml`, when it changes.
    pub(super) content_types: Option<Vec<u8>>,
    /// The parts left out.
    pub(super) removed: Vec<String>,
    /// The parts added, with their content.
    pub(super) added: Vec<(String, Vec<u8>)>,
    /// The formula cells the edit did not change whose cached result the save leaves out.
    pub(super) caches_dropped: usize,
    /// The worksheet parts rewritten that hold no changed cell, only to leave out cached results.
    pub(super) sheets_rewritten_for_caches: usize,
}

fn load_refusal(part: &str, error: LoadError) -> SaveRefusal {
    SaveRefusal::InvalidPart {
        part: part.to_owned(),
        detail: error.to_string(),
    }
}

/// The rows of a table's header and totals rows, as (first, last) ranges, from its root element:
/// `headerRowCount` rows (1 when absent) from the top of its area and `totalsRowCount` rows (0 when
/// absent) up from the bottom.
fn header_and_totals(
    reader: &Reader<'_>,
    table: &XmlElement,
    top: u32,
    bottom: u32,
) -> Result<[Option<(u32, u32)>; 2], SaveRefusal> {
    let count = |name: &str, absent: u32| match table.attribute(name) {
        None => Ok(absent),
        Some(text) => collapsed(text)
            .parse::<u32>()
            .map_err(|_| reader.invalid(format!("a table whose {name} is not a count"))),
    };
    let header = count("headerRowCount", 1)?;
    let totals = count("totalsRowCount", 0)?;
    let header = (header > 0).then(|| (top, top.saturating_add(header - 1).min(bottom)));
    let totals = (totals > 0).then(|| (bottom.saturating_sub(totals - 1).max(top), bottom));
    Ok([header, totals])
}

/// Refuses a change to a cell in the header row or the totals row of a table (ListObject) of the
/// sheet's part: the table part records its header row's names and its totals row's functions,
/// which an edit of those cells would contradict, and tables are authored by cdoc-m3.2. A cell of
/// a table's data rows is written like any other cell; the table part does not record it.
fn check_tables(
    package: &Package,
    sheet_part: &str,
    changes: &SheetChanges,
) -> Result<(), SaveRefusal> {
    let Some(relationships) = package.relationships_of(Some(sheet_part)) else {
        return Ok(());
    };
    for relationship in relationships.items() {
        if relationship.kind != TABLE || relationship.mode != TargetMode::Internal {
            continue;
        }
        let table = relationship
            .resolved
            .clone()
            .unwrap_or_else(|| relationship.target.clone());
        let xml = read_part(package, &table)?;
        let mut reader = Reader::new(&xml);
        let (root, _) = reader.root("table")?;
        let area = root
            .attribute("ref")
            .and_then(load::address::area)
            .ok_or_else(|| reader.invalid("a table without a ref that is an area"))?;
        let (top, left, bottom, right) = area;
        let rows = header_and_totals(&reader, &root, top, bottom)?;
        if let Some(&(row, column)) = changes.keys().find(|(row, column)| {
            (left..=right).contains(column)
                && rows
                    .iter()
                    .flatten()
                    .any(|(first, last)| (*first..=*last).contains(row))
        }) {
            return Err(SaveRefusal::EditInTable {
                part: sheet_part.to_owned(),
                table,
                row,
                column,
            });
        }
    }
    Ok(())
}

/// Refuses a formula the revision sets that a save could not write, as [`crate::prepare_edit`]
/// refuses it: a revision made another way, such as by [`calc_doc_model::ModelRoot::apply`], is
/// checked here. Only the changed cells are checked; every other formula is written as the file
/// held it, or, for a member of a shared group written as an individual formula, as the loader
/// moved it from the group's text.
fn check_formulas(sheet_part: &str, changes: &SheetChanges) -> Result<(), SaveRefusal> {
    for (&(row, column), input) in changes {
        let Some(CellInput::Formula(formula)) = input else {
            continue;
        };
        check_formula(formula).map_err(|check| {
            let part = sheet_part.to_owned();
            match check {
                FormulaCheck::TooLong(units) => SaveRefusal::FormulaTooLong {
                    part,
                    row,
                    column,
                    units,
                },
                FormulaCheck::Syntax(message) => SaveRefusal::FormulaSyntax {
                    part,
                    row,
                    column,
                    message,
                },
            }
        })?;
    }
    Ok(())
}

/// A part name beside the workbook part, `folder/<stem>.xml` or, when the package holds a part of
/// that name, `folder/<stem><n>.xml`, and the relationship target that names it from the workbook
/// part.
fn new_part_name(package: &Package, workbook_part: &str, stem: &str) -> (String, String) {
    let folder = workbook_part.rsplit_once('/').map(|(folder, _)| folder);
    let mut suffix = String::new();
    for n in 1u32.. {
        let target = format!("{stem}{suffix}.xml");
        let name = match folder {
            Some(folder) => format!("{folder}/{target}"),
            None => target.clone(),
        };
        let overridden = package.content_types().overrides().iter().any(|(part, _)| {
            part.strip_prefix('/')
                .is_some_and(|part| part.eq_ignore_ascii_case(&name))
        });
        if package.part(&name).is_none() && !overridden {
            return (name, target);
        }
        suffix = (n + 1).to_string();
    }
    unreachable!("a free name is found before the counter runs out")
}

/// The changes an authored save makes to the package for `edit`; none when the edited model's
/// cells are the loaded ones.
pub(super) fn plan(package: &Package, edit: &Edited) -> Result<Changes, SaveRefusal> {
    let Edited {
        workbook_part,
        sheets,
        base,
        model,
    } = edit;
    if !model.same_lineage(base) || model.sheets() != base.sheets() {
        return Err(SaveRefusal::ForeignModel);
    }
    let changes: Vec<SheetChanges> = base
        .sheets()
        .iter()
        .map(|sheet| sheet_changes(base, model, sheet.id()))
        .collect();
    let mut out = Changes::default();
    if changes.iter().all(SheetChanges::is_empty) {
        return Ok(out);
    }
    for (sheet, changes) in sheets.iter().zip(&changes) {
        if !changes.is_empty() {
            check_formulas(sheet.part().name(), changes)?;
            check_tables(package, sheet.part().name(), changes)?;
        }
    }
    // A sheet is rewritten when one of its cells changed or one of its formulas has a cached
    // result, which a save after an edit no longer trusts.
    let rewritten: Vec<bool> = sheets
        .iter()
        .zip(&changes)
        .map(|(sheet, changes)| {
            !changes.is_empty()
                || sheet
                    .formulas()
                    .any(|(_, _, record)| record.cache().is_some())
        })
        .collect();
    for (index, sheet) in sheets.iter().enumerate() {
        let name = sheet.part().name();
        let shared = sheets.iter().enumerate().any(|(other, loaded)| {
            other != index && loaded.part().name().eq_ignore_ascii_case(name)
        });
        if rewritten[index] && shared {
            return Err(SaveRefusal::SharedSheetPart {
                part: name.to_owned(),
            });
        }
    }
    let strings_part = load::workbook_part_of(package, workbook_part, SHARED_STRINGS)
        .map_err(|error| load_refusal(workbook_part, error))?;
    let mut strings = SharedStrings::read(package, strings_part.as_deref())?;
    for ((sheet, changes), rewrite) in sheets.iter().zip(&changes).zip(&rewritten) {
        if !rewrite {
            continue;
        }
        let name = sheet.part().name();
        let xml = read_part(package, name)?;
        let job = Job {
            changes,
            loaded: sheet,
            model,
        };
        let rewritten = sheet::rewrite(&xml, &job, &mut strings)?;
        out.rewritten.insert(name.to_owned(), rewritten.bytes);
        out.caches_dropped += rewritten.caches_dropped;
        if changes.is_empty() {
            out.sheets_rewritten_for_caches += 1;
        }
    }

    let workbook = read_part(package, workbook_part)?;
    out.rewritten
        .insert(workbook_part.clone(), parts::full_calc_on_load(&workbook)?);

    // The calculation chain lists formula cells; when the set of them changed, the chain no longer
    // lists exactly the workbook's formulas, so it is left out, with its relationship and its
    // content type override, rather than written stale. What Excel does with a stale or a missing
    // chain is measured by infra-m1.3's round trip, not here.
    let formulas_changed = sheets.iter().zip(&changes).any(|(sheet, changes)| {
        changes.iter().any(|(&(row, column), input)| {
            let key = CellKey::new(sheet.id(), row, column).expect("a changed cell is in the grid");
            let was = matches!(base.cell(key), Some(CellInput::Formula(_)));
            let is = matches!(input, Some(CellInput::Formula(_)));
            was != is
        })
    });
    let chain = if formulas_changed {
        load::workbook_part_of(package, workbook_part, CALC_CHAIN)
            .map_err(|error| load_refusal(workbook_part, error))?
    } else {
        None
    };

    if let Some(part) = strings.part()
        && strings.changed()
    {
        let xml = read_part(package, part)?;
        out.rewritten
            .insert(part.to_owned(), strings.rewrite(&xml)?);
    }
    let new_strings = (strings.part().is_none() && strings.appended() > 0)
        .then(|| new_part_name(package, workbook_part, "sharedStrings"));
    if let Some((name, _)) = &new_strings {
        out.added.push((name.clone(), strings.new_part()));
    }

    if chain.is_some() || new_strings.is_some() {
        let relationships = package
            .relationships_of(Some(workbook_part))
            .ok_or_else(|| SaveRefusal::InvalidPart {
                part: workbook_part.clone(),
                detail: "the workbook part has no relationships part".to_owned(),
            })?;
        let id = (1u32..)
            .map(|n| format!("rId{n}"))
            .find(|id| relationships.items().iter().all(|item| &item.id != id))
            .expect("a free relationship ID");
        let add = new_strings.as_ref().map(|(_, target)| NewRelationship {
            id: &id,
            kind: SHARED_STRINGS,
            target,
        });
        let xml = read_part(package, relationships.part())?;
        let remove = chain.is_some().then_some(CALC_CHAIN);
        out.rewritten.insert(
            relationships.part().to_owned(),
            parts::relationships(&xml, remove, add.as_ref())?,
        );
        let xml = package
            .content_types_xml()
            .map_err(|error| SaveRefusal::Part {
                part: crate::opc::CONTENT_TYPES_ENTRY.to_owned(),
                error,
            })?;
        out.content_types = Some(parts::content_types(
            &xml,
            chain.as_deref(),
            new_strings
                .as_ref()
                .map(|(name, _)| (name.as_str(), strings::CONTENT_TYPE)),
        )?);
    }
    if let Some(chain) = chain
        && package.part(&chain).is_some()
    {
        out.removed.push(chain);
    }
    Ok(out)
}
