//! Native measurement driver for dc-snz.7; no product behavior is changed.
//! Run through measure-dnacalc.ps1, which puts the temporary Cargo manifest in target/.
#![allow(dead_code)]
#[path = "../../cdoc_m1_1_support/mod.rs"]
mod existing_support;

use calc_doc::model::value::{CellKey, IdAllocator, SheetId};
use calc_doc::model::{CellInput, OpBatch, WorkbookOp};
use calc_doc::{MemorySource, OpenPolicy, PackageRead, SavePolicy};
use calc_fml::reference::{FormulaAnchor, NameDefinition, NameScope, ReferenceNotation};
use calc_fml::{FormulaForm, PlanCache, ReferenceCatalog};
use calc_func::value::CalcValue;
use calc_func::{UserFunction, UserFunctions};
use serde_json::{Value, json};
use std::{path::Path, rc::Rc, sync::Arc};

struct Catalog;
impl ReferenceCatalog for Catalog {
    fn table(&self, name: &str) -> calc_fml::reference::TableLookup {
        let _ = name;
        calc_fml::reference::TableLookup::NotModelled
    }

    fn table_containing(
        &self,
        cell: calc_func::value::CellKey,
    ) -> calc_fml::reference::TableLookup {
        let _ = cell;
        calc_fml::reference::TableLookup::NotModelled
    }

    fn sheet_span(
        &self,
        first: calc_func::value::SheetId,
        last: calc_func::value::SheetId,
    ) -> Option<Vec<calc_func::value::SheetId>> {
        let _ = (first, last);
        None
    }

    fn unknown_calls_are_names(&self) -> bool {
        false
    }

    fn workbook_name(&self) -> Option<&str> {
        None
    }

    fn external_sheet(
        &self,
        book: &calc_fml::reference::ExternalBookRef,
        sheet: &str,
    ) -> Option<calc_func::value::SheetId> {
        let _ = (book, sheet);
        None
    }

    fn external_function_target(
        &self,
        book: &calc_fml::reference::ExternalBookRef,
    ) -> Option<String> {
        let _ = book;
        None
    }

