//! Preserve validation source attributes while writing current ranges and formulas.
use super::{
    SaveRefusal,
    reader::Reader,
    text::{RawTag, Splice, a1, attribute_value, xstring},
};
use crate::{
    load::MAIN_NAMESPACE,
    xml::{XmlEvent, XmlLimits, XmlPart},
};
use calc_doc_model::Validation;
const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";

pub(crate) fn sources(xml: &XmlPart) -> Result<Vec<std::sync::Arc<[u8]>>, SaveRefusal> {
    super::reader::element_sources(xml,|e|e.name.local=="dataValidation" && matches!(e.name.namespace.as_deref(),Some(MAIN_NAMESPACE|X14))).map(|v|v.into_iter().map(Into::into).collect())
}
fn sqref(rule: &Validation) -> String {
    rule.areas
        .iter()
        .map(|&(t, l, b, r)| {
            if (t, l) == (b, r) {
                a1(t, l)
            } else {
                format!("{}:{}", a1(t, l), a1(b, r))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn render(rule: &Validation) -> Result<(bool, Vec<u8>), SaveRefusal> {
    let areas = sqref(rule);
    let Some(source) = &rule.source else {
        let mut bytes = format!("<dataValidation xmlns=\"{MAIN_NAMESPACE}\"").into_bytes();
        for (name, value) in [
            ("type", Some(rule.kind.as_str())),
            ("operator", Some(rule.operator.as_str())),
            ("sqref", Some(areas.as_str())),
            ("allowBlank", Some(if rule.allow_blank { "1" } else { "0" })),
            (
                "showInputMessage",
                Some(if rule.show_input { "1" } else { "0" }),
            ),
            (
                "showErrorMessage",
                Some(if rule.show_error { "1" } else { "0" }),
            ),
            (
                "showDropDown",
                Some(if rule.in_cell_dropdown { "0" } else { "1" }),
            ),
            ("errorStyle", Some(rule.error_style.as_str())),
            ("errorTitle", rule.error_title.as_deref()),
            ("error", rule.error.as_deref()),
            ("promptTitle", rule.prompt_title.as_deref()),
            ("prompt", rule.prompt.as_deref()),
        ] {
            if let Some(value) = value {
                bytes.extend_from_slice(format!(" {name}=").as_bytes());
                if matches!(name,"errorTitle"|"error"|"promptTitle"|"prompt"){super::text::xstring_attribute(&value.encode_utf16().collect::<Vec<_>>(),&mut bytes);}else{attribute_value(value, &mut bytes);}
            }
        }
        bytes.push(b'>');
        for (name, formula) in [("formula1", &rule.formula1), ("formula2", &rule.formula2)] {
            if let Some(f) = formula {
                bytes.extend_from_slice(format!("<{name}>").as_bytes());
                xstring(&f.encode_utf16().collect::<Vec<_>>(), &mut bytes);
                bytes.extend_from_slice(format!("</{name}>").as_bytes());
            }
        }
        bytes.extend_from_slice(b"</dataValidation>");
        return Ok((false, bytes));
    };
    let xml = XmlPart::new(
        "validation-source.xml".into(),
        if rule.authored_copy{super::reader::without_revision_ids(&XmlPart::new("validation-copy.xml".into(),source.to_vec(),XmlLimits::default()))?}else{source.to_vec()},
        XmlLimits::default(),
    );
    let mut reader = Reader::new(&xml);
    let mut splice = Splice::new(xml.bytes());
    let mut x14 = false;
    let mut slot = 0;
    while let Some((event, span)) = reader.next()? {
        if let XmlEvent::Start(e) = event {
            if e.name.is(MAIN_NAMESPACE, "dataValidation") || e.name.is(X14, "dataValidation") {
                x14 = e.name.is(X14, "dataValidation");
                if !x14 {
                    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                    splice.replace(
                        span,
                        &tag.rebuilt(&[("sqref", Some(&areas))], tag.is_empty()),
                    );
                }
            } else if (e.name.is(MAIN_NAMESPACE,"formula1") || e.name.is(MAIN_NAMESPACE,"formula2")) && !x14
                || (e.name.is(X14,"formula1") || e.name.is(X14,"formula2")) && x14 {
                slot = if e.name.local == "formula1" { 1 } else { 2 };
                if !x14 {
                    let end = reader.element_end()?;
                    let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                    let prefix = String::from_utf8_lossy(tag.prefix());
                    let mut text = tag.rebuilt(&[], false);
                    let f = if slot == 1 {
                        &rule.formula1
                    } else {
                        &rule.formula2
                    };
                    if let Some(f) = f {
                        xstring(&f.encode_utf16().collect::<Vec<_>>(), &mut text);
                    }
                    text.extend_from_slice(format!("</{prefix}{}>", e.name.local).as_bytes());
                    splice.replace((span.0, end), &text);
                }
            } else if x14 && (e.name.is(XM, "f") || e.name.is(XM, "sqref")) {
                let end = reader.element_end()?;
                let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                let prefix = String::from_utf8_lossy(tag.prefix());
                let mut text = tag.rebuilt(&[], false);
                let f = if e.name.local == "sqref" {
                    Some(areas.as_str())
                } else if slot == 1 {
                    rule.formula1.as_deref()
                } else {
                    rule.formula2.as_deref()
                };
                if let Some(f) = f {
                    xstring(&f.encode_utf16().collect::<Vec<_>>(), &mut text);
                }
                text.extend_from_slice(format!("</{prefix}{}>", e.name.local).as_bytes());
                splice.replace((span.0, end), &text);
            } else if !matches!(e.name.namespace.as_deref(), Some(MAIN_NAMESPACE | X14 | XM)) {
                reader.element_end()?;
            }
        }
    }
    Ok((x14, splice.finish()))
}
pub(super) fn rewrite(xml: &XmlPart, rules: &[Validation]) -> Result<Vec<u8>, SaveRefusal> {
    let mut main = Vec::new();
    let mut extended = Vec::new();
    let (mut mc, mut xc) = (0, 0);
    for rule in rules {
        let (x, bytes) = render(rule)?;
        if x {
            xc += 1;
            extended.extend(bytes);
        } else {
            mc += 1;
            main.extend(bytes);
        }
    }
    let main = if mc == 0 {
        Vec::new()
    } else {
        [
            format!("<dataValidations xmlns=\"{MAIN_NAMESPACE}\" count=\"{mc}\">").into_bytes(),
            main,
            b"</dataValidations>".to_vec(),
        ]
        .concat()
    };
    let extended = if xc == 0 {
        Vec::new()
    } else {
        [
            format!("<x14:dataValidations xmlns:x14=\"{X14}\" count=\"{xc}\">").into_bytes(),
            extended,
            b"</x14:dataValidations>".to_vec(),
        ]
        .concat()
    };
    let extension = [
        format!(
            "<ext xmlns=\"{MAIN_NAMESPACE}\" uri=\"{{CCE6A557-97BC-4B89-ADB6-D9C93CAAB3DF}}\">"
        )
        .into_bytes(),
        extended.clone(),
        b"</ext>".to_vec(),
    ]
    .concat();
    let mut reader = Reader::new(xml);
    let mut splice = Splice::new(xml.bytes());
    let (mut mi, mut xi) = (false, false);
    let mut ext_close = None;
    let mut sheet_close = None;
    while let Some((event, span)) = reader.next()? {
        match event {
            XmlEvent::Start(e)
                if e.name.is(MAIN_NAMESPACE, "dataValidations")
                    || e.name.is(X14, "dataValidations") =>
            {
                let is_main = e.name.is(MAIN_NAMESPACE, "dataValidations");
                let done = if is_main { &mut mi } else { &mut xi };
                let tag = RawTag::parse(&xml.bytes()[span.0..span.1]);
                let count = if is_main { mc } else { xc }.to_string();
                let mut extra = Vec::new();
                let end = loop {
                    match reader.expect()? {
                        (XmlEvent::Start(child), start) => {
                            let end = reader.element_end()?;
                            if !child
                                .name
                                .is(if is_main { MAIN_NAMESPACE } else { X14 }, "dataValidation")
                            {
                                extra.extend_from_slice(&xml.bytes()[start.0..end]);
                            }
                        }
                        (XmlEvent::End(_), end) => break end.1,
                        _ => {}
                    }
                };
                let mut replacement = Vec::new();
                if !*done && (count != "0" || !extra.is_empty()) {
                    replacement.extend(tag.rebuilt(&[("count", Some(&count))], false));
                    for rule in rules {
                        let (x, bytes) = render(rule)?;
                        if x != is_main {
                            replacement.extend(bytes);
                        }
                    }
                    replacement.extend(extra);
                    replacement.extend(
                        format!(
                            "</{}dataValidations>",
                            String::from_utf8_lossy(tag.prefix())
                        )
                        .as_bytes(),
                    );
                }
                *done = true;
                splice.replace((span.0, end), &replacement);
            }
            XmlEvent::Start(e)
                if !mi
                    && e.name.namespace.as_deref() == Some(MAIN_NAMESPACE)
                    && matches!(
                        e.name.local.as_ref(),
                        "hyperlinks"
                            | "printOptions"
                            | "pageMargins"
                            | "pageSetup"
                            | "headerFooter"
                            | "rowBreaks"
                            | "colBreaks"
                            | "customProperties"
                            | "cellWatches"
                            | "ignoredErrors"
                            | "smartTags"
                            | "drawing"
                            | "legacyDrawing"
                            | "legacyDrawingHF"
                            | "picture"
                            | "oleObjects"
                            | "controls"
                            | "webPublishItems"
                            | "tableParts"
                            | "extLst"
                    ) =>
            {
                splice.replace((span.0, span.0), &main);
                mi = true;
            }
            XmlEvent::End(e) if e.is(MAIN_NAMESPACE, "extLst") => ext_close = Some(span.0),
            XmlEvent::End(e) if e.is(MAIN_NAMESPACE, "worksheet") => sheet_close = Some(span.0),
            _ => {}
        }
    }
    if let Some(close) = sheet_close {
        // Appends must follow byte order for Splice.
        let mut tail = Vec::new();
        if !mi {
            tail.extend(main);
        }
        if !xi && xc > 0 {
            if let Some(ext) = ext_close {
                splice.replace((ext, ext), &extension);
            } else {
                tail.extend(format!("<extLst xmlns=\"{MAIN_NAMESPACE}\">").as_bytes());
                tail.extend(extension);
                tail.extend(b"</extLst>");
            }
        }
        if !tail.is_empty() {
            splice.replace((close, close), &tail);
        }
    }
    Ok(splice.finish())
}

pub fn copied_validation_source(rule:&Validation)->Result<Validation,String>{let mut rule=rule.clone();rule.authored_copy=true;Ok(rule)}
