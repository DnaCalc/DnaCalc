//! Legacy form controls as the grid draws them (dc-zjsg.8): the wire form (a control's kind, place, caption and the
//! state it shows now, worked out from its linked cell as Excel does), the markup of one control, and what a click on
//! one of its parts writes to the linked cell. The controls are drawn flat, in the look of Excel's form controls on
//! Windows 10 and later (assumed until Excel's render: sizes of the box, the circle, the arrows and the list rows).

use crate::projection::{ChartPlace, Key};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawnControl {
    pub id: u32,
    pub name: String,
    /// `Button`, `CheckBox`, `Radio`, `List`, `Drop`, `Scroll`, `Spin`, `GBox`, `Label` (others are drawn as a frame).
    pub kind: String,
    pub place: ChartPlace,
    pub caption: Option<String>,
    /// A check box's state: `Checked`, `Mixed` or `Unchecked`; a radio button's `Checked` or `Unchecked`.
    pub checked: String,
    /// A list or combo box's selected item (from 0), a scroll bar's or spinner's value.
    pub selected: Option<usize>,
    pub value: i64,
    pub min: i64,
    pub max: i64,
    pub increment: i64,
    pub page: i64,
    pub items: Vec<String>,
    pub horizontal: bool,
    /// The cell a click writes: the control's linked cell (a radio button's group's).
    pub link: Option<Key>,
    /// A radio button's place in its group, from 1: what it writes to the group's linked cell.
    pub position: usize,
    pub macro_name: Option<String>,
}

/// What a click on a control's part asks: the text to write to its linked cell, or a button's macro to run.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlAction {
    Write(Key, String),
    RunMacro(String),
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const FONT: &str = "font-family:'Segoe UI',Tahoma,sans-serif;color:#000;white-space:nowrap;overflow:hidden";
/// Excel 16's raised control face (dw1 Windows run, 2026-11-20): flat (240, 240, 240) with a black bottom and right
/// edge and a white top and left.
const FACE: &str = "background:#F0F0F0;border-style:solid;border-width:1px;border-color:#FFFFFF #000000 #000000 #FFFFFF";

