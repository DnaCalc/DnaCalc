use super::{Area, Item, Key, Kind, Sheet, State, first_cell, source_area, stage};
use calc_skin::{
    AnchorId, Answer, Change, ModelChange, Placement, PlacementKind, PlacementShape,
    PresentationChange, SourceRef, WorkbookId, query,
};
use dna_kit::{Following, Live, Seam, Session};
use leptos::prelude::*;

const BACKING_SHEET: &str = "Analysis";

#[derive(Clone, Copy, PartialEq, Eq)]
struct Request {
    workbook: WorkbookId,
    sheet: Option<Sheet>,
    columns: u32,
    slot: Option<AnchorId>,
}

#[derive(Clone, Copy)]
pub(super) struct Authoring {
    request: RwSignal<Option<Request>>,
    watch: Following<query::Placement>,
    new_sheet: Following<query::NewSheetPlacement>,
}

impl Authoring {
    pub(super) fn new(session: Session) -> Self {
        Self {
            request: RwSignal::new(None),
            watch: session.following(),
            new_sheet: session.following(),
        }
    }

    pub(super) fn open(self, state: State) -> Result<(), String> {
        self.open_at(state, None)
    }

    pub(super) fn open_at(self, state: State, slot: Option<AnchorId>) -> Result<(), String> {
        if !matches!(state.session.seam(), Seam::Idle) {
            return Err("Finish the change in progress first.".into());
        }
        let workbook = state
            .session
            .stamp()
            .ok_or("Open a workbook first.")?
            .workbook;
        let summary = state.session.summary().ok_or("Loading worksheets…")?;
        let preferred = summary
            .sheets
            .iter()
            .find(|sheet| sheet.name.eq_ignore_ascii_case(BACKING_SHEET));
        if preferred.is_some_and(|sheet| !sheet.visible || !sheet.worksheet) {
            return Err("Analysis must be a visible worksheet for Notebook formulas.".into());
        }
        self.request.set(Some(Request {
            workbook,
            sheet: preferred.map(|sheet| sheet.sheet),
            columns: 0,
            slot,
        }));
        let _ = self.placement(state);
        Ok(())
    }

    pub(super) fn columns(self, state: State, columns: u32) -> Result<(), String> {
        let mut request = self.current(state.session).ok_or("Add a formula first.")?;
        if columns >= 16_384 {
            return Err("Leave space for an array lane and its spacer column.".into());
        }
        request.columns = columns;
        self.request.set(Some(request));
        let _ = self.placement(state);
        Ok(())
    }

    pub(super) fn close(self) {
        self.request.set(None);
        self.watch.at(None);
        self.new_sheet.at(None);
    }

    fn current(self, session: Session) -> Option<Request> {
        self.request.get().filter(|request| {
            session
                .stamp()
                .is_some_and(|stamp| stamp.workbook == request.workbook)
        })
    }

    fn placement(self, state: State) -> Live<Answer<Placement>> {
        if let Some(Request { sheet: None, columns, .. }) = self.current(state.session) {
            self.watch.at(None);
            return self.new_sheet.at(Some(query::NewSheetPlacement {
                name: BACKING_SHEET.into(),
                kind: if columns == 0 { PlacementKind::Scalar } else { PlacementKind::Array },
                shape: (columns > 0).then_some(PlacementShape { rows: 1, columns }),
            }));
        }
        self.new_sheet.at(None);
        let query = self.current(state.session).and_then(|request| {
            let sheet = request.sheet?;
            let snapshot = state.snapshot().ok()?;
            let shape = (request.columns > 0).then_some(PlacementShape {
                rows: 1,
                columns: request.columns,
            });
            let reserved = if shape.is_some() {
                snapshot.reserved_lanes(sheet)
            } else {
                Vec::new()
            };
            Some(query::Placement {
                sheet,
                kind: if shape.is_some() {
                    PlacementKind::Array
                } else {
                    PlacementKind::Scalar
                },
                shape,
                reserved,
            })
        });
        self.watch.at(query)
    }

