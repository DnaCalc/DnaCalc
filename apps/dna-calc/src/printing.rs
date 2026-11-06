//! The browser adapter renders one pagination plan with the existing grid plan and Canvas2D painter.
use crate::{controller::Controller, style::css, wire};
use dna_calc_model::{
    geometry::{Viewport, sheet_metrics},
    printing::*,
    render_plan::{PlanOptions, build_render_plan_with},
};
use leptos::prelude::*;
use std::sync::Arc;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
fn address(area: wire::Area) -> String {
    format!(
        "{}{}:{}{}",
        dna_formula_ui::display::column_letters(area.0.3),
        area.0.2,
        dna_formula_ui::display::column_letters(area.1.3),
        area.1.2
    )
}
fn drawing_budget(document: &PrintDocument, dpr: f64) -> Result<(), String> {
    let mut total = 0.;
    let dpr = dpr.max(1.);
    for r in document.plan.pages.iter().flat_map(|p| &p.regions) {
        let w = (r.width * dpr).ceil();
        let h = (r.height * dpr).ceil();
        total += w * h;
        if !w.is_finite()
            || !h.is_finite()
            || w > 16_384.
            || h > 16_384.
            || w * h > 16_000_000.
            || total > 32_000_000.
        {
            return Err("Print drawings exceed this display's pixel budget; reduce the print area or increase the scale (dc-971p.57)".into());
        }
    }
    Ok(())
}
#[component]
fn Region(
    document: Arc<PrintDocument>,
    region: PrintRegion,
    fonts: u64,
    error: RwSignal<String>,
) -> impl IntoView {
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    let mut metrics = sheet_metrics(Some(&document.geometry));
    metrics.header_w = 0.;
    metrics.header_h = 0.;
    let viewport = Viewport {
        scroll_x: metrics.col_start(region.area.0.3),
        scroll_y: metrics.row_start(region.area.0.2),
        width: region.width,
        height: region.height,
    };
    let mut shown = document
        .cells
        .iter()
        .filter_map(|c| c.shown())
        .collect::<Vec<_>>();
    let errors = document
        .settings
        .get("pageSetup", "errors")
        .unwrap_or("displayed");
    for c in &mut shown {
        if c.kind == "error" {
            c.text = match errors {
                "blank" => "".into(),
                "dash" => "--".into(),
                "NA" => "#N/A".into(),
                _ => c.text.clone(),
            };
        }
    }
    let measure = |c: &wire::CellShown, t: &str| crate::grid::print_advance(c, t, fonts);
    let plan = Arc::new(build_render_plan_with(
        shown.iter(),
        &metrics,
        &viewport,
        1_048_576,
        16_384,
        &PlanOptions {
            geometry: Some(&document.geometry),
            measure: Some(&measure),
        },
    ));
    let held_plan = Arc::clone(&plan);
    let held_metrics = metrics.clone();
    let drawing_document = Arc::clone(&document);
    Effect::new(move |_| {
        if let Some(node) = canvas.get() {
            if let Err(e) = drawing_budget(&drawing_document, window().device_pixel_ratio()) {
                error.set(e);
                return;
            }
            crate::grid::paint_print(&node, &held_plan, &held_metrics);
        }
    });
    let gridlines = document.settings.print_gridlines();
    let black = document.settings.flag("pageSetup", "blackAndWhite", false);
    let style = format!(
        "left:{}px;top:{}px;width:{}px;height:{}px;--dc-gridline:{}{}",
        region.x,
        region.y,
        region.width,
        region.height,
        if gridlines { "#b6b6b6" } else { "transparent" },
        if black { ";filter:grayscale(1)" } else { "" }
    );
    let sample = shown
        .iter()
        .filter(|c| {
            !c.text.is_empty()
                && c.at.row() >= region.area.0.2
                && c.at.row() <= region.area.1.2
                && c.at.column() >= region.area.0.3
                && c.at.column() <= region.area.1.3
        })
        .take(8)
        .map(|c| {
            format!(
                "{}{}: {}",
                dna_formula_ui::display::column_letters(c.at.column()),
                c.at.row(),
                c.text.chars().take(80).collect::<String>()
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let chart_views=document.charts.iter().filter_map(|chart|{
        let (x,y,width,height)=match &chart.place {wire::ChartPlace::Cells{from,to}=>{let x=metrics.col_start(from.0.3)+from.1;let y=metrics.row_start(from.0.2)+from.2;(x,y,metrics.col_start(to.0.3)+to.1-x,metrics.row_start(to.0.2)+to.2-y)},wire::ChartPlace::Sized{from,width,height}=>(metrics.col_start(from.0.3)+from.1,metrics.row_start(from.0.2)+from.2,*width,*height),wire::ChartPlace::Absolute{x,y,width,height}=>(*x,*y,*width,*height)};
        let x=x-viewport.scroll_x;let y=y-viewport.scroll_y;if x+width<=0.||y+height<=0.||x>=region.width||y>=region.height{return None;}
        let style=format!("position:absolute;left:{x}px;top:{y}px;width:{width}px;height:{height}px");Some(view!{<div role="img" aria-label=chart.name.clone() use:css=style inner_html=chart.svg.clone()></div>})
    }).collect_view();
    view! {<div class="dc-print-region" data-dnacalc="print-region" data-range=address(region.area) use:css=style><canvas node_ref=canvas role="img" aria-label=format!("Worksheet cells {}. First shown values: {sample}",address(region.area)) use:css=format!("width:{}px;height:{}px",region.width,region.height)></canvas>{chart_views}</div>}
}
fn header_view(
    document: &PrintDocument,
    page: &PrintPage,
    index: usize,
    footer: bool,
    date: &str,
    time: &str,
) -> Result<AnyView, String> {
    let context = HeaderContext {
        page: page.number,
        pages: document.plan.pages.len(),
        sheet: &document.sheet_name,
        book: &document.book_name,
        date,
        time,
    };
    let runs = header_runs(
        page_header(&document.settings, index, page.number, footer),
        &context,
    )?;
    let factor = if document.settings.flag("headerFooter", "scaleWithDoc", true) {
        document.plan.scale
    } else {
        1.
    };
    let margin = if document
        .settings
        .flag("headerFooter", "alignWithMargins", true)
    {
        [document.plan.margins[0], document.plan.margins[1]]
    } else {
        [0.7 * 96.; 2]
    };
    let style = format!(
        "left:{}px;right:{}px;{}:{}px",
        margin[0],
        margin[1],
        if footer { "bottom" } else { "top" },
        document.plan.margins[if footer { 5 } else { 4 }]
    );
    Ok(view!{<div class=if footer{"dc-print-footer"}else{"dc-print-header"} data-dnacalc=if footer{"print-footer"}else{"print-header"} use:css=style>{runs.into_iter().map(|runs|view!{<span>{runs.into_iter().map(|r|{
        let style=format!("font-family:{};font-size:{}pt;font-weight:{};font-style:{};color:{};text-decoration-line:{};text-decoration-style:{};vertical-align:{}",crate::grid::print_family(&r.font),r.points*factor,if r.bold{"700"}else{"400"},if r.italic{"italic"}else{"normal"},r.color,if r.underline>0&&r.strike{"underline line-through"}else if r.underline>0{"underline"}else if r.strike{"line-through"}else{"none"},if r.underline==2{"double"}else{"solid"},if r.script>0{"super"}else if r.script<0{"sub"}else{"baseline"});view!{<span use:css=style>{r.text}</span>}
    }).collect_view()}</span>}).collect_view()}</div>}.into_any())
}
#[component]
fn Paper(
    document: Arc<PrintDocument>,
    page: PrintPage,
    index: usize,
    date: String,
    time: String,
    fonts: u64,
    error: RwSignal<String>,
) -> impl IntoView {
    let style = format!(
        "width:{}px;height:{}px;filter:{}",
        document.plan.paper_width,
        document.plan.paper_height,
        if document.settings.flag("pageSetup", "blackAndWhite", false) {
            "grayscale(1)"
        } else {
            "none"
        }
    );
    let body = format!(
        "left:{}px;top:{}px;width:{}px;height:{}px;transform:scale({})",
        page.offset_x, page.offset_y, page.width, page.height, document.plan.scale
    );
    let header = header_view(&document, &page, index, false, &date, &time)
        .unwrap_or_else(|e| view! {<p role="alert">{e}</p>}.into_any());
    let footer = header_view(&document, &page, index, true, &date, &time)
        .unwrap_or_else(|e| view! {<p role="alert">{e}</p>}.into_any());
    let headings = document.settings.flag("printOptions", "headings", false);
    let m = sheet_metrics(Some(&document.geometry));
    let mut labels = Vec::new();
    if headings {
        for region in &page.regions {
            if region.x == m.header_w {
                for row in region.area.0.2..=region.area.1.2 {
                    let h = m.height(row);
                    if h > 0. {
                        let style = format!(
                            "left:0;top:{}px;width:{}px;height:{h}px;line-height:{h}px",
                            region.y + m.row_start(row) - m.row_start(region.area.0.2),
                            m.header_w
                        );
                        labels.push(
                            view! {<span class="dc-print-heading" use:css=style>{row.to_string()}</span>},
                        );
                    }
                }
            }
            if region.y == m.header_h {
                for col in region.area.0.3..=region.area.1.3 {
                    let w = m.width(col);
                    if w > 0. {
                        let style = format!(
                            "left:{}px;top:0;width:{w}px;height:{}px",
                            region.x + m.col_start(col) - m.col_start(region.area.0.3),
                            m.header_h
                        );
                        labels.push(view!{<span class="dc-print-heading" use:css=style>{dna_formula_ui::display::column_letters(col)}</span>});
                    }
                }
            }
        }
    }
    let regions = page.regions.clone();
    let range = address(page.body);
    view! {<section class="dc-print-page-wrap" data-dnacalc="print-page-wrap"><p class="dc-print-page-label">{format!("Page {}",page.number)}</p><article class="dc-print-page" data-dnacalc="print-page" data-page=page.number.to_string() data-range=range.clone() aria-label=format!("Page {}, {range}",page.number) use:css=style>{header}<div class="dc-print-content" use:css=body>{regions.into_iter().map(|r|view!{<Region document=Arc::clone(&document) region=r fonts=fonts error=error/>}).collect_view()}{labels.collect_view()}</div>{footer}</article><p class="dc-print-range">{range}</p></section>}
}
#[cfg(target_arch = "wasm32")]
fn print_pages(
    root: &web_sys::Element,
    document: &PrintDocument,
    controller: Controller,
) -> Result<(), String> {
    use wasm_bindgen::closure::Closure;
    let display_epoch = controller.revision.get_untracked();
    let calculation_epoch = controller.validation_epoch().get_untracked();
    let generation = controller.generation.get_untracked();
    let printed_sheet = controller.sheet.get_untracked();
    let dom = leptos::prelude::document();
    if let Some(old) = dom.get_element_by_id("dc-print-frame") {
        old.remove();
    }
    let cloned = root
        .clone_node_with_deep(true)
        .map_err(|_| "Cannot copy print pages")?
        .dyn_into::<web_sys::Element>()
        .map_err(|_| "Cannot copy print pages")?;
    let canvases = root
        .query_selector_all("canvas")
        .map_err(|_| "Cannot read page drawings")?;
    let copies = cloned
        .query_selector_all("canvas")
        .map_err(|_| "Cannot copy page drawings")?;
    for i in 0..canvases.length() {
        let canvas = canvases
            .get(i)
            .ok_or("Missing page drawing")?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .map_err(|_| "Invalid page drawing")?;
        let copy = copies.get(i).ok_or("Missing copied drawing")?;
        let img = dom
            .create_element("img")
            .map_err(|_| "Cannot create a print image")?
            .dyn_into::<web_sys::HtmlImageElement>()
            .map_err(|_| "Cannot create a print image")?;
        img.set_src(
            &canvas
                .to_data_url()
                .map_err(|_| "Cannot read a page drawing")?,
        );
        crate::style::css(
            img.clone().unchecked_into(),
            format!(
                "display:block;width:{}px;height:{}px",
                canvas.client_width(),
                canvas.client_height()
            ),
        );
        if let Some(parent) = copy.parent_node() {
            parent
                .replace_child(&img, &copy)
                .map_err(|_| "Cannot install a print image")?;
        }
    }
    // Packed sites place app.css under assets/<id>/; use the stylesheet the page loaded.
    let css_url = dom
        .query_selector("link[rel=stylesheet]")
        .map_err(|_| "Cannot locate print stylesheet")?
        .ok_or("Print stylesheet unavailable")?
        .dyn_into::<web_sys::HtmlLinkElement>()
        .map_err(|_| "Cannot locate print stylesheet")?
        .href();
    let frame = dom
        .create_element("iframe")
        .map_err(|_| "Cannot open browser printing")?
        .dyn_into::<web_sys::HtmlIFrameElement>()
        .map_err(|_| "Cannot open browser printing")?;
    frame.set_id("dc-print-frame");
    frame.set_title("Worksheet print document");
    crate::style::css(
        frame.clone().unchecked_into(),
        "position:fixed;left:-20000px;top:0;width:1200px;height:1200px;border:0".into(),
    );
    let held = frame.clone();
    let paper_width = document.plan.paper_width;
    let paper_height = document.plan.paper_height;
    let loaded = Closure::once_into_js(move || {
        let Some(child) = held.content_document() else {
            controller.say("The browser did not open the print document", true);
            return;
        };
        let Some(body) = child.body() else {
            return;
        };
        body.set_text_content(None);
        if body.append_child(&cloned).is_err() {
            controller.say("The browser could not copy the print pages", true);
            return;
        }
        // Restore dynamic positions through CSSOM, as the application does under its style-src policy.
        if let Ok(nodes) = body.query_selector_all("[style]") {
            for i in 0..nodes.length() {
                if let Some(node) = nodes.get(i) {
                    let node: web_sys::Element = node.unchecked_into();
                    let style = node.get_attribute("style").unwrap_or_default();
                    crate::style::css(node, style);
                }
            }
        }
        let Ok(link) = child.create_element("link") else {
            controller.say("The browser could not create the print stylesheet", true);
            return;
        };
        // Nodes created by the iframe belong to its realm. DOM methods guarantee these
        // types; the parent's instanceof constructors would reject the child's objects.
        let link: web_sys::HtmlLinkElement = link.unchecked_into();
        link.set_rel("stylesheet");
        link.set_href(&css_url);
        let ready = js_sys::Promise::new(&mut |resolve, reject| {
            link.set_onload(Some(&resolve));
            link.set_onerror(Some(&reject));
        });
        if child
            .head()
            .is_none_or(|head| head.append_child(&link).is_err())
        {
            controller.say("The browser could not load print styles", true);
            return;
        }
        wasm_bindgen_futures::spawn_local(async move {
            let result: Result<(), String> = async {
                wasm_bindgen_futures::JsFuture::from(ready)
                    .await
                    .map_err(|_| "The browser could not load print styles")?;
                // This same-origin stylesheet was loaded under the site's style-src policy.
                // Change its CSSOM instead of introducing inline stylesheet text.
                let sheet = link
                    .sheet()
                    .map(|s| s.unchecked_into::<web_sys::CssStyleSheet>())
                    .ok_or("The browser refused the print paper stylesheet")?;
                sheet
                    .insert_rule(&format!(
                        "@page {{size:{paper_width}px {paper_height}px;margin:0}}"
                    ))
                    .map_err(|_| "The browser refused the print paper size")?;
                if let Ok(fonts) = child.fonts().ready() {
                    wasm_bindgen_futures::JsFuture::from(fonts)
                        .await
                        .map_err(|_| "Print fonts did not finish loading")?;
                }
                let images = child
                    .query_selector_all("img")
                    .map_err(|_| "Cannot read print images")?;
                for i in 0..images.length() {
                    if let Some(image) = images.get(i) {
                        let image: web_sys::HtmlImageElement = image.unchecked_into();
                        wasm_bindgen_futures::JsFuture::from(image.decode())
                            .await
                            .map_err(|_| "A page drawing could not be prepared for printing")?;
                    }
                }
                if controller.revision.get_untracked() != display_epoch
                    || controller.validation_epoch().get_untracked() != calculation_epoch
                    || controller.generation.get_untracked() != generation
                    || controller.sheet.get_untracked() != printed_sheet
                {
                    return Err(
                        "Workbook values changed while preparing printing; refresh preview".into(),
                    );
                }
                body.set_attribute("data-dnacalc-print-ready", "true")
                    .map_err(|_| "Cannot prepare print document")?;
                let window = held.content_window().ok_or("Print window unavailable")?;
                window.focus().map_err(|_| "Cannot focus print document")?;
                window.print().map_err(|_| "The browser refused printing")?;
                Ok(())
            }
            .await;
            if let Err(error) = result {
                controller.say(error, true);
            }
        });
    });
    frame.set_onload(Some(loaded.unchecked_ref()));
    dom.body()
        .ok_or("Document unavailable")?
        .append_child(&frame)
        .map_err(|_| "Cannot open browser printing")?;
    Ok(())
}
#[cfg(not(target_arch = "wasm32"))]
fn print_pages(_: &web_sys::Element, _: &PrintDocument, _: Controller) -> Result<(), String> {
    Err("Browser printing is available in the browser application".into())
}
#[component]
fn Preview(controller: Controller, breaks: bool) -> impl IntoView {
    let data = RwSignal::new(None::<Arc<PrintDocument>>);
    let error = RwSignal::new(String::new());
    let pages = NodeRef::<leptos::html::Div>::new();
    let loaded_epoch = RwSignal::new(None::<(u64, u64)>);
    let date = js_sys::Date::new_0()
        .to_locale_date_string("default", &js_sys::Object::new())
        .as_string()
        .unwrap_or_default();
    let time = js_sys::Date::new_0()
        .to_locale_time_string("default")
        .as_string()
        .unwrap_or_default();
    let epoch = StoredValue::new(0u64);
    let refresh = RwSignal::new(0u64);
    Effect::new(move |_| {
        refresh.track();
        let generation = controller.generation.get();
        let sheet = controller.sheet.get();
        let revision = controller.summary.get().revision;
        let display_epoch = controller.revision.get();
        let calculation_epoch = controller.validation_epoch().get();
        epoch.update_value(|e| *e += 1);
        let request = epoch.get_value();
        data.set(None);
        loaded_epoch.set(None);
        error.set("Preparing print preview…".into());
        let Some(sheet) = sheet else {
            return;
        };
        controller.ask_result(
            wire::Request::PrintPreview {
                sheet: wire::Sheet::of(sheet),
                revision,
            },
            move |reply| {
                if epoch.try_get_value() != Some(request)
                    || controller.generation.get_untracked() != generation
                    || controller.sheet.get_untracked() != Some(sheet)
                    || controller.summary.get_untracked().revision != revision
                    || controller.revision.get_untracked() != display_epoch
                    || controller.validation_epoch().get_untracked() != calculation_epoch
                {
                    return;
                }
                match reply {
                    Ok(wire::Reply::PrintPreview(Ok(value))) => {
                        if let Err(e) = drawing_budget(&value, window().device_pixel_ratio()) {
                            error.set(e);
                            return;
                        }
                        let context = HeaderContext {
                            page: 1,
                            pages: value.plan.pages.len(),
                            sheet: &value.sheet_name,
                            book: &value.book_name,
                            date: "",
                            time: "",
                        };
                        if let Some(e) = value.plan.pages.iter().enumerate().find_map(|(i, p)| {
                            [false, true].into_iter().find_map(|footer| {
                                header_runs(
                                    page_header(&value.settings, i, p.number, footer),
                                    &context,
                                )
                                .err()
                            })
                        }) {
                            error.set(e);
                            return;
                        }
                        error.set(String::new());
                        loaded_epoch.set(Some((display_epoch, calculation_epoch)));
                        data.set(Some(Arc::new(value)));
                    }
                    Ok(wire::Reply::PrintPreview(Err(e))) | Err(e) => error.set(e),
                    _ => error.set("Print preview did not answer".into()),
                }
            },
        );
    });
    on_cleanup(move || {
        if let Some(frame) = document().get_element_by_id("dc-print-frame") {
            frame.remove();
        }
    });
    let print = move |_| {
        if breaks {
            controller
                .tool_field(|s| &s.page.preview, |s| &mut s.page.preview)
                .set(true);
            return;
        }
        if let (Some(root), Some(doc)) = (pages.get_untracked(), data.get_untracked()) {
            if controller.summary.get_untracked().revision != doc.revision
                || loaded_epoch.get_untracked()
                    != Some((
                        controller.revision.get_untracked(),
                        controller.validation_epoch().get_untracked(),
                    ))
            {
                error.set("The workbook changed; refresh preview".into());
                return;
            }
            if let Err(e) = drawing_budget(&doc, window().device_pixel_ratio()) {
                error.set(e);
                return;
            }
            match print_pages(root.as_ref(), &doc, controller) {
                Ok(()) => controller.say(
                    "Browser print dialog requested; choose a printer or Save as PDF",
                    false,
                ),
                Err(e) => error.set(e),
            }
        }
    };
    let dates = date.clone();
    let times = time.clone();
    view! {<div class="dc-print-workspace" data-dnacalc="print-workspace" data-print-epoch=move ||loaded_epoch.get().map(|(d,c)|format!("{d}:{c}")) tabindex="0"><div class="dc-print-controls"><button type="button" data-dnacalc="print-browser" disabled=move ||!error.get().is_empty()||data.with(|d|d.as_ref().is_none_or(|d|d.plan.pages.is_empty())) on:click=print>"Print / Save PDF…"</button><button type="button" data-dnacalc="print-refresh" on:click=move |_|refresh.update(|n|*n+=1)>"Refresh"</button><span data-dnacalc="print-page-count">{move ||data.with(|d|d.as_ref().map(|d|format!("{} {} · {:.0}%",d.plan.pages.len(),if d.plan.pages.len()==1 {"page"}else{"pages"},d.plan.scale*100.)).unwrap_or_default())}</span></div><p role="status" data-dnacalc="print-message">{move ||error.get()}</p><p class="dc-print-note">"Switch to Normal view to edit cells. In the browser print dialog, use the matching paper size, no margins, 100% scale, and turn off browser headers and footers."</p>
    {move ||data.get().map(|d|view!{<>{d.notices.iter().cloned().map(|text|view!{<p class="dc-print-note">{text}</p>}).collect_view()}</>})}
    <div class="dc-print-pages" data-dnacalc="print-pages" node_ref=pages>{move ||data.get().map(|d|{
        if breaks {return break_map(Arc::clone(&d),controller.font_revision.get(),error).into_any();}
        let date=dates.clone();let time=times.clone();d.plan.pages.iter().cloned().enumerate().map(|(i,p)|view!{<Paper document=Arc::clone(&d) page=p index=i date=date.clone() time=time.clone() fonts=controller.font_revision.get() error=error/>}).collect_view().into_any()
    })}</div></div>}
}
fn break_map(document: Arc<PrintDocument>, fonts: u64, error: RwSignal<String>) -> AnyView {
    let m = sheet_metrics(Some(&document.geometry));
    let top = document
        .plan
        .pages
        .iter()
        .map(|p| p.body.0.2)
        .min()
        .unwrap_or(1);
    let left = document
        .plan
        .pages
        .iter()
        .map(|p| p.body.0.3)
        .min()
        .unwrap_or(1);
    let right = document
        .plan
        .pages
        .iter()
        .map(|p| p.body.1.3)
        .max()
        .unwrap_or(1);
    let bottom = document
        .plan
        .pages
        .iter()
        .map(|p| p.body.1.2)
        .max()
        .unwrap_or(1);
    let scale = 0.65;
    let width = (m.col_start(right + 1) - m.col_start(left)) * scale;
    let height = (m.row_start(bottom + 1) - m.row_start(top)) * scale;
    let style = format!("width:{width}px;height:{height}px");
    let pages=document.plan.pages.iter().map(|p|{let w=m.col_start(p.body.1.3+1)-m.col_start(p.body.0.3);let h=m.row_start(p.body.1.2+1)-m.row_start(p.body.0.2);let style=format!("left:{}px;top:{}px;width:{w}px;height:{h}px;transform-origin:top left;transform:scale({scale})",(m.col_start(p.body.0.3)-m.col_start(left))*scale,(m.row_start(p.body.0.2)-m.row_start(top))*scale);let region=PrintRegion{area:p.body,x:0.,y:0.,width:w,height:h};view!{<div class="dc-print-break-page" class:manual-bottom=p.manual_bottom class:manual-right=p.manual_right data-dnacalc="print-break-page" data-range=address(p.body) use:css=style><Region document=Arc::clone(&document) region=region fonts=fonts error=error/><span class="dc-print-break-label">{format!("Page {}",p.number)}</span></div>}}).collect_view();
    view!{<div class="dc-print-break-map" data-dnacalc="print-break-map" use:css=style>{pages}</div>}.into_any()
}
#[inline(never)]
pub fn toolbar(controller: Controller) -> AnyView {
    let mode = controller.tool_field(|s| &s.page.view, |s| &mut s.page.view);
    view!{<div class="dc-page-toolbar"><button type="button" data-dnacalc="page-setup" disabled=move ||controller.sheet.get().is_none() on:click=move |_|controller.command(dna_calc_model::Command::PageSetup)>"Page Setup…"</button><button type="button" data-dnacalc="print-preview" disabled=move ||controller.sheet.get().is_none() on:click=move |_|controller.command(dna_calc_model::Command::PrintPreview)>"Print preview…"</button><label>"Worksheet view"<select data-dnacalc="page-view" prop:value=move ||{let m=mode.get();if m.is_empty(){"normal".into()}else{m}} on:change=move |ev|{let next=event_target_value(&ev);controller.after_input(move ||{mode.set(next);controller.command(dna_calc_model::Command::SwitchSkin(dna_calc_model::SkinId::Grid));});}><option value="normal">"Normal"</option><option value="breaks">"Page Break Preview"</option><option value="layout">"Page Layout"</option></select></label></div>}.into_any()
}
#[inline(never)]
pub fn grid_workspace(controller: Controller) -> AnyView {
    let mode = controller.tool_field(|s| &s.page.view, |s| &mut s.page.view);
    view!{ {move ||{let mode=mode.get();if mode.is_empty()||mode=="normal"{crate::grid::grid_workspace(controller).into_any()}else{view!{<Preview controller=controller breaks=mode=="breaks"/>}.into_any()}}} }.into_any()
}
#[inline(never)]
pub fn controls(controller: Controller) -> AnyView {
    let open = controller.tool_field(|s| &s.page.preview, |s| &mut s.page.preview);
    let dialog = NodeRef::<leptos::html::Dialog>::new();
    Effect::new(move |_| {
        if let Some(d) = dialog.get() {
            if open.get() {
                if !d.open() {
                    let _ = d.show_modal();
                }
            } else if d.open() {
                d.close();
            }
        }
    });
    let close = move || {
        open.set(false);
        controller.focus_active_skin();
    };
    view!{<dialog class="dc-print-dialog" data-dnacalc="print-preview-dialog" node_ref=dialog aria-labelledby="dc-print-title" on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();close();}><h2 id="dc-print-title">"Print preview"</h2><button type="button" data-dnacalc="print-close" on:click=move |_|close()>"Close preview"</button>{move ||open.get().then(||view!{<Preview controller=controller breaks=false/>})}</dialog>}.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn print_canvas_limits_apply_to_physical_pixels_and_the_whole_document() {
        let area = wire::Area(wire::Key(0, 0, 1, 1), wire::Key(0, 0, 1, 1));
        let page = PrintPage {
            number: 1,
            body: area,
            regions: vec![PrintRegion {
                area,
                x: 0.,
                y: 0.,
                width: 4000.,
                height: 3000.,
            }],
            width: 4000.,
            height: 3000.,
            offset_x: 0.,
            offset_y: 0.,
            manual_bottom: false,
            manual_right: false,
        };
        let mut doc = PrintDocument {
            revision: 1,
            sheet: wire::Sheet(0, 0),
            sheet_name: "Sheet1".into(),
            book_name: "Book1.xlsx".into(),
            settings: Default::default(),
            geometry: Default::default(),
            plan: PrintPlan {
                paper_width: 816.,
                paper_height: 1056.,
                margins: [0.; 6],
                scale: 0.1,
                pages: vec![page],
            },
            cells: Vec::new(),
            charts: Vec::new(),
            notices: Vec::new(),
        };
        assert!(drawing_budget(&doc, 1.).is_ok());
        assert!(drawing_budget(&doc, 2.).is_err());
        doc.plan.pages[0].regions[0].width = 9000.;
        doc.plan.pages[0].regions[0].height = 1.;
        assert!(drawing_budget(&doc, 2.).is_err());
        doc.plan.pages[0].regions[0].width = 3000.;
        doc.plan.pages[0].regions[0].height = 1000.;
        doc.plan.pages = vec![doc.plan.pages[0].clone(); 3];
        assert!(drawing_budget(&doc, 1.).is_ok());
        assert!(drawing_budget(&doc, 2.).is_err());
    }
}