/// The markup of `c` in a box `w` by `h` pixels at `zoom`. The parts a click acts on carry `data-act`: `toggle`, `item`
/// (with `data-index`), `dec`, `inc`, `select` (a combo box's list), `run`.
pub fn control_markup(c: &DrawnControl, w: f64, h: f64, zoom: f64) -> String {
    let font = 11.0 * zoom;
    let caption = escape(c.caption.as_deref().unwrap_or(""));
    let mut out = String::new();
    match c.kind.as_str() {
        "Button" => {
            let _ = write!(out, "<div data-act=\"run\" style=\"position:absolute;inset:0;box-sizing:border-box;{FACE};display:flex;align-items:center;justify-content:flex-start;padding:0 {}px;font-size:{font}px;{FONT};cursor:pointer\">{caption}</div>", 3.0 * zoom);
        }
        "CheckBox" | "Radio" => {
            let size = 13.0 * zoom;
            let radio = c.kind == "Radio";
            let mark = match (radio, c.checked.as_str()) {
                (false, "Checked") => format!("<svg width=\"{size}\" height=\"{size}\" viewBox=\"0 0 13 13\"><path d=\"M3 6.5L5.5 9L10 3.5\" fill=\"none\" stroke=\"#000\" stroke-width=\"1.6\"/></svg>"),
                (false, "Mixed") => format!("<svg width=\"{size}\" height=\"{size}\" viewBox=\"0 0 13 13\"><rect x=\"3\" y=\"3\" width=\"7\" height=\"7\" fill=\"#000\"/></svg>"),
                (true, "Checked") => format!("<svg width=\"{size}\" height=\"{size}\" viewBox=\"0 0 13 13\"><circle cx=\"6.5\" cy=\"6.5\" r=\"3\" fill=\"#000\"/></svg>"),
                _ => String::new(),
            };
            let radius = if radio { "50%" } else { "0" };
            let _ = write!(out, "<div data-act=\"toggle\" style=\"position:absolute;inset:0;display:flex;align-items:center;gap:{}px;font-size:{font}px;{FONT};cursor:pointer\"><span style=\"flex:none;width:{size}px;height:{size}px;box-sizing:border-box;border:1px solid #333;border-radius:{radius};background:#fff;display:flex\">{mark}</span><span>{caption}</span></div>", 4.0 * zoom);
        }
        "List" => {
            let row = 15.0 * zoom;
            let _ = write!(out, "<div style=\"position:absolute;inset:0;box-sizing:border-box;border:{}px solid #000;background:#fff;overflow:hidden;font-size:{font}px;{FONT}\">", (2.0 * zoom).round().max(1.0));
            for (i, item) in c.items.iter().enumerate() {
                let on = c.selected == Some(i);
                let _ = write!(out, "<div data-act=\"item\" data-index=\"{i}\" style=\"height:{row}px;line-height:{row}px;padding:0 {}px;{}cursor:pointer\">{}</div>", 2.0 * zoom, if on { "background:#0078D7;color:#fff;" } else { "" }, escape(item));
            }
            out.push_str("</div>");
        }
        "Drop" => {
            let text = c.selected.and_then(|i| c.items.get(i)).map(|t| escape(t)).unwrap_or_default();
            let arrow = (19.0 * zoom).min(w / 2.0);
            let _ = write!(out, "<div style=\"position:absolute;inset:0;box-sizing:border-box;border:1px solid #ADADAD;background:#fff;display:flex;align-items:center;font-size:{font}px;{FONT}\"><span style=\"flex:1;padding:0 {}px;overflow:hidden\">{text}</span><span data-act=\"select\" style=\"flex:none;width:{arrow}px;height:100%;box-sizing:border-box;{FACE};display:flex;align-items:center;justify-content:center;color:#000;cursor:pointer\">&#9660;</span></div>", 2.0 * zoom);
        }
        "Scroll" | "Spin" => {
            let horizontal = if c.kind == "Spin" { false } else { c.horizontal };
            let (less, more) = if horizontal { ("&#9664;", "&#9654;") } else { ("&#9650;", "&#9660;") };
            let button = if horizontal { h.min(w / 2.0) } else { w.min(h / 2.0) };
            let part = |act: &str, glyph: &str, style: &str| {
                format!("<div data-act=\"{act}\" style=\"position:absolute;{style};box-sizing:border-box;{FACE};display:flex;align-items:center;justify-content:center;font-size:{}px;color:#000;cursor:pointer\">{glyph}</div>", 8.0 * zoom)
            };
            let _ = write!(out, "<div style=\"position:absolute;inset:0;background:#F0F0F0\"></div>");
            // A spinner's up arrow raises its value; a scroll bar's first arrow lowers it.
            let (first, second) = if c.kind == "Spin" { ("inc", "dec") } else { ("dec", "inc") };
            if horizontal {
                out.push_str(&part(first, less, &format!("left:0;top:0;width:{button}px;height:100%")));
                out.push_str(&part(second, more, &format!("right:0;top:0;width:{button}px;height:100%")));
            } else {
                out.push_str(&part(first, less, &format!("left:0;top:0;width:100%;height:{button}px")));
                out.push_str(&part(second, more, &format!("left:0;bottom:0;width:100%;height:{button}px")));
            }
            if c.kind == "Scroll" {
                let span = (c.max - c.min).max(1) as f64;
                let at = ((c.value - c.min) as f64 / span).clamp(0.0, 1.0);
                let track = if horizontal { w - 2.0 * button } else { h - 2.0 * button };
                let thumb = (track / 6.0).max(8.0 * zoom).min(track.max(0.0));
                let offset = button + at * (track - thumb).max(0.0);
                let style = if horizontal {
                    format!("left:{offset}px;top:1px;width:{thumb}px;bottom:1px")
                } else {
                    format!("top:{offset}px;left:1px;height:{thumb}px;right:1px")
                };
                let _ = write!(out, "<div style=\"position:absolute;{style};background:#C2C3C9\"></div>");
            }
        }
        "GBox" => {
            let _ = write!(out, "<div style=\"position:absolute;left:0;right:0;bottom:0;top:{}px;border:1px solid #000\"></div><span style=\"position:absolute;left:{}px;top:0;padding:0 {}px;background:#fff;font-size:{font}px;{FONT}\">{caption}</span>", font / 2.0, 6.0 * zoom, 2.0 * zoom);
        }
        "Label" => {
            let _ = write!(out, "<div style=\"position:absolute;inset:0;font-size:{font}px;{FONT}\">{caption}</div>");
        }
        _ => {
            let _ = write!(out, "<div style=\"position:absolute;inset:0;box-sizing:border-box;border:1px solid #ADADAD;font-size:{font}px;{FONT}\">{caption}</div>");
        }
    }
    out
}

