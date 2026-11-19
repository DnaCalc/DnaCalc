//! An equation's OMML (Office Math Markup, ECMA-376 Part 1 22.1) as MathML Core the page lays out (dc-zjsg.11). The
//! common subset converts: runs, fractions, scripts, radicals, n-ary operators, delimiters, matrices, functions,
//! accents, bars, limits, group characters and equation arrays. Any other element leaves the whole equation
//! unconverted (`None`), and the page shows Excel's fallback picture instead: an equation is never drawn half-done.

use calc_host::DrawingNode;
use std::fmt::Write as _;

/// The equation's MathML: one `math` element per `m:oMath`, each in a block of its own (a paragraph of several draws
/// them one under another), the paragraph's justification, and the size in points of its first run (`a:rPr sz`),
/// when stated. `None` when anything is outside the subset.
pub(crate) struct Converted {
    pub markup: String,
    pub align: &'static str,
    pub size: Option<f64>,
}

pub(crate) fn convert(math: &DrawingNode) -> Option<Converted> {
    let (maths, align) = match math.local.as_str() {
        "oMathPara" => {
            let jc = math.child("oMathParaPr").and_then(|p| p.child("jc")).and_then(|j| j.attribute("m:val"));
            let align = match jc {
                Some("left") => "left",
                Some("right") => "right",
                _ => "center",
            };
            (math.children.iter().filter(|c| c.local == "oMath").collect::<Vec<_>>(), align)
        }
        "oMath" => (vec![math], "center"),
        _ => return None,
    };
    if maths.is_empty() || math.children.iter().any(|c| !matches!(c.local.as_str(), "oMath" | "oMathParaPr")) && math.local == "oMathPara" {
        return None;
    }
    let mut markup = String::new();
    for m in maths {
        let _ = write!(markup, "<div><math>{}</math></div>", row(&m.children)?);
    }
    Some(Converted { markup, align, size: first_size(math) })
}

/// The `sz` (hundredths of a point) of the first run's `a:rPr`, in points.
fn first_size(node: &DrawingNode) -> Option<f64> {
    if node.local == "rPr"
        && let Some(sz) = node.attribute("sz").and_then(|v| v.parse::<f64>().ok())
    {
        return Some(sz / 100.0);
    }
    node.children.iter().find_map(first_size)
}

/// Elements that only carry properties: skipped where they appear among arguments.
fn properties(local: &str) -> bool {
    local.ends_with("Pr") || local == "rPr"
}

/// A sequence of OMML objects as an `mrow`.
fn row(children: &[DrawingNode]) -> Option<String> {
    let mut out = String::from("<mrow>");
    for child in children.iter().filter(|c| !properties(&c.local)) {
        out.push_str(&object(child)?);
    }
    out.push_str("</mrow>");
    Some(out)
}

/// An argument element (`m:e`, `m:num`, `m:sub`, ...) as an `mrow`; an absent one is an empty row.
fn arg(parent: &DrawingNode, local: &str) -> Option<String> {
    match parent.child(local) {
        Some(a) => row(&a.children),
        None => Some("<mrow></mrow>".into()),
    }
}

/// The property `name` of the `pr` element(s) of `parent` (a run has both `a:rPr` and `m:rPr`, local names alike).
fn property<'a>(parent: &'a DrawingNode, pr: &str, name: &str) -> Option<&'a DrawingNode> {
    parent.children.iter().filter(|c| c.local == pr).find_map(|p| p.child(name))
}

fn val<'a>(parent: &'a DrawingNode, pr: &str, name: &str) -> Option<&'a str> {
    property(parent, pr, name).and_then(|c| c.attribute("m:val"))
}

