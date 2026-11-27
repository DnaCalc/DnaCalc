use super::{Area, Item, Key, Kind, Sheet, State, first_cell, source_area, stage};
use calc_skin::{
    AnchorId, Answer, Change, ModelChange, Placement, PlacementKind, PlacementShape,
    PresentationChange, SourceRef, WorkbookId, query,
};
use dna_kit::{Following, Live, Seam, Session};
use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Request {
    workbook: WorkbookId,
    sheet: Sheet,
    columns: u32,
}

#[derive(Clone, Copy)]
pub(super) struct Authoring {
    request: RwSignal<Option<Request>>,
    watch: Following<query::Placement>,
}

impl Authoring {
    pub(super) fn new(session: Session) -> Self {
        Self {
            request: RwSignal::new(None),
            watch: session.following(),
        }
    }

    pub(super) fn open(self, state: State) -> Result<(), String> {
        if !matches!(state.session.seam(), Seam::Idle) {
            return Err("Finish the change in progress first.".into());
        }
        let workbook = state
            .session
            .stamp()
            .ok_or("Open a workbook first.")?
            .workbook;
        let summary = state.session.summary().ok_or("Loading worksheets…")?;
        let visible = |sheet: &&calc_skin::SheetSummary| sheet.visible && sheet.worksheet;
        let preferred = summary
            .sheets
            .iter()
            .filter(visible)
            .find(|sheet| sheet.name.eq_ignore_ascii_case("Analysis"));
        let active = state
            .session
            .active()
            .map(|Key(slot, generation, _, _)| Sheet(slot, generation));
        let destination = preferred
            .or_else(|| {
                summary
                    .sheets
                    .iter()
                    .filter(visible)
                    .find(|sheet| Some(sheet.sheet) == active)
            })
            .ok_or("Select a visible worksheet for the formula.")?;
        self.request.set(Some(Request {
            workbook,
            sheet: destination.sheet,
            columns: 0,
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
    }

    fn current(self, session: Session) -> Option<Request> {
        self.request.get().filter(|request| {
            session
                .stamp()
                .is_some_and(|stamp| stamp.workbook == request.workbook)
        })
    }

    fn placement(self, state: State) -> Live<Answer<Placement>> {
        let query = self.current(state.session).and_then(|request| {
            let snapshot = state.snapshot().ok()?;
            let shape = (request.columns > 0).then_some(PlacementShape {
                rows: 1,
                columns: request.columns,
            });
            let reserved = if shape.is_some() {
                snapshot
                    .chain
                    .cells
                    .iter()
                    .filter_map(|item| {
                        let at = snapshot.source(item.id).and_then(first_cell)?;
                        let width = item.lane_columns?;
                        if Sheet(at.0, at.1) != request.sheet {
                            return None;
                        }
                        let right = at.3.checked_add(width.checked_sub(1)?)?;
                        let bounds =
                            Area(Key(at.0, at.1, 10, at.3), Key(at.0, at.1, 1_048_576, right));
                        bounds.area().map(|_| bounds)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            Some(query::Placement {
                sheet: request.sheet,
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
        let id = AnchorId(snapshot.next_id()?);
        let mut chain = snapshot.chain.clone();
        let index = state.selected(&snapshot).map_or(0, |index| index + 1);
        let mut item = Item::new(id, Kind::Formula);
        item.owned = true;
        item.lane_columns = (request.columns > 0).then_some(request.columns);
        chain.cells.insert(index, item);
        let mut batch = state.prepare_batch(
            &snapshot,
            &chain,
            vec![
                Change::Model(ModelChange::Enter { at: slot, text }),
                Change::Presentation(PresentationChange::Bind {
                    anchor: id,
                    source: source.clone(),
                }),
            ],
            "Add Notebook formula",
        )?;
        batch.base = Some(placement.stamp.model);
        batch.expects.push(source_area(placement.body.footprint));
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
                        Live::Ready(answer) => format!("Worksheet location · {}", stage::address(state.session, Some(answer.body.at))),
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
