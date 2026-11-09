//! The result panel's parts: the array result that scrolls (sticky headers, only the rows in view
//! drawn), the Effective Display panel, the Formatting panel and the Conditional Formatting panel.
//! They hold only what the person typed ([`CellStyle`], [`CfRuleSpec`]) and draw what the session
//! said to show ([`Shown`]); nothing here calculates.

use crate::cf::{CellStyle, CfRuleSpec, LookWire};
use crate::shown::{Shown, ViewWire};
use leptos::prelude::*;

/// Applies the declarations the views carry in `data-css` through the CSS object model. The page's
/// content policy refuses inline style attributes, which a `style=` attribute would be; the object
/// model's changes are allowed, as the grid's skins make them.
pub fn apply_data_css(root: &web_sys::Element) {
    use wasm_bindgen::JsCast;
    let Ok(found) = root.query_selector_all("[data-css]") else {
        return;
    };
    for index in 0..found.length() {
        let Some(node) = found.item(index) else {
            continue;
        };
        let Ok(element) = node.dyn_into::<web_sys::HtmlElement>() else {
            continue;
        };
        let css = element.get_attribute("data-css").unwrap_or_default();
        let style = element.style();
        for declaration in css.split(';') {
            if let Some((name, value)) = declaration.split_once(':') {
                let _ = style.set_property(name.trim(), value.trim());
            }
        }
    }
}

/// [`apply_data_css`] once the views that changed in this update have been drawn: an effect runs
/// before the views its signals also drive, so the declarations are applied a tick later too.
pub fn apply_data_css_soon(root: web_sys::Element) {
    apply_data_css(&root);
    leptos::task::spawn_local(async move {
        leptos::task::tick().await;
        apply_data_css(&root);
    });
}

/// A column's name: A to Z, then AA.
fn column_name(column: u32) -> String {
    calc_doc::look::a1(1, column).trim_end_matches(char::is_numeric).to_owned()
}

/// The height of a result row, in CSS pixels. The scroll panel positions rows by it.
const ROW_PX: f64 = 24.0;

/// Rows drawn beyond the ones in view, above and below.
const OVERSCAN_ROWS: usize = 12;

/// Rows assumed in view before the panel has been measured.
const DEFAULT_VISIBLE_ROWS: usize = 24;

/// The role attributes of a look's cell, so that checks and screen readers can read it.
fn look_attributes(look: &Option<LookWire>) -> (String, String, String, String) {
    match look {
        None => (String::new(), String::new(), String::new(), String::new()),
        Some(look) => (
            look.fill.clone(),
            look.color.clone(),
            look.bar.map(|b| format!("{:.0}", b * 100.0)).unwrap_or_default(),
            look.icon_index.map(|i| format!("{}:{}", look.icon_set, i + 1)).unwrap_or_default(),
        ),
    }
}

/// One cell's content: the icon an icon set gave it, then its text.
fn cell_content(text: String, look: &Option<LookWire>) -> impl IntoView {
    let icon = look.as_ref().and_then(|look| {
        look.icon_index.map(|_| {
            let style = format!("color:{}", look.icon_color);
            view! { <span class="oc-icon" data-onecalc="cell-icon" aria-hidden="true" data-css=style>{look.icon_glyph.clone()}</span> }
        })
    });
    view! { {icon} {text} }
}

