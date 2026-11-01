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
