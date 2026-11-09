//! Clipboard metadata follows the same authored batch and formula transforms as cell content.
use crate::{CellEdit, CopyRefusal, HostState, PasteMode};
use calc_doc_model::{
    ConditionalRuleSource, FormulaText, ModelRoot, ReviewNote, ReviewSource, Validation,
};
use calc_value::{CellKey, ExcelText, GridArea, SheetId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
fn refusal(reason: &'static str) -> CopyRefusal {
    CopyRefusal::Metadata(reason)
}
fn areas(rule: &Validation, sheet: SheetId) -> Vec<GridArea> {
    rule.areas
        .iter()
        .filter_map(|&(t, l, b, r)| {
            GridArea::new(
                CellKey::new(sheet, t, l).ok()?,
                CellKey::new(sheet, b, r).ok()?,
            )
            .ok()
        })
        .collect()
}
fn tuples(areas: &[GridArea]) -> Vec<(u32, u32, u32, u32)> {
    areas
        .iter()
        .map(|a| {
            (
                a.first().row(),
                a.first().column(),
                a.last().row(),
                a.last().column(),
            )
        })
        .collect()
}
fn subtract(area: GridArea, clip: GridArea) -> Vec<GridArea> {
    let Some(i) = area.intersection(clip) else {
        return vec![area];
    };
    let mut out = Vec::new();
    let make = |t, l, b, r| {
        GridArea::new(
            CellKey::new(area.sheet(), t, l).unwrap(),
            CellKey::new(area.sheet(), b, r).unwrap(),
        )
        .unwrap()
    };
    let (a, b) = (area.first(), area.last());
    let (c, d) = (i.first(), i.last());
    if a.row() < c.row() {
        out.push(make(a.row(), a.column(), c.row() - 1, b.column()));
    }
    if d.row() < b.row() {
        out.push(make(d.row() + 1, a.column(), b.row(), b.column()));
    }
    if a.column() < c.column() {
        out.push(make(c.row(), a.column(), d.row(), c.column() - 1));
    }
    if d.column() < b.column() {
        out.push(make(c.row(), d.column() + 1, d.row(), b.column()));
    }
    out
}
fn trim(mut areas: Vec<GridArea>, from: GridArea, to: GridArea, cut: bool) -> Vec<GridArea> {
    areas = areas.into_iter().flat_map(|a| subtract(a, to)).collect();
    if cut {
        areas = areas.into_iter().flat_map(|a| subtract(a, from)).collect();
    }
    areas
}
pub(crate) fn merge(mut areas: Vec<GridArea>) -> Vec<GridArea> {
    let key = |a: &GridArea| {
        (
            a.sheet(),
            a.first().row(),
            a.first().column(),
            a.last().row(),
            a.last().column(),
        )
    };
    loop {
        areas.sort_by_key(key);
        areas.dedup();
        let before = areas.len();
        let mut out: Vec<GridArea> = Vec::new();
        for a in areas {
            if let Some(last) = out.last_mut()
                && last.sheet() == a.sheet()
            {
                let horizontal = last.first().row() == a.first().row()
                    && last.last().row() == a.last().row()
                    && last.last().column().checked_add(1) == Some(a.first().column());
                let vertical = last.first().column() == a.first().column()
                    && last.last().column() == a.last().column()
                    && last.last().row().checked_add(1) == Some(a.first().row());
                if horizontal || vertical {
                    *last = GridArea::new(last.first(), a.last()).unwrap();
                    continue;
                }
            }
            out.push(a);
        }
        if out.len() == before {
            return out;
        }
        areas = out;
    }
}
fn map_area(a: GridArea, from: GridArea, tile: CellKey, transpose: bool) -> GridArea {
    let map = |at: CellKey| {
        CellKey::new(
            tile.sheet(),
            tile.row()
                + if transpose {
                    at.column() - from.first().column()
                } else {
                    at.row() - from.first().row()
                },
            tile.column()
                + if transpose {
                    at.row() - from.first().row()
                } else {
                    at.column() - from.first().column()
                },
        )
        .unwrap()
    };
    GridArea::new(map(a.first()), map(a.last())).unwrap()
}
fn copied_areas(
    source: &[GridArea],
    from: GridArea,
    to: GridArea,
    transpose: bool,
) -> Result<Vec<GridArea>, CopyRefusal> {
    let mut clipped = source
        .iter()
        .filter_map(|a| a.intersection(from))
        .collect::<Vec<_>>();
    clipped.sort_by_key(|a| {
        (
            a.first().row(),
            a.first().column(),
            a.last().row(),
            a.last().column(),
        )
    });
    clipped.dedup();
    let (rows, cols) = if transpose {
        (from.column_count(), from.row_count())
    } else {
        (from.row_count(), from.column_count())
    };
    let mut out = Vec::new();
    for row in (to.first().row()..=to.last().row()).step_by(rows as usize) {
        for col in (to.first().column()..=to.last().column()).step_by(cols as usize) {
            let tile = CellKey::new(to.sheet(), row, col).unwrap();
            for area in &clipped {
                // Partial final fill tiles are clipped before constructing a coordinate outside the grid.
                let source_bottom = if transpose {
                    from.first().row() + (to.last().column() - col).min(from.row_count() - 1)
                } else {
                    from.first().row() + (to.last().row() - row).min(from.row_count() - 1)
                };
                let source_right = if transpose {
                    from.first().column() + (to.last().row() - row).min(from.column_count() - 1)
                } else {
                    from.first().column() + (to.last().column() - col).min(from.column_count() - 1)
                };
                let tile_source = GridArea::new(
                    from.first(),
                    CellKey::new(from.sheet(), source_bottom, source_right).unwrap(),
                )
                .unwrap();
                if let Some(area) = area.intersection(tile_source) {
                    out.push(map_area(area, from, tile, transpose));
                }
                if out.len() > crate::COPY_LIMIT as usize {
                    return Err(refusal("Too many rule areas are produced by this paste"));
                }
            }
        }
    }
    Ok(merge(out))
}
fn relocate(
    model: &ModelRoot,
    text: &str,
    from: CellKey,
    to: CellKey,
) -> Result<String, CopyRefusal> {
    if from == to {
        return Ok(text.to_owned());
    }
    let formula = FormulaText::new(ExcelText::from_utf16_code_units(
        text.encode_utf16().collect(),
    ))
    .map_err(|_| CopyRefusal::Unrelocatable { at: from })?;
    Ok(calc_engine::relocate_formula(model, &formula, from, to)
        .ok_or(CopyRefusal::Unrelocatable { at: from })?
        .text()
        .to_string_lossy())
}
fn moved(
    model: &ModelRoot,
    text: &str,
    at: CellKey,
    from: GridArea,
    to: GridArea,
) -> Result<String, CopyRefusal> {
    let formula = FormulaText::new(ExcelText::from_utf16_code_units(
        text.encode_utf16().collect(),
    ))
    .map_err(|_| CopyRefusal::Unrelocatable { at })?;
    Ok(
        calc_engine::move_references(model, &formula, at, from, to.first())
            .unwrap_or(formula)
            .text()
            .to_string_lossy(),
    )
}
fn rule_texts(
    rule: &mut ConditionalRuleSource,
    mut transform: impl FnMut(&str) -> Result<String, CopyRefusal>,
) -> Result<(), CopyRefusal> {
    for f in rule.formulas.iter_mut().chain(&mut rule.threshold_formulas) {
        *f = transform(f)?;
    }
    Ok(())
}
fn valid_texts(
    rule: &mut Validation,
    mut transform: impl FnMut(&str) -> Result<String, CopyRefusal>,
) -> Result<(), CopyRefusal> {
    for f in [&mut rule.formula1, &mut rule.formula2]
        .into_iter()
        .flatten()
    {
        *f = transform(f)?;
    }
    Ok(())
}

pub(crate) fn append(
    host: &HostState,
    source: &Arc<ModelRoot>,
    from: GridArea,
    to: GridArea,
    mode: PasteMode,
    cut: bool,
    transpose: bool,
    edits: &mut Vec<CellEdit>,
) -> Result<(), CopyRefusal> {
    if !matches!(mode, PasteMode::All | PasteMode::Formats)
        || (from == to && source.revision() == host.model().revision())
    {
        return Ok(());
    }
    if !source.same_lineage(host.model()) {
        return Err(CopyRefusal::StyleDefinitionsChanged);
    }
    for sheet in [from.sheet(), to.sheet()] {
        if host
            .loaded()
            .sheet(sheet)
            .is_some_and(|s| s.conditional_formats().iter().any(|b| b.unread_container))
        {
            return Err(refusal(
                "This paste touches conditional metadata that cannot be edited yet",
            ));
        }
    }
    let mut cf: BTreeMap<_, _> = host
        .model()
        .sheets()
        .iter()
        .map(|s| {
            (
                s.id(),
                host.model()
                    .definitions()
                    .conditional_rules(s.id())
                    .to_vec(),
            )
        })
        .collect();
    let mut dv: BTreeMap<_, _> = host
        .model()
        .sheets()
        .iter()
        .map(|s| {
            (
                s.id(),
                host.model().definitions().validations(s.id()).to_vec(),
            )
        })
        .collect();
    for (sheet, rules) in &mut cf {
        let mut next = Vec::new();
        for original in rules.iter() {
            let Some(origin) = original.origin() else {
                next.push(original.clone());
                continue;
            };
            let remaining = trim(original.areas.clone(), from, to, cut);
            let affected = remaining != original.areas;
            if !cut && !affected {
                next.push(original.clone());
                continue;
            }
            let parsed = calc_doc::conditional_rule(original)
                .map_err(|_| refusal("A conditional rule cannot be interpreted"))?;
            if parsed.extension_id.is_some() {
                if affected || cut {
                    return Err(refusal(
                        "Extension-backed conditional rules are preserved; this clipboard change is unavailable",
                    ));
                }
                next.push(original.clone());
                continue;
            }
            let mut rule = original.clone();
            rule.areas = merge(remaining);
            let Some(at) = rule.origin() else {
                continue;
            };
            rule_texts(&mut rule, |f| {
                let f = relocate(host.model(), f, origin, at)?;
                if cut {
                    moved(host.model(), &f, at, from, to)
                } else {
                    Ok(f)
                }
            })?;
            next.push(rule);
        }
        *rules = next;
        let _ = sheet;
    }
    if mode == PasteMode::All {
        for (sheet, rules) in &mut dv {
            let mut next = Vec::new();
            for original in rules.iter() {
                let original_areas = areas(original, *sheet);
                let Some(origin) = original_areas.first().map(|a| a.first()) else {
                    next.push(original.clone());
                    continue;
                };
                let remaining = trim(original_areas, from, to, cut);
                let Some(at) = remaining.first().map(|a| a.first()) else {
                    continue;
                };
                let mut rule = original.clone();
                rule.areas = tuples(&remaining);
                valid_texts(&mut rule, |f| {
                    let f = relocate(host.model(), f, origin, at)?;
                    if cut {
                        moved(host.model(), &f, at, from, to)
                    } else {
                        Ok(f)
                    }
                })?;
                next.push(rule);
            }
            *rules = next;
        }
    }
    let mut styles = edits
        .iter()
        .rev()
        .find_map(|e| match e {
            CellEdit::Styles(s) => Some((**s).clone()),
            _ => None,
        })
        .unwrap_or_else(|| (**host.styles()).clone());
    let loaded_dxfs = host
        .loaded()
        .model()
        .definitions()
        .styles()
        .differential_formats
        .len();
    for original in source.definitions().conditional_rules(from.sheet()) {
        let target = copied_areas(&original.areas, from, to, transpose)?;
        if target.is_empty() {
            continue;
        }
        let origin = original
            .origin()
            .ok_or_else(|| refusal("A conditional rule has no origin"))?;
        let parsed = calc_doc::conditional_rule(original)
            .map_err(|_| refusal("A conditional rule cannot be copied"))?;
        if parsed.extension_id.is_some() {
            return Err(refusal(
                "Copying extension-backed conditional rules is unavailable",
            ));
        }
        let mut rule = original.clone();
        rule.areas = target;
        rule.authored = true;
        let at = rule.origin().unwrap();
        let source_first = if cut {
            CellKey::new(
                from.sheet(),
                from.first().row() + at.row() - to.first().row(),
                from.first().column() + at.column() - to.first().column(),
            )
            .unwrap()
        } else {
            from.first()
        };
        rule_texts(&mut rule, |f| {
            if cut {
                moved(
                    host.model(),
                    &relocate(source, f, origin, source_first)?,
                    source_first,
                    from,
                    to,
                )
            } else {
                relocate(source, f, origin, at)
            }
        })?;
        rule = calc_doc::copied_conditional_source(&rule)
            .map_err(|_| refusal("The conditional source cannot be copied"))?;
        if let Some(id) = parsed.dxf {
            let value = source
                .definitions()
                .styles()
                .differential_formats
                .get(id as usize)
                .ok_or(CopyRefusal::StyleDefinitionsChanged)?;
            let new = if (id as usize) < loaded_dxfs {
                if styles.differential_formats.get(id as usize) != Some(value) {
                    return Err(CopyRefusal::StyleDefinitionsChanged);
                }
                id
            } else if let Some(index) = styles
                .differential_formats
                .iter()
                .enumerate()
                .skip(loaded_dxfs)
                .find(|(_, v)| *v == value)
                .map(|(i, _)| i)
            {
                index as u32
            } else {
                let id = styles.differential_formats.len() as u32;
                styles.differential_formats.push(value.clone());
                id
            };
            rule = calc_doc::conditional_with_dxf(&rule, new)
                .map_err(|_| refusal("The copied differential format cannot be assigned"))?;
        }
        cf.entry(to.sheet()).or_default().push(rule);
    }
    if mode == PasteMode::All {
        for original in source.definitions().validations(from.sheet()) {
            let source_areas = areas(original, from.sheet());
            let target = copied_areas(&source_areas, from, to, transpose)?;
            if target.is_empty() {
                continue;
            }
            let origin = source_areas.first().unwrap().first();
            let at = target[0].first();
            let source_first = if cut {
                CellKey::new(
                    from.sheet(),
                    from.first().row() + at.row() - to.first().row(),
                    from.first().column() + at.column() - to.first().column(),
                )
                .unwrap()
            } else {
                from.first()
            };
            let mut rule = original.clone();
            rule.areas = tuples(&target);
            valid_texts(&mut rule, |f| {
                if cut {
                    moved(
                        host.model(),
                        &relocate(source, f, origin, source_first)?,
                        source_first,
                        from,
                        to,
                    )
                } else {
                    relocate(source, f, origin, at)
                }
            })?;
            rule = calc_doc::copied_validation_source(&rule)
                .map_err(|_| refusal("The validation source cannot be copied"))?;
            dv.entry(to.sheet()).or_default().push(rule);
        }
        append_notes(host, source, from, to, cut, transpose, edits)?;
    }
    for (sheet, mut rules) in cf {
        if rules.len() > 1024 {
            return Err(refusal("This paste would exceed 1,024 conditional rules"));
        }
        if sheet == to.sheet() && rules != host.model().definitions().conditional_rules(sheet) {
            rules.sort_by_key(|r| r.priority);
            for (index, rule) in rules.iter_mut().enumerate() {
                rule.priority = index as i64 + 1;
            }
        }
        if rules != host.model().definitions().conditional_rules(sheet) {
            edits.push(CellEdit::ConditionalRules { sheet, rules });
        }
    }
    if mode == PasteMode::All {
        for (sheet, rules) in dv {
            if rules.len() > 4096 {
                return Err(refusal("This paste would exceed 4,096 validation rules"));
            }
            if rules != host.model().definitions().validations(sheet) {
                edits.push(CellEdit::Validations { sheet, rules });
            }
        }
    }
    let current = edits
        .iter()
        .rev()
        .find_map(|e| match e {
            CellEdit::Styles(s) => Some(s.as_ref()),
            _ => None,
        })
        .unwrap_or(host.styles());
    if &styles != current {
        if let Some(CellEdit::Styles(value)) =
            edits.iter_mut().find(|e| matches!(e, CellEdit::Styles(_)))
        {
            *value = Arc::new(styles);
        } else {
            edits.insert(0, CellEdit::Styles(Arc::new(styles)));
        }
    }
    Ok(())
}
fn append_notes(
    host: &HostState,
    source: &ModelRoot,
    from: GridArea,
    to: GridArea,
    cut: bool,
    transpose: bool,
    edits: &mut Vec<CellEdit>,
) -> Result<(), CopyRefusal> {
    let mut all: BTreeMap<_, _> = host
        .model()
        .sheets()
        .iter()
        .map(|s| {
            (
                s.id(),
                host.model()
                    .definitions()
                    .review_notes(s.id())
                    .iter()
                    .filter(|n| !to.contains(n.at) && !(cut && from.contains(n.at)))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let source_notes = source
        .definitions()
        .review_notes(from.sheet())
        .iter()
        .filter(|n| from.contains(n.at))
        .collect::<Vec<_>>();
    let mut used: BTreeSet<String> = host
        .loaded()
        .review_people()
        .keys()
        .map(|s| s.to_ascii_uppercase())
        .collect();
    for (_, notes) in host
        .model()
        .definitions()
        .all_review_notes()
        .chain(host.loaded().model().definitions().all_review_notes())
        .chain(source.definitions().all_review_notes())
    {
        for n in notes {
            match &n.source {
                Some(ReviewSource::Thread { id, person, .. }) => {
                    used.insert(id.to_ascii_uppercase());
                    used.insert(person.to_ascii_uppercase());
                }
                Some(ReviewSource::NewNote { id, .. }) => {
                    used.insert(id.to_ascii_uppercase());
                }
                Some(ReviewSource::Note { uid: Some(id), .. }) => {
                    used.insert(id.to_ascii_uppercase());
                }
                _ => {}
            }
        }
    }
    let fresh = |used: &mut BTreeSet<String>| {
        let id = (1u64..)
            .map(|n| format!("{{D1ACA118-0000-4000-8000-{n:012X}}}"))
            .find(|id| !used.contains(id))
            .expect("review identity");
        used.insert(id.clone());
        id
    };
    let part = |threaded| {
        crate::review::part_for(
            host,
            to.sheet(),
            host.model().definitions().review_notes(to.sheet()),
            threaded,
        )
        .map_err(|_| refusal("The target review part cannot be edited"))
    };
    let (rows, cols) = if transpose {
        (from.column_count(), from.row_count())
    } else {
        (from.row_count(), from.column_count())
    };
    for row in (to.first().row()..=to.last().row()).step_by(rows as usize) {
        for col in (to.first().column()..=to.last().column()).step_by(cols as usize) {
            let tile = CellKey::new(to.sheet(), row, col).unwrap();
            let mut ids = BTreeMap::new();
            for note in &source_notes {
                if let Some(ReviewSource::Thread { id, .. }) = &note.source {
                    ids.insert(
                        id.to_ascii_lowercase(),
                        if cut { id.clone() } else { fresh(&mut used) },
                    );
                }
            }
            for original in &source_notes {
                let target_row = row
                    + if transpose {
                        original.at.column() - from.first().column()
                    } else {
                        original.at.row() - from.first().row()
                    };
                let target_col = col
                    + if transpose {
                        original.at.row() - from.first().row()
                    } else {
                        original.at.column() - from.first().column()
                    };
                if target_row > to.last().row() || target_col > to.last().column() {
                    continue;
                }
                let mut note = (*original).clone();
                note.at = CellKey::new(to.sheet(), target_row, target_col).unwrap();
                if note.at == original.at && !cut {
                    all.entry(to.sheet()).or_default().push(note);
                    continue;
                }
                match &original.source {
                    Some(ReviewSource::Thread {
                        id,
                        parent,
                        person,
                        timestamp,
                        resolved,
                        ..
                    }) => {
                        if !cut && !original.editable {
                            return Err(refusal(
                                "Copying annotated threaded text needs mention-ID transforms; use Cut to preserve its identities",
                            ));
                        }
                        note.source = Some(ReviewSource::Thread {
                            part: part(true)?,
                            id: ids[&id.to_ascii_lowercase()].clone(),
                            parent: parent.as_ref().map(|p| {
                                ids.get(&p.to_ascii_lowercase())
                                    .cloned()
                                    .unwrap_or_else(|| p.clone())
                            }),
                            person: person.clone(),
                            timestamp: timestamp.clone(),
                            resolved: *resolved,
                        });
                    }
                    Some(source) => {
                        if original.compatibility {
                            let root = original
                                .author
                                .strip_prefix("tc=")
                                .and_then(|id| ids.get(&id.to_ascii_lowercase()))
                                .ok_or_else(|| {
                                    refusal("A threaded fallback has no root identity")
                                })?;
                            note.author = format!("tc={root}");
                        }
                        if cut && to.sheet() == from.sheet() {
                            note.source = Some(source.clone());
                        } else {
                            let id = if cut {
                                match source {
                                    ReviewSource::NewNote { id, .. } => id.clone(),
                                    ReviewSource::Note { uid: Some(id), .. } => id.clone(),
                                    _ => fresh(&mut used),
                                }
                            } else {
                                fresh(&mut used)
                            };
                            note.source = Some(ReviewSource::NewNote {
                                part: part(false)?,
                                id,
                            });
                        }
                    }
                    None => return Err(refusal("A note has no source identity")),
                }
                all.entry(to.sheet()).or_default().push(note);
            }
            let _ = tile;
        }
    }
    for (sheet, mut notes) in all {
        let positions: BTreeSet<_> = notes
            .iter()
            .filter(|n| n.compatibility)
            .map(|n| n.at)
            .collect();
        for at in positions {
            let threads = |n: &ReviewNote| {
                n.at == at && n.source.as_ref().is_some_and(ReviewSource::threaded)
            };
            if !notes.iter().filter(|n| threads(n)).eq(host
                .model()
                .definitions()
                .review_notes(sheet)
                .iter()
                .filter(|n| threads(n)))
            {
                crate::review::refresh_compatibility(&mut notes, at);
            }
        }
        if notes.len() > 10000 {
            return Err(refusal("This paste would exceed 10,000 review records"));
        }
        if notes != host.model().definitions().review_notes(sheet) {
            edits.push(CellEdit::ReviewNotes { sheet, notes });
        }
    }
    Ok(())
}
/// Apply one validated row permutation to authored metadata, using the same immutable source
/// as the cell snapshots. Metadata is replaced once, never independently for each copied row.
pub(crate) fn permute_rows(
    host: &HostState,
    data: GridArea,
    rows: &BTreeMap<u32, u32>,
    edits: &mut Vec<CellEdit>,
) -> Result<(), CopyRefusal> {
    if rows.iter().all(|(source, target)| source == target) {
        return Ok(());
    }
    let sheet = data.sheet();
    let mut notes = host.model().definitions().review_notes(sheet).to_vec();
    for note in &mut notes {
        if data.contains(note.at) {
            let row = *rows
                .get(&note.at.row())
                .ok_or_else(|| refusal("The sort permutation omits a note row"))?;
            note.at = CellKey::new(sheet, row, note.at.column()).unwrap();
        }
    }
    if notes != host.model().definitions().review_notes(sheet) {
        host.loaded()
            .check_sorted_note_shapes(sheet, &notes)
            .map_err(|_| refusal("The original note boxes cannot follow this sort (dc-971p.38)"))?;
        edits.push(CellEdit::ReviewNotes { sheet, notes });
    }
    let mut links = Vec::new();
    let mut fragments = 0usize;
    for link in host.model().definitions().review_links(sheet) {
        let mut target = subtract(link.area, data);
        if let Some(intersection) = link.area.intersection(data) {
            fragments = fragments.saturating_add(intersection.row_count() as usize);
            if fragments > crate::COPY_LIMIT as usize {
                return Err(refusal("Too many sorted hyperlink fragments (dc-971p.38)"));
            }
            for source in intersection.first().row()..=intersection.last().row() {
                let row = *rows
                    .get(&source)
                    .ok_or_else(|| refusal("The sort omits a hyperlink row"))?;
                target.push(
                    GridArea::new(
                        CellKey::new(sheet, row, intersection.first().column()).unwrap(),
                        CellKey::new(sheet, row, intersection.last().column()).unwrap(),
                    )
                    .unwrap(),
                );
            }
        }
        for area in merge(target) {
            let mut next = link.clone();
            next.area = area;
            links.push(next);
        }
        if links.len() > 10000 {
            return Err(refusal("Too many sorted hyperlink records (dc-971p.38)"));
        }
    }
    if links != host.model().definitions().review_links(sheet) {
        host.loaded()
            .check_sorted_links(sheet, &links)
            .map_err(|_| refusal("The original hyperlinks cannot follow this sort (dc-971p.38)"))?;
        edits.push(CellEdit::ReviewLinks { sheet, links });
    }
    permute_conditional_rows(host, data, rows, edits)?;
    let mut rules = Vec::new();
    let mut fragments = 0usize;
    for original in host.model().definitions().validations(sheet) {
        // Keep the inherited prompt-only source refusal. This increment moves active rules;
        // a type=none record has no condition whose relative origin can be checked here.
        if original.kind == "none" {
            return Err(refusal(
                "Sorting prompt-only validation records is unavailable (dc-971p.38)",
            ));
        }
        if !calc_doc::validation_movement_supported(original) {
            return Err(refusal(
                "Sorting unread validation source fields is unavailable (dc-971p.38)",
            ));
        }
        let original_areas = areas(original, sheet);
        let Some(origin) = original_areas.first().map(|a| a.first()) else {
            return Err(refusal(
                "A validation rule has no complete range (dc-971p.38)",
            ));
        };
        let mut target = Vec::new();
        for area in &original_areas {
            target.extend(subtract(*area, data));
            if let Some(intersection) = area.intersection(data) {
                fragments = fragments.saturating_add(intersection.row_count() as usize);
                if fragments > crate::COPY_LIMIT as usize {
                    return Err(refusal(
                        "This sort would produce too many validation row fragments (dc-971p.38)",
                    ));
                }
                for source in intersection.first().row()..=intersection.last().row() {
                    let row = *rows
                        .get(&source)
                        .ok_or_else(|| refusal("The sort permutation omits a validation row"))?;
                    target.push(
                        GridArea::new(
                            CellKey::new(sheet, row, intersection.first().column()).unwrap(),
                            CellKey::new(sheet, row, intersection.last().column()).unwrap(),
                        )
                        .unwrap(),
                    );
                }
            }
        }
        let target = merge(target);
        if target == original_areas {
            rules.push(original.clone());
            continue;
        }
        let mut rule = original.clone();
        let to = target
            .first()
            .ok_or_else(|| refusal("A sorted validation rule lost all ranges"))?
            .first();
        rule.areas = tuples(&target);
        valid_texts(&mut rule, |f| relocate(host.model(), f, origin, to))?;
        rules.push(rule);
    }
    if rules != host.model().definitions().validations(sheet) {
        edits.push(CellEdit::Validations { sheet, rules });
    }
    Ok(())
}

/// Pointwise standard conditions can follow one row permutation. Population rules and unread
/// records retain their sort refusal rather than changing their statistical scope.
pub(crate) fn conditional_sort_supported(host: &HostState, sheet: calc_value::SheetId) -> bool {
    let rules = host.model().definitions().conditional_rules(sheet);
    if rules.len() > 1024 {
        return false;
    }
    let mut priorities = std::collections::BTreeSet::new();
    for rule in rules {
        if rule.priority < 1
            || !priorities.insert(rule.priority)
            || !rule.threshold_formulas.is_empty()
            || rule.areas.iter().any(|a| a.sheet() != sheet)
        {
            return false;
        }
        let Ok(parsed) = calc_doc::conditional_rule(rule) else {
            return false;
        };
        if parsed.unread
            || parsed.extension_id.is_some()
            || parsed.color_scale.is_some()
            || parsed.data_bar.is_some()
            || parsed.icon_set.is_some()
            || parsed.rank.is_some()
            || parsed.percent
            || parsed.bottom
            || !parsed.above_average
            || parsed.equal_average
            || parsed.std_dev.is_some()
            || parsed.time_period.is_some() && parsed.kind != "timePeriod"
            || parsed.dxf.is_some_and(|id| {
                id as usize
                    >= host
                        .model()
                        .definitions()
                        .styles()
                        .differential_formats
                        .len()
            })
        {
            return false;
        }
        let count = match parsed.kind.as_str() {
            "expression" if parsed.operator.is_none() && parsed.text.is_none() => 1,
            "cellIs" if parsed.text.is_none() => match parsed.operator.as_deref() {
                Some("between" | "notBetween") => 2,
                Some(
                    "equal" | "notEqual" | "lessThan" | "lessThanOrEqual" | "greaterThan"
                    | "greaterThanOrEqual",
                ) => 1,
                _ => return false,
            },
            // These standard pointwise rules store their executable condition in <formula>,
            // just as expression rules do. Preserve the type/text/operator source and rebase
            // that condition through the same relocator; never infer a replacement condition.
            // Standard operators: https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.conditionalformattingoperatorvalues
            "containsText" | "notContainsText" | "beginsWith" | "endsWith"
                if parsed.text.is_some()
                    && parsed.operator.as_deref()
                        == Some(if parsed.kind == "notContainsText" {
                            "notContains"
                        } else {
                            parsed.kind.as_str()
                        }) =>
            {
                1
            }
            "containsBlanks" | "notContainsBlanks" | "containsErrors" | "notContainsErrors"
                if parsed.text.is_none() && parsed.operator.is_none() =>
            {
                1
            }
            // Standard calendar labels retain their stored executable predicate.
            // https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.timeperiodvalues
            "timePeriod"
                if parsed.text.is_none()
                    && parsed.operator.is_none()
                    && matches!(
                        parsed.time_period.as_deref(),
                        Some(
                            "today"
                                | "yesterday"
                                | "tomorrow"
                                | "last7Days"
                                | "thisMonth"
                                | "lastMonth"
                                | "nextMonth"
                                | "thisWeek"
                                | "lastWeek"
                                | "nextWeek"
                        )
                    ) =>
            {
                1
            }
            _ => return false,
        };
        if rule.formulas.len() != count {
            return false;
        }
        let Some(origin) = rule.origin() else {
            return false;
        };
        for text in &rule.formulas {
            let Ok(formula) = FormulaText::new(ExcelText::from_utf16_code_units(
                text.encode_utf16().collect(),
            )) else {
                return false;
            };
            if !crate::actions::sort_formula_references_supported(host, &formula, origin)
                || calc_engine::relocate_formula_losslessly(host.model(), &formula, origin, origin)
                    .is_none()
            {
                return false;
            }
        }
    }
    true
}
fn permute_conditional_rows(
    host: &HostState,
    data: GridArea,
    rows: &BTreeMap<u32, u32>,
    edits: &mut Vec<CellEdit>,
) -> Result<(), CopyRefusal> {
    let original = host.model().definitions().conditional_rules(data.sheet());
    if original.is_empty()
        || !original
            .iter()
            .any(|r| r.areas.iter().any(|a| a.intersection(data).is_some()))
    {
        return Ok(());
    }
    if !conditional_sort_supported(host, data.sheet()) {
        return Err(refusal(
            "Sorting unread or statistical conditional rules is unavailable (dc-971p.38)",
        ));
    }
    let mut ordered: Vec<_> = original.iter().collect();
    ordered.sort_by_key(|r| r.priority);
    let mut rules = Vec::new();
    let mut fragments = 0usize;
    for rule in ordered {
        let origin = rule.origin().unwrap();
        let mut outside = Vec::new();
        for area in &rule.areas {
            let pieces = subtract(*area, data);
            fragments = fragments.saturating_add(pieces.len());
            if fragments > crate::COPY_LIMIT as usize {
                return Err(refusal(
                    "This sort would produce too many conditional-format fragments (dc-971p.38)",
                ));
            }
            outside.extend(pieces);
        }
        let outside = merge(outside);
        let mut target = Vec::new();
        if !outside.is_empty() {
            target.push((outside, false));
        }
        for area in &rule.areas {
            if let Some(intersection) = area.intersection(data) {
                fragments = fragments.saturating_add(intersection.row_count() as usize);
                if fragments > crate::COPY_LIMIT as usize {
                    return Err(refusal(
                        "This sort would produce too many conditional-format fragments (dc-971p.38)",
                    ));
                }
                for source in intersection.first().row()..=intersection.last().row() {
                    let row = *rows.get(&source).ok_or_else(|| {
                        refusal("The sort permutation omits a conditional-format row")
                    })?;
                    target.push((
                        vec![
                            GridArea::new(
                                CellKey::new(data.sheet(), row, intersection.first().column())
                                    .unwrap(),
                                CellKey::new(data.sheet(), row, intersection.last().column())
                                    .unwrap(),
                            )
                            .unwrap(),
                        ],
                        true,
                    ));
                    if rules.len() + target.len() > 1024 {
                        return Err(refusal(
                            "This sort would exceed 1,024 conditional rules (dc-971p.38)",
                        ));
                    }
                }
            }
        }
        if fragments > crate::COPY_LIMIT as usize || rules.len() + target.len() > 1024 {
            return Err(refusal(
                "This sort would exceed the conditional-format fragment limit (dc-971p.38)",
            ));
        }
        for (areas, copied) in target {
            let mut next = rule.clone();
            next.areas = areas;
            let at = next.origin().unwrap();
            rule_texts(&mut next, |text| {
                let formula = FormulaText::new(ExcelText::from_utf16_code_units(
                    text.encode_utf16().collect(),
                ))
                .map_err(|_| CopyRefusal::Unrelocatable { at: origin })?;
                calc_engine::relocate_formula_losslessly(host.model(), &formula, origin, at)
                    .map(|f| f.text().to_string_lossy())
                    .ok_or_else(|| {
                        refusal(
                            "A conditional-format reference would be lost by sorting (dc-971p.38)",
                        )
                    })
            })?;
            if copied {
                next = calc_doc::copied_conditional_source(&next).map_err(|_| {
                    refusal("The conditional source cannot follow this sort (dc-971p.38)")
                })?;
            }
            next.authored = true;
            rules.push(next);
        }
    }
    for (index, rule) in rules.iter_mut().enumerate() {
        rule.priority = index as i64 + 1;
    }
    if rules != original {
        edits.push(CellEdit::ConditionalRules {
            sheet: data.sheet(),
            rules,
        });
    }
    Ok(())
}
