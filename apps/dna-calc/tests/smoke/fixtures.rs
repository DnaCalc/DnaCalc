//! Synthetic documented OOXML variations of the retained features fixture, not Excel readings.
#[path = "../../../../crates/calc-engine/tests/fixtures/seven_cell.rs"]
mod seven_cell;

/// Authored paint inputs, sharing the existing package writer and fixture generator.
/// These contain no reference pixels or cached formula results.
pub(crate) fn paint(kind: &str) -> Result<Vec<u8>, String> {
    paint_with_normal(kind, "Calibri", 11.)
}
pub(crate) fn paint_with_normal(kind: &str, family: &str, points: f64) -> Result<Vec<u8>, String> {
    use std::collections::BTreeMap;
    if family.is_empty() || !points.is_finite() || !(1. ..=409.).contains(&points) {
        return Err("Normal font needs a family and 1 to 409 points".into());
    }
    let family = family
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");
    let mut fonts = vec![format!(
        r#"<font><sz val="{points}"/><name val="{family}"/><color rgb="FF000000"/></font>"#
    )];
    for (size, effect) in [
        (8, ""),
        (16, ""),
        (24, ""),
        (11, "<b/>"),
        (11, "<i/>"),
        (11, "<u/>"),
        (11, "<u val=\"double\"/>"),
        (11, "<strike/>"),
    ] {
        fonts.push(format!("<font><sz val=\"{size}\"/><name val=\"Calibri\"/><color rgb=\"FF000000\"/>{effect}</font>"));
    }
    let mut borders = vec!["<border/>".to_owned()];
    for style in [
        "thin",
        "medium",
        "thick",
        "dotted",
        "dashed",
        "double",
        "hair",
        "dashDot",
        "dashDotDot",
        "mediumDashed",
        "mediumDashDot",
        "mediumDashDotDot",
        "slantDashDot",
    ] {
        borders.push(format!(
            "<border>{}</border>",
            ["left", "right", "top", "bottom"]
                .iter()
                .map(|side| format!("<{side} style=\"{style}\"><color rgb=\"FF154C94\"/></{side}>"))
                .collect::<String>()
        ));
    }
    let xf = |font, fill, border, number, alignment: &str| {
        format!(
            "<xf fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\" numFmtId=\"{number}\" xfId=\"0\" applyAlignment=\"1\"><alignment {alignment}/></xf>"
        )
    };
    let mut formats = vec![xf(0, 0, 0, 0, "")];
    for font in 1..fonts.len() {
        formats.push(xf(font, 0, 0, 0, ""));
    }
    let alignment_first = formats.len();
    for alignment in [
        "horizontal=\"left\"",
        "horizontal=\"center\"",
        "horizontal=\"right\"",
        "vertical=\"top\"",
        "vertical=\"center\"",
        "wrapText=\"1\"",
        "indent=\"1\"",
        "textRotation=\"45\"",
        "textRotation=\"135\"",
        "shrinkToFit=\"1\"",
        "horizontal=\"distributed\"",
        "textRotation=\"255\"",
    ] {
        formats.push(xf(0, 0, 0, 0, alignment));
    }
    let number_first = formats.len();
    for number in [1, 2, 9, 10, 14, 20, 37, 49, 164] {
        formats.push(xf(0, 0, 0, number, ""));
    }
    let border_first = formats.len();
    for border in 1..borders.len() {
        formats.push(xf(0, 0, border, 0, ""));
    }
    let fill_first = formats.len();
    for fill in 2..5 {
        formats.push(xf(0, fill, 0, 0, ""));
    }
    let styles = format!(
        r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="164" formatCode="0.00;[Red](0.00);&quot;zero&quot;"/></numFmts><fonts count="{}">{}</fonts><fills count="5"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FFFFCC33"/></patternFill></fill><fill><patternFill patternType="lightGrid"><fgColor rgb="FF154C94"/><bgColor rgb="FFFFF0CC"/></patternFill></fill><fill><gradientFill degree="0"><stop position="0"><color rgb="FFFFFFFF"/></stop><stop position="1"><color rgb="FF154C94"/></stop></gradientFill></fill></fills><borders count="{}">{}</borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="{}">{}</cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles><dxfs count="1"><dxf><font><b/><color rgb="FF9C0006"/></font><fill><patternFill patternType="solid"><fgColor rgb="FFFFC7CE"/></patternFill></fill></dxf></dxfs></styleSheet>"#,
        fonts.len(),
        fonts.join(""),
        borders.len(),
        borders.join(""),
        formats.len(),
        formats.join("")
    );
    let text = |address: &str, style: usize, value: &str| {
        format!(
            "<c r=\"{address}\" s=\"{style}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{value}</t></is></c>"
        )
    };
    let number = |address: &str, style: usize, value: f64| {
        format!("<c r=\"{address}\" s=\"{style}\"><v>{value}</v></c>")
    };
    let mut rows = BTreeMap::<usize, (Option<f64>, String)>::new();
    let mut add = |row: usize, height: Option<f64>, value: String| {
        let entry = rows.entry(row).or_insert((height, String::new()));
        entry.1.push_str(&value);
    };
    let mut columns = String::new();
    let mut tail = String::new();
    let mut view = String::from(r#"<selection activeCell="J18" sqref="J18"/>"#);
    match kind {
        "geometry" => {
            for (index, width) in [0.5, 1., 2., 8.43, 8.7109375, 12., 20., 8.43, 8.43, 8.43]
                .iter()
                .enumerate()
            {
                columns.push_str(&format!(
                    "<col min=\"{0}\" max=\"{0}\" width=\"{width}\" customWidth=\"1\"/>",
                    index + 1
                ));
            }
            for row in 1..=18 {
                let height = match row {
                    3 => Some(10.),
                    5 => Some(15.75),
                    7 => Some(22.5),
                    9 => Some(30.),
                    11 => Some(42.),
                    _ => None,
                };
                for col in 3..=10 {
                    add(
                        row,
                        height,
                        text(
                            &format!("{}{row}", char::from(b'A' + col - 1)),
                            0,
                            "0123456789",
                        ),
                    );
                }
            }
        }
        "text" => {
            columns.push_str(r#"<col min="1" max="10" width="12" customWidth="1"/>"#);
            for i in 0..fonts.len() {
                add(
                    i + 1,
                    Some(if i == 3 { 32. } else { 23. }),
                    text(&format!("A{}", i + 1), i, "Ag 0123"),
                );
            }
            for i in 0..12 {
                add(
                    i + 1,
                    Some(if i == 3 { 32. } else { 23. }),
                    text(
                        &format!("D{}", i + 1),
                        alignment_first + i,
                        if i == 5 {
                            "wrap words wrap words"
                        } else if i == 9 {
                            "shrink this text to fit"
                        } else {
                            "Align Ag"
                        },
                    ),
                );
            }
            for i in 0..9 {
                add(
                    i + 1,
                    Some(if i == 3 { 32. } else { 23. }),
                    number(
                        &format!("G{}", i + 1),
                        number_first + i,
                        if i == 8 { -1234.567 } else { 45234.567 },
                    ),
                );
            }
            add(
                14,
                Some(25.),
                text("A14", 0, "Text overflowing into blank neighbours"),
            );
            add(
                15,
                Some(25.),
                text("A15", 0, "Text clipped by occupied neighbour"),
            );
            add(15, None, text("B15", 0, "STOP"));
            add(
                16,
                Some(25.),
                text("A16", 0, "Text clipped by filled neighbour"),
            );
            add(16, None, text("B16", fill_first, ""));
            add(17, Some(25.), number("G17", number_first + 4, 1e10));
        }
        "borders" => {
            columns.push_str(r#"<col min="1" max="10" width="11" customWidth="1"/>"#);
            for i in 0..13 {
                let row = i + 1;
                add(
                    row,
                    Some(23.),
                    text(&format!("B{row}"), border_first + i, "Ag border"),
                );
                add(
                    row,
                    None,
                    text(&format!("C{row}"), border_first + (i + 1) % 13, "join"),
                );
            }
            for i in 0..3 {
                add(
                    2 + i * 3,
                    Some(23.),
                    text(&format!("F{}", 2 + i * 3), fill_first + i, "Fill Ag"),
                );
            }
            for row in 14..=16 {
                add(
                    row,
                    Some(23.),
                    number(&format!("B{row}"), 0, (row - 14) as f64),
                );
            }
            tail.push_str(r#"<conditionalFormatting sqref="B14:B16"><cfRule type="cellIs" dxfId="0" priority="1" operator="greaterThan"><formula>0</formula></cfRule></conditionalFormatting>"#);
        }
        "panes" => {
            columns.push_str(r#"<col min="1" max="10" width="10" customWidth="1"/>"#);
            view = String::from(
                r#"<pane xSplit="2" ySplit="2" topLeftCell="C3" activePane="bottomRight" state="frozen"/><selection pane="bottomRight" activeCell="D4" sqref="D4:F6"/>"#,
            );
            for row in 1..=18 {
                for col in 1..=10 {
                    add(
                        row,
                        Some(20.),
                        text(
                            &format!("{}{row}", char::from(b'A' + col - 1)),
                            0,
                            &format!("R{row}C{col}"),
                        ),
                    );
                }
            }
        }
        "everyday" => {
            // Narrow columns for ####, merged blocks, wrapped and overflowing text, a range selection.
            for (index, width) in [10., 10., 4., 6., 14., 10., 10., 10., 10., 10.]
                .iter()
                .enumerate()
            {
                columns.push_str(&format!(
                    "<col min=\"{0}\" max=\"{0}\" width=\"{width}\" customWidth=\"1\"/>",
                    index + 1
                ));
            }
            add(
                1,
                None,
                text("A1", 0, "Numbers in narrow columns show ####"),
            );
            // C is 4 wide, D is 6 wide: a fixed-decimal number and a date no longer fit.
            add(2, None, number("C2", number_first + 1, 123456.789));
            add(2, None, number("D2", number_first + 4, 45234.567));
            add(2, None, number("E2", number_first + 1, 123456.789));
            add(3, None, number("C3", 0, 12345.678));
            add(3, None, number("D3", 0, 0.000012345));
            add(3, None, number("E3", number_first + 8, -1234.567));
            // Merged blocks: horizontal, vertical, and a numeric one.
            add(5, None, text("B5", alignment_first + 1, "Merged block"));
            add(5, None, text("F5", alignment_first + 4, "Tall"));
            add(5, None, number("H5", number_first + 1, 3.14159));
            add(7, None, text("B7", alignment_first + 1, "Below the merge"));
            // Wrapped text in a ten wide column, left to automatic row height.
            add(
                10,
                None,
                text(
                    "A10",
                    alignment_first + 5,
                    "wrap words wrap words wrap words",
                ),
            );
            add(
                10,
                None,
                text(
                    "C10",
                    alignment_first + 5,
                    "Tight wrapped text in a narrow column",
                ),
            );
            // Overflow: left text runs right, right text runs left, centred text both ways, blocked by a neighbour.
            add(
                12,
                None,
                text(
                    "A12",
                    0,
                    "Left aligned text that overflows into blank neighbours",
                ),
            );
            add(
                13,
                None,
                text(
                    "H13",
                    alignment_first + 2,
                    "Right aligned text that overflows to the left",
                ),
            );
            add(
                14,
                None,
                text(
                    "F14",
                    alignment_first + 1,
                    "Centred text overflowing on both sides",
                ),
            );
            add(15, None, text("A15", 0, "Blocked by the next cell"));
            add(15, None, text("C15", 0, "STOP"));
            add(16, None, number("A16", number_first + 1, 1234567.891));
            tail.push_str(r#"<mergeCells count="3"><mergeCell ref="B5:D6"/><mergeCell ref="F5:F8"/><mergeCell ref="H5:I5"/></mergeCells>"#);
            view = String::from(r#"<selection activeCell="C2" sqref="C2:E4"/>"#);
        }
        _ => return Err(format!("unknown paint input {kind}")),
    }
    let rows = rows
        .into_iter()
        .map(|(row, (height, cells))| {
            format!(
                "<row r=\"{row}\"{}>{cells}</row>",
                height.map_or(String::new(), |h| format!(" ht=\"{h}\" customHeight=\"1\""))
            )
        })
        .collect::<String>();
    let sheet=format!(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:J18"/><sheetViews><sheetView workbookViewId="0" zoomScale="100">{view}</sheetView></sheetViews><sheetFormatPr defaultRowHeight="15"/ ><cols>{columns}</cols><sheetData>{rows}</sheetData>{tail}</worksheet>"#).replace("/ >","/>");
    crate::oracle_cases::replace_package_parts(
        &calc_doc::blank_package(),
        &BTreeMap::from([
            ("xl/styles.xml".into(), styles.into_bytes()),
            ("xl/worksheets/sheet1.xml".into(), sheet.into_bytes()),
        ]),
    )
}

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
        .replace(
            "$P03_VBA_SOURCE",
            &serde_json::to_string(seven_cell::P03_VBA_SOURCE).expect("VBA source serializes"),
        )
        .replace(
            "$P03_RANGE_CASES",
            &serde_json::to_string(seven_cell::P03_RANGE_CASES).expect("P03 cases serialize"),
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
    let font_count = styles
        .split("<fonts count=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .ok_or("fonts")?
        .parse::<usize>()
        .map_err(|e| e.to_string())?;
    let styles = styles
        .replacen(&format!("<fonts count=\"{font_count}\""), &format!("<fonts count=\"{}\"",font_count+1),1)
        .replacen("</fonts>","<font><sz val=\"11\"/><name val=\"DnacalcReflow\"/></font></fonts>",1)
        .replacen(
            &format!("<cellXfs count=\"{count}\""),
            &format!("<cellXfs count=\"{}\"", count + 15),
            1,
        )
        .replacen("</cellXfs>", &format!("{extra}<xf numFmtId=\"0\" fontId=\"{font_count}\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/></cellXfs>"), 1);
    let mut sheet = include_str!("alignment/sheet.xml").to_owned();
    for i in (0..14).rev() {
        sheet = sheet.replace(&format!("${i}"), &(count + i).to_string());
    }
    // Extend the same retained alignment fixture with a real applied-font reflow witness and
    // a two-dimensional merge. These are authored inputs, not oracle results.
    sheet = sheet.replace("A1:J9","A1:N10")
        .replacen("</row>", "<c r=\"L1\" t=\"inlineStr\"><is><t>merged</t></is></c></row>",1)
        .replacen("</sheetData>",&format!("<row r=\"10\" ht=\"30\" customHeight=\"1\"><c r=\"B10\" s=\"{}\" t=\"inlineStr\"><is><t>iiiiiiiiiiiiiiiiiiiiiiii</t></is></c></row></sheetData><mergeCells count=\"1\"><mergeCell ref=\"L1:N2\"/></mergeCells>",count+14),1);
    // Physical fill and border witnesses extend this same synthetic fixture.
    let record_count = |xml: &str, name: &str| -> Result<usize, String> {
        xml.split(&format!("<{name} count=\""))
            .nth(1)
            .and_then(|s| s.split('"').next())
            .ok_or_else(|| format!("no {name}"))?
            .parse::<usize>()
            .map_err(|e| e.to_string())
    };
    let fill_count = record_count(&styles, "fills")?;
    let border_count = record_count(&styles, "borders")?;
    let font_count = record_count(&styles, "fonts")?;
    let patterns = [
        "darkGray",
        "mediumGray",
        "lightGray",
        "gray125",
        "gray0625",
        "darkHorizontal",
        "lightHorizontal",
        "darkVertical",
        "lightVertical",
        "darkDown",
        "lightDown",
        "darkUp",
        "lightUp",
        "darkGrid",
        "lightGrid",
        "darkTrellis",
        "lightTrellis",
    ];
    let mut fills=patterns.iter().map(|name|format!("<fill><patternFill patternType=\"{name}\"><fgColor rgb=\"FF144C94\"/><bgColor rgb=\"FFFFF0CC\"/></patternFill></fill>")).collect::<String>();
    fills.push_str(r#"<fill><gradientFill degree="90"><stop position="0"><color rgb="FFFF0000"/></stop><stop position="1"><color rgb="FF0000FF"/></stop></gradientFill></fill><fill><gradientFill type="path" left="0.2" right="0.8" top="0.3" bottom="0.7"><stop position="0"><color rgb="FFFFFFFF"/></stop><stop position="1"><color rgb="FF008000"/></stop></gradientFill></fill><fill><patternFill patternType="solid"><fgColor rgb="FFFFFFFF"/></patternFill></fill>"#);
    let borders = r#"<border diagonalUp="1"><diagonal style="thin"><color rgb="FFCC0000"/></diagonal></border><border diagonalUp="1" diagonalDown="1"><diagonal style="double"><color rgb="FF008000"/></diagonal></border><border><left style="thick"><color rgb="FFCC0000"/></left><right style="thick"><color rgb="FFCC0000"/></right><top style="thick"><color rgb="FFCC0000"/></top><bottom style="thick"><color rgb="FFCC0000"/></bottom></border><border><right style="thick"><color rgb="FF008000"/></right></border><border><bottom style="medium"><color rgb="FF0000CC"/></bottom></border><border><right style="thick"><color rgb="FF008000"/></right><bottom style="medium"><color rgb="FF0000CC"/></bottom></border><border><left style="thin"><color rgb="FFCC0000"/></left><bottom style="medium"><color rgb="FF0000CC"/></bottom></border>"#;
    let mut xfs = Vec::new();
    let xf = |font, fill, border| {
        format!(
            "<xf numFmtId=\"0\" fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\" xfId=\"0\" applyFill=\"1\" applyBorder=\"1\"/>"
        )
    };
    for i in 0..20 {
        xfs.push(xf(0, fill_count + i, 0));
    }
    xfs.push(xf(0, 0, border_count));
    xfs.push(xf(0, 0, border_count + 1));
    xfs.push(xf(font_count, fill_count + 19, border_count + 2));
    for i in 3..7 {
        xfs.push(xf(0, 0, border_count + i));
    }
    let extra_count = xfs.len();
    let first = count + 15;
    let mut styles=styles.replacen(&format!("<fills count=\"{fill_count}\""),&format!("<fills count=\"{}\"",fill_count+20),1).replacen("</fills>",&format!("{fills}</fills>"),1)
        .replacen(&format!("<borders count=\"{border_count}\""),&format!("<borders count=\"{}\"",border_count+7),1).replacen("</borders>",&format!("{borders}</borders>"),1)
        .replacen(&format!("<fonts count=\"{font_count}\""),&format!("<fonts count=\"{}\"",font_count+1),1).replacen("</fonts>","<font><sz val=\"16\"/><name val=\"monospace\"/><color rgb=\"FF000000\"/><b/></font></fonts>",1)
        .replacen(&format!("<cellXfs count=\"{}\"",count+15),&format!("<cellXfs count=\"{}\"",count+15+extra_count),1).replacen("</cellXfs>",&format!("{}</cellXfs>",xfs.join("")),1);
    let mut rows = String::new();
    for row in 12..=13 {
        rows.push_str(&format!("<row r=\"{row}\" ht=\"30\" customHeight=\"1\">"));
        for col in 0..9 {
            let i = (row - 12) * 9 + col;
            if i < patterns.len() {
                let letter = char::from(b'B' + col as u8);
                rows.push_str(&format!("<c r=\"{letter}{row}\" s=\"{}\"/>", first + i));
            }
        }
        rows.push_str("</row>");
    }
    rows.push_str(&format!("<row r=\"15\" ht=\"30\" customHeight=\"1\"><c r=\"B15\" s=\"{}\"/><c r=\"F15\" s=\"{}\"/></row><row r=\"16\" ht=\"30\" customHeight=\"1\"/><row r=\"18\" ht=\"30\" customHeight=\"1\"><c r=\"B18\" s=\"{}\" t=\"inlineStr\"><is><t>up</t></is></c><c r=\"D18\" s=\"{}\" t=\"inlineStr\"><is><t>double X</t></is></c></row>",first+17,first+18,first+20,first+21));
    sheet=sheet.replace("A1:N10","A1:N18").replacen("</sheetData>",&format!("{rows}</sheetData>"),1).replacen("<mergeCells count=\"1\"","<mergeCells count=\"4\"",1).replacen("</mergeCells>","<mergeCell ref=\"L5:N6\"/><mergeCell ref=\"B15:D16\"/><mergeCell ref=\"F15:H16\"/></mergeCells>",1);
    for (row, cells) in [
        (
            5,
            format!(
                "<c r=\"L5\" s=\"{}\" t=\"inlineStr\"><is><t>MERGED GLYPHS WWWWWWWWWWWWWW</t></is></c><c r=\"N5\" s=\"{}\"/>",
                first + 22,
                first + 23
            ),
        ),
        (
            6,
            format!(
                "<c r=\"L6\" s=\"{}\"/><c r=\"M6\" s=\"{}\"/><c r=\"N6\" s=\"{}\"/>",
                first + 26,
                first + 24,
                first + 25
            ),
        ),
    ] {
        let start = sheet
            .find(&format!("<row r=\"{row}\""))
            .ok_or("fixture row")?;
        let end = start + sheet[start..].find("</row>").ok_or("fixture row end")?;
        sheet.insert_str(end, &cells);
    }
    // Font/orientation witnesses extend this same synthetic workbook.
    let fonts = record_count(&styles, "fonts")?;
    let formats = record_count(&styles, "cellXfs")?;
    let added_fonts = r#"<font><sz val="14"/><name val="monospace"/><u val="double"/></font><font><sz val="14"/><name val="monospace"/></font><font><sz val="14"/><name val="monospace"/><u val="singleAccounting"/></font>"#;
    let added_formats = format!(
        r#"<xf fontId="{fonts}" applyFont="1"/><xf fontId="{}" applyFont="1" applyAlignment="1"><alignment horizontal="center" vertical="center" textRotation="255"/></xf><xf fontId="{}" applyFont="1"/><xf fontId="{}" applyFont="1" applyAlignment="1"><alignment textRotation="255" wrapText="1"/></xf>"#,
        fonts + 1,
        fonts + 2,
        fonts + 1
    );
    styles = styles
        .replacen(
            &format!("<fonts count=\"{fonts}\""),
            &format!("<fonts count=\"{}\"", fonts + 3),
            1,
        )
        .replacen("</fonts>", &format!("{added_fonts}</fonts>"), 1)
        .replacen(
            &format!("<cellXfs count=\"{formats}\""),
            &format!("<cellXfs count=\"{}\"", formats + 4),
            1,
        )
        .replacen("</cellXfs>", &format!("{added_formats}</cellXfs>"), 1);
    let font_rows = format!(
        r#"<row r="20" ht="90" customHeight="1"><c r="B20" s="{formats}" t="inlineStr"><is><t>Double</t></is></c><c r="D20" s="{}" t="inlineStr"><is><t>Aé🧬</t></is></c><c r="F20" s="{}" t="inlineStr"><is><t>Accounting</t></is></c><c r="G20" s="{}" t="inlineStr"><is><t>AB</t></is></c></row><row r="21" ht="90" customHeight="1"><c r="D21" s="{}" t="inlineStr"><is><t>é👩‍🔬Z</t></is></c></row>"#,
        formats + 1,
        formats + 2,
        formats + 3,
        formats + 1
    );
    let font_rows = format!(
        r#"{font_rows}<row r="22" ht="90" customHeight="1"><c r="H22" s="{}" t="inlineStr"><is><t xml:space="preserve">A
B</t></is></c></row><row r="23" ht="90" customHeight="1"><c r="H23" s="{}" t="inlineStr"><is><t xml:space="preserve">A
B
</t></is></c></row>"#,
        formats + 1,
        formats + 1
    );
    sheet = sheet.replace("A1:N18", "A1:N23").replacen(
        "</sheetData>",
        &format!("{font_rows}</sheetData>"),
        1,
    );
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

/// A standard validation variation; its values/formulas are synthetic inputs, not Excel answers.
pub(crate) fn sort_metadata(bytes: &[u8], variant: &str) -> Result<Vec<u8>, String> {
    let sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:C6"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Key</t></is></c><c r="B1" t="inlineStr"><is><t>Payload</t></is></c><c r="C1" t="inlineStr"><is><t>Total</t></is></c></row><row r="2"><c r="A2"><v>3</v></c><c r="B2" t="inlineStr"><is><t>three</t></is></c><c r="C2"><f>A2*2</f></c></row><row r="3"><c r="A3"><v>1</v></c><c r="B3" t="inlineStr"><is><t>one</t></is></c><c r="C3"><f>A3*2</f></c></row><row r="4"><c r="A4"><v>2</v></c><c r="B4" t="inlineStr"><is><t>two</t></is></c><c r="C4"><f>A4*2</f></c></row><row r="5"><c r="A5"><v>1</v></c><c r="B5" t="inlineStr"><is><t>one second</t></is></c><c r="C5"><f>A5*2</f></c></row></sheetData><dataValidations count="2"><dataValidation type="custom" showErrorMessage="1" showInputMessage="1" promptTitle="Row rule" prompt="Keep the row payload" errorTitle="Row payload" error="Keep the row payload" sqref="B2 B6"><formula1>AND($A2=3,B2="three")</formula1></dataValidation><dataValidation type="whole" allowBlank="1" sqref="C2:C5"><formula1>0</formula1><formula2>100</formula2></dataValidation></dataValidations></worksheet>"#;
    let mut replacements = std::collections::BTreeMap::new();
    let sheet = if matches!(variant, "sort-pointwise" | "sort-periods") {
        let parts = crate::oracle_cases::package_parts(bytes)?;
        let styles = std::str::from_utf8(parts.get("xl/styles.xml").ok_or("no styles")?)
            .map_err(|e| e.to_string())?;
        if !styles.contains("<dxfs count=\"0\"/>") {
            return Err("The retained sort fixture differential formats changed".into());
        }
        // dc-qgth: Excel reads a solid differential fill from bgColor, unlike a cell fill.
        // These are synthetic inputs; keep the existing colour and sorting assertions.
        replacements.insert("xl/styles.xml".into(),styles.replace("<dxfs count=\"0\"/>","<dxfs count=\"1\"><dxf><fill><patternFill patternType=\"solid\"><bgColor rgb=\"FF123456\"/></patternFill></fill></dxf></dxfs>").into_bytes());
        let start = sheet
            .find("<dataValidations")
            .ok_or("no validation block")?;
        let end = sheet
            .find("</dataValidations>")
            .ok_or("no validation end")?
            + "</dataValidations>".len();
        let mut sheet = sheet.to_owned();
        if variant == "sort-periods" {
            sheet.replace_range(start..end,r#"<conditionalFormatting sqref="C2:C6"><cfRule type="timePeriod" timePeriod="today" dxfId="0" priority="1" stopIfTrue="1"><formula>FLOOR(C2,1)=TODAY()</formula></cfRule></conditionalFormatting>"#);
            sheet.replace("<f>A2*2</f>", "<f>TODAY()</f>").replace(
                "</sheetData>",
                "<row r=\"6\"><c r=\"C6\"><f>TODAY()</f></c></row></sheetData>",
            )
        } else {
            sheet.replace_range(start..end,r#"<conditionalFormatting sqref="B2:B6"><cfRule type="containsText" operator="containsText" text="three" dxfId="0" priority="1" stopIfTrue="1"><formula>NOT(ISERROR(SEARCH("three",B2)))</formula></cfRule></conditionalFormatting><conditionalFormatting sqref="C2:C6"><cfRule type="containsErrors" dxfId="0" priority="2"><formula>ISERROR(C2)</formula></cfRule><cfRule type="containsBlanks" dxfId="0" priority="3"><formula>LEN(TRIM(C2))=0</formula></cfRule></conditionalFormatting>"#);
            sheet.replace("<f>A3*2</f>", "<f>1/0</f>")
        }
    } else if variant == "sort-rich" {
        sheet.replace("<t>three</t>", "<r><rPr><b/></rPr><t>three</t></r><rPh sb=\"0\" eb=\"5\"><t>reading</t></rPh><phoneticPr fontId=\"0\" type=\"Hiragana\"/>")
            .replace("<t>one</t>", "<r><rPr><i/></rPr><t>one</t></r>")
    } else if variant == "sort-links" {
        sheet.replace("</worksheet>",r#"<hyperlinks><hyperlink ref="B2" location="DV!$A$1" display="Three link" tooltip="Three row"/><hyperlink ref="B3:C4" location="DV!$D$1" display="Range link" tooltip="Range rows"/><hyperlink ref="B6" location="DV!$A$1" display="Outside" tooltip="Outside row"/></hyperlinks></worksheet>"#)
    } else {
        sheet.to_owned()
    };
    replacements.insert("xl/worksheets/sheet1.xml".into(), sheet.into_bytes());
    crate::oracle_cases::replace_package_parts(bytes, &replacements)
}