/// What a click on the part `act` (with `index` for a list's item) of `c` does, as Excel's control does it: a check box
/// writes TRUE or FALSE, a radio button its place in its group, a list its item's place (from 1), a scroll bar or
/// spinner its value moved by its increment (a scroll bar's track by its page), within its limits; a button runs its
/// macro. `None` when the click writes nothing (no linked cell, a frame or a label).
pub fn control_action(c: &DrawnControl, act: &str, index: Option<usize>) -> Option<ControlAction> {
    if act == "run" {
        return c.macro_name.clone().map(ControlAction::RunMacro);
    }
    let link = c.link?;
    let text = match (c.kind.as_str(), act) {
        ("CheckBox", "toggle") => if c.checked == "Checked" { "FALSE" } else { "TRUE" }.to_owned(),
        ("Radio", "toggle") => c.position.to_string(),
        ("List" | "Drop", "item") => (index? + 1).to_string(),
        ("Scroll" | "Spin", _) => {
            let step = match act {
                "inc" => c.increment,
                "dec" => -c.increment,
                "pageinc" => c.page,
                "pagedec" => -c.page,
                _ => return None,
            };
            (c.value + step).clamp(c.min.min(c.max), c.max.max(c.min)).to_string()
        }
        _ => return None,
    };
    Some(ControlAction::Write(link, text))
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    fn control(kind: &str) -> DrawnControl {
        DrawnControl {
            id: 1025,
            name: "Control".into(),
            kind: kind.into(),
            place: ChartPlace::Absolute { x: 0.0, y: 0.0, width: 100.0, height: 20.0 },
            caption: Some("A & B".into()),
            checked: "Unchecked".into(),
            selected: None,
            value: 0,
            min: 0,
            max: 100,
            increment: 1,
            page: 10,
            items: Vec::new(),
            horizontal: false,
            link: Some(Key(0, 1, 2, 2)),
            position: 2,
            macro_name: None,
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_8__clicks_write_the_linked_cell_as_excel_does() {
        let mut check = control("CheckBox");
        assert_eq!(control_action(&check, "toggle", None), Some(ControlAction::Write(Key(0, 1, 2, 2), "TRUE".into())));
        check.checked = "Checked".into();
        assert_eq!(control_action(&check, "toggle", None), Some(ControlAction::Write(Key(0, 1, 2, 2), "FALSE".into())));
        assert_eq!(control_action(&control("Radio"), "toggle", None), Some(ControlAction::Write(Key(0, 1, 2, 2), "2".into())));
        assert_eq!(control_action(&control("List"), "item", Some(2)), Some(ControlAction::Write(Key(0, 1, 2, 2), "3".into())));
        let mut scroll = control("Scroll");
        scroll.value = 95;
        assert_eq!(control_action(&scroll, "pageinc", None), Some(ControlAction::Write(Key(0, 1, 2, 2), "100".into())));
        scroll.value = 3;
        assert_eq!(control_action(&scroll, "dec", None), Some(ControlAction::Write(Key(0, 1, 2, 2), "2".into())));
        let mut unlinked = control("CheckBox");
        unlinked.link = None;
        assert_eq!(control_action(&unlinked, "toggle", None), None);
        let mut button = control("Button");
        button.macro_name = Some("[0]!Run".into());
        assert_eq!(control_action(&button, "run", None), Some(ControlAction::RunMacro("[0]!Run".into())));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_8__each_kind_draws_its_parts() {
        let mut check = control("CheckBox");
        check.checked = "Checked".into();
        let markup = control_markup(&check, 100.0, 20.0, 1.0);
        assert!(markup.contains("data-act=\"toggle\"") && markup.contains("<path") && markup.contains("A &amp; B"), "{markup}");
        let mut list = control("List");
        list.items = vec!["North".into(), "South".into()];
        list.selected = Some(1);
        let markup = control_markup(&list, 100.0, 60.0, 1.0);
        assert_eq!(markup.matches("data-act=\"item\"").count(), 2);
        assert!(markup.contains("data-index=\"1\" style=\"height:15px;line-height:15px;padding:0 2px;background:#0078D7"), "{markup}");
        let markup = control_markup(&control("Scroll"), 17.0, 100.0, 1.0);
        assert!(markup.contains("data-act=\"dec\"") && markup.contains("data-act=\"inc\""));
        assert!(control_markup(&control("GBox"), 100.0, 60.0, 1.0).contains("A &amp; B"));
    }
}
