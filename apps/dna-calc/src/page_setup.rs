//! Page Setup drafts live in the portable tool state; Apply uses the host command transaction.
use crate::{controller::Controller, wire};
use dna_calc_model::printing::{PageBreak, PageSetupChange, PageSetupState};
use leptos::prelude::*;
use std::collections::BTreeMap;
type Fields = BTreeMap<String, String>;
const ATTRIBUTES: &[(&str, &str, &str)] = &[
    ("pageSetup.orientation", "Orientation", "default"),
    ("pageSetup.paperSize", "Paper size", ""),
    ("pageSetup.scale", "Adjust to (%)", "100"),
    ("pageSetUpPr.fitToPage", "Fit to pages", "0"),
    ("pageSetup.fitToWidth", "Pages wide (0 = automatic)", "1"),
    ("pageSetup.fitToHeight", "Pages tall (0 = automatic)", "1"),
    ("pageSetup.pageOrder", "Page order", "downThenOver"),
    ("pageSetup.firstPageNumber", "First page number", "1"),
    ("pageSetup.useFirstPageNumber", "Use first page number", "0"),
    ("pageMargins.left", "Left (inches)", "0.7"),
    ("pageMargins.right", "Right (inches)", "0.7"),
    ("pageMargins.top", "Top (inches)", "0.75"),
    ("pageMargins.bottom", "Bottom (inches)", "0.75"),
    ("pageMargins.header", "Header from edge (inches)", "0.3"),
    ("pageMargins.footer", "Footer from edge (inches)", "0.3"),
    (
        "printOptions.horizontalCentered",
        "Centre horizontally",
        "0",
    ),
    ("printOptions.verticalCentered", "Centre vertically", "0"),
    (
        "headerFooter.differentOddEven",
        "Different odd and even pages",
        "0",
    ),
    ("headerFooter.differentFirst", "Different first page", "0"),
    (
        "headerFooter.scaleWithDoc",
        "Scale headers and footers with document",
        "1",
    ),
    (
        "headerFooter.alignWithMargins",
        "Align headers and footers with margins",
        "1",
    ),
    ("printOptions.gridLines", "Print gridlines", "0"),
    (
        "printOptions.headings",
        "Print row and column headings",
        "0",
    ),
    ("pageSetup.blackAndWhite", "Black and white", "0"),
    ("pageSetup.draft", "Draft quality", "0"),
    ("pageSetup.cellComments", "Print comments", "none"),
    ("pageSetup.errors", "Cell errors as", "displayed"),
];
const HEADERS: &[(&str, &str)] = &[
    ("oddHeader", "Odd page header"),
    ("oddFooter", "Odd page footer"),
    ("evenHeader", "Even page header"),
    ("evenFooter", "Even page footer"),
    ("firstHeader", "First page header"),
    ("firstFooter", "First page footer"),
];
fn fields(state: &PageSetupState) -> Fields {
    let mut out = Fields::new();
    for (key, _, default) in ATTRIBUTES {
        let (section, name) = key.split_once('.').unwrap();
        let mut value = state
            .settings
            .get(section, name)
            .unwrap_or(default)
            .to_owned();
        if *default == "0" || *default == "1" {
            if matches!(
                name,
                "fitToPage"
                    | "useFirstPageNumber"
                    | "horizontalCentered"
                    | "verticalCentered"
                    | "differentOddEven"
                    | "differentFirst"
                    | "scaleWithDoc"
                    | "alignWithMargins"
                    | "gridLines"
                    | "headings"
                    | "blackAndWhite"
                    | "draft"
            ) {
                value = if value == "1" || value == "true" {
                    "1"
                } else {
                    "0"
                }
                .into();
            }
        }
        out.insert((*key).into(), value);
    }
    out.insert(
        "printOptions.gridLines".into(),
        if state.settings.print_gridlines() {
            "1"
        } else {
            "0"
        }
        .into(),
    );
    for (key, _) in HEADERS {
        out.insert(
            format!("header.{key}"),
            state
                .settings
                .headers
                .get(*key)
                .cloned()
                .unwrap_or_default(),
        );
    }
    out.insert("print_area".into(), state.print_area.clone());
    out.insert("print_titles".into(), state.print_titles.clone());
    for (key, breaks) in [
        ("row_breaks", &state.settings.row_breaks),
        ("column_breaks", &state.settings.column_breaks),
    ] {
        out.insert(
            key.into(),
            breaks
                .iter()
                .filter(|b| b.manual)
                .map(|b| (b.id + 1).to_string())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    out
}
fn patch(
    base: &Fields,
    draft: &Fields,
    source: &PageSetupState,
) -> Result<PageSetupChange, String> {
    let mut change = PageSetupChange::default();
    for (key, value) in draft {
        if base.get(key) == Some(value) {
            continue;
        }
        if let Some(name) = key.strip_prefix("header.") {
            change.headers.push((name.into(), value.clone()));
        } else if let Some((section, name)) = key.split_once('.') {
            change.attributes.push((
                section.into(),
                name.into(),
                (!value.is_empty()).then_some(value.clone()),
            ));
        } else {
            match key.as_str() {
                "print_area" => change.print_area = Some(value.clone()),
                "print_titles" => change.print_titles = Some(value.clone()),
                "row_breaks" | "column_breaks" => {
                    let rows = key == "row_breaks";
                    let max = if rows { 1048576 } else { 16384 };
                    let old = if rows {
                        &source.settings.row_breaks
                    } else {
                        &source.settings.column_breaks
                    };
                    let mut breaks = old
                        .iter()
                        .copied()
                        .filter(|b| !b.manual)
                        .collect::<Vec<_>>();
                    for text in value.split([',', ';', ' ']).filter(|s| !s.is_empty()) {
                        let n=text.parse::<u32>().ok().filter(|n|(2..=max).contains(n)).ok_or("Manual breaks need row or column numbers from 2 to the worksheet limit")?;
                        breaks.retain(|b| b.id != n - 1 || b.manual);
                        let retained = old
                            .iter()
                            .copied()
                            .filter(|b| b.id == n - 1 && b.manual)
                            .collect::<Vec<_>>();
                        if retained.is_empty() {
                            let added = PageBreak {
                                id: n - 1,
                                min: 0,
                                max: if rows { 16383 } else { 1048575 },
                                manual: true,
                                pivot: false,
                            };
                            if !breaks.contains(&added) {
                                breaks.push(added);
                            }
                        } else {
                            for kept in retained {
                                if !breaks.contains(&kept) {
                                    breaks.push(kept);
                                }
                            }
                        }
                    }
                    breaks.sort_by_key(|b| b.id);
                    if rows {
                        change.row_breaks = Some(breaks);
                    } else {
                        change.column_breaks = Some(breaks);
                    }
                }
                _ => {}
            }
        }
    }
    if change
        .attributes
        .iter()
        .any(|(section, name, _)| section == "printOptions" && name == "gridLines")
    {
        change.attributes.push((
            "printOptions".into(),
            "gridLinesSet".into(),
            Some("1".into()),
        ));
    }
    if change
        .attributes
        .iter()
        .any(|(section, name, _)| section == "pageSetup" && name == "paperSize")
    {
        // Explicit custom dimensions override paperSize in SpreadsheetML.
        for name in ["paperWidth", "paperHeight"] {
            if source.settings.get("pageSetup", name).is_some() {
                change
                    .attributes
                    .push(("pageSetup".into(), name.into(), None));
            }
        }
    }
    Ok(change)
}
fn input(controller: Controller, key: &'static str, label: &'static str) -> AnyView {
    let draft = controller.tool_field(|s| &s.page.draft, |s| &mut s.page.draft);
    let id = format!("page-{}", key.replace('.', "-"));
    view!{<label>{label}<input data-dnacalc=id prop:value=move ||draft.with(|d|d.get(key).cloned().unwrap_or_default()) on:input=move |ev|draft.update(|d|{d.insert(key.into(),event_target_value(&ev));})/></label>}.into_any()
}
fn attribute(controller: Controller, key: &'static str, label: &'static str) -> AnyView {
    let draft = controller.tool_field(|s| &s.page.draft, |s| &mut s.page.draft);
    let id = format!("page-{}", key.replace('.', "-"));
    let name = key.split_once('.').unwrap().1;
    let options: &[(&str, &str)] = match name {
        "orientation" => &[
            ("default", "Printer default"),
            ("portrait", "Portrait"),
            ("landscape", "Landscape"),
        ],
        "paperSize" => &[
            ("", "Printer default (Letter preview)"),
            ("1", "Letter"),
            ("5", "Legal"),
            ("3", "Tabloid"),
            ("7", "Executive"),
            ("8", "A3"),
            ("9", "A4"),
            ("11", "A5"),
        ],
        "pageOrder" => &[
            ("downThenOver", "Down, then over"),
            ("overThenDown", "Over, then down"),
        ],
        "cellComments" => &[
            ("none", "None"),
            ("asDisplayed", "As displayed"),
            ("atEnd", "At end of sheet"),
        ],
        "errors" => &[
            ("displayed", "As displayed"),
            ("blank", "Blank"),
            ("dash", "--"),
            ("NA", "#N/A"),
        ],
        _ => &[],
    };
    if !options.is_empty() {
        return view!{<label>{label}<select data-dnacalc=id prop:value=move ||draft.with(|d|d.get(key).cloned().unwrap_or_default()) on:change=move |ev|draft.update(|d|{d.insert(key.into(),event_target_value(&ev));})>{options.iter().map(|(v,label)|view!{<option value=*v>{*label}</option>}).collect_view()}{move ||{let value=draft.with(|d|d.get(key).cloned().unwrap_or_default());(!options.iter().any(|(v,_)|*v==value)).then(||view!{<option value=value.clone()>{format!("Imported setting ({value})")}</option>})}}</select></label>}.into_any();
    }
    if matches!(
        name,
        "fitToPage"
            | "useFirstPageNumber"
            | "horizontalCentered"
            | "verticalCentered"
            | "differentOddEven"
            | "differentFirst"
            | "scaleWithDoc"
            | "alignWithMargins"
            | "gridLines"
            | "headings"
            | "blackAndWhite"
            | "draft"
    ) {
        return view!{<label class="dc-page-check"><input type="checkbox" data-dnacalc=id prop:checked=move ||draft.with(|d|d.get(key).is_some_and(|v|v=="1")) on:change=move |ev|draft.update(|d|{d.insert(key.into(),if event_target_checked(&ev){"1"}else{"0"}.into());})/>{label}</label>}.into_any();
    }
    input(controller, key, label)
}
#[inline(never)]
pub fn controls(controller: Controller) -> AnyView {
    let open = controller.tool_field(|s| &s.page.open, |s| &mut s.page.open);
    let draft = controller.tool_field(|s| &s.page.draft, |s| &mut s.page.draft);
    let base = controller.tool_field(|s| &s.page.base, |s| &mut s.page.base);
    let source = controller.tool_field(|s| &s.page.source, |s| &mut s.page.source);
    let target = controller.tool_field(|s| &s.page.target, |s| &mut s.page.target);
    let message = controller.tool_field(|s| &s.page.message, |s| &mut s.page.message);
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    let pending = RwSignal::new(false);
    let epoch = StoredValue::new(0u64);
    Effect::new(move |_| {
        let visible = open.get();
        let generation = controller.generation.get();
        let sheet = controller.sheet.get();
        epoch.update_value(|e| *e += 1);
        let request = epoch.get_value();
        if !visible {
            if let Some(d) = dialog.get() {
                if d.open() {
                    d.close();
                }
            }
            return;
        }
        if let Some(d) = dialog.get() {
            if !d.open() {
                let _ = d.show_modal();
            }
        }
        let Some(sheet) = sheet else {
            return;
        };
        source.set(None);
        pending.set(false);
        message.set("Reading page setup…".into());
        controller.ask_result(
            wire::Request::PageSetup(wire::Sheet::of(sheet)),
            move |reply| {
                if epoch.try_get_value() != Some(request)
                    || !open.get_untracked()
                    || controller.generation.get_untracked() != generation
                    || controller.sheet.get_untracked() != Some(sheet)
                {
                    return;
                }
                match reply {
                    Ok(wire::Reply::PageSetup(Ok(value))) => {
                        let values = fields(&value);
                        base.set(values.clone());
                        draft.set(values);
                        source.set(Some(value));
                        target.set(Some((generation, wire::Sheet::of(sheet))));
                        message.set(String::new());
                        crate::app::focus("[data-dnacalc=page-pageSetup-orientation]");
                    }
                    Ok(wire::Reply::PageSetup(Err(e))) | Err(e) => message.set(e),
                    _ => message.set("Page Setup did not answer".into()),
                }
            },
        );
    });
    let cancel = move || {
        open.set(false);
        controller.focus_active_skin();
    };
    let apply = move || {
        let Some(state) = source.get_untracked() else {
            return;
        };
        let Some((generation, sheet)) = target.get_untracked() else {
            return;
        };
        let change = match patch(&base.get_untracked(), &draft.get_untracked(), &state) {
            Ok(c) => c,
            Err(e) => {
                message.set(e.into());
                return;
            }
        };
        if change == PageSetupChange::default() {
            message.set("There are no changes to apply".into());
            return;
        }
        if pending.get_untracked() {
            return;
        }
        pending.set(true);
        let request = epoch.get_value();
        controller.after_input(move ||{
            if controller.generation.get_untracked()!=generation||controller.sheet.get_untracked().map(wire::Sheet::of)!=Some(sheet)||controller.summary.get_untracked().revision!=state.revision {pending.set(false);message.set("The workbook changed; cancel and reopen Page Setup".into());return;}
            controller.dispatch(wire::Intent::PageSetup{sheet,revision:state.revision,change},move |outcome|{if epoch.try_get_value()!=Some(request)||controller.generation.get_untracked()!=generation||controller.sheet.get_untracked().map(wire::Sheet::of)!=Some(sheet){return;}pending.set(false);if outcome.accepted{open.set(false);controller.say("Page setup updated",false);controller.focus_active_skin();}else{message.set("Page setup was refused; see the action message. Your draft is retained.".into());}});
        });
    };
    view!{<dialog class="dc-page-dialog" data-dnacalc="page-setup-dialog" node_ref=dialog aria-labelledby="dc-page-setup-title" on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();cancel();}>
        <form on:submit=move |ev|{ev.prevent_default();apply();}>
        <h2 id="dc-page-setup-title">"Page Setup"</h2>
        <fieldset class="dc-page-all" disabled=move ||source.get().is_none()||pending.get()>
        <fieldset><legend>"Page"</legend><div class="dc-page-fields">{ATTRIBUTES[..9].iter().map(|(k,l,_)|attribute(controller,k,l)).collect_view()}</div></fieldset>
        <details open><summary>"Margins"</summary><div class="dc-page-fields">{ATTRIBUTES[9..17].iter().map(|(k,l,_)|attribute(controller,k,l)).collect_view()}</div></details>
        <details data-dnacalc="page-headers"><summary>"Headers and footers"</summary><p>"Use &L, &C and &R for left, centre and right; &P for page, &N for total pages, &A for sheet and &F for file. Use && for an ampersand."</p><div class="dc-page-fields">{HEADERS.iter().map(|(key,label)|input(controller,match *key {"oddHeader"=>"header.oddHeader","oddFooter"=>"header.oddFooter","evenHeader"=>"header.evenHeader","evenFooter"=>"header.evenFooter","firstHeader"=>"header.firstHeader",_=>"header.firstFooter"},label)).collect_view()}{ATTRIBUTES[17..21].iter().map(|(k,l,_)|attribute(controller,k,l)).collect_view()}</div></details>
        <details open><summary>"Sheet"</summary><div class="dc-page-fields">{input(controller,"print_area","Print area (comma-separated ranges)")}{input(controller,"print_titles","Rows / columns to repeat (for example 1:2,A:B)")}{input(controller,"row_breaks","Break before rows (for example 21,41)")}{input(controller,"column_breaks","Break before columns (numbers; E is 5)")}{ATTRIBUTES[21..].iter().map(|(k,l,_)|attribute(controller,k,l)).collect_view()}</div>
        <button type="button" data-dnacalc="page-use-selection" on:click=move |_|{let areas=controller.areas.get_untracked();let text=areas.iter().map(|a|format!("{}{}:{}{}",dna_formula_ui::display::column_letters(a.first().column()),a.first().row(),dna_formula_ui::display::column_letters(a.last().column()),a.last().row())).collect::<Vec<_>>().join(",");draft.update(|d|{d.insert("print_area".into(),text);});}>"Use current selection as print area"</button></details>
        </fieldset>
        <p role="status" data-dnacalc="page-setup-message">{move ||message.get()}</p>
        <div class="dc-dialog-actions"><button type="submit" data-dnacalc="page-setup-apply" disabled=move ||source.get().is_none()||pending.get()>"Apply"</button><button type="button" data-dnacalc="page-setup-cancel" on:click=move |_|cancel()>"Cancel"</button></div>
        </form></dialog>}.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn editing_break_numbers_retains_the_imported_spans_and_pivot_flags() {
        let imported = PageBreak {
            id: 20,
            min: 5,
            max: 10,
            manual: true,
            pivot: true,
        };
        let mut state = PageSetupState {
            revision: 7,
            settings: dna_calc_model::printing::PageSettings {
                row_breaks: vec![imported],
                ..Default::default()
            },
            print_area: String::new(),
            print_titles: String::new(),
        };
        state.settings.sections.insert(
            "printOptions".into(),
            BTreeMap::from([
                ("gridLines".into(), "1".into()),
                ("gridLinesSet".into(), "0".into()),
            ]),
        );
        let base = fields(&state);
        assert_eq!(base["printOptions.gridLines"], "0");
        let mut draft = base.clone();
        draft.insert("row_breaks".into(), "21,41".into());
        let changed = patch(&base, &draft, &state).unwrap();
        let breaks = changed.row_breaks.unwrap();
        assert_eq!(breaks[0], imported);
        assert_eq!(
            breaks[1],
            PageBreak {
                id: 40,
                min: 0,
                max: 16383,
                manual: true,
                pivot: false
            }
        );
        assert!(changed.attributes.is_empty());
        assert!(changed.headers.is_empty());
        draft.insert("printOptions.gridLines".into(), "1".into());
        let flags = patch(&base, &draft, &state).unwrap();
        assert!(flags.attributes.contains(&(
            "printOptions".into(),
            "gridLinesSet".into(),
            Some("1".into())
        )));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn choosing_standard_paper_removes_the_overriding_custom_dimensions() {
        let mut state = PageSetupState {
            revision: 1,
            settings: Default::default(),
            print_area: String::new(),
            print_titles: String::new(),
        };
        state.settings.sections.insert(
            "pageSetup".into(),
            BTreeMap::from([
                ("paperSize".into(), "256".into()),
                ("paperWidth".into(), "8in".into()),
                ("paperHeight".into(), "10in".into()),
            ]),
        );
        let base = fields(&state);
        assert_eq!(
            patch(&base, &base, &state).unwrap(),
            PageSetupChange::default()
        );
        let mut draft = base.clone();
        draft.insert("pageSetup.paperSize".into(), "9".into());
        let changed = patch(&base, &draft, &state).unwrap();
        assert!(
            changed
                .attributes
                .contains(&("pageSetup".into(), "paperWidth".into(), None))
        );
        assert!(
            changed
                .attributes
                .contains(&("pageSetup".into(), "paperHeight".into(), None))
        );
        assert!(changed.attributes.contains(&(
            "pageSetup".into(),
            "paperSize".into(),
            Some("9".into())
        )));
    }
}
