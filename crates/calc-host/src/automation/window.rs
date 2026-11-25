//! The `Window` of a workbook (dc-oxh8w.50, dc-p3rg9): `Application.ActiveWindow`, `Application.Windows`,
//! `Workbook.Windows` (one window per workbook), and its view members. `ActiveSheet`, `ActiveCell` and `Selection`
//! read the session's selection; `FreezePanes`, `Split` and `SplitRow`/`SplitColumn` read and write the sheet's
//! stored frozen panes through `WorkbookCommand::FreezePanes`. `Zoom`, `ScrollRow`, `ScrollColumn` and the five
//! worksheet `Display*` flags (`DisplayGridlines`, `DisplayHeadings`, `DisplayFormulas`, `DisplayZeros`,
//! `DisplayOutline`) update the host's sheet view and persist to `<sheetView>` on save. Workbook-level flags
//! (`DisplayHorizontalScrollBar`, `DisplayVerticalScrollBar`, `DisplayWorkbookTabs`), `Caption` and `WindowState`
//! are held per workbook for the macro session (remainder: dc-p3rg9). Errors: out-of-range Zoom (10 to 400),
//! scroll positions, invalid WindowState and non-numeric values are 1004; a wrong type is a type mismatch.
use super::*;
use crate::{CommandIngress, SkinIntent, WorkbookCommand};
use calc_object_model::{Descriptor, Handler};

const REMAINDER: &str = "dc-p3rg9";
const XL_NORMAL: i32 = -4143;
const XL_MAXIMIZED: i32 = -4137;
const XL_MINIMIZED: i32 = -4140;

/// The flags `Window` exposes, with Excel's defaults for a fresh window.
const FLAGS: &[(&str, bool)] = &[
    ("DisplayGridlines", true),
    ("DisplayHeadings", true),
    ("DisplayFormulas", false),
    ("DisplayZeros", true),
    ("DisplayOutline", true),
    ("DisplayHorizontalScrollBar", true),
    ("DisplayVerticalScrollBar", true),
    ("DisplayWorkbookTabs", true),
];

/// Workbook-level window flags held for the macro session (`<workbookView>` persistence stays dc-p3rg9).
#[derive(Default, Clone)]
pub(super) struct View {
    flags: Vec<(&'static str, bool)>,
}

fn refused() -> AutoError {
    AutoError::excel(1004, "Application-defined or object-defined error")
}

impl WorkbookAutomation {
    pub(super) fn register_window(handlers: &mut Handlers<Self>) -> Vec<String> {
        let mut pending = Vec::new();
        let mut on = |interface: &str, member: &str, kind: InvokeKind, handler: Handler<Self>| {
            let descriptor = Descriptor::get();
            let present = descriptor.id(interface).is_some_and(|id| {
                let id = descriptor.default_interface(id);
                descriptor.member(id, member, kind).is_some()
            });
            if present {
                handlers.on(interface, member, kind, handler);
            } else {
                pending.push(format!("{interface}.{member} ({kind:?})"));
            }
        };
        use InvokeKind::{Get, Put};
        on("Application", "ActiveWindow", Get, Self::app_active_window);
        on("Application", "Windows", Get, Self::app_windows);
        on("Workbook", "Windows", Get, Self::book_windows);
        on("Windows", "Count", Get, Self::windows_count);
        on("Windows", "Item", Get, Self::windows_item);
        on("Windows", "_Default", Get, Self::windows_item);
        on("Window", "Application", Get, Self::delegate);
        on("Window", "ActiveSheet", Get, Self::window_active_sheet);
        on("Window", "ActiveCell", Get, Self::window_active_cell);
        on("Window", "Selection", Get, Self::window_selection);
        on("Window", "Caption", Get, Self::window_book_member);
        on("Window", "Caption", Put, Self::window_put_caption);
        on("Window", "WindowState", Get, Self::window_get_state);
        on("Window", "WindowState", Put, Self::window_put_state);
        on("Window", "Type", Get, Self::window_book_member);
        on("Window", "WindowNumber", Get, Self::window_book_member);
        on("Window", "Visible", Get, Self::window_book_member);
        on("Window", "Activate", InvokeKind::Method, Self::window_book_member);
        on("Window", "Index", Get, Self::window_index);
        on("Window", "Zoom", Get, Self::window_get);
        on("Window", "Zoom", Put, Self::window_put);
        for member in ["ScrollRow", "ScrollColumn", "FreezePanes", "Split", "SplitRow", "SplitColumn"] {
            on("Window", member, Get, Self::window_get);
        }
        for member in ["ScrollRow", "ScrollColumn", "FreezePanes", "Split"] {
            on("Window", member, Put, Self::window_put);
        }
        for (member, _) in FLAGS {
            on("Window", member, Get, Self::window_get);
            on("Window", member, Put, Self::window_put);
        }
        pending
    }

