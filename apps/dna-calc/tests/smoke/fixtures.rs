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
    let fonts = record_count(&styles,"fonts")?;
    let formats = record_count(&styles,"cellXfs")?;
    let added_fonts = r#"<font><sz val="14"/><name val="monospace"/><u val="double"/></font><font><sz val="14"/><name val="monospace"/></font><font><sz val="14"/><name val="monospace"/><u val="singleAccounting"/></font>"#;
    let added_formats = format!(r#"<xf fontId="{fonts}" applyFont="1"/><xf fontId="{}" applyFont="1" applyAlignment="1"><alignment horizontal="center" vertical="center" textRotation="255"/></xf><xf fontId="{}" applyFont="1"/><xf fontId="{}" applyFont="1" applyAlignment="1"><alignment textRotation="255" wrapText="1"/></xf>"#,fonts+1,fonts+2,fonts+1);
    styles=styles.replacen(&format!("<fonts count=\"{fonts}\""),&format!("<fonts count=\"{}\"",fonts+3),1).replacen("</fonts>",&format!("{added_fonts}</fonts>"),1)
        .replacen(&format!("<cellXfs count=\"{formats}\""),&format!("<cellXfs count=\"{}\"",formats+4),1).replacen("</cellXfs>",&format!("{added_formats}</cellXfs>"),1);
    let font_rows=format!(r#"<row r="20" ht="90" customHeight="1"><c r="B20" s="{formats}" t="inlineStr"><is><t>Double</t></is></c><c r="D20" s="{}" t="inlineStr"><is><t>Aé🧬</t></is></c><c r="F20" s="{}" t="inlineStr"><is><t>Accounting</t></is></c><c r="G20" s="{}" t="inlineStr"><is><t>AB</t></is></c></row><row r="21" ht="90" customHeight="1"><c r="D21" s="{}" t="inlineStr"><is><t>é👩‍🔬Z</t></is></c></row>"#,formats+1,formats+2,formats+3,formats+1);
    let font_rows=format!(r#"{font_rows}<row r="22" ht="90" customHeight="1"><c r="H22" s="{}" t="inlineStr"><is><t xml:space="preserve">A
B</t></is></c></row><row r="23" ht="90" customHeight="1"><c r="H23" s="{}" t="inlineStr"><is><t xml:space="preserve">A
B
</t></is></c></row>"#,formats+1,formats+1);
    sheet=sheet.replace("A1:N18","A1:N23").replacen("</sheetData>",&format!("{font_rows}</sheetData>"),1);
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
