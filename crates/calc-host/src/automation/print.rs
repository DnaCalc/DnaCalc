//! Printing for macro workbooks (dc-oxh8w.8): `Worksheet.PageSetup` and its members, the
//! `HPageBreaks` / `VPageBreaks` collections and `ResetAllPageBreaks`. Each member maps onto the
//! one stored `PageSetup` and the scoped `Print_Area` / `Print_Titles` names through
//! [`crate::print_members`], and writes go through `WorkbookCommand::PageSetup`, the command the
//! Page Setup UI uses. Handlers register where the descriptor describes the interface (see
//! `report.rs`); the others stay pending until the descriptor is regenerated on Windows.
//!
//! The collections list the automatic breaks of the pagination route print preview takes
//! (`crate::printing::paginate` through [`crate::print_plan`]) with the stored manual ones
//! (dc-oxh8w.11).

use super::report::report;
use super::*;
use crate::print_plan;
use crate::print_members::{self, PrintError, PrintValue};
use crate::{CommandIngress, PageBreak, PageSetupChange, SkinIntent, WorkbookCommand};
use calc_object_model::{Descriptor, Handler, ReportItem};

const XL_PAGE_BREAK_MANUAL: i64 = -4135;
const XL_PAGE_BREAK_AUTOMATIC: i64 = -4105;

fn refused() -> AutoError {
    AutoError::excel(1004, "Application-defined or object-defined error")
}

fn error(e: PrintError) -> AutoError {
    match e {
        PrintError::Refused => refused(),
        PrintError::Mismatch => geometry::type_mismatch(),
    }
}

fn to_auto(value: PrintValue, member: &str) -> AutoValue {
    match value {
        PrintValue::Bool(b) => AutoValue::Bool(b),
        PrintValue::Int(n) => AutoValue::I4(n as i32),
        PrintValue::Real(r) => AutoValue::R8(r),
        PrintValue::Text(t) => {
            let _ = member;
            AutoValue::text(&t)
        }
    }
}

fn from_auto(value: &AutoValue) -> Result<PrintValue, AutoError> {
    Ok(match value {
        AutoValue::Bool(b) => PrintValue::Bool(*b),
        AutoValue::I2(v) => PrintValue::Int(i64::from(*v)),
        AutoValue::I4(v) => PrintValue::Int(i64::from(*v)),
        AutoValue::R4(v) => PrintValue::Real(f64::from(*v)),
        AutoValue::R8(v) => PrintValue::Real(*v),
        AutoValue::Text(_) => PrintValue::Text(value.as_text().unwrap_or_default()),
        _ => return Err(geometry::type_mismatch()),
    })
}

/// One page break as the collections list it: where it falls, the stored manual break when there is one.
struct BreakLine {
    line: u32,
    stored: Option<PageBreak>,
    partial: bool,
}

impl WorkbookAutomation {
    pub(super) fn register_print(handlers: &mut Handlers<Self>) -> Vec<String> {
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
        use InvokeKind::{Get, Method, Put};
        on("Worksheet", "PageSetup", Get, Self::sheet_page_setup);
        on("Worksheet", "HPageBreaks", Get, Self::sheet_h_breaks);
        on("Worksheet", "VPageBreaks", Get, Self::sheet_v_breaks);
        on("Worksheet", "ResetAllPageBreaks", Method, Self::sheet_reset_breaks);
        for member in print_members::MEMBERS {
            on("PageSetup", member, Get, Self::page_setup_get);
            on("PageSetup", member, Put, Self::page_setup_put);
        }
        for (collection, one) in [("HPageBreaks", "HPageBreak"), ("VPageBreaks", "VPageBreak")] {
            on(collection, "Count", Get, Self::breaks_count);
            on(collection, "Item", Get, Self::breaks_item);
            on(collection, "_Default", Get, Self::breaks_item);
            on(collection, "Add", Method, Self::breaks_add);
            on(one, "Location", Get, Self::break_location);
            on(one, "Type", Get, Self::break_type);
            on(one, "Type", Put, Self::break_put_type);
            on(one, "Extent", Get, Self::break_extent);
            on(one, "Delete", Method, Self::break_delete);
        }
        pending
    }

    /// The printing members the committed descriptor does not describe yet.
    pub fn print_members_pending(&self) -> Vec<String> {
        Self::register_print(&mut Handlers::new())
    }

    fn sheet_page_setup(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.collection(target, "PageSetup", ReportItem::PageSetup)
    }
    fn sheet_h_breaks(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.collection(target, "HPageBreaks", ReportItem::PageBreaks(true))
    }
    fn sheet_v_breaks(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.collection(target, "VPageBreaks", ReportItem::PageBreaks(false))
    }

