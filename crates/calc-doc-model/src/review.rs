//! Authored note and threaded-comment text with the source identity retained for lossless saves.
use crate::value::CellKey;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReviewSource {
    NewNote { part:String, id:String },
    Note {
        part: String,
        origin: CellKey,
        uid:Option<String>,
    },
    Thread {
        part: String,
        id: String,
        parent: Option<String>,
        person: String,
        timestamp: Option<String>,
        resolved: bool,
    },
}
impl ReviewSource {
    pub fn part(&self) -> &str {
        match self {
            Self::Note { part, .. } | Self::Thread { part, .. } | Self::NewNote {part,..} => part,
        }
    }
    pub fn threaded(&self) -> bool {
        matches!(self, Self::Thread { .. })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReviewNote {
    pub at: CellKey,
    pub author: String,
    pub text: String,
    pub spans: Vec<NoteSpan>,
    pub source_xml: Option<std::sync::Arc<[u8]>>,
    pub source: Option<ReviewSource>,
    /// Text carrying mentions or unsupported annotations is preserved until their offsets can move.
    pub editable: bool,
    /// Text annotations whose offsets this editor cannot transform.
    pub annotated:bool,
    /// Legacy compatibility text for a threaded comment, retained but not a second annotation.
    pub compatibility: bool,
}

/// Plain compatibility text for readers that do not understand threaded comments.
pub fn review_thread_text(notes: &[ReviewNote], at: CellKey) -> String {
    notes
        .iter()
        .filter(|n| n.at == at && n.source.as_ref().is_some_and(ReviewSource::threaded))
        .map(|n| format!("{}: {}", n.author, n.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}
// Rich note spans use UTF-16 offsets.

use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct NoteStyle {
    /// Unedited run properties, including properties the renderer does not understand.
    pub source: Option<Arc<[u8]>>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<String>,
    pub size: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NoteSpan {
    pub start: u32,
    pub end: u32,
    pub style: NoteStyle,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteStylePatch {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub color: Option<String>,
    pub clear: bool,
}
impl NoteStyle {
    fn patched(&self, patch: &NoteStylePatch) -> Self {
        let mut next = if patch.clear {
            Self::default()
        } else {
            self.clone()
        };
        if let Some(v) = patch.bold {
            next.bold = v;
        }
        if let Some(v) = patch.italic {
            next.italic = v;
        }
        if let Some(v) = patch.underline {
            next.underline = v;
        }
        if let Some(v) = &patch.color {
            next.color = Some(v.clone());
        }
        next
    }
}
impl ReviewNote {
    pub fn valid_spans(&self) -> bool {
        let len = self.text.encode_utf16().count() as u32;
        let mut last = 0;
        self.spans.iter().all(|span| {
            let valid = span.start >= last
                && span.start < span.end
                && span.end <= len
                && boundary(&self.text, span.start)
                && boundary(&self.text, span.end);
            last = span.end;
            valid
        })
    }
    pub fn style_at(&self, offset: u32) -> NoteStyle {
        self.spans
            .iter()
            .find(|s| s.start <= offset && offset < s.end)
            .map(|s| s.style.clone())
            .unwrap_or_default()
    }
    /// Existing style survives unchanged prefixes/suffixes; replacement text takes the style at
    /// its first replaced character (or the preceding character when appended).
    pub fn replace_text(&mut self, text: String) {
        if self.text == text {
            return;
        }
        let old: Vec<_> = self.text.encode_utf16().collect();
        let new: Vec<_> = text.encode_utf16().collect();
        let mut prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        while !boundary(&self.text, prefix as u32) || !boundary(&text, prefix as u32) {
            prefix -= 1;
        }
        let mut suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        while !boundary(&self.text, (old.len() - suffix) as u32)
            || !boundary(&text, (new.len() - suffix) as u32)
        {
            suffix -= 1;
        }
        let inherit = self.style_at((prefix as u32).min((old.len() as u32).saturating_sub(1)));
        let mut spans = Vec::new();
        for span in &self.spans {
            if span.start < prefix as u32 {
                push_span(
                    &mut spans,
                    span.start,
                    span.end.min(prefix as u32),
                    span.style.clone(),
                );
            }
        }
        push_span(
            &mut spans,
            prefix as u32,
            (new.len() - suffix) as u32,
            inherit,
        );
        let delta = new.len() as i64 - old.len() as i64;
        for span in &self.spans {
            let start = span.start.max((old.len() - suffix) as u32);
            if start < span.end {
                push_span(
                    &mut spans,
                    (i64::from(start) + delta) as u32,
                    (i64::from(span.end) + delta) as u32,
                    span.style.clone(),
                );
            }
        }
        self.text = text;
        self.spans = spans;
    }
    pub fn format(
        &mut self,
        start: u32,
        end: u32,
        patch: &NoteStylePatch,
    ) -> Result<(), &'static str> {
        let len = self.text.encode_utf16().count() as u32;
        if start >= end || end > len || !boundary(&self.text, start) || !boundary(&self.text, end) {
            return Err("Select complete characters within the note");
        }
        let mut boundaries = vec![0, len, start, end];
        for span in &self.spans {
            boundaries.extend([span.start, span.end]);
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut spans = Vec::new();
        for pair in boundaries.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let style = self.style_at(a);
            let style = if a >= start && b <= end {
                style.patched(patch)
            } else {
                style
            };
            push_span(&mut spans, a, b, style);
        }
        self.spans = spans;
        Ok(())
    }
}
pub fn note_segments(note: &ReviewNote) -> Vec<(String, NoteStyle)> {
    if !note.valid_spans(){return vec![(note.text.clone(),NoteStyle::default())];}
    let units: Vec<_> = note.text.encode_utf16().collect();
    let mut out = Vec::new();
    let mut at = 0usize;
    for span in &note.spans {
        let (a, b) = (span.start as usize, span.end as usize);
        if a > at {
            out.push((
                String::from_utf16_lossy(&units[at..a]),
                NoteStyle::default(),
            ));
        }
        out.push((String::from_utf16_lossy(&units[a..b]), span.style.clone()));
        at = b;
    }
    if at < units.len() {
        out.push((String::from_utf16_lossy(&units[at..]), NoteStyle::default()));
    }
    out
}
fn push_span(spans: &mut Vec<NoteSpan>, start: u32, end: u32, style: NoteStyle) {
    if start >= end || style == NoteStyle::default() {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.end == start
        && last.style == style
    {
        last.end = end;
    } else {
        spans.push(NoteSpan { start, end, style });
    }
}
fn boundary(text: &str, offset: u32) -> bool {
    let mut at = 0;
    for c in text.chars() {
        if at == offset {
            return true;
        }
        at += c.len_utf16() as u32;
    }
    at == offset
}