/// The array result: a panel that scrolls both ways inside the result panel, its column and row
/// headers fixed, drawing only the rows in view. The panel stays mounted while the result changes,
/// so editing the formula does not move what the person was reading.
pub fn array_panel(shown: RwSignal<Shown>, kept: RwSignal<(i32, i32)>) -> impl IntoView {
    let first_row = RwSignal::new((kept.get_untracked().0.max(0) as f64 / ROW_PX).floor() as usize);
    let visible_rows = RwSignal::new(DEFAULT_VISIBLE_ROWS);
    let scroller = NodeRef::<leptos::html::Div>::new();
    // A panel that comes back (the formula was briefly not a value) returns to where the person was.
    Effect::new(move |placed: Option<bool>| {
        if placed == Some(true) {
            return true;
        }
        let Some(panel) = scroller.get() else {
            return false;
        };
        let (top, left) = kept.get_untracked();
        panel.set_scroll_top(top);
        panel.set_scroll_left(left);
        true
    });
    let on_scroll = move |_| {
        if let Some(panel) = scroller.get_untracked() {
            kept.set((panel.scroll_top(), panel.scroll_left()));
            let row = (f64::from(panel.scroll_top()) / ROW_PX).floor().max(0.0) as usize;
            let visible = (f64::from(panel.client_height()) / ROW_PX).ceil().max(1.0) as usize;
            // Only a change of the first row draws again: the browser scrolls the pixels itself.
            if first_row.get_untracked() != row {
                first_row.set(row);
            }
            if visible_rows.get_untracked() != visible {
                visible_rows.set(visible);
            }
        }
    };
    // The cells' looks are applied after each draw, through the CSS object model.
    Effect::new(move |_| {
        shown.track();
        first_row.track();
        visible_rows.track();
        if let Some(panel) = scroller.get() {
            apply_data_css_soon(panel.into());
        }
    });
    let caption = move || {
        shown.with(|shown| match &shown.view {
            ViewWire::Array { total_rows, total_cols, truncated, cells } => format!(
                "{total_rows} x {total_cols} array{}",
                if *truncated {
                    format!(
                        ", first {} rows by {} columns shown",
                        cells.len(),
                        cells.first().map_or(0, Vec::len)
                    )
                } else {
                    String::new()
                }
            ),
            _ => String::new(),
        })
    };
    let head = move || {
        shown.with(|shown| match &shown.view {
            ViewWire::Array { cells, .. } => (1..=cells.first().map_or(0, Vec::len) as u32)
                .map(|column| view! { <th scope="col">{column_name(column)}</th> })
                .collect_view()
                .into_any(),
            _ => ().into_any(),
        })
    };
    let body = move || {
        let first = first_row.get();
        let visible = visible_rows.get();
        shown.with(|shown| {
            let ViewWire::Array { cells, .. } = &shown.view else {
                return ().into_any();
            };
            let total = cells.len();
            let cols = cells.first().map_or(0, Vec::len);
            let start = first.saturating_sub(OVERSCAN_ROWS).min(total);
            let end = (first + visible + OVERSCAN_ROWS).min(total);
            let css = &shown.style_css;
            let spacer = |rows: usize| {
                (rows > 0).then(|| {
                    view! {
                        <tr class="oc-spacer" aria-hidden="true" data-css=format!("height:{}px", rows as f64 * ROW_PX)>
                            <td colspan=(cols + 1).to_string()></td>
                        </tr>
                    }
                })
            };
            let rows = (start..end)
                .map(|r| {
                    let row = &cells[r];
                    let tds = row
                        .iter()
                        .map(|cell| {
                            let (fill, color, bar, icon) = look_attributes(&cell.look);
                            let style = format!(
                                "{css}{}",
                                cell.look.as_ref().map(LookWire::css).unwrap_or_default()
                            );
                            view! {
                                <td data-kind=cell.kind.clone() title=cell.kind.clone() data-css=style
                                    data-fill=fill data-color=color data-bar=bar data-icon=icon>
                                    {cell_content(cell.text.clone(), &cell.look)}
                                </td>
                            }
                        })
                        .collect_view();
                    view! { <tr><th scope="row">{r + 1}</th>{tds}</tr> }
                })
                .collect_view();
            view! {
                {spacer(start)}
                {rows}
                {spacer(total - end)}
            }
            .into_any()
        })
    };
    view! {
        <div>
            <span class="oc-kind" data-onecalc="result-type">"array"</span>
            <span data-onecalc="result-shape">{caption}</span>
            <div class="oc-array-scroll" data-onecalc="result-scroll" node_ref=scroller on:scroll=on_scroll tabindex="0"
                role="region" aria-label="Array result, scrolls">
                <table class="oc-array" data-onecalc="result-array" role="grid">
                    <thead><tr><th class="oc-corner" scope="col"></th>{head}</tr></thead>
                    <tbody>{body}</tbody>
                </table>
            </div>
        </div>
    }
}

/// The Effective Display panel: the result as it is drawn, in words.
pub fn effective_panel(shown: RwSignal<Shown>) -> impl IntoView {
    view! {
        <details class="oc-effective" data-onecalc="effective-controls">
            <summary data-onecalc="effective-tools">"Effective display"</summary>
            <ul data-onecalc="effective-display">
                {move || shown.with(|s| s.effective.clone()).into_iter().map(|line| view! { <li>{line}</li> }).collect_view()}
            </ul>
        </details>
    }
}

