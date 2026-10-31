//! Shared clipboard output encoding.
pub fn encode_tsv(rows: &[Vec<String>]) -> String {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|text| {
                    if text.contains(['\t', '\r', '\n', '"']) {
                        format!("\"{}\"", text.replace('"', "\"\""))
                    } else {
                        text.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn encode_html(rows: &[Vec<String>], token: u64, session: &str) -> String {
    format!(
        "<table data-dnacalc-clipboard=\"{token}\" data-dnacalc-session=\"{}\">{}</table>",
        escape(session),
        rows.iter()
            .map(|row| format!(
                "<tr>{}</tr>",
                row.iter()
                    .map(|cell| format!("<td>{}</td>", escape(cell).replace('\n', "<br>")))
                    .collect::<String>()
            ))
            .collect::<String>()
    )
}