    fn sheet_named(&self, _: &str) -> Option<SheetId> {
        None
    }
    fn defined_name(&self, _: &str, _: NameScope) -> NameDefinition {
        NameDefinition::Unknown
    }
}
#[derive(Debug)]
struct Identity;
impl UserFunction for Identity {
    fn call(&self, args: &[CalcValue]) -> Result<CalcValue, String> {
        Ok(args.first().cloned().unwrap_or_else(CalcValue::empty))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = Path::new(&args[1]);
    let out = Path::new(&args[2]);
    std::fs::create_dir_all(out).unwrap();
    let mut data = vec![json!({"kind":"environment","source_commit":args.get(3),
                               "source_worktree_dirty":args.get(4).map(|s| s=="true"),
                               "os":std::env::consts::OS,"arch":std::env::consts::ARCH})];
    for name in [
        "local.xlsm",
        "local.xlsb",
        "stripped.xlsx",
        "consumer.xlsx",
        "consumer.xlsb",
        "loaded-xml.xlsb",
        "collision.xlsm",
        "collision.xlsb",
        "metadata-after.xlsb",
        "metadata-unqualified.xlsb",
        "metadata-custom-vba.xlsb",
    ] {
        let bytes = std::fs::read(dir.join(name)).unwrap();
        let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
        let document = match calc_doc::open(source, &OpenPolicy::default()) {
            Ok(d) => d,
            Err(e) => {
                data.push(json!({"file":name,"open_error":format!("{e:?}")}));
                continue;
            }
        };
        let mut ids = IdAllocator::<SheetId>::new();
        let loaded = match calc_doc::load(&document, &mut ids) {
            Ok(w) => w,
            Err(e) => {
                data.push(json!({"file":name,"load_error":format!("{e:?}")}));
                continue;
            }
        };
        let root = loaded.model();
        let cells: Vec<Value> = root.sheets().iter().flat_map(|s| {
            root.sheet_cells(s.id()).into_iter().flatten().filter_map(|(at, input)| {
                if let CellInput::Formula(f) = input {
                    Some(json!({"at":format!("{at:?}"),"formula":String::from_utf16_lossy(f.text().utf16_code_units())}))
                } else { None }
            })
        }).collect();
        let mut row = json!({"file":name,"cells":cells,"names":format!("{:?}",root.definitions().names()),
                             "macro_enabled":root.definitions().macro_enabled()});
        let plan = calc_doc::prepare_save(&loaded.snapshot(root), &SavePolicy::default()).unwrap();
        let mut saved = Vec::new();
        let receipt = calc_doc::write(plan, &mut saved).unwrap();
        let stem = name.replace('.', "-");
        let output_name = format!(
            "{stem}-saved.{}",
            if root.definitions().macro_enabled() {
                "xlsm"
            } else {
                "xlsx"
            }
        );
        std::fs::write(out.join(&output_name), saved).unwrap();
        row["saved"] = json!(output_name);
        row["receipt"] = json!(format!("{receipt:?}"));
        if name.ends_with(".xlsb") {
            let policy = SavePolicy {
                format: calc_doc::SaveFormat::Xlsb,
                ..SavePolicy::default()
            };
            let plan = calc_doc::prepare_save(&loaded.snapshot(root), &policy).unwrap();
            let mut binary = Vec::new();
            let receipt = calc_doc::write(plan, &mut binary).unwrap();
            let binary_name = format!("{stem}-preserved.xlsb");
            std::fs::write(out.join(&binary_name), binary).unwrap();
            row["binary_preserved"] = json!(binary_name);
            row["binary_receipt"] = json!(format!("{receipt:?}"));
        }
        let sheet = root.sheets()[0].id();
        // Re-enter each measured file formula into a separate empty cell. This exercises
        // prepare_edit/file_form, without replacing the untouched round-trip observation.
        let mut edits = Vec::new();
        let mut accepted = Vec::new();
        for (at, input) in root.sheet_cells(sheet).into_iter().flatten() {
            if let CellInput::Formula(_) = input {
                let target = CellKey::new(sheet, at.row(), 4).unwrap();
                let batch = OpBatch {
                    base: root.revision(),
                    operations: vec![WorkbookOp::SetCell {
                        at: target,
                        input: input.clone(),
                    }],
                };
                match calc_doc::prepare_edit(root, &batch) {
                    Err(e) => {
                        edits.push(json!({"at":format!("{at:?}"),"refused":format!("{e:?}")}))
                    }
                    Ok(edit) => {
                        accepted.extend(batch.operations.clone());
                        let planned = calc_doc::prepare_save(
                            &loaded.snapshot(edit.model()),
                            &SavePolicy::default(),
                        );
                        match planned {
                            Err(e) => edits.push(
                                json!({"at":format!("{at:?}"),"save_refused":format!("{e:?}")}),
                            ),
                            Ok(plan) => {
                                let mut saved = Vec::new();
                                calc_doc::write(plan, &mut saved).unwrap();
                                // Store output for Excel reopen, for the whole-sheet authored
                                // batch below; here record only admission of each formula.
                                edits.push(json!({"at":format!("{at:?}"),"admitted":true}));
                            }
                        }
                    }
                }
            }
        }
        row["reentry"] = json!(edits);
        if !accepted.is_empty() {
            let batch = OpBatch {
                base: root.revision(),
                operations: accepted,
            };
            let edit = calc_doc::prepare_edit(root, &batch).unwrap();
            let plan =
                calc_doc::prepare_save(&loaded.snapshot(edit.model()), &SavePolicy::default())
                    .unwrap();
            let mut saved = Vec::new();
            calc_doc::write(plan, &mut saved).unwrap();
            std::fs::write(
                out.join(format!(
                    "{stem}-reentered.{}",
                    if root.definitions().macro_enabled() {
                        "xlsm"
                    } else {
                        "xlsx"
                    }
                )),
                saved,
            )
            .unwrap();
        }
        // An unrelated value edit rewrites a sheet holding UDF calls.
        let batch = OpBatch {
            base: root.revision(),
            operations: vec![WorkbookOp::SetCell {
                at: CellKey::new(sheet, 50, 4).unwrap(),
                input: existing_support::d1::number(7.0),
            }],
        };
        let edit = calc_doc::prepare_edit(root, &batch).unwrap();
        let plan =
            calc_doc::prepare_save(&loaded.snapshot(edit.model()), &SavePolicy::default()).unwrap();
        let mut edited = Vec::new();
        calc_doc::write(plan, &mut edited).unwrap();
        std::fs::write(
            out.join(format!(
                "{stem}-edited.{}",
                if root.definitions().macro_enabled() {
                    "xlsm"
                } else {
                    "xlsx"
                }
            )),
            edited,
        )
        .unwrap();
        data.push(row);
    }
    let mut registry = UserFunctions::new();
    for name in [
        "MyFunc",
        "Module1.MyFunc",
        "SUM",
        "ACOT",
        "Café",
        "Ångström",
        "Foo_2",
    ] {
        registry.register(name, Rc::new(Identity)).unwrap();
    }
    let mut ids = IdAllocator::<SheetId>::new();
    let anchor = FormulaAnchor::new(CellKey::new(ids.issue().unwrap(), 1, 1).unwrap());
    for formula in [
        "=MyFunc(2)",
        "=myfunc(2)",
        "=Module1.MyFunc(2)",
        "=[0]!MyFunc(2)",
        "='local.xlsm'!MyFunc(2)",
        "=[1]!OtherFunc(2)",
        "=[2]!AddFunc(2)",
        "=_xludf.MyFunc(2)",
        "=_xludf.SUM(2)",
        "=_xludf.ACOT(2)",
        "=_xlfn.ACOT(2)",
        "=_xll.MyFunc(2)",
        "=Café(2)",
        "=CAFÉ(2)",
        "=Ångström(2)",
        "=ångström(2)",
        "=Foo_2(2)",
        "=A1(2)",
        "=XFD1(2)",
    ] {
        let tree = calc_fml::parse_formula(formula);
        let mut cache = PlanCache::new();
        let result = calc_fml::compile_with_functions(
            &tree,
            ReferenceNotation::A1,
            anchor,
            &Catalog,
            &mut cache,
            FormulaForm::Dynamic,
            &registry,
        );
        data.push(match result {
            Ok(p)=>json!({"binding":formula,"user_calls":p.user_calls().collect::<Vec<_>>(),"plan":format!("{p:?}")}),
            Err(e)=>json!({"binding":formula,"error":format!("{e:?}")})
        });
    }
    std::fs::write(
        out.join("product-observations.json"),
        serde_json::to_string_pretty(&data).unwrap() + "\n",
    )
    .unwrap();
}