fn text_input(
    role: &'static str,
    label: &'static str,
    placeholder: &'static str,
    list: &'static str,
    read: impl Fn() -> String + Send + Sync + 'static,
    write: impl Fn(String) + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <label>{label}
            <input type="text" data-onecalc=role placeholder=placeholder list=list autocomplete="off"
                prop:value=read on:input=move |ev| write(event_target_value(&ev))/>
        </label>
    }
}

fn number_input(
    role: &'static str,
    label: &'static str,
    read: impl Fn() -> u32 + Send + Sync + 'static,
    write: impl Fn(u32) + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <label>{label}
            <input type="number" min="0" max="1000" data-onecalc=role
                prop:value=move || { let n = read(); if n == 0 { String::new() } else { n.to_string() } }
                on:input=move |ev| write(event_target_value(&ev).trim().parse().unwrap_or(0))/>
        </label>
    }
}

fn check_input(
    role: &'static str,
    label: &'static str,
    read: impl Fn() -> bool + Send + Sync + 'static,
    write: impl Fn(bool) + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <label class="oc-check">
            <input type="checkbox" data-onecalc=role prop:checked=read on:change=move |ev| write(event_target_checked(&ev))/>
            {label}
        </label>
    }
}

fn select_input(
    role: &'static str,
    label: &'static str,
    options: &'static [(&'static str, &'static str)],
    read: impl Fn() -> String + Send + Sync + 'static,
    write: impl Fn(String) + Send + Sync + 'static,
) -> impl IntoView {
    let items = options
        .iter()
        .map(|(value, text)| view! { <option value=*value>{*text}</option> })
        .collect_view();
    view! {
        <label>{label}
            <select data-onecalc=role prop:value=read on:change=move |ev| write(event_target_value(&ev))>{items}</select>
        </label>
    }
}

const COLOURS: &str = "oc-colours";

/// The colours the colour boxes suggest.
pub fn colour_list() -> impl IntoView {
    view! {
        <datalist id=COLOURS>
            <option value="#ffc7ce">"light red"</option>
            <option value="#9c0006">"dark red"</option>
            <option value="#ffeb9c">"light yellow"</option>
            <option value="#9c5700">"dark yellow"</option>
            <option value="#c6efce">"light green"</option>
            <option value="#006100">"dark green"</option>
            <option value="#bdd7ee">"light blue"</option>
            <option value="#000000">"black"</option>
            <option value="#ffffff">"white"</option>
        </datalist>
    }
}

/// The Formatting panel: font, fill, borders and alignment of the result.
pub fn formatting_panel(style: RwSignal<CellStyle>) -> impl IntoView {
    macro_rules! text {
        ($field:ident) => {
            (move || style.with(|s| s.$field.clone()), move |v: String| style.update(|s| s.$field = v))
        };
    }
    macro_rules! flag {
        ($field:ident) => {
            (move || style.with(|s| s.$field), move |v: bool| style.update(|s| s.$field = v))
        };
    }
    let (family, set_family) = text!(font_family);
    let (text_color, set_text_color) = text!(text_color);
    let (fill, set_fill) = text!(fill);
    let (border_color, set_border_color) = text!(border_color);
    let (bold, set_bold) = flag!(bold);
    let (italic, set_italic) = flag!(italic);
    let (underline, set_underline) = flag!(underline);
    let (strike, set_strike) = flag!(strike);
    let (wrap, set_wrap) = flag!(wrap);
    let (top, set_top) = flag!(border_top);
    let (bottom, set_bottom) = flag!(border_bottom);
    let (left, set_left) = flag!(border_left);
    let (right, set_right) = flag!(border_right);
    view! {
        <details class="oc-format-controls oc-formatting" data-onecalc="formatting-panel">
            <summary data-onecalc="formatting-tools">"Formatting"</summary>
            <p class="oc-prov">"How the result is drawn: font, fill, borders and alignment, as the result's effective display. The number format is under Format & locale."</p>
            <div class="oc-fields">
                {text_input("style-font", "Font", "default", "oc-fonts", family, set_family)}
                {number_input("style-size", "Size (pt)", move || style.with(|s| s.font_size), move |n| style.update(|s| s.font_size = n))}
                {check_input("style-bold", "Bold", bold, set_bold)}
                {check_input("style-italic", "Italic", italic, set_italic)}
                {check_input("style-underline", "Underline", underline, set_underline)}
                {check_input("style-strike", "Strikethrough", strike, set_strike)}
                {text_input("style-color", "Text colour", "#rrggbb", COLOURS, text_color, set_text_color)}
                {text_input("style-fill", "Fill", "#rrggbb", COLOURS, fill, set_fill)}
                {select_input("style-align", "Alignment", &[("", "General"), ("left", "Left"), ("center", "Centre"), ("right", "Right")],
                    move || style.with(|s| s.horizontal.clone()), move |v| style.update(|s| s.horizontal = v))}
                {check_input("style-wrap", "Wrap text", wrap, set_wrap)}
                {check_input("style-border-top", "Border top", top, set_top)}
                {check_input("style-border-bottom", "Border bottom", bottom, set_bottom)}
                {check_input("style-border-left", "Border left", left, set_left)}
                {check_input("style-border-right", "Border right", right, set_right)}
                {text_input("style-border-color", "Border colour", "#000000", COLOURS, border_color, set_border_color)}
            </div>
            <datalist id="oc-fonts">
                <option value="Arial"/><option value="Calibri"/><option value="Cambria"/><option value="Consolas"/>
                <option value="Courier New"/><option value="Georgia"/><option value="Times New Roman"/><option value="Verdana"/>
            </datalist>
            <button type="button" data-onecalc="style-reset" on:click=move |_| style.set(CellStyle::default())>"Reset formatting"</button>
        </details>
    }
}

