//! Clipboard interchange and the grid's clipboard controls. HTML is read as an inert table;
//! no pasted markup, script, style or link enters the live document.
use crate::{controller::Controller, wire};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

pub const TEXT_LIMIT: usize = 8_000_000;

/// The extension outside the seed rectangle. The larger distance chooses the axis at a corner.
pub fn fill_extension(
    from: calc_value::GridArea,
    row: u32,
    col: u32,
) -> Option<(calc_value::GridArea, bool)> {
    use calc_value::{CellKey, GridArea};
    let (a, b) = (from.first(), from.last());
    let dr = if row < a.row() {
        a.row() - row
    } else {
        row.saturating_sub(b.row())
    };
    let dc = if col < a.column() {
        a.column() - col
    } else {
        col.saturating_sub(b.column())
    };
    if dr == 0 && dc == 0 {
        return None;
    }
    let down = dr >= dc;
    let (r0, c0, r1, c1) = if down {
        if row > b.row() {
            (b.row() + 1, a.column(), row, b.column())
        } else {
            (row, a.column(), a.row() - 1, b.column())
        }
    } else if col > b.column() {
        (a.row(), b.column() + 1, b.row(), col)
    } else {
        (a.row(), col, b.row(), a.column() - 1)
    };
    Some((
        GridArea::new(
            CellKey::new(from.sheet(), r0, c0).ok()?,
            CellKey::new(from.sheet(), r1, c1).ok()?,
        )
        .ok()?,
        down,
    ))
}

pub use calc_value::matrix_shape;
pub use dna_calc_model::clipboard::{encode_html, encode_tsv, escape};

/// Excel-style quoted fields preserve tabs, line breaks, empty fields and doubled quotes.
pub fn decode_tsv(text: &str) -> Result<Vec<Vec<String>>, String> {
    if text.len() > TEXT_LIMIT {
        return Err("The clipboard text is too large".to_owned());
    }
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    let mut ended = false;
    while let Some(ch) = chars.next() {
        ended = false;
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(ch);
            }
            continue;
        }
        match ch {
            '"' if field.is_empty() && !closed => quoted = true,
            '\t' => {
                row.push(std::mem::take(&mut field));
                closed = false;
            }
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                closed = false;
                ended = true;
            }
            _ if closed => return Err("Unexpected text after a quoted clipboard field".to_owned()),
            _ => field.push(ch),
        }
    }
    if quoted {
        return Err("A quoted clipboard field is unfinished".to_owned());
    }
    if !ended {
        row.push(field);
        rows.push(row);
    }
    matrix_shape(&rows)?;
    Ok(rows)
}

/// Reconstruct only table structure before asking the browser to parse it. Inert templates still
/// report CSP violations for style attributes in some browsers, so those never reach the parser.
pub fn table_markup(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    let mut blocked = None::<String>;
    while !rest.is_empty() {
        let Some(start) = rest.find('<') else {
            if blocked.is_none() {
                out.push_str(rest);
            }
            break;
        };
        if blocked.is_none() {
            out.push_str(&rest[..start]);
        }
        rest = &rest[start..];
        if rest.starts_with("<!--") {
            if let Some(end) = rest.find("-->") {
                rest = &rest[end + 3..];
                continue;
            } else {
                break;
            }
        }
        let mut quote = None;
        let mut end = None;
        for (index, ch) in rest.char_indices().skip(1) {
            if let Some(held) = quote {
                if ch == held {
                    quote = None;
                }
            } else if matches!(ch, '\'' | '"') {
                quote = Some(ch);
            } else if ch == '>' {
                end = Some(index);
                break;
            }
        }
        let Some(end) = end else {
            if blocked.is_none() {
                out.push_str("&lt;");
                out.push_str(&rest[1..]);
            }
            break;
        };
        let inside = rest[1..end].trim();
        rest = &rest[end + 1..];
        let closing = inside.starts_with('/');
        let inside = inside.trim_start_matches('/').trim_start();
        let n = inside
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .unwrap_or(inside.len());
        let name = inside[..n].to_ascii_lowercase();
        let mut attrs = &inside[n..];
        if let Some(held) = &blocked {
            if closing && &name == held {
                blocked = None;
            }
            continue;
        }
        if matches!(name.as_str(), "script" | "style" | "template") {
            if !closing {
                blocked = Some(name);
            }
            continue;
        }
        if !matches!(
            name.as_str(),
            "table" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th" | "br"
        ) {
            continue;
        }
        out.push('<');
        if closing {
            out.push('/');
        }
        out.push_str(&name);
        while !closing && !attrs.trim_start().is_empty() {
            attrs = attrs.trim_start();
            let len = attrs
                .find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '/')
                .unwrap_or(attrs.len());
            if len == 0 {
                attrs = &attrs[1..];
                continue;
            }
            let key = attrs[..len].to_ascii_lowercase();
            attrs = attrs[len..].trim_start();
            if !attrs.starts_with('=') {
                continue;
            }
            attrs = attrs[1..].trim_start();
            let value = if attrs.starts_with(['\'', '"']) {
                let quote = attrs.as_bytes()[0] as char;
                attrs = &attrs[1..];
                let len = attrs.find(quote).unwrap_or(attrs.len());
                let value = &attrs[..len];
                attrs = &attrs[(len + 1).min(attrs.len())..];
                value
            } else {
                let len = attrs
                    .find(|c: char| c.is_ascii_whitespace())
                    .unwrap_or(attrs.len());
                let value = &attrs[..len];
                attrs = &attrs[len..];
                value
            };
            if matches!(
                key.as_str(),
                "rowspan" | "colspan" | "data-dnacalc-clipboard" | "data-dnacalc-session"
            ) {
                out.push(' ');
                out.push_str(&key);
                out.push_str("=\"");
                out.push_str(&escape(value));
                out.push('"');
            }
        }
        out.push('>');
    }
    out
}

