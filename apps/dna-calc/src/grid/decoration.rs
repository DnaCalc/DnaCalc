//! The two painters share the line weight and dash vocabulary of stored cell borders.
use dna_calc_model::projection::BorderShown;

pub struct Line {
    pub width: f64,
    pub css: &'static str,
    pub dash: &'static [f64],
    pub double: bool,
}
pub fn line(edge: &BorderShown, scale: f64) -> Line {
    let (width, css, dash, double) = match edge.style.as_str() {
        "hair" => (0.5, "solid", &[][..], false),
        "medium" => (2., "solid", &[][..], false),
        "thick" => (3., "solid", &[][..], false),
        "double" => (3., "double", &[][..], true),
        "dotted" => (1., "dotted", &[1., 1.][..], false),
        "dashed" => (1., "dashed", &[3., 2.][..], false),
        "mediumDashed" => (2., "dashed", &[3., 2.][..], false),
        "dashDot" | "slantDashDot" => (1., "dashed", &[3., 2., 1., 2.][..], false),
        "dashDotDot" => (1., "dashed", &[3., 2., 1., 2., 1., 2.][..], false),
        "mediumDashDot" => (2., "dashed", &[3., 2., 1., 2.][..], false),
        "mediumDashDotDot" => (2., "dashed", &[3., 2., 1., 2., 1., 2.][..], false),
        _ => (1., "solid", &[][..], false),
    };
    Line {
        width: width * scale,
        css,
        dash,
        double,
    }
}
pub fn css(borders: &[Option<BorderShown>; 4], scale: f64) -> String {
    let mut css = String::new();
    for (side, edge) in ["left", "right", "top", "bottom"].into_iter().zip(borders) {
        if let Some(edge) = edge {
            let line = line(edge, scale);
            css.push_str(&format!(
                ";border-{side}:{}px {} {}",
                line.width,
                line.css,
                edge.color.as_deref().unwrap_or("#000000")
            ));
        }
    }
    css
}