    pub(super) fn submit(self, state: State, text: String) -> Result<(), String> {
        if !text.starts_with('=') {
            return Err("Enter an Excel formula beginning with =.".into());
        }
        let request = self.current(state.session).ok_or("Add a formula first.")?;
        let placement = match self.placement(state) {
            Live::Ready(answer) => answer,
            Live::Refused(reason) => return Err(format!("Placement is unavailable · {reason:?}")),
            _ => return Err("Wait for the formula location.".into()),
        };
        let snapshot = state.snapshot()?;
        if placement.stamp.workbook != request.workbook
            || snapshot.part.stamp.workbook != request.workbook
        {
            return Err("The workbook changed. Open the formula form again.".into());
        }
        let slot = placement.body.at;
        let source = if request.columns == 0 {
            source_area(Area(slot, slot))
        } else {
            SourceRef::Spill { anchor: slot }
        };
        let mut chain = snapshot.chain.clone();
        let id = request.slot.map_or_else(|| snapshot.next_id().map(AnchorId), Ok)?;
        let index = if request.slot.is_some() {
            chain.cells.iter().position(|item| item.id == id && matches!(item.kind, Kind::CodeSlot))
                .ok_or("That formula slot was removed.")?
        } else {
            state.selected(&snapshot).map_or(0, |index| index + 1)
        };
        let mut item = Item::new(id, Kind::Formula);
        item.owned = true;
        item.lane_columns = (request.columns > 0).then_some(request.columns);
        if request.slot.is_some() {
            chain.cells[index] = item;
        } else {
            chain.cells.insert(index, item);
        }
        let mut changes = Vec::new();
        if request.sheet.is_none() {
            changes.push(Change::Model(ModelChange::InsertWorksheet {
                name: BACKING_SHEET.into(),
                sheet: Sheet(slot.0, slot.1),
            }));
        }
        changes.extend([
            Change::Model(ModelChange::Enter { at: slot, text }),
            Change::Presentation(PresentationChange::Bind {
                anchor: id,
                source: source.clone(),
            }),
        ]);
        let mut batch = state.prepare_batch(&snapshot, &chain, changes, "Add Notebook formula")?;
        batch.expects.push(if request.sheet.is_some() {
            batch.base = Some(placement.stamp.model);
            source_area(placement.body.footprint)
        } else {
            let saved = snapshot.part.stamp;
            let current = state.session.stamp().filter(|stamp| {
                stamp.workbook == saved.workbook && stamp.view == saved.view
            }).ok_or("The Notebook changed. Open the formula form again.")?;
            batch.base = Some(current.model);
            SourceRef::Workbook
        });
        let stamp = snapshot.part.stamp;
        state.session.submit_then(batch, move || {
            self.close();
            if state.session.view().0 == super::ID {
                state.select(stamp, id, Some(&source));
            }
        });
        Ok(())
    }
}

impl super::Snapshot {
    pub(super) fn reserved_lanes(&self, sheet: Sheet) -> Vec<Area> {
        self.chain.cells.iter().filter_map(|item| {
            let at = self.source(item.id).and_then(first_cell)?;
            if Sheet(at.0, at.1) != sheet { return None; }
            let right = at.3.checked_add(item.lane_columns?.checked_sub(1)?)?;
            let bounds = Area(Key(at.0, at.1, 10, at.3), Key(at.0, at.1, 1_048_576, right));
            bounds.area().map(|_| bounds)
        }).collect()
    }
}

pub(super) fn mount(state: State) -> impl IntoView {
    let author = state.author;
    let entry = NodeRef::<leptos::html::Input>::new();
    let placement = Memo::new(move |_| author.placement(state));
    view! {
        <Show when=move || author.current(state.session).is_some()>
            <form class="dna-notebook-tools" aria-label="Add formula" on:submit=move |event| {
                event.prevent_default();
                if let Some(entry) = entry.get_untracked() {
                    super::commands::invoke(state.session, "notebook.author", entry.value());
                }
            }>
                <label>"Excel formula" <input type="text" node_ref=entry /></label>
                <label>"Array columns (0 for a scalar)"
                    <input type="number" min="0" max="16383"
                        prop:value=move || author.current(state.session).map_or(0, |request| request.columns).to_string()
                        on:change=move |event| {
                            if let Ok(columns) = event_target_value(&event).parse::<u32>() {
                                super::commands::invoke(state.session, "notebook.author-columns", columns);
                            }
                        } />
                </label>
                <p role="status" data-notebook-placement>
                    {move || match placement.get() {
                        Live::Ready(answer) => {
                            let address = stage::address(state.session, Some(answer.body.at));
                            if author.current(state.session).is_some_and(|request| request.sheet.is_none()) {
                                format!("New worksheet · {BACKING_SHEET}!{address}")
                            } else {
                                format!("Worksheet location · {address}")
                            }
                        },
                        Live::Refused(reason) => format!("Placement is unavailable · {reason:?}"),
                        _ => "Finding a worksheet location…".into(),
                    }}
                </p>
                <button type="submit" data-command="notebook.author"
                    disabled=move || !matches!(placement.get(), Live::Ready(_)) || state.session.seam() != Seam::Idle>
                    "Add formula"
                </button>
                {stage::button(state.session, "notebook.cancel-formula", "Cancel")}
            </form>
        </Show>
    }
}
