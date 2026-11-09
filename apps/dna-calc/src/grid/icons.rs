//! The icons of Excel's icon sets as small inline SVG drawings (dc-6yw), by the set's name and the
//! icon's index (0 is the lowest band's). The shapes and colours follow Excel's (arrows, traffic
//! lights, flags, signs, symbols, stars, triangles, ratings, quarters and boxes); they are drawn
//! here, not copied from Excel's images. The markup holds only fixed text and numbers.

const RED: &str = "#d9534f";
const YELLOW: &str = "#f0ad4e";
const GREEN: &str = "#5cb85c";
const GREY: &str = "#8c8c8c";
const BLACK: &str = "#404040";

fn svg(body: &str) -> String {
    format!(
        "<svg width=\"14\" height=\"14\" viewBox=\"0 0 16 16\" aria-hidden=\"true\">{body}</svg>"
    )
}

/// An arrow pointing at `angle` degrees (0 right, 90 up), filled with `color`.
fn arrow(angle: i32, color: &str) -> String {
    svg(&format!(
        "<g transform=\"rotate({} 8 8)\"><path d=\"M2 6.5h7V3l5 5-5 5V9.5H2z\" fill=\"{color}\"/></g>",
        -angle
    ))
}

fn circle(color: &str) -> String {
    svg(&format!(
        "<circle cx=\"8\" cy=\"8\" r=\"6\" fill=\"{color}\" stroke=\"#555\" stroke-width=\"0.6\"/>"
    ))
}

fn flag(color: &str) -> String {
    svg(&format!(
        "<path d=\"M3 2v13\" stroke=\"#555\" stroke-width=\"1.4\"/><path d=\"M3.7 2.5h9l-2.5 3 2.5 3h-9z\" fill=\"{color}\"/>"
    ))
}

fn bars(filled: usize, of: usize) -> String {
    let mut body = String::new();
    for i in 0..of {
        let h = 4 + i * 10 / of;
        let x = 2 + i * 12 / of;
        let color = if i < filled { "#2f6fb0" } else { "#c8d3e0" };
        body.push_str(&format!(
            "<rect x=\"{x}\" y=\"{}\" width=\"{}\" height=\"{h}\" fill=\"{color}\"/>",
            14 - h,
            (12 / of).max(2) - 1
        ));
    }
    svg(&body)
}

fn quarters(filled: usize) -> String {
    let path = match filled {
        0 => String::new(),
        1 => "<path d=\"M8 8V2a6 6 0 0 1 6 6z\" fill=\"#404040\"/>".to_owned(),
        2 => "<path d=\"M8 2a6 6 0 0 1 0 12z\" fill=\"#404040\"/>".to_owned(),
        3 => "<path d=\"M8 2a6 6 0 1 1-6 6h6z\" fill=\"#404040\"/>".to_owned(),
        _ => "<circle cx=\"8\" cy=\"8\" r=\"6\" fill=\"#404040\"/>".to_owned(),
    };
    svg(&format!(
        "<circle cx=\"8\" cy=\"8\" r=\"6\" fill=\"#fff\" stroke=\"#404040\" stroke-width=\"1\"/>{path}"
    ))
}

fn star(fill: f64) -> String {
    let width = (14.0 * fill).round();
    svg(&format!(
        "<defs><clipPath id=\"s\"><rect width=\"{width}\" height=\"16\" x=\"1\"/></clipPath></defs>\
        <path d=\"M8 1.5l2 4.2 4.6.6-3.3 3.2.8 4.6L8 11.9 3.9 14.1l.8-4.6L1.4 6.3 6 5.7z\" fill=\"#fff\" stroke=\"#e0a800\"/>\
        <path clip-path=\"url(#s)\" d=\"M8 1.5l2 4.2 4.6.6-3.3 3.2.8 4.6L8 11.9 3.9 14.1l.8-4.6L1.4 6.3 6 5.7z\" fill=\"#ffc000\"/>"
    ))
}