const KINDS: &[(&str, &str)] = &[
    ("cellIs", "Cell value"),
    ("text", "Text"),
    ("blanks", "Blanks"),
    ("errors", "Errors"),
    ("dates", "Dates"),
    ("expression", "Formula"),
    ("rank", "Top or bottom"),
    ("average", "Above or below average"),
    ("duplicates", "Duplicate values"),
    ("uniques", "Unique values"),
    ("colorScale", "Colour scale"),
    ("dataBar", "Data bar"),
    ("iconSet", "Icon set"),
];

const COMPARISONS: &[(&str, &str)] = &[
    ("greaterThan", "greater than"),
    ("lessThan", "less than"),
    ("greaterThanOrEqual", "greater than or equal to"),
    ("lessThanOrEqual", "less than or equal to"),
    ("equal", "equal to"),
    ("notEqual", "not equal to"),
    ("between", "between"),
    ("notBetween", "not between"),
];

const TEXT_TESTS: &[(&str, &str)] = &[
    ("containsText", "contains"),
    ("notContainsText", "does not contain"),
    ("beginsWith", "begins with"),
    ("endsWith", "ends with"),
];

const PERIODS: &[(&str, &str)] = &[
    ("today", "today"),
    ("yesterday", "yesterday"),
    ("tomorrow", "tomorrow"),
    ("last7Days", "in the last 7 days"),
    ("thisWeek", "this week"),
    ("lastWeek", "last week"),
    ("nextWeek", "next week"),
    ("thisMonth", "this month"),
    ("lastMonth", "last month"),
    ("nextMonth", "next month"),
];

const ICON_SETS: &[(&str, &str)] = &[
    ("3Arrows", "3 arrows"),
    ("3ArrowsGray", "3 grey arrows"),
    ("3Flags", "3 flags"),
    ("3TrafficLights1", "3 traffic lights"),
    ("3Signs", "3 signs"),
    ("3Symbols", "3 symbols"),
    ("4Arrows", "4 arrows"),
    ("4Rating", "4 ratings"),
    ("4TrafficLights", "4 traffic lights"),
    ("5Arrows", "5 arrows"),
    ("5Quarters", "5 quarters"),
    ("5Rating", "5 ratings"),
];

/// The Conditional Formatting panel: the rules in priority order, each applied to the result cell or
/// to every cell of an array result.
pub fn cf_panel(rules: RwSignal<Vec<CfRuleSpec>>) -> impl IntoView {
    let adding = RwSignal::new("cellIs".to_owned());
    let count = move || rules.with(Vec::len);
    let add = move |_| {
        let kind = adding.get_untracked();
        rules.update(|rules| rules.push(CfRuleSpec::new(&kind)));
    };
    let indexes = move || (0..count()).collect::<Vec<_>>();
    view! {
        <details class="oc-format-controls oc-cf" data-onecalc="cf-panel">
            <summary data-onecalc="cf-tools">"Conditional formatting · "<span data-onecalc="cf-count">{move || format!("{} rule(s)", count())}</span></summary>
            <p class="oc-prov">"Rules are applied to the result as Excel applies them to a selected range: a single value is a range of one cell, an array result a range of its own cells, so colour scales, data bars, icon sets, top and bottom, average and duplicate rules read the whole array. Earlier rules win; Stop if true ends the rules for a cell. The rules are calc-doc's, the code DNA Calc paints a sheet with."</p>
            <div class="oc-cf-add">
                <label>"New rule"
                    <select data-onecalc="cf-new-kind" on:change=move |ev| adding.set(event_target_value(&ev))>
                        {KINDS.iter().map(|(value, text)| view! { <option value=*value>{*text}</option> }).collect_view()}
                    </select>
                </label>
                <button type="button" data-onecalc="cf-add" on:click=add>"Add rule"</button>
                <button type="button" data-onecalc="cf-clear" on:click=move |_| rules.set(Vec::new())>"Clear rules"</button>
            </div>
            <ol class="oc-cf-rules" data-onecalc="cf-rules">
                <For each=indexes key=|i| *i let:i>{cf_rule(rules, i)}</For>
            </ol>
        </details>
    }
}