pub fn decode_html(html: &str) -> Result<(Vec<Vec<String>>, Option<(String, u64)>), String> {
    if html.len() > TEXT_LIMIT {
        return Err("The clipboard HTML is too large".to_owned());
    }
    let doc = document();
    let template = doc
        .create_element("template")
        .map_err(|_| "HTML parsing is unavailable")?
        .dyn_into::<web_sys::HtmlTemplateElement>()
        .map_err(|_| "HTML parsing is unavailable")?;
    template.set_inner_html(&table_markup(html));
    let table = template
        .content()
        .query_selector("table")
        .map_err(|_| "Cannot read the clipboard table")?
        .ok_or("The clipboard HTML has no table")?;
    if table.query_selector("table").ok().flatten().is_some() {
        return Err("Nested clipboard tables are not modelled".to_owned());
    }
    let token = table
        .get_attribute("data-dnacalc-clipboard")
        .and_then(|id| id.parse().ok())
        .zip(table.get_attribute("data-dnacalc-session"))
        .map(|(token, session)| (session, token));
    let elements = table
        .query_selector_all("tr")
        .map_err(|_| "Cannot read the clipboard rows")?;
    let mut rows = Vec::new();
    for i in 0..elements.length() {
        let tr = elements
            .item(i)
            .unwrap()
            .dyn_into::<web_sys::Element>()
            .unwrap();
        let cells = tr
            .query_selector_all(":scope > td, :scope > th")
            .map_err(|_| "Cannot read clipboard cells")?;
        let mut row = Vec::new();
        for j in 0..cells.length() {
            let cell = cells
                .item(j)
                .unwrap()
                .dyn_into::<web_sys::Element>()
                .unwrap();
            if ["rowspan", "colspan"]
                .iter()
                .any(|attr| cell.get_attribute(attr).is_some_and(|v| v != "1"))
            {
                return Err("Pasted merged HTML cells are not modelled; paste an unmerged table or plain text".to_owned());
            }
            // Replace only inert break elements, then read text. Hidden/script content is removed.
            for tag in ["script,style,template", "br"] {
                let nodes = cell
                    .query_selector_all(tag)
                    .map_err(|_| "Cannot read clipboard text")?;
                for k in 0..nodes.length() {
                    let node = nodes.item(k).unwrap();
                    if tag == "br" {
                        node.set_text_content(Some("\n"));
                        let text = doc.create_text_node("\n");
                        let _ = node.parent_node().unwrap().replace_child(&text, &node);
                    } else {
                        let _ = node.parent_node().unwrap().remove_child(&node);
                    }
                }
            }
            row.push(cell.text_content().unwrap_or_default());
        }
        if !row.is_empty() {
            rows.push(row);
        }
    }
    matrix_shape(&rows)?;
    Ok((rows, token))
}