    fn app_active_window(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.with_target(target, |_, meta| {
            let id = self.active_book_id(meta.workbook)?;
            Ok(AutoValue::Object(object("Window", ObjectKey::BookWindow(id))))
        })
    }

    fn app_windows(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.with_target(target, |_, _| Ok(AutoValue::Object(object("Windows", ObjectKey::Windows(0)))))
    }

    fn book_windows(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.with_target(target, |_, meta| Ok(AutoValue::Object(object("Windows", ObjectKey::Windows(meta.workbook)))))
    }

    fn windows_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if target.key == ObjectKey::Windows(0) {
            return self.with_target(target, |_, _| Ok(AutoValue::I4(self.open_books()?.len() as i32)));
        }
        self.with_target(target, |_, _| Ok(AutoValue::I4(1)))
    }

    fn window_caption_for(&self, id: u64, full_name: &str, holder: Option<&WorkbookAutomation>) -> String {
        let map = holder.unwrap_or(self);
        map.window_captions
            .borrow()
            .get(&id)
            .cloned()
            .unwrap_or_else(|| book_name(full_name).to_string())
    }

    /// `Application.Windows` indexes open workbooks (one window each) by 1-based index or caption/name;
    /// `Workbook.Windows` holds its one window. Any other index or unknown name is 1004.
    fn windows_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        if target.key == ObjectKey::Windows(0) {
            return self.with_target(target, |_, _| {
                let books = self.open_books()?;
                let id = match call.arg(0) {
                    AutoValue::Text(name) => {
                        let wanted = name.to_string_lossy();
                        books
                            .iter()
                            .find(|b| {
                                let cap = self.window_caption_for(b.id, &b.full_name, b.automation.as_deref());
                                calc_value::workbook_names_equal(&cap, &wanted)
                                    || calc_value::workbook_names_equal(book_name(&b.full_name), &wanted)
                            })
                            .map(|b| b.id)
                            .ok_or_else(refused)?
                    }
                    other => {
                        let i = geometry::integer(other).map_err(|_| refused())?;
                        let idx = usize::try_from(i.checked_sub(1).ok_or_else(refused)?).map_err(|_| refused())?;
                        books.get(idx).map(|b| b.id).ok_or_else(refused)?
                    }
                };
                Ok(AutoValue::Object(object("Window", ObjectKey::BookWindow(id))))
            });
        }
        self.with_target(target, |_, meta| {
            let matched = match call.arg(0) {
                AutoValue::Text(name) => {
                    let wanted = name.to_string_lossy();
                    let cap = self.window_caption_for(meta.workbook, &meta.full_name, None);
                    calc_value::workbook_names_equal(&cap, &wanted)
                        || calc_value::workbook_names_equal(book_name(&meta.full_name), &wanted)
                }
                other => geometry::integer(other).map_err(|_| refused())? == 1,
            };
            if !matched {
                return Err(refused());
            }
            Ok(AutoValue::Object(object("Window", ObjectKey::BookWindow(meta.workbook))))
        })
    }

    fn window_active_sheet(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.get_active_sheet(target, call)
    }
    fn window_active_cell(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.own_active_cell(target, call)
    }
    fn window_selection(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.own_selection(target, call)
    }
    /// Caption (the workbook's name), Type (xlWorkbook), WindowNumber (the one window), Visible and Activate.
    fn window_book_member(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let ObjectKey::BookWindow(workbook) = target.key else {
            return Err(geometry::type_mismatch());
        };
        match call.member.name.as_str() {
            "Caption" => {
                let held = self.window_captions.borrow().get(&workbook).cloned();
                self.with_target(target, |_, meta| Ok(AutoValue::text(&held.clone().unwrap_or_else(|| book_name(&meta.full_name).to_string()))))
            }
            "Type" | "WindowNumber" => self.with_target(target, |_, _| Ok(AutoValue::I4(1))),
            "Visible" => self.with_target(target, |_, _| Ok(AutoValue::Bool(true))),
            _ => self.workbook_activate(&object("Workbook", ObjectKey::Workbook(workbook)), call),
        }
    }
    /// `Window.Caption = value`: text, numbers, booleans and Empty coerce to text (ww21: 12 -> "12", "" and Empty
    /// clear caption to ""); objects and other types are 1004. The workbook's name is unchanged (dc-p3rg9).
    fn window_put_caption(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let ObjectKey::BookWindow(workbook) = target.key else {
            return Err(geometry::type_mismatch());
        };
        let caption = match call.arg(0) {
            AutoValue::Text(s) => s.to_string_lossy(),
            AutoValue::Empty => String::new(),
            AutoValue::I2(n) => n.to_string(),
            AutoValue::I4(n) => n.to_string(),
            AutoValue::R4(n) if n.is_finite() => n.to_string(),
            AutoValue::R8(n) if n.is_finite() => n.to_string(),
            AutoValue::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            _ => return Err(refused()),
        };
        self.with_target(target, |_, _| Ok(AutoValue::Empty))?;
        self.window_captions.borrow_mut().insert(workbook, caption);
        Ok(AutoValue::Empty)
    }

    /// `Window.WindowState`: defaults to `xlNormal` (-4143, ww21); accepts `xlNormal`, `xlMaximized`, `xlMinimized`.
    fn window_get_state(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let ObjectKey::BookWindow(workbook) = target.key else {
            return Err(geometry::type_mismatch());
        };
        self.with_target(target, |_, _| Ok(AutoValue::Empty))?;
        let state = self.window_states.borrow().get(&workbook).copied().unwrap_or(XL_NORMAL);
        Ok(AutoValue::I4(state))
    }

    fn window_put_state(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let ObjectKey::BookWindow(workbook) = target.key else {
            return Err(geometry::type_mismatch());
        };
        let state = i32::try_from(geometry::integer(call.arg(0)).map_err(|_| refused())?).map_err(|_| refused())?;
        if !matches!(state, XL_NORMAL | XL_MAXIMIZED | XL_MINIMIZED) {
            return Err(refused());
        }
        self.with_target(target, |_, _| Ok(AutoValue::Empty))?;
        self.window_states.borrow_mut().insert(workbook, state);
        Ok(AutoValue::Empty)
    }

    fn window_index(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.with_target(target, |_, _| Ok(AutoValue::I4(1)))
    }

    /// The workbook's active worksheet and the stored view of it.
    fn window_sheet(&self, target: &ObjectRef) -> Result<(u64, SheetId), AutoError> {
        let ObjectKey::BookWindow(workbook) = target.key else {
            return Err(geometry::type_mismatch());
        };
        let mut found = None;
        self.with_target(target, |access, meta| {
            let sheet = match access {
                Access::Session(session, _) => session.selection().active.map(|at| at.sheet()),
                Access::Invocation(_) => None,
            }
            .or_else(|| meta.active.map(|at| at.sheet()))
            .ok_or_else(|| AutoError::not_modelled("Window of a workbook without an active sheet", REMAINDER))?;
            found = Some(sheet);
            Ok(AutoValue::Empty)
        })?;
        Ok((workbook, found.ok_or_else(refused)?))
    }

    fn window_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (workbook, sheet) = self.window_sheet(target)?;
        let member = call.member.name.as_str();
        let held = self.window_views.borrow().get(&(workbook, sheet)).cloned().unwrap_or_default();
        self.with_target(target, |access, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled("Window members inside a worksheet invocation", REMAINDER));
            };
            let stored = session.host().sheet_view(sheet).unwrap_or_default();
            Ok(match member {
                "Zoom" => AutoValue::I4(i32::from(stored.zoom.unwrap_or(100))),
                "ScrollRow" => AutoValue::I4(stored.top_left.unwrap_or((1, 1)).0 as i32),
                "ScrollColumn" => AutoValue::I4(stored.top_left.unwrap_or((1, 1)).1 as i32),
                "FreezePanes" => AutoValue::Bool(stored.frozen.is_some()),
                "Split" => AutoValue::Bool(stored.frozen.is_some() || stored.split.is_some()),
                "SplitRow" | "SplitColumn" => {
                    if stored.split.is_some() {
                        return Err(AutoError::not_modelled("Window.SplitRow of a split that is not frozen", REMAINDER));
                    }
                    let panes = stored.frozen;
                    let (rows, columns) = panes.map_or((0, 0), |p| (p.rows(), p.columns()));
                    AutoValue::I4(if member == "SplitRow" { rows } else { columns } as i32)
                }
                "DisplayGridlines" => AutoValue::Bool(stored.show_grid_lines.unwrap_or(true)),
                "DisplayHeadings" => AutoValue::Bool(stored.show_row_col_headers.unwrap_or(true)),
                "DisplayFormulas" => AutoValue::Bool(stored.show_formulas.unwrap_or(false)),
                "DisplayZeros" => AutoValue::Bool(stored.show_zeros.unwrap_or(true)),
                "DisplayOutline" => AutoValue::Bool(stored.show_outline_symbols.unwrap_or(true)),
                flag => {
                    let default = FLAGS.iter().find(|(n, _)| *n == flag).map(|(_, d)| *d).ok_or_else(refused)?;
                    AutoValue::Bool(held.flags.iter().find(|(n, _)| *n == flag).map_or(default, |(_, v)| *v))
                }
            })
        })
    }

    fn window_put(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let (workbook, sheet) = self.window_sheet(target)?;
        let member = call.member.name.as_str();
        let value = call.arg(0);
        match member {
            "Zoom" => {
                let n = geometry::integer(value)?;
                if !(10..=400).contains(&n) {
                    return Err(refused());
                }
                let zoom = n as u16;
                return self.with_write(|session| {
                    session.host.sheet_view_overrides.entry(sheet).or_default().zoom = Some(zoom);
                    Ok(AutoValue::Empty)
                });
            }
            "ScrollRow" | "ScrollColumn" => {
                let n = geometry::integer(value)?;
                let limit = if member == "ScrollRow" { 1_048_576 } else { 16_384 };
                if !(1..=limit).contains(&n) {
                    return Err(refused());
                }
                let n = n as u32;
                let row = member == "ScrollRow";
                return self.with_write(|session| {
                    let (r, c) = session.host().sheet_view(sheet).and_then(|v| v.top_left).unwrap_or((1, 1));
                    session.host.sheet_view_overrides.entry(sheet).or_default().top_left =
                        Some(if row { (n, c) } else { (r, n) });
                    Ok(AutoValue::Empty)
                });
            }
            "FreezePanes" => {
                let freeze = geometry::boolean(value).map_err(|_| geometry::type_mismatch())?;
                return self.freeze_panes(target, sheet, freeze);
            }
            "Split" => {
                let split = geometry::boolean(value).map_err(|_| geometry::type_mismatch())?;
                if split {
                    return Err(AutoError::not_modelled("Window.Split = True", REMAINDER));
                }
                let mut state = (false, false);
                self.with_target(target, |access, _| {
                    let Access::Session(session, _) = access else {
                        return Err(AutoError::not_modelled("Window members inside a worksheet invocation", REMAINDER));
                    };
                    let stored = session.host().sheet_view(sheet).unwrap_or_default();
                    state = (stored.frozen.is_some(), stored.split.is_some());
                    Ok(AutoValue::Empty)
                })?;
                if state.1 {
                    return Err(AutoError::not_modelled("Window.Split = False on a non-frozen split", REMAINDER));
                }
                if state.0 {
                    return self.freeze_panes(target, sheet, false);
                }
                return Ok(AutoValue::Empty);
            }
            "DisplayGridlines"
            | "DisplayHeadings"
            | "DisplayFormulas"
            | "DisplayZeros"
            | "DisplayOutline" => {
                let on = geometry::boolean(value).map_err(|_| geometry::type_mismatch())?;
                return self.with_write(|session| {
                    let entry = session.host.sheet_view_overrides.entry(sheet).or_default();
                    match member {
                        "DisplayGridlines" => entry.show_grid_lines = Some(on),
                        "DisplayHeadings" => entry.show_row_col_headers = Some(on),
                        "DisplayFormulas" => entry.show_formulas = Some(on),
                        "DisplayZeros" => entry.show_zeros = Some(on),
                        "DisplayOutline" => entry.show_outline_symbols = Some(on),
                        _ => unreachable!(),
                    }
                    Ok(AutoValue::Empty)
                });
            }
            flag => {
                let name = FLAGS.iter().find(|(n, _)| *n == flag).map(|(n, _)| *n).ok_or_else(refused)?;
                let on = geometry::boolean(value).map_err(|_| geometry::type_mismatch())?;
                self.with_target(target, |_, _| Ok(AutoValue::Empty))?;
                let mut views = self.window_views.borrow_mut();
                let entry = views.entry((workbook, sheet)).or_default();
                match entry.flags.iter_mut().find(|(n, _)| *n == name) {
                    Some(slot) => slot.1 = on,
                    None => entry.flags.push((name, on)),
                }
                Ok(AutoValue::Empty)
            }
        }
    }

    /// `Window.FreezePanes = True` freezes above and left of the active cell (relative to the scroll position);
    /// `False` removes the freeze. Freezing at the first cell (Excel splits the window at its middle) is refused.
    fn freeze_panes(&self, target: &ObjectRef, sheet: SheetId, freeze: bool) -> Reply {
        let _ = target;
        self.with_write(|session| {
            let (rows, columns) = if freeze {
                let active = session.selection().active.ok_or_else(|| {
                    AutoError::not_modelled("Window.FreezePanes without an active cell", REMAINDER)
                })?;
                let stored = session.host().sheet_view(sheet).unwrap_or_default();
                let (top, left) = stored.top_left.unwrap_or((1, 1));
                let rows = active.row().saturating_sub(top);
                let columns = active.column().saturating_sub(left);
                if rows == 0 && columns == 0 {
                    return Err(AutoError::not_modelled(
                        "Window.FreezePanes at the first visible cell (Excel freezes at the window's middle)",
                        REMAINDER,
                    ));
                }
                (rows, columns)
            } else {
                (0, 0)
            };
            let receipt = session.dispatch_from(
                CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::FreezePanes { sheet, rows, columns }),
            );
            if let Some(error) = receipt.error {
                return Err(AutoError::not_modelled(format!("Window.FreezePanes: {error:?}"), REMAINDER));
            }
            if let Some(o) = session.host.sheet_view_overrides.get_mut(&sheet) {
                o.top_left = None;
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }
}
