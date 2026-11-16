//! dc-8ari6: the drawing part calc-doc saves keeps ECMA-376 (transitional, dml-spreadsheetDrawing.xsd
//! and dml-main.xsd) element order. Excel repairs a drawing part that breaks the content model, so
//! the saved part of every committed drawing fixture is checked here: each child of each element is
//! one the schema allows there and the children come in the schema's sequence. An element the
//! writer starts to emit that is not in the table below fails the test until its place is added.
//! (The order was also checked once against the XSDs with a validating parser: 0 errors.)

#![allow(non_snake_case)]

mod cdoc_m1_1_support;

use cdoc_m1_1_support::d1::*;
use quick_xml::events::Event;
use quick_xml::Reader;

const FIXTURES: [(&str, &[u8]); 4] = [
    ("shapes-paint", include_bytes!("fixtures/drawing/shapes-paint.xlsx")),
    ("shapes-paint2", include_bytes!("fixtures/drawing/shapes-paint2.xlsx")),
    ("shapes-paint3", include_bytes!("fixtures/drawing/shapes-paint3.xlsx")),
    ("pictures-paint", include_bytes!("fixtures/drawing/pictures-paint.xlsx")),
];

const COLOURS: [&str; 6] = ["srgbClr", "schemeClr", "scrgbClr", "sysClr", "hslClr", "prstClr"];
const FILLS: [&str; 6] = ["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];
const OBJECTS: [&str; 5] = ["sp", "grpSp", "graphicFrame", "cxnSp", "pic"];

/// The children a parent allows, in schema order: each slot is the names that may stand in that
/// position (a slot's names repeat or alternate freely, slots go forward only). `None` when the
/// element is not known.
fn content_model(parent: &str) -> Option<Vec<Vec<&'static str>>> {
    let one = |n: &'static str| vec![n];
    let colour = || COLOURS.to_vec();
    Some(match parent {
        "wsDr" => vec![vec!["twoCellAnchor", "oneCellAnchor", "absoluteAnchor"]],
        "twoCellAnchor" => vec![one("from"), one("to"), OBJECTS.to_vec(), one("clientData")],
        "oneCellAnchor" => vec![one("from"), one("ext"), OBJECTS.to_vec(), one("clientData")],
        "absoluteAnchor" => vec![one("pos"), one("ext"), OBJECTS.to_vec(), one("clientData")],
        "from" | "to" => vec![one("col"), one("colOff"), one("row"), one("rowOff")],
        "sp" => vec![one("nvSpPr"), one("spPr"), one("style"), one("txBody")],
        "cxnSp" => vec![one("nvCxnSpPr"), one("spPr"), one("style")],
        "pic" => vec![one("nvPicPr"), one("blipFill"), one("spPr"), one("style")],
        "grpSp" => vec![one("nvGrpSpPr"), one("grpSpPr"), OBJECTS.to_vec()],
        "nvSpPr" => vec![one("cNvPr"), one("cNvSpPr")],
        "nvCxnSpPr" => vec![one("cNvPr"), one("cNvCxnSpPr")],
        "nvPicPr" => vec![one("cNvPr"), one("cNvPicPr")],
        "nvGrpSpPr" => vec![one("cNvPr"), one("cNvGrpSpPr")],
        "cNvPr" => vec![one("hlinkClick"), one("hlinkHover"), one("extLst")],
        "cNvSpPr" => vec![one("spLocks")],
        "cNvCxnSpPr" => vec![one("cxnSpLocks"), one("stCxn"), one("endCxn")],
        "cNvPicPr" => vec![one("picLocks")],
        "cNvGrpSpPr" => vec![one("grpSpLocks")],
        "spPr" => vec![
            one("xfrm"),
            vec!["custGeom", "prstGeom"],
            FILLS.to_vec(),
            one("ln"),
            vec!["effectLst", "effectDag"],
            one("scene3d"),
            one("sp3d"),
            one("extLst"),
        ],
        "grpSpPr" => vec![one("xfrm"), FILLS.to_vec(), vec!["effectLst", "effectDag"], one("scene3d"), one("extLst")],
        "xfrm" => vec![one("off"), one("ext"), one("chOff"), one("chExt")],
        "prstGeom" => vec![one("avLst")],
        "avLst" => vec![one("gd")],
        "solidFill" => vec![colour()],
        "gradFill" => vec![one("gsLst"), vec!["lin", "path"], one("tileRect")],
        "gsLst" => vec![one("gs")],
        "gs" => vec![colour()],
        "path" => vec![one("fillToRect")],
        "ln" => vec![
            vec!["noFill", "solidFill", "gradFill", "pattFill"],
            vec!["prstDash", "custDash"],
            vec!["round", "bevel", "miter"],
            one("headEnd"),
            one("tailEnd"),
            one("extLst"),
        ],
        "effectLst" => vec![
            one("blur"),
            one("fillOverlay"),
            one("glow"),
            one("innerShdw"),
            one("outerShdw"),
            one("prstShdw"),
            one("reflection"),
            one("softEdge"),
        ],
        "outerShdw" => vec![colour()],
        "style" => vec![one("lnRef"), one("fillRef"), one("effectRef"), one("fontRef")],
        "lnRef" | "fillRef" | "effectRef" | "fontRef" => vec![colour()],
        "txBody" => vec![one("bodyPr"), one("lstStyle"), one("p")],
        "bodyPr" => vec![one("prstTxWarp"), vec!["noAutofit", "normAutofit", "spAutoFit"], one("scene3d"), one("sp3d"), one("flatTx"), one("extLst")],
        "lstStyle" => vec![],
        "p" => vec![one("pPr"), vec!["r", "br", "fld"], one("endParaRPr")],
        "pPr" => vec![one("lnSpc"), one("spcBef"), one("spcAft"), vec!["buClrTx", "buClr"], one("defRPr"), one("extLst")],
        "r" => vec![one("rPr"), one("t")],
        "rPr" | "endParaRPr" | "defRPr" => vec![one("ln"), FILLS.to_vec(), vec!["effectLst", "effectDag"], one("highlight"), one("latin"), one("ea"), one("cs"), one("sym")],
        "blipFill" => vec![one("blip"), one("srcRect"), vec!["tile", "stretch"]],
        "blip" => vec![one("extLst")],
        "stretch" => vec![one("fillRect")],
        "extLst" => vec![one("ext")],
        "ext" => vec![vec!["svgBlip"]],
        // leaves
        "clientData" | "col" | "colOff" | "row" | "rowOff" | "t" | "off" | "chOff" | "chExt" | "gd" | "lin"
        | "fillToRect" | "srcRect" | "fillRect" | "picLocks" | "noFill" | "prstDash" | "headEnd" | "tailEnd"
        | "stCxn" | "endCxn" | "pos" | "svgBlip" => vec![],
        c if COLOURS.contains(&c) => vec![vec![
            "alpha", "alphaMod", "alphaOff", "hue", "hueMod", "hueOff", "sat", "satMod", "satOff", "lum", "lumMod", "lumOff", "red",
            "redMod", "redOff", "green", "greenMod", "greenOff", "blue", "blueMod", "blueOff", "shade", "tint", "comp", "inv", "gray", "gamma", "invGamma",
        ]],
        // a colour modifier: a leaf
        "alpha" | "alphaMod" | "alphaOff" | "hue" | "hueMod" | "hueOff" | "sat" | "satMod" | "satOff" | "lum" | "lumMod" | "lumOff"
        | "red" | "redMod" | "redOff" | "green" | "greenMod" | "greenOff" | "blue" | "blueMod" | "blueOff" | "shade" | "tint" | "comp"
        | "inv" | "gray" | "gamma" | "invGamma" => vec![],
        _ => return None,
    })
}

