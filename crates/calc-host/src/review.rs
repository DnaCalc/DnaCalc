//! Authored review operations share the workbook transaction, history and save route.
use crate::{CellEdit, HostState};
use calc_doc_model::{NoteStylePatch, ReviewNote, ReviewSource};
use calc_value::{CellKey, SheetId};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewAction {
    Create {
        parent: Option<u32>,
        threaded: bool,
        author: String,
        text: String,
        timestamp: String,
    },
    Format {
        index: u32,
        text: String,
        start: u32,
        end: u32,
        patch: NoteStylePatch,
    },
    Resolve {
        index: u32,
        resolved: bool,
    },
    Edit {
        index: u32,
        text: String,
    },
}
pub(crate) fn prepare(
    host: &HostState,
    at: CellKey,
    revision: u64,
    action: ReviewAction,
) -> Result<Vec<CellEdit>, String> {
    if revision != host.revision().get() {
        return Err("The workbook changed; reopen this note or comment".into());
    }
    editable(host, at.sheet())?;
    let mut notes = host.model().definitions().review_notes(at.sheet()).to_vec();
    match action {
        ReviewAction::Create {
            parent,
            threaded,
            author,
            text,
            timestamp,
        } => {
            if notes.len() >= 5000 {
                return Err("This worksheet already has 5,000 review records (dc-971p.40)".into());
            }
            let author = author.trim().to_owned();
            if author.is_empty() || author.encode_utf16().count() > 255 || text.trim().is_empty() {
                return Err("Enter an author and some note or comment text".into());
            }
            check_text(&text)?;
            let parent = if let Some(index) = parent {
                let note = notes
                    .get(index as usize)
                    .filter(|n| n.at == at)
                    .ok_or("The parent comment no longer belongs to this cell")?;
                match &note.source {
                    Some(ReviewSource::Thread {
                        id,
                        parent,
                        resolved,
                        ..
                    }) if threaded => {
                        let root = root_id(&notes, id)?;
                        if *resolved
                            || notes.iter().any(|n| {
                                matches!(&n.source, Some(ReviewSource::Thread { id, resolved: true, .. }) if id == &root)
                            })
                        {
                            return Err("Reopen the resolved thread before replying".into());
                        }
                        Some(root)
                    }
                    _ => return Err("Replies need a threaded comment".into()),
                }
            } else {
                if notes.iter().any(|n| n.at == at && !n.compatibility) {
                    return Err(
                        "This cell already has a note or comment; edit it or reply to its thread"
                            .into(),
                    );
                }
                None
            };
            let part = part_for(host, at.sheet(), &notes, threaded)?;
            let id = fresh_guid(host, &[]);
            if threaded {
                if timestamp.is_empty()
                    || timestamp.len() > 40
                    || !timestamp
                        .bytes()
                        .all(|b| b.is_ascii_digit() || b"-:TZ.+".contains(&b))
                {
                    return Err(
                        "A comment needs an ISO date and time supplied by the application".into(),
                    );
                }
                let person = host
                    .model()
                    .definitions()
                    .all_review_notes()
                    .flat_map(|(_, n)| n)
                    .find_map(|n| match &n.source {
                        Some(ReviewSource::Thread { person, .. }) if n.author == author => {
                            Some(person.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| fresh_guid(host, &[&id]));
                let root = parent.is_none();
                notes.push(ReviewNote {
                    at,
                    author,
                    text,
                    spans: Vec::new(),
                    source_xml: None,
                    source: Some(ReviewSource::Thread {
                        part,
                        id: id.clone(),
                        parent,
                        person,
                        timestamp: Some(timestamp),
                        resolved: false,
                    }),
                    editable: true,
                    annotated: false,
                    compatibility: false,
                });
                if root {
                    let part = part_for(host, at.sheet(), &notes, false)?;
                    let used = notes
                        .iter()
                        .filter_map(|n| n.source.as_ref())
                        .flat_map(|s| match s {
                            ReviewSource::Thread { id, person, .. } => {
                                vec![id.as_str(), person.as_str()]
                            }
                            ReviewSource::NewNote { id, .. } => vec![id.as_str()],
                            _ => vec![],
                        })
                        .collect::<Vec<_>>();
                    let fallback_id = fresh_guid(host, &used);
                    notes.push(ReviewNote {
                        at,
                        author: format!("tc={id}"),
                        text: String::new(),
                        spans: Vec::new(),
                        source_xml: None,
                        source: Some(ReviewSource::NewNote {
                            part,
                            id: fallback_id,
                        }),
                        editable: false,
                        annotated: false,
                        compatibility: true,
                    });
                }
            } else {
                notes.push(ReviewNote {
                    at,
                    author,
                    text,
                    spans: Vec::new(),
                    source_xml: None,
                    source: Some(ReviewSource::NewNote { part, id }),
                    editable: true,
                    annotated: false,
                    compatibility: false,
                });
            }
        }
        ReviewAction::Edit { index, text } => {
            check_text(&text)?;
            let note = notes
                .get_mut(index as usize)
                .filter(|n| n.at == at)
                .ok_or("The note no longer belongs to this cell")?;
            if !note.editable {
                return Err("This comment has annotations whose text offsets cannot be edited yet (dc-971p.40)".into());
            }
            note.replace_text(text);
        }
        ReviewAction::Format {
            index,
            text,
            start,
            end,
            mut patch,
        } => {
            check_text(&text)?;
            if let Some(color) = &patch.color {
                patch.color = crate::format::color(color)?.and_then(|c| c.rgb);
            }
            let note = notes
                .get_mut(index as usize)
                .filter(|n| n.at == at)
                .ok_or("The note no longer belongs to this cell")?;
            if !note.editable || note.source.as_ref().is_some_and(ReviewSource::threaded) {
                return Err("Rich text formatting applies to legacy notes".into());
            }
            note.replace_text(text);
            note.format(start, end, &patch).map_err(str::to_owned)?;
        }
        ReviewAction::Resolve { index, resolved } => {
            let note = notes
                .get(index as usize)
                .filter(|n| n.at == at)
                .ok_or("The comment no longer belongs to this cell")?;
            let root = match &note.source {
                Some(ReviewSource::Thread { id, .. }) => root_id(&notes, id)?,
                _ => return Err("Choose a threaded comment".into()),
            };
            let root = notes
                .iter_mut()
                .find(|n| matches!(&n.source,Some(ReviewSource::Thread{id,..})if id==&root))
                .ok_or("The root comment is unavailable")?;
            if let Some(ReviewSource::Thread { resolved: old, .. }) = &mut root.source {
                *old = resolved;
            }
        }
    }
    refresh_compatibility(&mut notes, at);
    validate(host, at.sheet(), &notes)?;
    if notes == host.model().definitions().review_notes(at.sheet()) {
        Ok(Vec::new())
    } else {
        Ok(vec![CellEdit::ReviewNotes {
            sheet: at.sheet(),
            notes,
        }])
    }
}
fn check_text(text: &str) -> Result<(), String> {
    if text.encode_utf16().count() > 32767 {
        Err("Use at most 32,767 characters".into())
    } else {
        Ok(())
    }
}
fn editable(host: &HostState, sheet: SheetId) -> Result<(), String> {
    if host.model().sheet_by_id(sheet).is_none()
        || host.loaded().sheet(sheet).is_some_and(|s| {
            s.protected() || s.is_macro_sheet() || !s.review().unsupported.is_empty()
        })
    {
        return Err(
            "Review editing needs an editable worksheet with readable review metadata".into(),
        );
    }
    if host
        .model()
        .definitions()
        .review_notes(sheet)
        .iter()
        .filter_map(|n| n.source.as_ref())
        .any(|source| {
            host.loaded()
                .sheets()
                .iter()
                .filter(|s| {
                    s.review()
                        .notes
                        .iter()
                        .any(|n| n.source.as_ref().is_some_and(|v| v.part() == source.part()))
                })
                .count()
                > 1
        })
    {
        return Err(
            "Editing shared review parts is unavailable (dc-971p.40); nothing changed".into(),
        );
    }
    Ok(())
}
pub(crate) fn fresh_guid(host: &HostState, extra: &[&str]) -> String {
    let mut used: BTreeSet<String> = host
        .loaded()
        .review_people()
        .keys()
        .map(|id| id.to_ascii_uppercase())
        .collect();
    for (_, notes) in host
        .model()
        .definitions()
        .all_review_notes()
        .chain(host.loaded().model().definitions().all_review_notes())
    {
        for note in notes {
            match &note.source {
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
    for id in extra {
        used.insert(id.to_ascii_uppercase());
    }
    (1u64..)
        .map(|n| format!("{{D1ACA118-0000-4000-8000-{n:012X}}}"))
        .find(|id| !used.contains(id))
        .expect("review identity")
}
pub(crate) fn part_for(
    host: &HostState,
    sheet: SheetId,
    notes: &[ReviewNote],
    threaded: bool,
) -> Result<String, String> {
    if let Some(part) = notes
        .iter()
        .filter_map(|n| n.source.as_ref())
        .find(|s| s.threaded() == threaded)
        .map(|s| s.part().to_owned())
    {
        return Ok(part);
    }
    if let Some(part) = host.loaded().review_part(sheet, threaded) {
        return Ok(part);
    }
    let stem = if threaded {
        "threadedComments/dnaReview"
    } else {
        "comments/dnaReview"
    };
    let reserved = host
        .model()
        .definitions()
        .all_review_notes()
        .flat_map(|(_, n)| n)
        .filter_map(|n| n.source.as_ref().map(|s| s.part().to_owned()))
        .collect::<Vec<_>>();
    Ok(host.loaded().available_part_name(stem, "xml", &reserved))
}

pub(crate) fn refresh_compatibility(notes: &mut [ReviewNote], at: CellKey) {
    let text = calc_doc_model::review_thread_text(notes, at);
    for note in notes.iter_mut().filter(|n| n.at == at && n.compatibility) {
        note.replace_text(text.clone());
    }
}
fn same_identity(a: &ReviewSource, b: &ReviewSource) -> bool {
    match (a, b) {
        (
            ReviewSource::Note {
                part: a, origin: x, ..
            },
            ReviewSource::Note {
                part: b, origin: y, ..
            },
        ) => a == b && x == y,
        (ReviewSource::NewNote { id: a, .. }, ReviewSource::NewNote { id: b, .. }) => {
            a.eq_ignore_ascii_case(b)
        }
        (ReviewSource::Thread { id: a, .. }, ReviewSource::Thread { id: b, .. }) => {
            a.eq_ignore_ascii_case(b)
        }
        _ => false,
    }
}
pub(crate) fn validate(
    host: &HostState,
    sheet: SheetId,
    notes: &[ReviewNote],
) -> Result<(), String> {
    editable(host, sheet)?;
    if notes.len() > 10000 {
        return Err("Review metadata exceeds 10,000 records (dc-971p.40)".into());
    }
    let previous: Vec<_> = host
        .model()
        .definitions()
        .all_review_notes()
        .flat_map(|(_, n)| n)
        .chain(
            host.loaded()
                .model()
                .definitions()
                .all_review_notes()
                .flat_map(|(_, n)| n),
        )
        .collect();
    let spatial = |n: &[ReviewNote]| {
        n.iter()
            .filter(|n| !n.source.as_ref().is_some_and(ReviewSource::threaded))
            .map(|n| n.at)
            .collect::<BTreeSet<_>>()
    };
    if spatial(notes) != spatial(host.model().definitions().review_notes(sheet))
        && host.loaded().review_drawing_shared(sheet)
    {
        return Err("Moving review markers in a shared drawing is unavailable (dc-971p.40)".into());
    }
    let loaded_people = host.loaded().review_people();
    for (index, note) in notes.iter().enumerate() {
        check_text(&note.text)?;
        if note.at.sheet() != sheet || !note.valid_spans() {
            return Err("Review text spans or worksheet are invalid".into());
        }
        if note
            .spans
            .iter()
            .filter_map(|s| s.style.source.as_ref())
            .any(|xml| {
                !previous
                    .iter()
                    .flat_map(|n| &n.spans)
                    .any(|span| span.style.source.as_ref() == Some(xml))
            })
        {
            return Err("Note run properties must retain a readable imported source".into());
        }
        let source = note
            .source
            .as_ref()
            .ok_or("A review record needs a source identity")?;
        let threaded = source.threaded();
        if threaded && !note.spans.is_empty() {
            return Err("Threaded comment text cannot carry legacy note runs".into());
        }
        for span in &note.spans {
            let previous_style = span
                .style
                .source
                .as_ref()
                .and_then(|xml| {
                    previous
                        .iter()
                        .flat_map(|n| &n.spans)
                        .find(|s| s.style.source.as_ref() == Some(xml))
                })
                .map(|s| &s.style);
            if span.style.size != previous_style.and_then(|s| s.size.clone()) {
                return Err("Note font sizes retain their imported source".into());
            }
            if span.style.color != previous_style.and_then(|s| s.color.clone())
                && span.style.color.as_ref().is_some_and(|c| {
                    !matches!(c.len(), 6 | 8) || !c.bytes().all(|b| b.is_ascii_hexdigit())
                })
            {
                return Err("Note colours need a six- or eight-digit RGB value".into());
            }
        }

        if threaded && note.compatibility {
            return Err("Threaded comments cannot be compatibility notes".into());
        }
        if note.compatibility
            && (note.editable
                || !note.author.strip_prefix("tc=").is_some_and(|root| {
                    notes.iter().any(|n| {
                        n.at == note.at
                            && matches!(
                                &n.source,
                                Some(ReviewSource::Thread { id, parent: None, .. })
                                    if id.eq_ignore_ascii_case(root)
                            )
                    })
                }))
        {
            return Err("A compatibility note needs its root comment identity".into());
        }

        let approved = host.loaded().review_part(sheet, threaded);
        if host.loaded().has_part(source.part()) {
            if approved.as_deref() != Some(source.part()) {
                return Err("Review records must use this worksheet's review part".into());
            }
        } else {
            let parent = host
                .loaded()
                .workbook_part()
                .rsplit_once('/')
                .map_or("", |(p, _)| p);
            let folder = if threaded {
                "threadedComments"
            } else {
                "comments"
            };
            let prefix = format!("{parent}/{folder}/dnaReview")
                .trim_start_matches('/')
                .to_owned();
            if !source
                .part()
                .strip_prefix(&prefix)
                .and_then(|s| s.strip_suffix(".xml"))
                .is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit()))
            {
                return Err("New review parts need an authored package name".into());
            }
        }
        if notes[..index]
            .iter()
            .filter_map(|n| n.source.as_ref())
            .any(|s| same_identity(s, source))
        {
            return Err("Review source identities must be unique".into());
        }
        if let Some(old) = previous
            .iter()
            .find(|n| n.source.as_ref().is_some_and(|s| same_identity(s, source)))
        {
            let unchanged_source = match (&old.source, source) {
                (
                    Some(ReviewSource::Thread {
                        id: a,
                        parent: b,
                        person: c,
                        timestamp: d,
                        ..
                    }),
                    ReviewSource::Thread {
                        id: x,
                        parent: y,
                        person: z,
                        timestamp: t,
                        ..
                    },
                ) => a == x && b == y && c == z && d == t,
                (
                    Some(ReviewSource::NewNote { id: a, .. }),
                    ReviewSource::NewNote { id: b, .. },
                ) => a == b,
                (Some(old), new) => old == new,
                _ => false,
            };
            if old.author != note.author
                || old.editable != note.editable
                || old.compatibility != note.compatibility
                || old.annotated != note.annotated
                || old.source_xml != note.source_xml
                || !unchanged_source
                || ((!note.editable && !note.compatibility || note.annotated)
                    && (note.text != old.text || note.spans != old.spans))
            {
                return Err("Review edits must retain source identities, authors and unsupported annotations (dc-971p.40)".into());
            }
        } else {
            if note.source_xml.is_none()
                && (note.annotated
                    || threaded && !note.editable
                    || !threaded && !note.compatibility && !note.editable)
            {
                return Err("New review text must have editable plain content".into());
            }
            let id = match source {
                ReviewSource::Thread {
                    id,
                    person,
                    timestamp,
                    ..
                } => {
                    if !valid_id(person)
                        || timestamp.as_ref().is_none_or(|t| {
                            t.is_empty()
                                || t.len() > 40
                                || !t
                                    .bytes()
                                    .all(|b| b.is_ascii_digit() || b"-:TZ.+".contains(&b))
                        })
                    {
                        return Err("A new comment needs valid person and time fields".into());
                    }
                    id
                }
                ReviewSource::NewNote { id, .. } => id,
                _ => return Err("New legacy notes need an authored source identity".into()),
            };
            if !valid_id(id)
                || note.author.trim().is_empty()
                || note.author.encode_utf16().count() > 255
            {
                return Err("A new review record needs a valid identity and author".into());
            }
            if let Some(xml) = &note.source_xml
                && !previous.iter().any(|n| {
                    n.source_xml.as_ref() == Some(xml)
                        && n.source.as_ref().is_some_and(|s| s.threaded() == threaded)
                        && n.editable == note.editable
                        && n.compatibility == note.compatibility
                        && n.annotated == note.annotated
                        && (!threaded || n.editable)
                })
            {
                return Err("Copied review XML must retain a readable original record".into());
            }
        }
        if let ReviewSource::Thread { person, .. } = source {
            if loaded_people
                .get(&person.to_ascii_lowercase())
                .is_some_and(|author| author != &note.author)
                || notes.iter().any(|n| {
                    matches!(
                        &n.source,
                        Some(ReviewSource::Thread { person: p, .. })
                            if p.eq_ignore_ascii_case(person)
                    ) && n.author != note.author
                })
            {
                return Err("A person identity must retain its author".into());
            }
        }
        if let ReviewSource::Thread {
            parent: Some(parent),
            person,
            ..
        } = source
        {
            let root = root_id(notes, parent)?;
            if !notes.iter().any(|n| {
                n.at == note.at
                    && matches!(
                        &n.source,
                        Some(ReviewSource::Thread { id, parent: None, .. })
                            if id.eq_ignore_ascii_case(&root)
                    )
            }) {
                return Err("A reply needs a root comment on the same cell".into());
            }
            if previous.iter().any(|n| {
                matches!(&n.source,Some(ReviewSource::Thread{person:p,..})if p==person)
                    && n.author != note.author
            }) {
                return Err("A person identity must retain its author".into());
            }
        }
    }
    for note in notes.iter().filter(|n| n.compatibility) {
        let original = host.loaded().model().definitions().review_notes(sheet);
        let originals_unchanged = notes
            .iter()
            .filter(|n| n.at == note.at && n.source.as_ref().is_some_and(ReviewSource::threaded))
            .all(|n| {
                original
                    .iter()
                    .any(|o| o.source == n.source && o.text == n.text)
            });
        let original_fallback = original.iter().any(|o| {
            o.compatibility
                && o.source == note.source
                && o.author == note.author
                && o.text == note.text
        });
        if note.text != calc_doc_model::review_thread_text(notes, note.at)
            && !(originals_unchanged && original_fallback)
        {
            return Err("Thread compatibility text must follow the authoritative comments".into());
        }
    }
    Ok(())
}

fn root_id(notes: &[ReviewNote], id: &str) -> Result<String, String> {
    let mut id = id.to_owned();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(id.to_ascii_lowercase()) {
            return Err("The comment thread contains a parent cycle".into());
        }
        let source = notes
            .iter()
            .filter_map(|n| n.source.as_ref())
            .find(|s| matches!(s,ReviewSource::Thread{id:n,..}if n.eq_ignore_ascii_case(&id)))
            .ok_or("The parent comment is unavailable")?;
        match source {
            ReviewSource::Thread {
                id: current,
                parent: None,
                ..
            } => return Ok(current.clone()),
            ReviewSource::Thread {
                parent: Some(parent),
                ..
            } => id = parent.clone(),
            _ => unreachable!(),
        }
    }
}

pub(crate) fn validate_batch(
    host: &HostState,
    batch: &calc_doc_model::OpBatch,
) -> Result<(), String> {
    let mut sheets = host
        .model()
        .definitions()
        .all_review_notes()
        .map(|(s, n)| (s, n.to_vec()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for op in &batch.operations {
        if let calc_doc_model::WorkbookOp::SetReviewNotes { sheet, notes } = op {
            validate(host, *sheet, notes)?;
            sheets.insert(*sheet, notes.clone());
        }
    }
    let mut identities = Vec::new();
    let mut revision_ids = BTreeSet::new();
    let mut people = std::collections::BTreeMap::new();
    let mut parts = std::collections::BTreeMap::new();
    for notes in sheets.values() {
        let mut cells = BTreeSet::new();
        for note in notes {
            if let Some(source) = &note.source {
                let uid = match source {
                    ReviewSource::Note { uid, .. } => uid.as_ref(),
                    ReviewSource::NewNote { id, .. } => Some(id),
                    _ => None,
                };
                if uid.is_some_and(|id| !revision_ids.insert(id.to_ascii_lowercase())) {
                    return Err("Legacy note revision IDs must be unique".into());
                }
                if parts
                    .insert(source.part().to_owned(), note.at.sheet())
                    .is_some_and(|s| s != note.at.sheet())
                {
                    return Err("A review part cannot be shared by authored worksheets".into());
                }
                if let ReviewSource::Thread { person, .. } = source {
                    if people
                        .insert(person.to_ascii_lowercase(), note.author.clone())
                        .is_some_and(|a| a != note.author)
                    {
                        return Err(
                            "A person identity must have one author across the workbook".into()
                        );
                    }
                }
            }
            if !note.compatibility
                && !note.source.as_ref().is_some_and(ReviewSource::threaded)
                && !cells.insert(note.at)
            {
                return Err("A cell cannot contain two legacy notes".into());
            }
            if let Some(source) = &note.source {
                if identities.iter().any(|s| same_identity(s, source)) {
                    return Err("Review identities must be unique across the workbook".into());
                }
                identities.push(source.clone());
            }
        }
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 38
        && b[0] == b'{'
        && b[37] == b'}'
        && (1..37).all(|i| {
            if [9, 14, 19, 24].contains(&i) {
                b[i] == b'-'
            } else {
                b[i].is_ascii_hexdigit()
            }
        })
}
/// The document currently replays axis edits before note reconciliation. Until that replay
/// records note-box permutations in order, do not let it act on an imported box's old row.
pub(crate) fn has_sorted_imported_notes(host: &HostState, sheet: SheetId) -> bool {
    host.model()
        .definitions()
        .review_notes(sheet)
        .iter()
        .any(|note| {
            if !matches!(note.source, Some(ReviewSource::Note { .. })) {
                return false;
            }
            let original = host
                .loaded()
                .model()
                .definitions()
                .review_notes(sheet)
                .iter()
                .find(|old| old.source == note.source);
            let Some(old) = original else {
                return true;
            };
            let expected = host
                .axis_edits
                .iter()
                .filter(|e| e.sheet() == sheet)
                .try_fold(old.at, |at, e| calc_doc_model::moved_cell(*e, at));
            expected != Some(note.at)
        })
}
