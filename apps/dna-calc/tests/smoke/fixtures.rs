//! Synthetic documented OOXML variations of the retained features fixture, not Excel readings.
#[path = "../../../../crates/calc-engine/tests/fixtures/seven_cell.rs"]
mod seven_cell;

pub(crate) fn calculation_script() -> String {
    include_str!("recalc-worker.js")
        .replace(
            "$FIXTURE",
            &serde_json::to_string(seven_cell::CELLS).expect("the fixture serializes"),
        )
        .replace(
            "$PURE_SCOPE",
            &serde_json::to_string(seven_cell::PURE_SCOPE).expect("the scope serializes"),
        )
}

pub(crate) fn alignment(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let parts = crate::oracle_cases::package_parts(bytes)?;
    let styles = std::str::from_utf8(parts.get("xl/styles.xml").ok_or("no styles")?)
        .map_err(|e| e.to_string())?;
    let count = styles
        .split("<cellXfs count=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .ok_or("cell formats")?
        .parse::<usize>()
        .map_err(|e| e.to_string())?;
    let extra = include_str!("alignment/styles.xml");
    let styles = styles
        .replacen(
            &format!("<cellXfs count=\"{count}\""),
            &format!("<cellXfs count=\"{}\"", count + 14),
            1,
        )
        .replacen("</cellXfs>", &format!("{extra}</cellXfs>"), 1);
    let mut sheet = include_str!("alignment/sheet.xml").to_owned();
    for i in (0..14).rev() {
        sheet = sheet.replace(&format!("${i}"), &(count + i).to_string());
    }
    crate::oracle_cases::replace_package_parts(
        bytes,
        &std::collections::BTreeMap::from([
            ("xl/styles.xml".into(), styles.into_bytes()),
            ("xl/worksheets/sheet1.xml".into(), sheet.into_bytes()),
        ]),
    )
}

/// A dense, explicitly synthetic renderer workload in the existing OOXML fixture adapter.
/// Values are authored inputs; formulas still run through the application's worker and engine.
pub(crate) fn renderer_grid(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let parts = crate::oracle_cases::package_parts(bytes)?;
    let book = std::str::from_utf8(parts.get("xl/workbook.xml").ok_or("no workbook")?)
        .map_err(|e| e.to_string())?;
    let book = if book.contains("calcMode=\"manual\"") {
        book.replacen("calcMode=\"manual\"", "calcMode=\"auto\"", 1)
    } else if book.contains("calcMode=\"auto\"") {
        book.to_owned()
    } else if !book.contains("calcMode=") {
        book.replacen("<calcPr ", "<calcPr calcMode=\"auto\" ", 1)
    } else {
        return Err("Renderer fixture needs automatic calculation".into());
    };
    let mut sheet = String::from(
        "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><dimension ref=\"A1:X512\"/><sheetData>",
    );
    for row in 1..=512 {
        sheet.push_str(&format!("<row r=\"{row}\">"));
        for col in 0..24u8 {
            let address = format!("{}{row}", char::from(b'A' + col));
            let value = if col % 3 == 0 {
                format!("<c r=\"{address}\"><v>{}</v></c>", row + u32::from(col))
            } else if col % 3 == 1 {
                let text = match row % 4 {
                    0 => "العربية",
                    1 => "café 🚀",
                    2 => "Wiii 0123",
                    _ => "wide mixed text clipped at its neighbour",
                };
                format!("<c r=\"{address}\" t=\"inlineStr\"><is><t>{text}</t></is></c>")
            } else {
                format!("<c r=\"{address}\"><f>$A$1+{row}+{col}</f></c>")
            };
            sheet.push_str(&value);
        }
        sheet.push_str("</row>");
    }
    sheet.push_str("</sheetData></worksheet>");
    crate::oracle_cases::replace_package_parts(
        bytes,
        &std::collections::BTreeMap::from([
            ("xl/worksheets/sheet1.xml".into(), sheet.into_bytes()),
            ("xl/workbook.xml".into(), book.into_bytes()),
        ]),
    )
}