fn local(name: &[u8]) -> String {
    let text = String::from_utf8_lossy(name).into_owned();
    text.rsplit(':').next().unwrap_or("").to_owned()
}

/// Walks `xml` and returns each place where an element has a child outside its content model or out of order.
fn order_errors(xml: &[u8]) -> Vec<String> {
    let mut reader = Reader::from_reader(xml);
    let mut stack: Vec<(String, usize)> = Vec::new(); // element name, the slot of its last child
    let mut errors = Vec::new();
    let mut buffer = Vec::new();
    let count = |name: String, stack: &mut Vec<(String, usize)>, errors: &mut Vec<String>| {
        if let Some((parent, last)) = stack.last_mut() {
            match content_model(parent) {
                None => errors.push(format!("{parent} is not in the content-model table")),
                Some(slots) => match slots.iter().position(|slot| slot.contains(&name.as_str())) {
                    None => errors.push(format!("{name} may not be a child of {parent}")),
                    Some(position) if position < *last => errors.push(format!("{name} comes after a later sibling in {parent}")),
                    Some(position) => *last = position,
                },
            }
        }
        if content_model(&name).is_none() {
            errors.push(format!("{name} is not in the content-model table"));
        }
    };
    loop {
        match reader.read_event_into(&mut buffer).expect("the drawing part is well-formed") {
            Event::Start(e) => {
                let name = local(e.name().as_ref());
                count(name.clone(), &mut stack, &mut errors);
                stack.push((name, 0));
            }
            Event::Empty(e) => {
                let name = local(e.name().as_ref());
                count(name.clone(), &mut stack, &mut errors);
            }
            Event::End(_) => {
                stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    errors.sort();
    errors.dedup();
    errors
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8ari6__saved_drawing_parts_keep_schema_element_order() {
    for (name, package) in FIXTURES {
        let part = part(package, "xl/drawings/dnaDrawing.xml")
            .or_else(|| part(package, "xl/drawings/drawing1.xml"))
            .unwrap_or_else(|| panic!("{name}: no drawing part"));
        let errors = order_errors(&part);
        assert!(errors.is_empty(), "{name}: {errors:#?}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8ari6__the_order_check_catches_a_misordered_part() {
    // ln children out of order (tailEnd before solidFill) and an unknown element in spPr.
    let bad = br#"<xdr:wsDr xmlns:xdr="x" xmlns:a="y"><xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>
<xdr:to><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
<xdr:sp><xdr:nvSpPr><xdr:cNvPr id="2" name="x"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:ln><a:tailEnd type="oval"/><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln><a:frob/></xdr:spPr></xdr:sp><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"#;
    let errors = order_errors(bad);
    assert!(errors.iter().any(|e| e.contains("solidFill comes after a later sibling in ln")), "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("frob")), "{errors:?}");
}

/// Every `a:prstGeom prst="hexagon"` avLst in a drawing part, as the guide names it lists.
fn hexagon_guides(xml: &str) -> Vec<Vec<String>> {
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("prst=\"hexagon\"") {
        rest = &rest[at..];
        let end = rest.find("</a:prstGeom>").unwrap_or(rest.len());
        let geometry = &rest[..end];
        let names = geometry.split("<a:gd name=\"").skip(1).map(|s| s.split('"').next().unwrap().to_owned()).collect();
        found.push(names);
        rest = &rest[end..];
    }
    found
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_8ari6__a_hexagon_with_a_guide_list_names_adj_and_vf() {
    // Excel 20527 repairs a hexagon whose avLst has `adj` without `vf` (the bisect of shapes-paint2/3: that one
    // anchor alone failed). It writes both; so does the saved part, whatever the model holds.
    let mut saw = 0;
    for (name, package) in FIXTURES {
        let part = part(package, "xl/drawings/dnaDrawing.xml").or_else(|| part(package, "xl/drawings/drawing1.xml")).unwrap();
        for names in hexagon_guides(std::str::from_utf8(&part).unwrap()) {
            saw += 1;
            assert!(names.is_empty() || (names.contains(&"adj".to_owned()) && names.contains(&"vf".to_owned())), "{name}: {names:?}");
        }
    }
    assert!(saw >= 3, "{saw}");
}
