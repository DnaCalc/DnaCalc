//! `Chart.Export(Filename, FilterName, Interactive)` (dc-oxh8w.22): the chart drawn at its frame's size
//! at 100% (points to pixels at 96 dots per inch) and written through the application host. The drawing
//! is the installed rasteriser's ([`crate::chart::set_chart_exporter`]); without one the call refuses.

use super::*;
use crate::chart::ExportFormat;

const OWNER: &str = "dc-oxh8w.22";

/// The format a filter name or, failing that, a file extension names.
fn format_of(name: &str) -> Option<ExportFormat> {
    match name.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "png" => Some(ExportFormat::Png),
        "gif" => Some(ExportFormat::Gif),
        "jpg" | "jpeg" => Some(ExportFormat::Jpg),
        _ => None,
    }
}

impl WorkbookAutomation {
    /// Excel returns True when the picture was written and False when the filter is not one it knows.
    pub(super) fn chart_export(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.is_empty() || call.args.len() > 3 {
            return Err(geometry::bad_count());
        }
        let AutoValue::Text(file) = call.arg(0) else {
            return Err(geometry::type_mismatch());
        };
        let file = file.to_string_lossy();
        if file.is_empty() {
            return Err(AutoError::excel(1004, "Method 'Export' of object '_Chart' failed"));
        }
        let format = match call.arg(1) {
            AutoValue::Missing | AutoValue::Empty => file.rsplit_once('.').and_then(|(_, ext)| format_of(ext)),
            AutoValue::Text(filter) => format_of(&filter.to_string_lossy()),
            _ => return Err(geometry::type_mismatch()),
        };
        let Some(format) = format else {
            return Ok(AutoValue::Bool(false));
        };
        let part = Self::chart_token(target)?;
        let (view, [_, _, width, height]) = self.read_host(target, |s, sheet, _| {
            let rectangle = Self::chart_rectangle(s, sheet, &part)?;
            let view = s.host().charts(sheet).0.into_iter().find(|c| c.part == part);
            let view = view.ok_or_else(|| AutoError::excel(424, "Object required"))?;
            Ok((view, rectangle))
        })?;
        let pixels = |points: f64| (points * 96.0 / 72.0).round().max(1.0) as u32;
        let exporter = crate::chart::chart_exporter()
            .ok_or_else(|| AutoError::not_modelled("Chart.Export without an installed chart rasteriser", OWNER))?;
        let bytes = exporter(&view, pixels(width), pixels(height), format)
            .map_err(|why| AutoError::excel(1004, format!("Method 'Export' of object '_Chart' failed: {why}")))?;
        self.host.write_file(&file, &bytes)?;
        Ok(AutoValue::Bool(true))
    }
}
