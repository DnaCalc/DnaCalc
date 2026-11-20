//! Inline positions and colours through the CSS object model (dc-xib).
//!
//! The site's Content-Security-Policy (`deploy/caddy/headers.caddy`) allows styles only from the
//! site's own stylesheets: a `style="..."` attribute in markup, or one set with `setAttribute`,
//! is refused. Declarations set through the element's `CSSStyleDeclaration` are not markup and
//! are allowed. Every computed style in the skins goes through [`css`] (as the directive
//! `use:css=declarations` on an element a view builds again when its declarations change) or
//! through `style:property=` directives, which Leptos applies the same way; the static rules live
//! in the stylesheets under `web/css/`.

use leptos::wasm_bindgen::JsCast;

/// Sets `declarations` (`"left:4px;top:8px"`) as the element's inline style through the CSS
/// object model (`style.cssText`).
pub fn css(element: web_sys::Element, declarations: String) {
    if let Some(element) = element.dyn_ref::<web_sys::HtmlElement>() {
        element.style().set_css_text(&declarations);
    } else if let Some(element) = element.dyn_ref::<web_sys::SvgElement>() {
        element.style().set_css_text(&declarations);
    }
}

/// Markup with every `style` attribute of its tags renamed `data-css` (text and attribute values left alone), so the
/// browser parses it without an inline style the policy refuses (dc-zjsg.12).
pub fn styles_out(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + 64);
    let (mut in_tag, mut quote) = (false, None::<char>);
    let mut rest = html;
    while let Some(c) = rest.chars().next() {
        match (in_tag, quote, c) {
            (false, _, '<') => in_tag = true,
            (true, None, '>') => in_tag = false,
            (true, None, '"' | '\'') => quote = Some(c),
            (true, Some(q), c) if c == q => quote = None,
            (true, None, c) if c.is_ascii_whitespace() && rest[1..].starts_with("style=") => {
                out.push(c);
                out.push_str("data-css=");
                rest = &rest[1 + "style=".len()..];
                continue;
            }
            _ => {}
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Sets `html` as the element's content with its inline styles applied through the CSS object model rather than as
/// `style` attributes (see [`styles_out`]): the drawing objects' and controls' markup (dc-zjsg.12).
pub fn styled(element: web_sys::Element, html: String) {
    element.set_inner_html(&styles_out(&html));
    if let Ok(nodes) = element.query_selector_all("[data-css]") {
        for i in 0..nodes.length() {
            if let Some(node) = nodes.get(i) {
                let node: web_sys::Element = node.unchecked_into();
                let declarations = node.get_attribute("data-css").unwrap_or_default();
                let _ = node.remove_attribute("data-css");
                css(node, declarations);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::styles_out;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_12__style_attributes_are_renamed_inside_tags_only() {
        assert_eq!(
            styles_out("<div style=\"left:1px\"><p style='x'>a style=\"b\" &lt;i style=1&gt;</p><svg width=\"2\" data-style=\"k\"/></div>"),
            "<div data-css=\"left:1px\"><p data-css='x'>a style=\"b\" &lt;i style=1&gt;</p><svg width=\"2\" data-style=\"k\"/></div>"
        );
    }
}