pub async fn write_system(payload: &wire::Clipboard) -> Result<(), wasm_bindgen::JsValue> {
    let items = js_sys::Object::new();
    for (kind, text) in [("text/plain", &payload.text), ("text/html", &payload.html)] {
        let parts = js_sys::Array::new();
        parts.push(&text.into());
        let options = web_sys::BlobPropertyBag::new();
        options.set_type(kind);
        let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options)?;
        js_sys::Reflect::set(&items, &kind.into(), &blob)?;
    }
    let item = web_sys::ClipboardItem::new_with_record_from_str_to_blob_promise(&items)?;
    let list = js_sys::Array::new();
    list.push(&item);
    wasm_bindgen_futures::JsFuture::from(window().navigator().clipboard().write(&list)).await?;
    Ok(())
}

pub async fn read_system() -> Result<(String, String), wasm_bindgen::JsValue> {
    let result =
        wasm_bindgen_futures::JsFuture::from(window().navigator().clipboard().read()).await?;
    let items = js_sys::Array::from(&result);
    let mut text = String::new();
    let mut html = String::new();
    for value in items.iter() {
        let item = value.dyn_into::<web_sys::ClipboardItem>()?;
        for kind in ["text/html", "text/plain"] {
            if !item.types().includes(&kind.into(), 0) {
                continue;
            }
            let blob = wasm_bindgen_futures::JsFuture::from(item.get_type(kind))
                .await?
                .dyn_into::<web_sys::Blob>()?;
            let value = wasm_bindgen_futures::JsFuture::from(blob.text())
                .await?
                .as_string()
                .unwrap_or_default();
            if kind == "text/html" {
                html = value;
            } else {
                text = value;
            }
        }
        if !text.is_empty() || !html.is_empty() {
            break;
        }
    }
    Ok((text, html))
}

pub fn native_copy(controller: Controller, event: web_sys::ClipboardEvent, cut: bool) {
    if controller.editing.get_untracked().is_some() {
        return;
    }
    event.prevent_default();
    // A repeat copy of the same revision can fill the synchronous native event immediately.
    if let Some(payload) = controller.copied.get_untracked().filter(|p| {
        Some(p.area.area().unwrap()) == controller.selected_area_untracked()
            && p.revision == controller.summary.get_untracked().revision
    }) {
        if let Some(data) = event.clipboard_data() {
            let _ = data.set_data("text/plain", &payload.text);
            let _ = data.set_data("text/html", &payload.html);
            controller.clipboard.set(payload.area.area());
            controller.cut.set(cut);
            return;
        }
    }
    if cut {
        controller.cut();
    } else {
        controller.copy();
    }
}

pub fn native_paste(controller: Controller, event: web_sys::ClipboardEvent) {
    if controller.editing.get_untracked().is_some() {
        return;
    }
    event.prevent_default();
    if let Some(data) = event.clipboard_data() {
        let text = data.get_data("text/plain").unwrap_or_default();
        let html = data.get_data("text/html").unwrap_or_default();
        controller.paste_external(text, html);
    } else {
        controller.clipboard_fallback.set(Some(false));
    }
}

