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