fn cf_rule(rules: RwSignal<Vec<CfRuleSpec>>, i: usize) -> impl IntoView {
    macro_rules! text {
        ($field:ident) => {
            (
                move || rules.with(|r| r.get(i).map(|x| x.$field.clone()).unwrap_or_default()),
                move |v: String| {
                    rules.update(|r| {
                        if let Some(x) = r.get_mut(i) {
                            x.$field = v;
                        }
                    })
                },
            )
        };
    }
    macro_rules! flag {
        ($field:ident) => {
            (
                move || rules.with(|r| r.get(i).map(|x| x.$field).unwrap_or_default()),
                move |v: bool| {
                    rules.update(|r| {
                        if let Some(x) = r.get_mut(i) {
                            x.$field = v;
                        }
                    })
                },
            )
        };
    }
    let kind = Memo::new(move |_| rules.with(|r| r.get(i).map(|x| x.kind.clone()).unwrap_or_default()));
    let operator = Memo::new(move |_| rules.with(|r| r.get(i).map(|x| x.operator.clone()).unwrap_or_default()));
    let title = move || rules.with(|r| r.get(i).map(|x| format!("{}. {}", i + 1, x.title())).unwrap_or_default());
    let change_kind = move |value: String| {
        rules.update(|r| {
            if let Some(x) = r.get_mut(i) {
                *x = CfRuleSpec::new(&value);
            }
        })
    };
    let (operand1, set_operand1) = text!(operand1);
    let (operand2, set_operand2) = text!(operand2);
    let (fill, set_fill) = text!(fill);
    let (font_color, set_font_color) = text!(font_color);
    let (number_format, set_number_format) = text!(number_format);
    let (low, set_low) = text!(low);
    let (mid, set_mid) = text!(mid);
    let (high, set_high) = text!(high);
    let (bar_color, set_bar_color) = text!(bar_color);
    let (bold, set_bold) = flag!(bold);
    let (italic, set_italic) = flag!(italic);
    let (underline, set_underline) = flag!(underline);
    let (strike, set_strike) = flag!(strike);
    let (stop, set_stop) = flag!(stop);
    let (percent, set_percent) = flag!(percent);
    let (equal, set_equal) = flag!(equal);
    let (gradient, set_gradient) = flag!(gradient);
    let (reverse, set_reverse) = flag!(reverse);
    let set_operator = move |value: String| {
        rules.update(|r| {
            if let Some(x) = r.get_mut(i) {
                x.operator = value;
            }
        })
    };
    let set_icons = move |value: String| {
        rules.update(|r| {
            if let Some(x) = r.get_mut(i) {
                x.icon_set = value;
            }
        })
    };
    let operator_value = move || operator.get();
    let icon_value = move || rules.with(|r| r.get(i).map(|x| x.icon_set.clone()).unwrap_or_default());
    let move_rule = move |by: isize| {
        rules.update(|r| {
            let to = i as isize + by;
            if to >= 0 && (to as usize) < r.len() {
                r.swap(i, to as usize);
            }
        })
    };
    let remove = move |_| {
        rules.update(|r| {
            if i < r.len() {
                r.remove(i);
            }
        })
    };
    let rank = move || rules.with(|r| r.get(i).map(|x| x.rank).unwrap_or(10));
    let set_rank = move |n: u32| {
        rules.update(|r| {
            if let Some(x) = r.get_mut(i) {
                x.rank = n;
            }
        })
    };
    let condition = move || {
        let kind = kind.get();
        let fields = match kind.as_str() {
            "cellIs" => view! {
                {select_input("cf-operator", "Cell value is", COMPARISONS, operator_value, set_operator)}
                {text_input("cf-operand1", "Value or formula", "e.g. 5, \"text\" or A1", "", operand1, set_operand1)}
                <div hidden=move || !matches!(operator.get().as_str(), "between" | "notBetween")>
                    {text_input("cf-operand2", "and", "second value", "", operand2, set_operand2)}
                </div>
            }
            .into_any(),
            "text" => view! {
                {select_input("cf-operator", "Cell text", TEXT_TESTS, operator_value, set_operator)}
                {text_input("cf-operand1", "Text", "text to look for", "", operand1, set_operand1)}
            }
            .into_any(),
            "blanks" => select_input("cf-operator", "Cell", &[("blanks", "is blank"), ("noBlanks", "is not blank")], operator_value, set_operator).into_any(),
            "errors" => select_input("cf-operator", "Cell", &[("errors", "has an error"), ("noErrors", "has no error")], operator_value, set_operator).into_any(),
            "dates" => select_input("cf-operator", "Date is", PERIODS, operator_value, set_operator).into_any(),
            "expression" => text_input("cf-operand1", "Formula for the cell (A1 is the cell itself)", "e.g. A1>5", "", operand1, set_operand1).into_any(),
            "rank" => view! {
                {select_input("cf-operator", "Show", &[("top", "top"), ("bottom", "bottom")], operator_value, set_operator)}
                {number_input("cf-rank", "Count", rank, set_rank)}
                {check_input("cf-percent", "Percent", percent, set_percent)}
            }
            .into_any(),
            "average" => view! {
                {select_input("cf-operator", "Values", &[("above", "above average"), ("below", "below average")], operator_value, set_operator)}
                {check_input("cf-equal", "Or equal", equal, set_equal)}
            }
            .into_any(),
            "colorScale" => view! {
                {text_input("cf-low", "Lowest value colour", "#rrggbb", COLOURS, low, set_low)}
                {text_input("cf-mid", "Middle colour (empty: two-colour)", "#rrggbb", COLOURS, mid, set_mid)}
                {text_input("cf-high", "Highest value colour", "#rrggbb", COLOURS, high, set_high)}
            }
            .into_any(),
            "dataBar" => view! {
                {text_input("cf-bar-color", "Bar colour", "#rrggbb", COLOURS, bar_color, set_bar_color)}
                {check_input("cf-gradient", "Gradient fill", gradient, set_gradient)}
            }
            .into_any(),
            "iconSet" => view! {
                {select_input("cf-icon-set", "Icons", ICON_SETS, icon_value, set_icons)}
                {check_input("cf-reverse", "Reverse order", reverse, set_reverse)}
            }
            .into_any(),
            _ => ().into_any(),
        };
        let appearance = (!matches!(kind.as_str(), "colorScale" | "dataBar" | "iconSet")).then(|| {
            view! {
                <div class="oc-fields">
                    {text_input("cf-fill", "Fill", "#rrggbb", COLOURS, fill, set_fill)}
                    {text_input("cf-font-color", "Text colour", "#rrggbb", COLOURS, font_color, set_font_color)}
                    {check_input("cf-bold", "Bold", bold, set_bold)}
                    {check_input("cf-italic", "Italic", italic, set_italic)}
                    {check_input("cf-underline", "Underline", underline, set_underline)}
                    {check_input("cf-strike", "Strikethrough", strike, set_strike)}
                    {text_input("cf-number-format", "Number format", "e.g. 0.0%", "", number_format, set_number_format)}
                </div>
            }
        });
        view! { <div class="oc-fields">{fields}</div>{appearance} }
    };
    view! {
        <li class="oc-cf-rule" data-onecalc="cf-rule">
            <header>
                <strong data-onecalc="cf-rule-title">{title}</strong>
                <span>
                    <button type="button" data-onecalc="cf-up" aria-label="Move rule up" on:click=move |_| move_rule(-1)>"Up"</button>
                    <button type="button" data-onecalc="cf-down" aria-label="Move rule down" on:click=move |_| move_rule(1)>"Down"</button>
                    <button type="button" data-onecalc="cf-remove" aria-label="Remove rule" on:click=remove>"Remove"</button>
                </span>
            </header>
            {select_input("cf-kind", "Rule type", KINDS, move || kind.get(), change_kind)}
            {condition}
            {check_input("cf-stop", "Stop if true", stop, set_stop)}
        </li>
    }
}