fn symbol(kind: usize, circled: bool) -> String {
    let (color, glyph) = match kind {
        0 => (
            RED,
            "<path d=\"M5.5 5.5l5 5m0-5l-5 5\" stroke=\"#fff\" stroke-width=\"1.8\"/>",
        ),
        1 => (
            YELLOW,
            "<path d=\"M8 4.5v4.5m0 1.8v1\" stroke=\"#fff\" stroke-width=\"1.8\"/>",
        ),
        _ => (
            GREEN,
            "<path d=\"M4.8 8.2l2.2 2.2 4.2-4.6\" stroke=\"#fff\" stroke-width=\"1.8\" fill=\"none\"/>",
        ),
    };
    if circled {
        svg(&format!(
            "<circle cx=\"8\" cy=\"8\" r=\"6.5\" fill=\"{color}\"/>{glyph}"
        ))
    } else {
        let glyph = glyph.replace("#fff", color);
        svg(&glyph)
    }
}

/// The icon `index` of the set `set`, as SVG markup; a set not drawn here gives a grey circle.
pub fn icon_svg(set: &str, index: usize) -> String {
    let count: usize = set.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(3) as usize;
    let top = count.saturating_sub(1).max(1);
    let name = &set[1..];
    match name {
        "Arrows" | "ArrowsGray" => {
            let angle = 90 - (180 * index / top) as i32;
            let angle = match count {
                3 => [-90, 0, 90][index.min(2)],
                4 => [-90, -45, 45, 90][index.min(3)],
                _ => angle,
            };
            let color = if name == "ArrowsGray" {
                GREY
            } else {
                match (index, count) {
                    (0, _) => RED,
                    (i, n) if i + 1 == n => GREEN,
                    _ => YELLOW,
                }
            };
            arrow(angle, color)
        }
        "TrafficLights1" | "TrafficLights2" | "Signs" | "TrafficLights" | "RedToBlack" => {
            let color = match (name, count, index) {
                ("RedToBlack", _, i) => ["#404040", "#808080", "#d9a0a0", RED][i.min(3)],
                (_, 4, 0) => BLACK,
                (_, 4, i) => [BLACK, RED, YELLOW, GREEN][i.min(3)],
                (_, _, i) => [RED, YELLOW, GREEN][i.min(2)],
            };
            circle(color)
        }
        "Flags" => flag([RED, YELLOW, GREEN][index.min(2)]),
        "Symbols" => symbol(index, true),
        "Symbols2" => symbol(index, false),
        "Stars" => star(index as f64 / 2.0),
        "Triangles" => match index {
            0 => arrow(-90, RED),
            1 => svg(&format!(
                "<rect x=\"3\" y=\"6.5\" width=\"10\" height=\"3\" fill=\"{YELLOW}\"/>"
            )),
            _ => arrow(90, GREEN),
        },
        "Rating" | "Boxes" => bars(index + if count == 5 { 0 } else { 1 }, count.max(4) - 1),
        "Quarters" => quarters(index),
        _ => circle(GREY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn every_set_draws_each_icon() {
        for set in [
            "3Arrows",
            "3ArrowsGray",
            "3Flags",
            "3TrafficLights1",
            "3TrafficLights2",
            "3Signs",
            "3Symbols",
            "3Symbols2",
            "3Stars",
            "3Triangles",
            "4Arrows",
            "4ArrowsGray",
            "4RedToBlack",
            "4Rating",
            "4TrafficLights",
            "5Arrows",
            "5ArrowsGray",
            "5Rating",
            "5Quarters",
            "5Boxes",
        ] {
            let count: usize = set[..1].parse().unwrap();
            for index in 0..count {
                let markup = icon_svg(set, index);
                assert!(markup.starts_with("<svg "), "{set} {index}");
            }
        }
        assert!(icon_svg("3Arrows", 2).contains(GREEN));
        assert!(icon_svg("3Arrows", 0).contains(RED));
    }
}