/// An on/off property (`m:degHide`, `m:subHide`): present without a value, or `1`/`on`/`true`.
fn on(parent: &DrawingNode, pr: &str, name: &str) -> bool {
    property(parent, pr, name).is_some_and(|c| c.attribute("m:val").is_none_or(|v| matches!(v, "1" | "on" | "true")))
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

fn mo(text: &str) -> String {
    format!("<mo>{}</mo>", escape(text))
}

/// The spacing form of a combining accent OMML stores (`m:acc m:chr`), which MathML draws over its base.
fn spacing_accent(c: &str) -> &str {
    match c {
        "\u{0300}" => "`",
        "\u{0301}" => "\u{00B4}",
        "\u{0302}" => "\u{02C6}",
        "\u{0303}" => "\u{02DC}",
        "\u{0304}" | "\u{0305}" => "\u{00AF}",
        "\u{0306}" => "\u{02D8}",
        "\u{0307}" => "\u{02D9}",
        "\u{0308}" => "\u{00A8}",
        "\u{030C}" => "\u{02C7}",
        "\u{20D6}" => "\u{2190}",
        "\u{20D7}" => "\u{2192}",
        "\u{20E1}" => "\u{2194}",
        other => other,
    }
}

/// A run's text as tokens: digits (with a decimal point inside) as `mn`, letters as `mi` (each its own, italic, as
/// math draws variables; a plain run's letters together, upright), everything else as `mo`.
fn run(r: &DrawingNode) -> Option<String> {
    let text: String = r.children.iter().filter(|c| c.local == "t").map(|t| t.text.as_str()).collect();
    let style = val(r, "rPr", "sty");
    let normal = property(r, "rPr", "nor").is_some();
    if normal {
        return Some(format!("<mtext>{}</mtext>", escape(&text)));
    }
    let variant = match style {
        Some("p") => Some("normal"),
        Some("b") => Some("bold"),
        Some("bi") => Some("bold-italic"),
        _ => None,
    };
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
                i += 1;
            }
            let _ = write!(out, "<mn>{}</mn>", chars[start..i].iter().collect::<String>());
            continue;
        }
        if c.is_alphabetic() {
            let start = i;
            if variant == Some("normal") {
                while i < chars.len() && chars[i].is_alphabetic() {
                    i += 1;
                }
            } else {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            match variant {
                // One upright letter needs saying so; a word of several is upright already.
                Some("normal") if word.chars().count() > 1 => { let _ = write!(out, "<mi>{}</mi>", escape(&word)); }
                Some(v) => { let _ = write!(out, "<mi mathvariant=\"{v}\">{}</mi>", escape(&word)); }
                None => { let _ = write!(out, "<mi>{}</mi>", escape(&word)); }
            }
            continue;
        }
        i += 1;
        if c.is_whitespace() {
            continue;
        }
        out.push_str(&mo(&c.to_string()));
    }
    Some(out)
}

/// The integral signs: their limits sit beside them unless the file says otherwise.
fn integral(chr: &str) -> bool {
    matches!(chr, "\u{222B}" | "\u{222C}" | "\u{222D}" | "\u{222E}" | "\u{222F}" | "\u{2230}" | "\u{2231}" | "\u{2232}" | "\u{2233}")
}