/// Compact, keyboard-reachable commands, a native modal for conflicts and a permission fallback.
// Bound the parent view size: retained controller closures otherwise accumulate on the wasm stack.
pub fn controls(controller: Controller) -> AnyView {
    let transpose = controller.paste_transpose();
    let dialog: NodeRef<leptos::html::Dialog> = NodeRef::new();
    let scope = dna_calc_model::SkinId::new("clipboard-dialog");
    let fallback_text = controller.view_field::<dna_calc_model::ClipboardDraftState, _>(
        scope,
        |s| &s.text,
        |s| &mut s.text,
    );
    let fill_end = controller.view_field::<dna_calc_model::ClipboardDraftState, _>(
        scope,
        |s| &s.fill_end,
        |s| &mut s.fill_end,
    );
    Effect::new(move |_| {
        let fallback = controller.clipboard_fallback.get();
        let preview = controller.paste_preview.get();
        if let Some(element) = dialog.get() {
            if fallback.is_some() || preview.is_some() {
                if let Some(copy) = fallback {
                    fallback_text.set(if copy {
                        controller
                            .copied
                            .get_untracked()
                            .map(|c| c.text.clone())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    });
                }
                let _ = element.show_modal();
            } else {
                element.close();
            }
        }
    });
    let cancel = move || {
        if let Some(element) = dialog.get_untracked() {
            element.close();
        }
        controller.clipboard_fallback.set(None);
        controller.paste_preview.set(None);
        controller.focus_active_skin();
    };
    view! {
        <details class="dc-clipboard-tools" data-dnacalc="clipboard-tools" name="worksheet-tools">
            <summary title="Paste Special (Ctrl+Alt+V)">"Clipboard & fill"</summary>
            <p class="dc-note">"External tables paste contents. Cell formats apply to copies within this workbook. Merged HTML tables are unavailable."</p>
            <div class="dc-command-row">
                <button data-dnacalc="copy" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.copy()>"Copy"</button>
                <button data-dnacalc="cut" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.cut()>"Cut"</button>
                <button data-dnacalc="paste" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.paste_system()>"Paste"</button>
                <button data-dnacalc="paste-snapshot" disabled=move ||(controller.copied.get().is_none() && !controller.copy_requested()) || controller.editing.get().is_some() on:click=move |_|controller.paste()>"Paste copied cells"</button>
                <label>"Paste Special "<select data-dnacalc="paste-mode" prop:value=move ||controller.paste_mode.get() on:change=move |ev|controller.paste_mode.set(event_target_value(&ev))>
                    <option value="all">"Contents and cell formats"</option><option value="values">"Values"</option><option value="formulas">"Formulas"</option><option value="formats">"Cell formats"</option>
                </select></label>
                <label><input type="checkbox" data-dnacalc="paste-transpose" prop:checked=move ||transpose.get() on:change=move |ev|transpose.set(event_target_checked(&ev))/><span>"Transpose rows and columns"</span></label>
                <button data-dnacalc="paste-fallback" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.clipboard_fallback.set(Some(false))>"Paste text…"</button>
                <button data-dnacalc="copy-fallback" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.clipboard_fallback.set(Some(true))>"Copy text…"</button>
                <button data-dnacalc="flash-fill" disabled=move ||controller.command_availability(dna_calc_model::CommandId::FlashFill).is_err() title="Flash Fill (Ctrl+E): select an output column with two text examples and blanks; source text is immediately to its left" aria-describedby="dc-flash-fill-help" on:click=move |_|controller.command(dna_calc_model::Command::FlashFill)>"Flash Fill"</button>
                <p class="dc-sr-only" id="dc-flash-fill-help">"Flash Fill (Ctrl+E): select one output column with two text examples and blanks. Source text goes immediately to its left; preview supports first/last words and delimiter parts."</p>
                <button data-dnacalc="fill-down" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.fill(true)>"Fill down"</button>
                <button data-dnacalc="fill-right" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.fill(false)>"Fill right"</button>
                <label>"Fill mode "<select data-dnacalc="fill-mode" prop:value=move ||controller.fill_series.get() on:change=move |ev|controller.fill_series.set(event_target_value(&ev))><option value="series">"Series from seeds / formulas"</option><option value="copy">"Copy cells"</option></select></label>
                <label>"Fill through "<input data-dnacalc="fill-through" aria-label="Fill through cell" placeholder="End cell, e.g. A10" disabled=move ||controller.editing.get().is_some() prop:value=move ||fill_end.get() on:input=move |ev|fill_end.set(event_target_value(&ev)) on:keydown=move |ev: web_sys::KeyboardEvent|if ev.key()=="Enter"{ev.prevent_default();controller.fill_through(fill_end.get_untracked());}/></label>
                <button data-dnacalc="fill-through-preview" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.fill_through(fill_end.get_untracked())>"Preview fill"</button>
            </div>
        </details>
        <dialog class="dc-clipboard-dialog" data-dnacalc="clipboard-dialog" node_ref=dialog on:cancel=move |ev: web_sys::Event|{ev.prevent_default();cancel();}>
            {move ||controller.paste_preview.get().map(|p|view! {
                <h2>"Review paste, move or fill"</h2>
                <p data-dnacalc="paste-preview">{format!("{} cells at {}. {} existing inputs will be replaced. One Undo restores the operation.",p.area.area().unwrap().cell_count(),controller.area_label(p.area.area().unwrap()),p.replaces)}</p>
                {(!p.samples.is_empty()).then(||view!{<div data-dnacalc="paste-samples"><p>"First generated text values:"</p><ul>{p.samples.iter().map(|sample|view!{<li>{sample.clone()}</li>}).collect_view()}</ul></div>})}
                <button data-dnacalc="paste-confirm" on:click=move |_|controller.apply_paste(p.id)>"Apply"</button>
            })}
            {move ||controller.clipboard_fallback.get().map(|copy|view! {
                <h2>{if copy {"Copy table text"} else {"Paste table text"}}</h2>
                <p>{if copy {"Select the text and press Ctrl+C (or Command+C). The workbook snapshot remains available through Paste copied cells."} else {"Paste with Ctrl+V (or Command+V), then choose Paste text. Tabs separate columns; new lines separate rows."}}</p>
                <textarea data-dnacalc="clipboard-text" aria-label="Clipboard table text" prop:value=move ||fallback_text.get() on:input=move |ev|fallback_text.set(event_target_value(&ev))></textarea>
                <button data-dnacalc="paste-text" disabled=copy on:click=move |_|{let text=fallback_text.get_untracked();controller.clipboard_fallback.set(None);controller.paste_external(text,String::new());}>"Paste text"</button>
            })}
            <button data-dnacalc="paste-cancel" on:click=move |_|cancel()>"Cancel"</button>
        </dialog>
    }.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn clipboard_markup_has_no_active_tags_or_inline_styles() {
        assert_eq!(
            table_markup(
                "<html><style>bad</style><script>bad</script><body><table style='color:red' onclick='bad'><tr><td style='x:y' colspan=2>one &amp; two<br/>three<img src='https://invalid.example/x'></td></tr></table></body></html>"
            ),
            "<table><tr><td colspan=\"2\">one &amp; two<br>three</td></tr></table>"
        );
        assert_eq!(
            table_markup(
                "<table data-dnacalc-clipboard='4' data-dnacalc-session='abc'><tr><td title='1 > 2'>3</td></tr></table>"
            ),
            "<table data-dnacalc-clipboard=\"4\" data-dnacalc-session=\"abc\"><tr><td>3</td></tr></table>"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn separate_workers_never_share_a_clipboard_identity() {
        fn capture(mut server: dna_calc_worker::server::Server) -> wire::Clipboard {
            let sheet = server.book().host().sheets()[0].id();
            let at = calc_value::CellKey::new(sheet, 1, 1).unwrap();
            match server
                .handle(
                    wire::Request::Capture {
                        area: wire::Area::of(calc_value::GridArea::cell(at)),
                        keep: None,
                    },
                    None,
                )
                .reply
                .unwrap()
            {
                wire::Reply::Clipboard(payload) => payload,
                other => panic!("{other:?}"),
            }
        }
        let a = capture(dna_calc_worker::server::Server::new());
        let b = capture(dna_calc_worker::server::Server::new());
        assert_eq!(a.token, b.token);
        assert!(!a.session.is_empty());
        assert!(!b.session.is_empty());
        assert_ne!(a.session, b.session);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tsv_quotes_blanks_unicode_and_terminal_newlines() {
        let rows = vec![
            vec!["a\tb".to_owned(), String::new(), "λ\n\"x\"".to_owned()],
            vec!["1".to_owned(), "2".to_owned(), "3".to_owned()],
        ];
        assert_eq!(decode_tsv(&encode_tsv(&rows)).unwrap(), rows);
        assert_eq!(
            decode_tsv("1\t\r\n2\t\r\n").unwrap(),
            vec![vec!["1", ""], vec!["2", ""]]
        );
        assert!(decode_tsv("1\t2\n3").is_err());
        assert!(decode_tsv("\"unfinished").is_err());
        assert!(decode_tsv("\"a\"b").is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fill_extent_chooses_one_axis_and_never_overwrites_the_seed() {
        use calc_value::{CellKey, GridArea, IdAllocator, SheetId};
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let s = ids.issue().unwrap();
        let area = |a, b, c, d| {
            GridArea::new(
                CellKey::new(s, a, b).unwrap(),
                CellKey::new(s, c, d).unwrap(),
            )
            .unwrap()
        };
        let seed = area(3, 3, 4, 4);
        assert_eq!(fill_extension(seed, 3, 3), None);
        assert_eq!(fill_extension(seed, 8, 4), Some((area(5, 3, 8, 4), true)));
        assert_eq!(fill_extension(seed, 1, 4), Some((area(1, 3, 2, 4), true)));
        assert_eq!(fill_extension(seed, 4, 1), Some((area(3, 1, 4, 2), false)));
        assert_eq!(fill_extension(seed, 4, 8), Some((area(3, 5, 4, 8), false)));
    }
}

/// The shell shortcut opens the same retained controls used by pointer users.
pub fn open_paste_special(){
    #[cfg(target_arch="wasm32")]{
        if let Some(details)=document().query_selector("[data-dnacalc=clipboard-tools]").ok().flatten().and_then(|e|e.dyn_into::<web_sys::HtmlDetailsElement>().ok()){
            details.set_open(true);
            if let Some(field)=document().query_selector("[data-dnacalc=paste-mode]").ok().flatten().and_then(|e|e.dyn_into::<web_sys::HtmlElement>().ok()){let _=field.focus();}
        }
    }
}
