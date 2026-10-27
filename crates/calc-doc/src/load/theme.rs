//! The workbook's theme colours (dc-6yw): the twelve colours of the theme part's colour scheme
//! (`a:clrScheme`), in the scheme's order (dk1, lt1, dk2, lt2, accent1 to accent6, hlink,
//! folHlink), each as `RRGGBB`: an `a:srgbClr`'s value, or an `a:sysClr`'s last colour. A colour
//! that names its theme colour by index (`theme="4"`) reads it from here (calc-host maps the
//! index, which counts lt1 and dk1 first, and applies the tint).

use super::{Events, LoadError, read_xml};
use crate::opc::Package;
use crate::xml::XmlEvent;

const DRAWING_NAMESPACE: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// The scheme's colours by their element names, in the scheme's order.
const SCHEME: [&str; 12] = [
    "dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6",
    "hlink", "folHlink",
];

/// The colours of the theme part `part`, in [`SCHEME`]'s order; a colour the part does not give
/// is empty.
pub(crate) fn read_theme(package: &Package, part: &str) -> Result<Vec<String>, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    let mut colors = vec![String::new(); SCHEME.len()];
    let mut current: Option<usize> = None;
    let mut in_scheme = false;
    let mut depth = 0u32;
    loop {
        match events.next()? {
            Some(XmlEvent::Start(element)) => {
                depth += 1;
                let local = element.name.local.as_str();
                let drawing = element.name.namespace.as_deref() == Some(DRAWING_NAMESPACE);
                if drawing && local == "clrScheme" {
                    in_scheme = true;
                } else if in_scheme
                    && drawing
                    && let Some(index) = SCHEME.iter().position(|n| *n == local)
                {
                    current = Some(index);
                } else if let (Some(index), true) = (current, drawing) {
                    let value = match local {
                        "srgbClr" => element.attribute("val"),
                        "sysClr" => element.attribute("lastClr"),
                        _ => None,
                    };
                    if let Some(value) = value
                        && colors[index].is_empty()
                    {
                        colors[index] = value.trim().to_ascii_uppercase();
                    }
                }
            }
            Some(XmlEvent::End(name)) => {
                depth = depth.saturating_sub(1);
                let drawing = name.namespace.as_deref() == Some(DRAWING_NAMESPACE);
                if drawing && name.local == "clrScheme" {
                    in_scheme = false;
                } else if drawing && SCHEME.contains(&name.local.as_str()) {
                    current = None;
                }
                if depth == 0 {
                    break;
                }
            }
            Some(XmlEvent::Text(_)) => {}
            None => break,
        }
    }
    Ok(colors)
}