    /// Dispatch one `PageSetupChange` as the Automation ingress; a change that changes nothing is
    /// not an error in Excel.
    fn print_change(
        &self,
        sheet: SheetId,
        label: String,
        mut make: impl FnMut(&HostSession) -> Result<PageSetupChange, AutoError>,
    ) -> Reply {
        self.with_write(|session| {
            let change = make(session)?;
            let revision = session.host().revision().get();
            let receipt = session.dispatch_from(
                CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::PageSetup {
                    sheet,
                    revision,
                    change,
                }),
            );
            if let Some(e) = receipt.error {
                let text = format!("{e:?}");
                if !text.contains(crate::page_setup::NO_CHANGES) {
                    return Err(AutoError::not_modelled(format!("{label}: {text}"), "dc-oxh8w"));
                }
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    fn page_setup_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let member = call.member.name.clone();
        self.read_host(target, |s, sheet, _| {
            print_members::read(s.host(), sheet, &member)
                .map(|v| to_auto(v, &member))
                .ok_or_else(refused)
        })
    }

    fn page_setup_put(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let (_, sheet, _) = Self::report_key(target)?;
        let member = call.member.name.clone();
        let value = from_auto(call.arg(0))?;
        self.print_change(sheet, call.label(), |s| {
            print_members::write(s.host(), sheet, &member, &value).map_err(error)
        })
    }

    // --- page breaks ---------------------------------------------------------------------

    fn breaks_direction(target: &ObjectRef) -> Result<bool, AutoError> {
        match Self::report_key(target)?.2 {
            ReportItem::PageBreaks(h) | ReportItem::PageBreak(h, _) => Ok(h),
            _ => Err(geometry::type_mismatch()),
        }
    }

    fn manual_lines(s: &HostSession, sheet: SheetId, horizontal: bool) -> Vec<PageBreak> {
        print_members::manual_breaks(&s.host().model().definitions().page_setup(sheet), horizontal)
    }

    /// The breaks of one direction as Excel lists them (dc-oxh8w.11): the automatic breaks of the pagination route
    /// print preview takes, with the stored manual breaks, in position order. A pagination the route refuses
    /// (a partial-span manual break, titles inside the area) is refused here and not answered with a guess.
    fn break_lines(s: &HostSession, sheet: SheetId, horizontal: bool) -> Result<Vec<BreakLine>, AutoError> {
        let host = s.host();
        let manual = Self::manual_lines(s, sheet, horizontal);
        let geometry = print_plan::print_geometry(host, sheet);
        let plan = print_plan::print_plan(host, sheet, &geometry, (0, 0))
            .map_err(|e| AutoError::not_modelled(format!("page breaks: {e}"), "dc-oxh8w"))?;
        // Excel calls a break partial when the print range does not start in the first column (row) of the sheet.
        let partial = if horizontal {
            plan.pages.iter().map(|p| p.body.0.3).min().is_some_and(|c| c > 1)
        } else {
            plan.pages.iter().map(|p| p.body.0.2).min().is_some_and(|r| r > 1)
        };
        let breaks = plan.breaks();
        let mut lines: std::collections::BTreeMap<u32, BreakLine> = std::collections::BTreeMap::new();
        for &(line, _) in if horizontal { &breaks.rows } else { &breaks.columns } {
            lines.insert(line, BreakLine { line, stored: None, partial });
        }
        for b in manual {
            lines.insert(b.id + 1, BreakLine { line: b.id + 1, stored: Some(b), partial });
        }
        Ok(lines.into_values().collect())
    }

    fn breaks_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let horizontal = Self::breaks_direction(target)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::I4(Self::break_lines(s, sheet, horizontal)?.len() as i32))
        })
    }

    fn breaks_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let horizontal = Self::breaks_direction(target)?;
        let (workbook, _, _) = Self::report_key(target)?;
        self.read_host(target, |s, sheet, _| {
            let index = geometry::integer(call.arg(0))?;
            let lines = Self::break_lines(s, sheet, horizontal)?;
            let found = usize::try_from(index - 1).ok().and_then(|n| lines.get(n));
            let b = found.ok_or_else(|| AutoError::excel(9, "Subscript out of range"))?;
            Ok(report(
                workbook,
                sheet,
                if horizontal { "HPageBreak" } else { "VPageBreak" },
                ReportItem::PageBreak(horizontal, b.line),
            ))
        })
    }

    /// `HPageBreaks.Add(Before)` / `VPageBreaks.Add(Before)`: a manual break before the row (or
    /// column) of the cell. A break already there stays one.
    fn breaks_add(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let horizontal = Self::breaks_direction(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        let range = self.source_range(target, call.arg(0), sheet)?;
        if range.area_count() != 1 {
            return Err(refused());
        }
        let first = range.areas()[0].first();
        let line = if horizontal { first.row() } else { first.column() };
        self.print_change(sheet, call.label(), |s| {
            let setup = s.host().model().definitions().page_setup(sheet);
            let breaks = print_members::with_break(&setup, horizontal, line).map_err(error)?;
            Ok(Self::break_change(horizontal, breaks))
        })?;
        Ok(report(
            workbook,
            sheet,
            if horizontal { "HPageBreak" } else { "VPageBreak" },
            ReportItem::PageBreak(horizontal, line),
        ))
    }

    fn break_change(horizontal: bool, breaks: Vec<PageBreak>) -> PageSetupChange {
        if horizontal {
            PageSetupChange { row_breaks: Some(breaks), ..Default::default() }
        } else {
            PageSetupChange { column_breaks: Some(breaks), ..Default::default() }
        }
    }

    fn break_of(target: &ObjectRef) -> Result<(bool, u32), AutoError> {
        match Self::report_key(target)?.2 {
            ReportItem::PageBreak(h, line) => Ok((h, line)),
            _ => Err(geometry::type_mismatch()),
        }
    }

    fn break_here(
        s: &HostSession,
        sheet: SheetId,
        horizontal: bool,
        line: u32,
    ) -> Result<BreakLine, AutoError> {
        Self::break_lines(s, sheet, horizontal)?
            .into_iter()
            .find(|b| b.line == line)
            .ok_or_else(refused)
    }

    fn break_location(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (horizontal, line) = Self::break_of(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        self.read_host(target, |s, sheet, _| Self::break_here(s, sheet, horizontal, line).map(|_| ()))?;
        let at = if horizontal { CellKey::new(sheet, line, 1) } else { CellKey::new(sheet, 1, line) }
            .map_err(|_| refused())?;
        let area = GridArea::new(at, at).map_err(|_| refused())?;
        self.with_target(target, |access, _| {
            self.acquire(access, &ReferenceLike::area(area), RangeAxis::Cells)
        })
    }

    fn break_type(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (horizontal, line) = Self::break_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let b = Self::break_here(s, sheet, horizontal, line)?;
            let kind = if b.stored.is_some() { XL_PAGE_BREAK_MANUAL } else { XL_PAGE_BREAK_AUTOMATIC };
            Ok(AutoValue::I4(kind as i32))
        })
    }

    fn break_extent(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (horizontal, line) = Self::break_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let b = Self::break_here(s, sheet, horizontal, line)?;
            Ok(AutoValue::I4(match &b.stored {
                Some(stored) => print_members::break_extent(stored, horizontal),
                None if b.partial => 2,
                None => 1,
            } as i32))
        })
    }

    fn break_remove(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (horizontal, line) = Self::break_of(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        self.print_change(sheet, call.label(), |s| {
            if Self::break_here(s, sheet, horizontal, line)?.stored.is_none() {
                return Err(refused());
            }
            let setup = s.host().model().definitions().page_setup(sheet);
            Ok(Self::break_change(horizontal, print_members::without_break(&setup, horizontal, line)))
        })
    }

    fn break_delete(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.break_remove(target, call)
    }

    /// `HPageBreak.Type = xlPageBreakAutomatic` removes the manual break; `xlPageBreakManual`
    /// keeps it.
    fn break_put_type(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let kind = geometry::integer(call.arg(0))?;
        match kind {
            XL_PAGE_BREAK_MANUAL => {
                let (horizontal, line) = Self::break_of(target)?;
                let (_, sheet, _) = Self::report_key(target)?;
                let found = self.read_host(target, |s, sheet, _| {
                    Self::break_here(s, sheet, horizontal, line).map(|b| AutoValue::Bool(b.stored.is_some()))
                })?;
                if matches!(found, AutoValue::Bool(true)) {
                    return Ok(AutoValue::Empty);
                }
                // An automatic break set to manual stays where it is and is stored.
                self.print_change(sheet, call.label(), |s| {
                    let setup = s.host().model().definitions().page_setup(sheet);
                    let breaks = print_members::with_break(&setup, horizontal, line).map_err(error)?;
                    Ok(Self::break_change(horizontal, breaks))
                })
            }
            XL_PAGE_BREAK_AUTOMATIC => {
                let (horizontal, line) = Self::break_of(target)?;
                let stored = self.read_host(target, |s, sheet, _| {
                    Self::break_here(s, sheet, horizontal, line).map(|b| AutoValue::Bool(b.stored.is_some()))
                })?;
                if matches!(stored, AutoValue::Bool(true)) { self.break_remove(target, call) } else { Ok(AutoValue::Empty) }
            }
            _ => Err(refused()),
        }
    }

    fn sheet_reset_breaks(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (_, sheet, _) = Self::report_key(target)?;
        self.print_change(sheet, call.label(), |s| {
            let setup = s.host().model().definitions().page_setup(sheet);
            let automatic =
                |v: &[PageBreak]| v.iter().filter(|b| !b.manual).copied().collect::<Vec<_>>();
            Ok(PageSetupChange {
                row_breaks: Some(automatic(&setup.row_breaks)),
                column_breaks: Some(automatic(&setup.column_breaks)),
                ..Default::default()
            })
        })
    }
}