/// One OMML object; `None` for an element outside the subset.
fn object(node: &DrawingNode) -> Option<String> {
    Some(match node.local.as_str() {
        "r" => run(node)?,
        "f" => {
            let (num, den) = (arg(node, "num")?, arg(node, "den")?);
            match val(node, "fPr", "type") {
                Some("noBar") => format!("<mfrac linethickness=\"0\">{num}{den}</mfrac>"),
                Some("lin") | Some("skw") => format!("<mrow>{num}{}{den}</mrow>", mo("/")),
                _ => format!("<mfrac>{num}{den}</mfrac>"),
            }
        }
        "sSup" => format!("<msup>{}{}</msup>", arg(node, "e")?, arg(node, "sup")?),
        "sSub" => format!("<msub>{}{}</msub>", arg(node, "e")?, arg(node, "sub")?),
        "sSubSup" => format!("<msubsup>{}{}{}</msubsup>", arg(node, "e")?, arg(node, "sub")?, arg(node, "sup")?),
        "sPre" => format!("<mmultiscripts>{}<mprescripts/>{}{}</mmultiscripts>", arg(node, "e")?, arg(node, "sub")?, arg(node, "sup")?),
        "rad" => {
            let degree = node.child("deg").filter(|d| d.children.iter().any(|c| !properties(&c.local)));
            match degree {
                Some(d) if !on(node, "radPr", "degHide") => format!("<mroot>{}{}</mroot>", arg(node, "e")?, row(&d.children)?),
                _ => format!("<msqrt>{}</msqrt>", arg(node, "e")?),
            }
        }
        "nary" => {
            let chr = val(node, "naryPr", "chr").unwrap_or("\u{222B}");
            let beside = match val(node, "naryPr", "limLoc") {
                Some("subSup") => true,
                Some(_) => false,
                None => integral(chr),
            };
            let (sub, sup) = (!on(node, "naryPr", "subHide"), !on(node, "naryPr", "supHide"));
            let op = format!("<mo largeop=\"true\" movablelimits=\"false\">{}</mo>", escape(chr));
            let (sub_row, sup_row) = (arg(node, "sub")?, arg(node, "sup")?);
            let operator = match (beside, sub, sup) {
                (_, false, false) => op,
                (true, true, true) => format!("<msubsup>{op}{sub_row}{sup_row}</msubsup>"),
                (true, true, false) => format!("<msub>{op}{sub_row}</msub>"),
                (true, false, true) => format!("<msup>{op}{sup_row}</msup>"),
                (false, true, true) => format!("<munderover>{op}{sub_row}{sup_row}</munderover>"),
                (false, true, false) => format!("<munder>{op}{sub_row}</munder>"),
                (false, false, true) => format!("<mover>{op}{sup_row}</mover>"),
            };
            format!("<mrow>{operator}{}</mrow>", arg(node, "e")?)
        }
        "d" => {
            let begin = val(node, "dPr", "begChr").unwrap_or("(");
            let end = val(node, "dPr", "endChr").unwrap_or(")");
            let separator = val(node, "dPr", "sepChr").unwrap_or("|");
            let mut out = String::from("<mrow>");
            if !begin.is_empty() {
                let _ = write!(out, "<mo fence=\"true\">{}</mo>", escape(begin));
            }
            for (i, e) in node.children.iter().filter(|c| c.local == "e").enumerate() {
                if i > 0 {
                    let _ = write!(out, "<mo separator=\"true\">{}</mo>", escape(separator));
                }
                out.push_str(&row(&e.children)?);
            }
            if !end.is_empty() {
                let _ = write!(out, "<mo fence=\"true\">{}</mo>", escape(end));
            }
            out.push_str("</mrow>");
            out
        }
        "m" => {
            let mut out = String::from("<mtable>");
            for mr in node.children.iter().filter(|c| c.local == "mr") {
                out.push_str("<mtr>");
                for e in mr.children.iter().filter(|c| c.local == "e") {
                    let _ = write!(out, "<mtd>{}</mtd>", row(&e.children)?);
                }
                out.push_str("</mtr>");
            }
            out.push_str("</mtable>");
            out
        }
        "eqArr" => {
            let mut out = String::from("<mtable>");
            for e in node.children.iter().filter(|c| c.local == "e") {
                let _ = write!(out, "<mtr><mtd>{}</mtd></mtr>", row(&e.children)?);
            }
            out.push_str("</mtable>");
            out
        }
        "func" => format!("<mrow>{}<mo>\u{2061}</mo>{}</mrow>", arg(node, "fName")?, arg(node, "e")?),
        "acc" => {
            let chr = spacing_accent(val(node, "accPr", "chr").unwrap_or("\u{0302}"));
            format!("<mover accent=\"true\">{}<mo>{}</mo></mover>", arg(node, "e")?, escape(chr))
        }
        "bar" => match val(node, "barPr", "pos") {
            Some("top") => format!("<mover accent=\"true\">{}<mo>\u{203E}</mo></mover>", arg(node, "e")?),
            _ => format!("<munder accentunder=\"true\">{}<mo>_</mo></munder>", arg(node, "e")?),
        },
        "limLow" => format!("<munder>{}{}</munder>", arg(node, "e")?, arg(node, "lim")?),
        "limUpp" => format!("<mover>{}{}</mover>", arg(node, "e")?, arg(node, "lim")?),
        "groupChr" => {
            let chr = val(node, "groupChrPr", "chr").unwrap_or("\u{23DF}");
            match val(node, "groupChrPr", "pos") {
                Some("top") => format!("<mover>{}<mo>{}</mo></mover>", arg(node, "e")?, escape(chr)),
                _ => format!("<munder>{}<mo>{}</mo></munder>", arg(node, "e")?, escape(chr)),
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    fn n(local: &str, children: Vec<DrawingNode>) -> DrawingNode {
        DrawingNode { local: local.into(), children, ..Default::default() }
    }
    fn with(local: &str, attributes: &[(&str, &str)], children: Vec<DrawingNode>) -> DrawingNode {
        DrawingNode {
            local: local.into(),
            attributes: attributes.iter().map(|(a, b)| ((*a).to_owned(), (*b).to_owned())).collect(),
            children,
            ..Default::default()
        }
    }
    fn r(text: &str) -> DrawingNode {
        n("r", vec![DrawingNode { local: "t".into(), text: text.into(), ..Default::default() }])
    }
    fn e(children: Vec<DrawingNode>) -> DrawingNode {
        n("e", children)
    }
    fn math(children: Vec<DrawingNode>) -> String {
        convert(&n("oMath", children)).unwrap().markup
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_11__runs_split_into_identifiers_numbers_and_operators() {
        assert_eq!(math(vec![r("x=12.5+ab")]), "<div><math><mrow><mi>x</mi><mo>=</mo><mn>12.5</mn><mo>+</mo><mi>a</mi><mi>b</mi></mrow></math></div>");
        let plain = with("r", &[], vec![n("rPr", vec![with("sty", &[("m:val", "p")], vec![])]), DrawingNode { local: "t".into(), text: "sin".into(), ..Default::default() }]);
        assert_eq!(math(vec![plain]), "<div><math><mrow><mi>sin</mi></mrow></math></div>");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_11__the_subset_converts_to_mathml() {
        let frac = n("f", vec![n("num", vec![r("a")]), n("den", vec![r("b")])]);
        assert!(math(vec![frac]).contains("<mfrac><mrow><mi>a</mi></mrow><mrow><mi>b</mi></mrow></mfrac>"));
        let lin = n("f", vec![n("fPr", vec![with("type", &[("m:val", "lin")], vec![])]), n("num", vec![r("a")]), n("den", vec![r("b")])]);
        assert!(math(vec![lin]).contains("<mo>/</mo>"));
        let sup = n("sSup", vec![e(vec![r("x")]), n("sup", vec![r("2")])]);
        assert!(math(vec![sup]).contains("<msup><mrow><mi>x</mi></mrow><mrow><mn>2</mn></mrow></msup>"));
        let sqrt = n("rad", vec![n("radPr", vec![with("degHide", &[("m:val", "1")], vec![])]), n("deg", vec![]), e(vec![r("y")])]);
        assert!(math(vec![sqrt]).contains("<msqrt><mrow><mi>y</mi></mrow></msqrt>"));
        let cube = n("rad", vec![n("deg", vec![r("3")]), e(vec![r("y")])]);
        assert!(math(vec![cube]).contains("<mroot><mrow><mi>y</mi></mrow><mrow><mn>3</mn></mrow></mroot>"));
        // A sum's limits go under and over it, an integral's beside it.
        let sum = n("nary", vec![n("naryPr", vec![with("chr", &[("m:val", "\u{2211}")], vec![])]), n("sub", vec![r("i=1")]), n("sup", vec![r("n")]), e(vec![r("i")])]);
        assert!(math(vec![sum]).contains("<munderover><mo largeop=\"true\" movablelimits=\"false\">\u{2211}</mo>"));
        let int = n("nary", vec![n("sub", vec![r("0")]), n("sup", vec![r("1")]), e(vec![r("x")])]);
        assert!(math(vec![int]).contains("<msubsup><mo largeop=\"true\" movablelimits=\"false\">\u{222B}</mo>"));
        let brackets = n("d", vec![n("dPr", vec![with("begChr", &[("m:val", "[")], vec![]), with("endChr", &[("m:val", "")], vec![])]), e(vec![r("a")]), e(vec![r("b")])]);
        let d = math(vec![brackets]);
        assert!(d.contains("<mo fence=\"true\">[</mo>") && d.contains("<mo separator=\"true\">|</mo>") && d.matches("fence").count() == 1, "{d}");
        let matrix = n("m", vec![n("mr", vec![e(vec![r("1")]), e(vec![r("0")])]), n("mr", vec![e(vec![r("0")]), e(vec![r("1")])])]);
        assert_eq!(math(vec![matrix]).matches("<mtd>").count(), 4);
        let func = n("func", vec![n("fName", vec![r("f")]), e(vec![r("x")])]);
        assert!(math(vec![func]).contains("<mo>\u{2061}</mo>"));
        let hat = n("acc", vec![e(vec![r("x")])]);
        assert!(math(vec![hat]).contains("<mo>\u{02C6}</mo>"));
        let low = n("limLow", vec![e(vec![r("lim")]), n("lim", vec![r("n")])]);
        assert!(math(vec![low]).starts_with("<div><math><mrow><munder>"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_11__anything_outside_the_subset_leaves_the_picture() {
        assert!(convert(&n("oMath", vec![n("borderBox", vec![e(vec![r("x")])])])).is_none());
        assert!(convert(&n("oMath", vec![n("f", vec![n("num", vec![n("phant", vec![])]), n("den", vec![r("b")])])])).is_none());
        // Justification and size come from the paragraph and its first run.
        let para = n("oMathPara", vec![
            n("oMathParaPr", vec![with("jc", &[("m:val", "left")], vec![])]),
            n("oMath", vec![n("r", vec![with("rPr", &[("sz", "1400")], vec![]), DrawingNode { local: "t".into(), text: "x".into(), ..Default::default() }])]),
        ]);
        let converted = convert(&para).unwrap();
        assert_eq!((converted.align, converted.size), ("left", Some(14.0)));
    }
}
