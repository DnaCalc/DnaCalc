//! The workbook's server (dc-p96): one open [`Workbook`] and the page's subscriptions to it,
//! answering [`Request`]s and turning calc-host's deltas into the cells the page shows
//! ([`Event::Tiles`]). The calculation worker owns one ([`crate::worker`]); native composition
//! injects the same route through [`crate::transport::InProcessTransport`].
//!
//! A cell crosses as the page shows it: the server works out its text in its number format and
//! its conditional look ([`Workbook::shown`]), its input as it would be edited, and the words for
//! where its value came from. A chart crosses drawn: the server lays it out with the sheet's
//! sizes at 100% and draws it ([`crate::chart::chart_svg`]).

use crate::cells;
use crate::wire::{
    self, Area, Chart, ChartPlace, Event, Intent, Interest, Key, Outcome, Reply, Request, Sheet,
    Summary, Validation,
};
use crate::workbook::Workbook;
use calc_host::value::{CellKey, GridArea};
use calc_host::{
    Axis, AxisChange, AxisEdit, CalcMode, CellEdit, ProjectedValue, Selection, SkinIntent,
    SubscriptionId,
};
use std::collections::BTreeMap;

struct OpenOriginScope {
    slot: std::rc::Rc<std::cell::Cell<Option<crate::workbook::FileOrigin>>>,
    earlier: Option<crate::workbook::FileOrigin>,
}
impl Drop for OpenOriginScope {
    fn drop(&mut self) {
        self.slot.set(self.earlier);
    }
}

/// What the server answers to one request: the reply (or why there is none) and the bytes that
/// travel beside it.
pub struct Answer {
    pub reply: Result<Reply, String>,
    pub bytes: Option<Vec<u8>>,
}

enum PendingSave {
    Calculated(calc_host::CalculatingSave),
    Ordinary(calc_host::OrdinarySave),
}

struct PendingName {
    token: u64,
    id: (u32, u32),
    version: u64,
    anchor: CellKey,
    command: u64,
    work: Option<Box<calc_host::NameCalculation>>,
    result: Option<dna_calc_model::names::NameEvaluation>,
}
impl PendingName {
    fn reply(&self) -> dna_calc_model::names::NameCalculationReply {
        use dna_calc_model::names::NameCalculationReply;
        match &self.result {
            Some(evaluation) => NameCalculationReply::Ready {
                token: self.token,
                evaluation: evaluation.clone(),
            },
            None => NameCalculationReply::Pending { token: self.token },
        }
    }
}
impl PendingSave {
    fn needs_advance(&self, host: &calc_host::HostState) -> bool {
        match self {
            Self::Calculated(s) => s.needs_advance(host),
            Self::Ordinary(s) => s.needs_advance(host),
        }
    }
    fn progress(&self) -> calc_host::CalculatedSaveProgress {
        match self {
            Self::Calculated(s) => s.progress(),
            Self::Ordinary(s) => s.progress(),
        }
    }
    fn model_revision(&self) -> calc_host::AuthoredRevision {
        match self {
            Self::Calculated(s) => s.model_revision(),
            Self::Ordinary(s) => s.model_revision(),
        }
    }
}

pub struct Server {
    workbook_observer: Option<std::rc::Rc<dyn calc_host::WorkbookEventObserver>>,
    open_file_location: Option<String>,
    calculation_budget: calc_host::WorkBudget,
    workbook: u64,
    commands: calc_host::CommandJournal<(Result<Reply, String>, Option<Vec<u8>>)>,
    validation_pending: Option<(u64, u64, Selection, SkinIntent)>,
    book: Workbook,
    /// The browser's way to fetch what WEBSERVICE and IMAGE name; it outlives a workbook.
    fetch_route: Option<std::rc::Rc<dyn calc_func::FetchRoute>>,
    /// A native host's way to read the file IMPORTCSV and IMPORTTEXT name; the browser has none.
    import_route: Option<std::rc::Rc<dyn calc_func::ImportRoute>>,
    /// A native host's user functions, applied to every workbook this server installs.
    user_functions: Option<calc_host::CallableTable>,
    rtd_connector: Option<std::rc::Rc<dyn calc_host::RtdConnector>>,
    model_clock: Option<std::rc::Rc<crate::model_clock::ModelClock>>,
    /// The page's subscriptions: its number for each, and calc-host's.
    subscriptions: BTreeMap<u64, SubscriptionId>,
    subscription_interests: BTreeMap<u64, calc_host::Interest>,
    /// Per subscription: the authored revision its overflow anchors were found at, and those anchors.
    overhang: BTreeMap<u64, (u64, Vec<GridArea>)>,
    last_subscription: Option<u64>,
    events: Vec<Event>,
    deferred_calculation: Vec<Event>,
    tile_streams: BTreeMap<u64, crate::publication::TileStream>,
    next_publication: u64,
    snapshots: BTreeMap<u64, calc_host::CopySnapshot>,
    next_clip: u64,
    clipboard_session: String,
    paste_plan: Option<(u64, calc_host::PastePlan)>,
    pending_save: Option<(u64, PendingSave)>,
    pending_name: Option<PendingName>,
    next_name: u64,
    save_as_target: Option<(String, calc_host::SpreadsheetFormat)>,
    next_save: u64,
    /// Successful creations, per template stem, across workbook replacements in this session.
    template_counters: BTreeMap<String, u64>,
    /// Where the file of the next open came from, when a native host read its path (its Mark of
    /// the Web, the network); taken by that open. A browser cannot tell: unknown (dc-0f1k.22).
    open_origin: std::rc::Rc<std::cell::Cell<Option<crate::workbook::FileOrigin>>>,
    /// The macro security level in force (dc-15dk): Off unless the native host sets another.
    macro_policy: dna_calc_model::MacroPolicy,
    /// The bytes beside the intent being carried out (a picture's image, dc-zjsg.4).
    intent_bytes: Option<Vec<u8>>,
}

impl Default for Server {
    fn default() -> Self {
        Server::new()
    }
}

#[cfg(test)]
mod model_clock_tests {
    use super::*;
    use calc_host::{ProjectedValue, value::CalcValue};
    use std::time::Duration;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn model_clock_drives_one_iteration_per_tick_and_resets_in_the_workbook() {
        let mut server = Server::new();
        server.enable_model_clock();
        let sheet = server.book().host().sheets()[0].id();
        let at = |column| CellKey::new(sheet, 1, column).unwrap();
        let apply = |server: &mut Server, intent| {
            let Reply::Done { outcome, .. } = server
                .execute_current(Request::Intent(intent), None)
                .reply
                .unwrap()
            else {
                panic!("expected a workbook receipt");
            };
            assert!(outcome.accepted, "{:?}", outcome.error);
            while server.has_pending_calculation() {
                server.advance_calculation();
            }
        };
        let enter = |server: &mut Server, column, text: &str| {
            apply(server, Intent::EnterCell {
                at: Key::of(at(column)),
                text: text.into(),
            });
        };
        let expect = |server: &Server, tick, stock| {
            for (column, expected) in [(1, tick), (2, stock)] {
                let cell = server.book().cell(at(column)).unwrap();
                let ProjectedValue::Calculated { value, .. } = cell.value() else {
                    panic!("{}: {:?}", column, cell.value());
                };
                assert_eq!(*value, CalcValue::number(expected), "column {column}");
            }
        };
        let advance = |server: &mut Server, millis| {
            server.advance_model_clock(Duration::from_millis(millis));
            while server.has_pending_calculation() {
                server.advance_calculation();
            }
        };

        apply(
            &mut server,
            Intent::SetIteration {
                enabled: true,
                max_iterations: 1,
                max_change_bits: 0.001_f64.to_bits(),
            },
        );
        enter(&mut server, 3, "2");
        enter(&mut server, 4, "TRUE");
        enter(&mut server, 5, "0");
        enter(&mut server, 1, "=RTD(\"DnaCalc.Clock\",,\"stock\",\"tick\",C1,D1,E1)");
        enter(&mut server, 2, "=IF(OR(A1=0,A1=3),10,B1-1)");
        expect(&server, 0.0, 10.0);
        advance(&mut server, 499);
        expect(&server, 0.0, 10.0);
        advance(&mut server, 1);
        expect(&server, 1.0, 9.0);
        advance(&mut server, 500);
        expect(&server, 2.0, 8.0);
        enter(&mut server, 4, "FALSE");
        advance(&mut server, 1000);
        assert!(matches!(server.book().cell(at(1)).unwrap().value(),
            ProjectedValue::Calculated { value, .. } if *value == CalcValue::number(2.0)));
        enter(&mut server, 5, "1");
        expect(&server, 3.0, 10.0);
        enter(&mut server, 3, "4");
        enter(&mut server, 4, "TRUE");
        advance(&mut server, 250);
        assert!(matches!(server.book().cell(at(1)).unwrap().value(),
            ProjectedValue::Calculated { value, .. } if *value == CalcValue::number(4.0)));
    }
}

#[cfg(test)]
mod mixed_range_tests {
    use super::*;

    /// Exercise the same requests, calculation driver and serialized events as the page.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_hsa5_11__mixed_range_worker_reports_unresolved_targets() {
        for budget in [usize::MAX, 1] {
            let mut server = Server::new();
            server
                .execute_current(Request::Intent(Intent::SetCalcMode { manual: true }), None)
                .reply
                .unwrap();
            server
                .execute_current(
                    Request::Intent(Intent::SetIteration {
                        enabled: true,
                        max_iterations: 1,
                        max_change_bits: 0.001_f64.to_bits(),
                    }),
                    None,
                )
                .reply
                .unwrap();
            let sheet = server.book.host().sheets()[0].id();
            let at = |column| CellKey::new(sheet, 1, column).unwrap();
            for (column, text) in [(1, "=B1+1"), (2, "=A1+1"), (3, "=A1+1"), (4, "=2+2")] {
                server
                    .execute_current(
                        Request::Intent(Intent::EnterCell {
                            at: Key::of(at(column)),
                            text: text.into(),
                        }),
                        None,
                    )
                    .reply
                    .unwrap();
            }
            server
                .execute_current(Request::Intent(Intent::Rebuild), None)
                .reply
                .unwrap();
            while server.has_pending_calculation() {
                server.advance_calculation();
            }
            let old = calc_host::project(
                server.book.host(),
                &calc_host::ProjectionRequest {
                    cells: (1..=3).map(at).collect(),
                },
            )
            .unwrap();
            server.take_events();
            server.set_calculation_budget(calc_host::WorkBudget::evaluations(budget));
            server
                .execute_current(
                    Request::Intent(Intent::CalculateRange {
                        areas: vec![Area::of(GridArea::new(at(1), at(4)).unwrap())],
                    }),
                    None,
                )
                .reply
                .unwrap();
            let mut steps = 0;
            while server.has_pending_calculation() {
                server.advance_calculation();
                steps += 1;
                assert!(steps < 1000);
            }
            let events = server.take_events();
            let events = events
                .iter()
                .map(|event| {
                    let text = dna_calc_protocol::encode(event);
                    dna_calc_protocol::decode::<Event>(&text).unwrap()
                })
                .collect::<Vec<_>>();
            let command = events
                .iter()
                .find_map(|event| match event {
                    Event::Calculation {
                        receipt: wire::CalculationEvent::Admitted { command, .. },
                        ..
                    } => Some(*command),
                    _ => None,
                })
                .unwrap();
            let mut refused = Vec::new();
            for event in &events {
                if let Event::Calculation {
                    receipt:
                        wire::CalculationEvent::TargetsRefused {
                            command: id,
                            targets,
                            owner,
                            ..
                        },
                    ..
                } = event
                    && *id == command
                {
                    assert_eq!(owner, "dc-hsa5.11");
                    for target in targets {
                        assert!(!refused.contains(target));
                        refused.push(*target);
                    }
                }
            }
            assert_eq!(refused.len(), 3);
            assert!((1..=3).all(|c| refused.contains(&Key::of(at(c)))));
            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, Event::Calculation {
                receipt: wire::CalculationEvent::Completed {
                    command: id, outcome: wire::CalculationOutcome::Incomplete,
                    required: 4, calculated: 1, refused: 3, ..
                }, ..
            } if *id == command))
            );
            let now = calc_host::project(
                server.book.host(),
                &calc_host::ProjectionRequest {
                    cells: (1..=4).map(at).collect(),
                },
            )
            .unwrap();
            for (before, after) in old.cells().iter().zip(now.cells()) {
                let retained = |value: &calc_host::ProjectedValue| match value {
                    calc_host::ProjectedValue::Calculated { value, calculation } => {
                        (value.clone(), calculation.tick())
                    }
                    calc_host::ProjectedValue::Stale(stale) => (
                        stale.last_value().unwrap().clone(),
                        stale.last_calculation().unwrap().tick(),
                    ),
                    other => panic!("expected a warm result: {other:?}"),
                };
                assert_eq!(retained(before.value()), retained(after.value()));
            }
            let (value, calculation) = match now.cells()[3].value() {
                calc_host::ProjectedValue::Calculated { value, calculation } => {
                    (value, calculation)
                }
                calc_host::ProjectedValue::Stale(stale) => (
                    stale.last_value().unwrap(),
                    stale.last_calculation().unwrap(),
                ),
                other => panic!("the independent scalar must be evaluated: {other:?}"),
            };
            assert_eq!(value, &calc_host::value::CalcValue::number(4.0));
            assert_eq!(
                calculation.stamp().unwrap().certification.command.sequence,
                command
            );
        }
    }
}

impl Server {
    /// A server over a new, empty workbook.
    pub fn new() -> Server {
        let book = Workbook::new_blank();
        Server {
            workbook_observer: None,
            open_file_location: None,
            calculation_budget: calc_host::WorkBudget::UNLIMITED,
            workbook: 0,
            commands: calc_host::CommandJournal::new(128, 16 * 1024 * 1024),
            validation_pending: None,
            book,
            fetch_route: None,
            import_route: None,
            user_functions: None,
            rtd_connector: None,
            model_clock: None,
            subscriptions: BTreeMap::new(),
            subscription_interests: BTreeMap::new(),
            overhang: BTreeMap::new(),
            last_subscription: None,
            events: Vec::new(),
            deferred_calculation: Vec::new(),
            tile_streams: BTreeMap::new(),
            next_publication: 1,
            snapshots: BTreeMap::new(),
            next_clip: 1,
            clipboard_session: crate::services::session_nonce(),
            paste_plan: None,
            pending_save: None,
            pending_name: None,
            next_name: 1,
            save_as_target: None,
            next_save: 1,
            template_counters: BTreeMap::new(),
            open_origin: Default::default(),
            macro_policy: dna_calc_model::MacroPolicy::default(),
            intent_bytes: None,
        }
    }

    pub fn set_calculation_budget(&mut self, budget: calc_host::WorkBudget) {
        self.calculation_budget = budget;
        self.book.set_calculation_budget(budget);
    }
    pub fn has_pending_calculation(&self) -> bool {
        self.pending_name
            .as_ref()
            .is_some_and(|name| name.work.is_some())
            || self
                .pending_save
                .as_ref()
                .is_some_and(|(_, save)| save.needs_advance(self.book.host()))
            || self.deferred_calculation.len() < wire::MAX_PENDING_REQUESTS - 2
                && self.book.has_pending_calculation()
    }
    pub fn advance_calculation(&mut self) -> calc_host::HostProgress {
        if let Some(name) = &mut self.pending_name
            && let Some(work) = &mut name.work
        {
            let quantum = if self.calculation_budget == calc_host::WorkBudget::UNLIMITED {
                usize::MAX
            } else if self.calculation_budget == calc_host::WorkBudget::evaluations(0) {
                0
            } else {
                64
            };
            let progress = self.book.advance_name_calculation(work, quantum);
            self.collect_name_result();
            self.collect();
            return if progress == calc_host::NameCalculationProgress::Calculating {
                calc_host::HostProgress::Calculating { remaining: 1 }
            } else if self.book.has_pending_calculation() {
                calc_host::HostProgress::Calculating { remaining: 0 }
            } else if self.book.host().calc_mode() == CalcMode::Manual {
                calc_host::HostProgress::Manual
            } else {
                calc_host::HostProgress::Current
            };
        }
        if let Some((token, save)) = self.pending_save.as_mut()
            && save.needs_advance(self.book.host())
        {
            let token = *token;
            let ordinary = matches!(save, PendingSave::Ordinary(_));
            let progress = match save {
                PendingSave::Calculated(save) => {
                    save.advance(self.book.host(), self.calculation_budget)
                }
                PendingSave::Ordinary(save) => self
                    .book
                    .advance_ordinary_save(save, self.calculation_budget),
            };
            let remaining = match save {
                PendingSave::Calculated(save) => save.remaining(),
                _ => 0,
            };
            self.collect();
            if ordinary
                && matches!(
                    progress,
                    calc_host::CalculatedSaveProgress::Ready
                        | calc_host::CalculatedSaveProgress::Failed
                )
            {
                self.events.push(Event::SaveReady {
                    workbook: self.workbook,
                    token,
                });
            }
            return match progress {
                calc_host::CalculatedSaveProgress::Calculating => {
                    calc_host::HostProgress::Calculating { remaining }
                }
                calc_host::CalculatedSaveProgress::Waiting
                | calc_host::CalculatedSaveProgress::WaitingForLive => {
                    calc_host::HostProgress::Waiting { remaining }
                }
                _ if self.book.has_pending_calculation() => {
                    calc_host::HostProgress::Calculating { remaining: 0 }
                }
                _ if self.book.host().calc_mode() == CalcMode::Manual => {
                    calc_host::HostProgress::Manual
                }
                _ => calc_host::HostProgress::Current,
            };
        }
        let progress = self.book.advance_calculation();
        self.collect();
        progress
    }

    fn collect_name_result(&mut self) {
        let Some(name) = &mut self.pending_name else {
            return;
        };
        let Some(work) = &mut name.work else {
            return;
        };
        let Some(result) = work.take_result() else {
            return;
        };
        name.result = Some(crate::names::calculated(
            &self.book,
            name.id,
            name.version,
            name.anchor,
            name.command,
            result,
        ));
        name.work = None;
    }

    pub fn update_rtd(
        &mut self,
        topic: calc_host::TopicKey,
        lease: calc_host::RtdLease,
        sequence: u64,
        value: calc_host::value::CalcValue,
    ) -> calc_host::RtdDisposition {
        let disposition = self.book.update_rtd(topic, lease, sequence, value);
        self.collect();
        disposition
    }
    /// A provider posts its return to this owner. User code resumes in a later budgeted task.
    pub fn complete_async(
        &mut self,
        token: &calc_host::AsyncToken,
        value: calc_host::value::CalcValue,
    ) -> calc_host::AsyncDisposition {
        let disposition = if let calc_host::ResultDestination::PinnedSave { save: id } =
            token.destination
            && let Some((_, PendingSave::Calculated(save))) = self.pending_save.as_mut().filter(
                |(_, save)| matches!(save, PendingSave::Calculated(save) if save.id() == id),
            ) {
            save.complete_async(self.book.host(), token, value)
        } else {
            self.book.complete_async(token, value)
        };
        self.collect();
        disposition
    }
    pub fn pending_async(&self) -> Vec<calc_host::AsyncToken> {
        let mut tokens = self.book.host().pending_async();
        if let Some((_, PendingSave::Calculated(save))) = &self.pending_save {
            tokens.extend(save.pending_async());
        }
        tokens
    }

    /// Changes whenever this server installs a replacement workbook.
    pub fn workbook_id(&self) -> u64 {
        self.workbook
    }

    /// The open workbook, available to native composition and tests.
    pub fn book(&self) -> &Workbook {
        &self.book
    }

    pub fn book_mut(&mut self) -> &mut Workbook {
        &mut self.book
    }

    /// The events since the last call, in order.
    pub fn take_events(&mut self) -> Vec<Event> {
        self.collect();
        std::mem::take(&mut self.events)
    }

    /// Answers `request`; `bytes` are those that travel beside it (an open's package).
    pub fn handle(&mut self, request: Request, bytes: Option<Vec<u8>>) -> Answer {
        if let Request::Command(envelope) = request {
            return self.command(envelope, bytes);
        }
        if let Request::CellImage { at } = request {
            return self.cell_image(at);
        }
        if let Request::ShapePicture { sheet, id } = request {
            return self.shape_picture(sheet, id);
        }
        if request.is_command() {
            return Answer {
                reply: Err("A workbook command needs an identity and base revision".into()),
                bytes: None,
            };
        }
        self.handle_admitted(request, bytes)
    }

    /// Immediate clients resolve their revision and target immediately before delivery. Remote
    /// clients construct an envelope at submission and retain it unchanged for any retry.
    pub fn envelope(&self, request: Request) -> Request {
        Request::Command(wire::CommandEnvelope {
            session: self.clipboard_session.clone(),
            id: self.commands.next_id(),
            workbook: self.workbook,
            base_revision: self.book.host().revision().get(),
            source: "headless".into(),
            undo_group: None,
            resolved_selection: self.current_selection(),
            request: Box::new(request),
        })
    }
    pub fn execute_current(&mut self, request: Request, bytes: Option<Vec<u8>>) -> Answer {
        let request = if request.is_command() {
            self.envelope(request)
        } else {
            request
        };
        self.handle(request, bytes)
    }
    fn current_selection(&self) -> Option<(Key, Vec<Area>)> {
        let selection = self.book.session().selection();
        selection.active.map(|at| {
            (
                Key::of(at),
                selection.areas.iter().copied().map(Area::of).collect(),
            )
        })
    }
    fn command(&mut self, envelope: wire::CommandEnvelope, bytes: Option<Vec<u8>>) -> Answer {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hash);
        if envelope.session != self.clipboard_session {
            return Answer {
                reply: Err("This command belongs to another workbook session".into()),
                bytes: None,
            };
        }
        let payload = wire::encode(&(envelope.clone(), hash.finish()));
        let stamp = calc_host::CommandStamp {
            id: envelope.id,
            workbook: envelope.workbook,
            base_revision: envelope.base_revision,
        };
        let admission = self.commands.admit(
            stamp,
            self.workbook,
            self.book.host().revision().get(),
            &payload,
        );
        match admission {
            Ok(Some((reply, bytes))) => return Answer { reply, bytes },
            Err(reason) => {
                return Answer {
                    reply: Err(reason.to_string()),
                    bytes: None,
                };
            }
            Ok(None) => {}
        }
        let mut answer = if !envelope.request.is_command() {
            Answer {
                reply: Err("A command envelope must contain one workbook command".into()),
                bytes: None,
            }
        } else if self.deferred_calculation.len() >= wire::MAX_PENDING_REQUESTS - 2 {
            Answer {
                reply: Err("Calculation delivery is waiting for tile acknowledgements".into()),
                bytes: None,
            }
        } else if envelope.resolved_selection != self.current_selection() {
            Answer {reply:Err("The command's resolved selection has changed; submit it again for the current target".into()),bytes:None}
        } else {
            self.handle_admitted(*envelope.request, bytes)
        };
        if let Ok(Reply::NameEvaluation(value)) = &mut answer.reply {
            value.command = stamp.id;
        }
        if let Ok(Reply::NameCalculation(dna_calc_model::names::NameCalculationReply::Pending {
            token,
        })) = &answer.reply
            && let Some(name) = self
                .pending_name
                .as_mut()
                .filter(|name| name.token == *token && name.command == 0)
        {
            name.command = stamp.id;
        }
        let reply_bytes = wire::encode(&answer.reply)
            .len()
            .saturating_add(answer.bytes.as_ref().map_or(0, Vec::len));
        self.commands.remember(
            stamp.id,
            payload,
            (answer.reply.clone(), answer.bytes.clone()),
            reply_bytes,
        );
        answer
    }
    fn handle_admitted(&mut self, request: Request, bytes: Option<Vec<u8>>) -> Answer {
        let mut out = None;
        let reply = (|| -> Result<Reply, String> {
            match request {
                Request::AutomationBatch(_) | Request::AutomationRelease(_) => {
                    Err("Office.js automation is not available on this transport (dc-17spk.21)".into())
                }
                Request::Names { query, page } => {
                    Ok(Reply::Names(crate::names::page(&self.book, query, page)))
                }
                Request::EvaluateName {
                    id,
                    version,
                    anchor,
                } => Ok(Reply::NameEvaluation(crate::names::evaluate(
                    &self.book,
                    id,
                    version,
                    cell(anchor)?,
                ))),
                Request::EvaluatePart {
                    formula,
                    anchor,
                    start,
                    len,
                } => Ok(Reply::Part(cell(anchor).and_then(|anchor| {
                    crate::parts::evaluate_part(
                        self.book.host(),
                        self.book.profile(),
                        anchor,
                        &formula,
                        start,
                        len,
                    )
                }))),
                Request::TraceFormula { formula, anchor } => {
                    let anchor = cell(anchor)?;
                    Ok(Reply::Trace(crate::parts::trace(
                        self.book.host(),
                        self.book.profile(),
                        anchor,
                        &formula,
                    )))
                }
                Request::UserFunctions => {
                    Ok(Reply::UserFunctions(self.book.host().user_function_names()))
                }
                // Add-ins are native code: the desktop app answers these itself (dc-0f1k.23); a
                // worker on its own, as in a browser, has none and loads none.
                Request::Addins => Ok(Reply::Addins(wire::AddinList::default())),
                Request::LoadAddinNative
                | Request::LoadAddin { .. }
                | Request::UnloadAddin { .. } => Err(ADDINS_DESKTOP_ONLY.to_owned()),
                Request::CalculateName {
                    id,
                    version,
                    anchor,
                } => {
                    if self
                        .pending_name
                        .as_ref()
                        .is_some_and(|name| name.work.is_some())
                    {
                        return Err("A selected-name calculation is already running".into());
                    }
                    let anchor = cell(anchor)?;
                    let token = self.next_name;
                    let next = token
                        .checked_add(1)
                        .ok_or("Name calculation tokens exhausted")?;
                    let work = self.book.begin_name_calculation(
                        calc_host::value::NameId::from_parts(id.0, id.1),
                        version,
                        anchor,
                    )?;
                    self.next_name = next;
                    self.pending_name = Some(PendingName {
                        token,
                        id,
                        version,
                        anchor,
                        command: 0,
                        work: Some(Box::new(work)),
                        result: None,
                    });
                    Ok(Reply::NameCalculation(
                        self.pending_name
                            .as_ref()
                            .expect("an admitted name task")
                            .reply(),
                    ))
                }
                Request::PollNameCalculation { token } => {
                    let name = self
                        .pending_name
                        .as_ref()
                        .filter(|name| name.token == token)
                        .ok_or("The selected-name calculation token has expired")?;
                    Ok(Reply::NameCalculation(name.reply()))
                }
                Request::CancelNameCalculation { token } => {
                    let name = self
                        .pending_name
                        .as_mut()
                        .filter(|name| name.token == token)
                        .ok_or("The selected-name calculation token has expired")?;
                    if let Some(work) = &mut name.work {
                        work.cancel();
                    }
                    self.collect_name_result();
                    Ok(Reply::NameCalculation(
                        self.pending_name
                            .as_ref()
                            .expect("the selected name task")
                            .reply(),
                    ))
                }
                Request::FormatSelection(areas) => Ok(Reply::FormatSelection(
                    areas
                        .into_iter()
                        .map(area)
                        .collect::<Result<Vec<_>, _>>()
                        .and_then(|areas| crate::format::selection(&self.book, areas)),
                )),
                Request::PreviewFormat {
                    at,
                    revision,
                    patch,
                } => Ok(Reply::FormatPreview(
                    if revision != self.book.host().revision().get() {
                        Err("The workbook changed; reopen Format Cells".into())
                    } else {
                        cell(at).and_then(|at| crate::format::preview(&self.book, at, patch))
                    },
                )),
                Request::Summary => Ok(Reply::Summary(self.summary())),
                Request::Command(_) => Err("Nested command envelope".into()),
                Request::Find(options) => {
                    Ok(Reply::Found(crate::search::find(&self.book, &options)))
                }
                Request::GoToSpecial { sheet, areas, kind } => Ok(Reply::SpecialCells(
                    crate::search::special_cells(&self.book, sheet, areas, kind),
                )),
                Request::SelectionStats => Ok(Reply::SelectionStats(
                    crate::search::selection_stats(&self.book, self.book.session().selection()),
                )),
                Request::CanEdit(key) => Ok(Reply::CanEdit(
                    cell(key).and_then(|at| calc_host::can_edit(self.book.host(), at)),
                )),
                Request::FitContents { area: target } => self.fit_contents(area(target)?),
                Request::Capture { area: source, keep } => {
                    let from = area(source)?;
                    let snapshot = calc_host::CopySnapshot::capture(self.book.host(), from)
                        .map_err(|e| e.to_string())?;
                    let token = self.next_clip;
                    self.next_clip += 1;
                    let cells: Vec<_> = (from.first().row()..=from.last().row())
                        .flat_map(|row| {
                            (from.first().column()..=from.last().column())
                                .map(move |col| CellKey::new(from.sheet(), row, col).unwrap())
                        })
                        .collect();
                    let page = calc_host::project(
                        self.book.host(),
                        &calc_host::ProjectionRequest { cells },
                    )
                    .map_err(|e| e.to_string())?;
                    let rows: Vec<Vec<_>> = page
                        .cells()
                        .chunks(from.column_count() as usize)
                        .map(|row| row.iter().map(|cell| self.book.shown(cell).text).collect())
                        .collect();
                    let text = dna_calc_model::clipboard::encode_tsv(&rows);
                    let html = dna_calc_model::clipboard::encode_html(
                        &rows,
                        token,
                        &self.clipboard_session,
                    );
                    self.snapshots.retain(|id, _| Some(*id) == keep);
                    self.snapshots.insert(token, snapshot);
                    Ok(Reply::Clipboard(wire::Clipboard {
                        token,
                        session: self.clipboard_session.clone(),
                        area: source,
                        revision: self.book.host().revision().get(),
                        text,
                        html,
                    }))
                }
                Request::PreparePaste {
                    source,
                    to,
                    mode,
                    transpose,
                } => self.prepare_paste(source, area(to)?, mode, transpose),
                Request::Ping => Ok(Reply::Pong {
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                }),
                Request::InspectTemplate => {
                    let host = calc_host::HostState::open(
                        std::sync::Arc::new(calc_host::MemorySource::new(
                            bytes.unwrap_or_default(),
                        )),
                        &calc_host::OpenPolicy::default(),
                        calc_host::HostOptions::default(),
                    )
                    .map_err(|e| e.to_string())?;
                    let format = match host.loaded().spreadsheet_format() {
                        calc_host::SpreadsheetFormat::Xltx => wire::FileFormat::Xltx,
                        calc_host::SpreadsheetFormat::Xltm => wire::FileFormat::Xltm,
                        _ => {
                            return Err(
                                "Choose an actual .xltx or .xltm template package (dc-bctq)".into(),
                            );
                        }
                    };
                    Ok(Reply::TemplateFormat(format))
                }
                Request::InsertTemplate { name, revision } => self.dispatch_entry(
                    SkinIntent::Command(calc_host::WorkbookCommand::InsertTemplate {
                        bytes: bytes.unwrap_or_default().into(),
                        name,
                        revision,
                    }),
                    false,
                ),
                Request::OpenNative => {
                    Err("Native file selection is unavailable in this worker".into())
                }
                Request::New => {
                    self.install(Workbook::new_blank());
                    Ok(self.done(true, None, 0.0))
                }
                request @ (Request::Open { .. }
                | Request::OpenForEditing { .. }
                | Request::OpenTemplate { .. }) => {
                    let template_only = matches!(request, Request::OpenTemplate { .. });
                    let edit_template = matches!(request, Request::OpenForEditing { .. });
                    let name = match request {
                        Request::Open { name }
                        | Request::OpenForEditing { name }
                        | Request::OpenTemplate { name } => name,
                        _ => unreachable!(),
                    };
                    let started = crate::services::now_ms();
                    let bytes = bytes.unwrap_or_default();
                    if template_only {
                        let host = calc_host::HostState::open(
                            std::sync::Arc::new(calc_host::MemorySource::new(bytes.clone())),
                            &calc_host::OpenPolicy::default(),
                            calc_host::HostOptions::default(),
                        )
                        .map_err(|e| e.to_string())?;
                        if !host.loaded().spreadsheet_format().is_template() {
                            return Err("The stored default is not an actual template package; remove it and add an .xltx or .xltm template (dc-bctq)".into());
                        }
                    }
                    crate::workbook::reserve_for_open(bytes.len());
                    let counter_key =
                        calc_value::fold_workbook_name(crate::workbook::template_basename(&name));
                    let counter = self
                        .template_counters
                        .get(&counter_key)
                        .copied()
                        .unwrap_or(0)
                        .checked_add(1)
                        .ok_or("The template workbook name counter is exhausted (dc-bctq)")?;
                    match Workbook::open_with_observer(
                        &name,
                        bytes,
                        self.calculation_budget,
                        self.open_origin
                            .take()
                            .unwrap_or(crate::workbook::FileOrigin::Unknown),
                        edit_template,
                        counter,
                        self.workbook_observer.clone(),
                        self.open_file_location.take(),
                    ) {
                        Ok(book) => {
                            if book.unsaved() {
                                self.template_counters.insert(counter_key, counter);
                            }
                            let elapsed = crate::services::now_ms() - started;
                            let calculated = book.calculated_on_open();
                            self.install(book);
                            self.run_macros_on_open();
                            self.book.notify_workbook_open();
                            Ok(Reply::Done {
                                outcome: self.outcome(true, None, elapsed),
                                opened: Some((elapsed, calculated)),
                                saved: None,
                            })
                        }
                        Err(refused) => Ok(Reply::Done {
                            outcome: Outcome {
                                error: Some(refused.to_string()),
                                ..self.outcome(false, None, 0.0)
                            },
                            opened: None,
                            saved: None,
                        }),
                    }
                }
                Request::LoadVbaModule {
                    name,
                    source,
                    max_steps,
                } => {
                    self.book.load_vba_module(&name, &source, max_steps)?;
                    self.collect();
                    Ok(self.done(true, None, 0.0))
                }
                Request::LoadVbaProject { modules, max_steps } => {
                    self.book.load_vba_project(&modules, max_steps)?;
                    self.collect();
                    Ok(self.done(true, None, 0.0))
                }
                Request::SetVbaEventsEnabled { enabled } => {
                    self.book.set_vba_events_enabled(enabled);
                    Ok(self.done(true, None, 0.0))
                }
                request @ (Request::EnableMacros | Request::UnblockMacros) => {
                    let mode = if matches!(request, Request::UnblockMacros) {
                        crate::workbook::EnableMode::Unblock
                    } else {
                        crate::workbook::EnableMode::Person
                    };
                    self.book
                        .enable_macros_as(mode)
                        .map_err(|refused| refused.to_string())?;
                    self.collect();
                    Ok(self.done(true, None, 0.0))
                }
                Request::SetMacroSecurity { level } => {
                    self.macro_policy.choose(level)?;
                    Ok(self.done(true, None, 0.0))
                }
                request @ (Request::Save
                | Request::SaveWithContext { .. }
                | Request::SaveAs { .. }) => {
                    if self.pending_save.is_some() {
                        return Err("Finish or cancel the pending save first".into());
                    }
                    // An explicit setting is honoured; otherwise the workbook's own (calcOnSave), which
                    // only a Manual workbook acts on (Excel recalculates before saving in Manual mode).
                    let calculate_before_save = match &request {
                        Request::SaveWithContext {
                            calculate_before_save: Some(explicit),
                            ..
                        } => *explicit,
                        Request::SaveWithContext { .. } => self.book.calculates_before_save(),
                        _ => false,
                    };
                    let views = match &request {
                        Request::SaveWithContext { views: Some(v), .. }
                        | Request::SaveAs { views: Some(v), .. } => {
                            Some(std::sync::Arc::new(self.save_window(v)?))
                        }
                        _ => None,
                    };
                    let target = if let Request::SaveAs { name, format, .. } = &request {
                        if name.trim().is_empty() || name.contains(['/', '\\', '\0']) {
                            return Err("Type a file name without folders".into());
                        }
                        use wire::FileFormat as F;
                        let format = match format {
                            F::Xlsx => calc_host::SpreadsheetFormat::Xlsx,
                            F::Xlsm => calc_host::SpreadsheetFormat::Xlsm,
                            F::Xltx => calc_host::SpreadsheetFormat::Xltx,
                            F::Xltm => calc_host::SpreadsheetFormat::Xltm,
                        };
                        if !name
                            .to_ascii_lowercase()
                            .ends_with(&format!(".{}", format.extension()))
                        {
                            return Err("The file name must match the selected format".into());
                        }
                        Some((name.clone(), format))
                    } else {
                        None
                    };
                    let format = target
                        .as_ref()
                        .map_or(self.book.output_format(), |(_, f)| *f);
                    let policy = calc_host::SavePolicy {
                        views,
                        format: calc_host::SaveFormat::Spreadsheet(format),
                        ..Default::default()
                    };
                    let save = self.book.begin_ordinary_save_operation(
                        calculate_before_save,
                        &policy,
                        target.is_some(),
                    )?;
                    if matches!(
                        save.progress(),
                        calc_host::CalculatedSaveProgress::Ready
                            | calc_host::CalculatedSaveProgress::Failed
                    ) {
                        let name = target
                            .as_ref()
                            .map_or_else(|| self.book.save_name(), |(n, _)| n.clone());
                        match self.book.finish_ordinary_save(save) {
                            Ok((bytes, _)) => {
                                if let Some((name, format)) = target {
                                    self.book.saved_as(name, format);
                                }
                                out = Some(bytes);
                                Ok(Reply::Done {
                                    outcome: self.outcome(true, None, 0.0),
                                    opened: None,
                                    saved: Some(name),
                                })
                            }
                            Err(failure) => Ok(Reply::Done {
                                outcome: Outcome {
                                    error: Some(failure),
                                    ..self.outcome(false, None, 0.0)
                                },
                                opened: None,
                                saved: None,
                            }),
                        }
                    } else {
                        let token = self.next_save;
                        self.next_save = self
                            .next_save
                            .checked_add(1)
                            .expect("save request identity exhausted");
                        let waiting = save.progress() == calc_host::CalculatedSaveProgress::Waiting;
                        self.save_as_target = target;
                        self.pending_save = Some((token, PendingSave::Ordinary(save)));
                        Ok(Reply::SavePending { token, waiting })
                    }
                }
                Request::SaveCalculated => {
                    if self.pending_save.is_some() {
                        return Err("Finish or cancel the pending calculated save first".into());
                    }
                    let save = self.book.begin_calculated_save()?;
                    let token = self.next_save;
                    self.next_save = self
                        .next_save
                        .checked_add(1)
                        .expect("save request identity exhausted");
                    self.pending_save = Some((token, PendingSave::Calculated(save)));
                    Ok(Reply::SavePending {
                        token,
                        waiting: false,
                    })
                }
                Request::PollSave { token } => {
                    use calc_host::CalculatedSaveProgress as P;
                    let (current, save) = self
                        .pending_save
                        .as_ref()
                        .ok_or("No calculated save is pending")?;
                    if *current != token {
                        return Err("This calculated save is no longer pending".into());
                    }
                    match save.progress() {
                        P::Ready | P::Failed => {
                            let (_, save) = self.pending_save.take().expect("checked save");
                            let model_revision = save.model_revision().get();
                            let target = self.save_as_target.take();
                            let name = target
                                .as_ref()
                                .map_or_else(|| self.book.save_name(), |(n, _)| n.clone());
                            let (bytes, _) = match save {
                                PendingSave::Calculated(save) => {
                                    self.book.finish_calculated_save(save)?
                                }
                                PendingSave::Ordinary(save) => {
                                    self.book.finish_ordinary_save(save)?
                                }
                            };
                            if let Some((name, format)) = target {
                                self.book.saved_as(name, format);
                            }
                            out = Some(bytes);
                            Ok(Reply::SaveImage {
                                outcome: self.outcome(true, None, 0.0),
                                saved: name,
                                model_revision,
                            })
                        }
                        progress => Ok(Reply::SavePending {
                            token,
                            waiting: matches!(progress, P::Waiting | P::WaitingForLive),
                        }),
                    }
                }
                Request::Intent(intent) => {
                    self.intent_bytes = bytes;
                    let reply = self.intent(intent);
                    self.intent_bytes = None;
                    reply
                }
                Request::Subscribe {
                    workbook,
                    id,
                    interest,
                } => {
                    if workbook != self.workbook {
                        return Err("This subscription belongs to a replaced workbook".into());
                    }
                    if self.last_subscription.is_some_and(|last| id <= last) {
                        return Err(
                            "Subscription identities must increase and cannot be reused".into()
                        );
                    }
                    if self.subscriptions.contains_key(&id) {
                        return Err("Subscription identity is already in use".into());
                    }
                    if self.subscriptions.len() >= wire::MAX_SUBSCRIPTIONS {
                        return Err("Too many tile subscriptions".into());
                    }
                    let interest = host_interest(interest)?;
                    let extras = self.overhang_of(&interest);
                    match self.book.subscribe(widened(&interest, &extras)) {
                        Ok(host) => {
                            self.last_subscription = Some(id);
                            self.overhang
                                .insert(id, (self.book.host().revision().get(), extras));
                            self.subscription_interests.insert(id, interest);
                            self.subscriptions.insert(id, host);
                            self.tile_streams.insert(id, Default::default());
                            Ok(Reply::Subscribed)
                        }
                        Err(error) => Err(error.to_string()),
                    }
                }
                Request::SetInterest {
                    workbook,
                    id,
                    interest,
                } => {
                    if workbook != self.workbook {
                        return Err("This interest belongs to a replaced workbook".into());
                    }
                    let interest = host_interest(interest)?;
                    let subscription =
                        *self.subscriptions.get(&id).ok_or("no such subscription")?;
                    let extras = self.overhang_of(&interest);
                    let receipt = self.book.dispatch(SkinIntent::SetInterest {
                        subscription,
                        interest: widened(&interest, &extras),
                    });
                    if let Some(error) = receipt.error {
                        return Err(error.to_string());
                    }
                    self.overhang
                        .insert(id, (self.book.host().revision().get(), extras));
                    self.subscription_interests.insert(id, interest);
                    Ok(Reply::Subscribed)
                }
                Request::TileAck(barrier) => {
                    if barrier.workbook == self.workbook {
                        if let Some(stream) = self.tile_streams.get_mut(&barrier.id) {
                            stream.acknowledge(barrier);
                        }
                    }
                    Ok(Reply::Subscribed)
                }
                Request::ResetTiles { workbook, id } => {
                    if workbook != self.workbook {
                        return Err("This reset belongs to a replaced workbook".into());
                    }
                    let failed = self
                        .tile_streams
                        .get(&id)
                        .ok_or("No such tile subscription")?
                        .failed();
                    self.retire_tile_events(id);
                    if failed {
                        // A budget failure discarded the retained root. Only a host snapshot
                        // can repair it; later deltas cannot reconstruct untouched cells.
                        let subscription =
                            *self.subscriptions.get(&id).ok_or("No such subscription")?;
                        let interest = self
                            .subscription_interests
                            .get(&id)
                            .ok_or("No such interest")?
                            .clone();
                        let extras = self.overhang_of(&interest);
                        let receipt = self.book.dispatch(SkinIntent::SetInterest {
                            subscription,
                            interest: widened(&interest, &extras),
                        });
                        self.overhang
                            .insert(id, (self.book.host().revision().get(), extras));
                        if let Some(error) = receipt.error {
                            return Err(error.to_string());
                        }
                    }
                    self.tile_streams.get_mut(&id).unwrap().reset();
                    Ok(Reply::Subscribed)
                }
                Request::Unsubscribe { workbook, id } => {
                    if workbook != self.workbook {
                        return Ok(Reply::Subscribed);
                    }
                    self.retire_tile_events(id);
                    self.subscription_interests.remove(&id);
                    self.overhang.remove(&id);
                    self.tile_streams.remove(&id);
                    if let Some(host) = self.subscriptions.remove(&id) {
                        self.book.unsubscribe(host);
                    }
                    Ok(Reply::Subscribed)
                }
                Request::FilterValues {
                    sheet,
                    table,
                    column,
                } => Ok(Reply::FilterValues {
                    revision: self.book.host().revision().get(),
                    values: calc_host::filter_values_for(
                        self.book.host(),
                        sheet.id(),
                        table.as_deref(),
                        column,
                    ),
                    criteria: table
                        .as_ref()
                        .map_or_else(
                            || {
                                self.book
                                    .host()
                                    .model()
                                    .definitions()
                                    .auto_filter(sheet.id())
                            },
                            |part| {
                                self.book
                                    .host()
                                    .model()
                                    .definitions()
                                    .tables()
                                    .iter()
                                    .find(|t| t.part == *part && t.sheet == sheet.id())
                                    .and_then(|t| t.filter.as_ref())
                            },
                        )
                        .and_then(|f| f.columns.get(&column))
                        .map(|c| match c {
                            calc_host::FilterCriteria::Values { values, blank } => {
                                wire::FilterChange::Values {
                                    column,
                                    values: values.clone(),
                                    blank: *blank,
                                }
                            }
                            calc_host::FilterCriteria::Dates {
                                groups,
                                values,
                                blank,
                            } => wire::FilterChange::Dates {
                                column,
                                groups: groups.clone(),
                                values: values.clone(),
                                blank: *blank,
                            },
                            calc_host::FilterCriteria::Top {
                                top,
                                percent,
                                value,
                            } => wire::FilterChange::Top {
                                column,
                                top: *top,
                                percent: *percent,
                                value: value.clone(),
                            },
                            calc_host::FilterCriteria::Calendar { period, .. } => {
                                wire::FilterChange::Calendar {
                                    column,
                                    period: period.clone(),
                                }
                            }
                            calc_host::FilterCriteria::Average { above, .. } => {
                                wire::FilterChange::Average {
                                    column,
                                    above: *above,
                                }
                            }
                            calc_host::FilterCriteria::Color { dxf_id, cell_color } => {
                                wire::FilterChange::ColorReference {
                                    column,
                                    dxf_id: *dxf_id,
                                    cell_color: *cell_color,
                                }
                            }
                            calc_host::FilterCriteria::Custom { and, predicates } => {
                                wire::FilterChange::Custom {
                                    column,
                                    and: *and,
                                    predicates: predicates
                                        .iter()
                                        .map(|p| (p.operator.xml().to_owned(), p.value.clone()))
                                        .collect(),
                                }
                            }
                        }),
                }),
                Request::Geometry(sheet) => Ok(Reply::Geometry(self.geometry(sheet))),
                Request::UsedArea(sheet) => Ok(Reply::UsedArea(
                    self.book.used_area(sheet.id()).map(Area::of),
                )),
                Request::ResolveReference { anchor, text } => {
                    let at = cell(anchor)?;
                    let resolved = match self.book.host().resolve_reference_text(at, text.trim()) {
                        calc_host::ReferenceTarget::Area(area) => {
                            if self
                                .book
                                .host()
                                .model()
                                .sheet_by_id(area.sheet())
                                .is_some_and(|s| {
                                    s.visibility() != calc_host::SheetVisibility::Visible
                                })
                            {
                                Err("Unhide this worksheet before navigating to it".to_owned())
                            } else {
                                Ok(Area::of(area))
                            }
                        }
                        calc_host::ReferenceTarget::Invalid => Err(format!(
                            "{text:?} is not a valid address, range or reference name"
                        )),
                        calc_host::ReferenceTarget::NotModelled => Err(format!(
                            "The reference {text:?} is not modelled; use an explicit sheet address or range"
                        )),
                    };
                    Ok(Reply::ResolvedReference(resolved))
                }
                Request::Review(at) => Ok(Reply::Review(self.review(cell(at)?))),
                Request::ReviewMarkers(sheet) => {
                    let review = self.book.host().sheet_review(sheet.id());
                    let mut markers: BTreeMap<Key, (String, &str)> = BTreeMap::new();
                    for note in review.notes.iter().filter(|n| !n.compatibility) {
                        let kind = if note.source.as_ref().is_some_and(|s| s.threaded()) {
                            "Comment"
                        } else {
                            "Note"
                        };
                        markers
                            .entry(Key::of(note.at))
                            .and_modify(|(label, preview)| {
                                if label != kind {
                                    *label = "Comment and note".into();
                                }
                                if preview.is_empty() {
                                    *preview = note.text.trim();
                                }
                            })
                            .or_insert_with(|| (kind.into(), note.text.trim()));
                    }
                    for link in review.links {
                        markers
                            .entry(Key::of(link.area.first()))
                            .and_modify(|(label, _)| {
                                if !label.ends_with(" and link") {
                                    label.push_str(" and link");
                                }
                            })
                            .or_insert_with(|| ("Link".into(), ""));
                    }
                    Ok(Reply::ReviewMarkers(
                        markers
                            .into_iter()
                            .take(5000)
                            .map(|(at, (kind, text))| {
                                let mut chars = text.chars();
                                let mut preview: String = chars.by_ref().take(240).collect();
                                if chars.next().is_some() {
                                    preview.push('…');
                                }
                                wire::ReviewMarker { at, kind, preview }
                            })
                            .collect(),
                    ))
                }
                Request::PageSetup(sheet) => Ok(Reply::PageSetup(crate::printing::settings(
                    &self.book, sheet,
                ))),
                Request::PrintPreview { sheet, revision } => {
                    Ok(Reply::PrintPreview(crate::printing::preview(
                        &self.book,
                        sheet,
                        revision,
                        self.geometry(sheet),
                        self.charts(sheet),
                        self.drawn_shapes_where(sheet, |shape| shape.prints),
                        self.drawn_sparklines(sheet),
                        {
                            let printed: std::collections::BTreeSet<u32> = self
                                .book
                                .host()
                                .form_controls(sheet.id())
                                .iter()
                                .filter(|v| v.control.prints)
                                .map(|v| v.control.id)
                                .collect();
                            self.drawn_controls(sheet).into_iter().filter(|c| printed.contains(&c.id)).collect()
                        },
                    )))
                }
                Request::PivotSourceFields { area, revision } => Ok(Reply::PivotSourceFields(
                    if revision != self.book.host().revision().get() {
                        Err("The source changed; select it again".into())
                    } else {
                        area.area()
                            .ok_or_else(|| "Select one source range".into())
                            .and_then(|area| calc_host::pivot_source_fields(self.book.host(), area))
                    },
                )),
                Request::PivotTables(sheet) => {
                    let (tables, issues) = self.pivots(sheet);
                    Ok(Reply::PivotTables { tables, issues })
                }
                Request::Charts(sheet) => Ok(Reply::Charts(self.charts(sheet))),
                Request::Shapes(sheet) => Ok(Reply::Shapes(self.drawn_shapes(sheet))),
                Request::Sparklines(sheet) => Ok(Reply::Sparklines(self.drawn_sparklines(sheet))),
                Request::Controls(sheet) => Ok(Reply::Controls(self.drawn_controls(sheet))),
                Request::CellImage { .. } => Err("handle answers a cell image".into()),
                Request::ShapePicture { .. } => Err("handle answers a shape picture".into()),
                Request::AtlasRefusals { reason, page } => Ok(Reply::AtlasRefusals(
                    self.book
                        .refusal_cells(&reason, usize::try_from(page).unwrap_or(usize::MAX)),
                )),
                Request::Atlas => {
                    let census = self.book.census();
                    let session = self.book.session();
                    let (undo, redo) = session.history_depth();
                    Ok(Reply::Atlas(wire::Atlas {
                        mode: match session.calc_mode() {
                            CalcMode::Automatic => "automatic",
                            CalcMode::AutomaticExceptTables => "automatic except tables",
                            CalcMode::Manual => "manual",
                        }
                        .to_owned(),
                        revision: self.book.host().revision().get(),
                        undo,
                        redo,
                        intents: session.intent_log().len(),
                        name: self.book.name().to_owned(),
                        changed: self.book.changed(),
                        memory: crate::services::memory_bytes(),
                        census,
                    }))
                }
                Request::Outline {
                    shown,
                    open,
                    reveal,
                    query,
                } => Ok(Reply::Outline(self.outline(
                    &shown,
                    &open,
                    reveal.and_then(|k| k.cell()),
                    &query,
                ))),
                Request::Notebook {
                    sheet,
                    page,
                    reveal,
                    placement,
                } => Ok(Reply::Notebook(self.notebook(
                    sheet,
                    usize::try_from(page).unwrap_or(usize::MAX),
                    reveal.and_then(|k| k.cell()),
                    placement.as_deref(),
                ))),
                Request::BlockDetail(target) => {
                    Ok(Reply::BlockDetail(self.block_detail(area(target)?)))
                }
                Request::Dependents(at) => {
                    let at = cell(at)?;
                    Ok(Reply::Keys(
                        self.book
                            .host()
                            .dependents(at)
                            .into_iter()
                            .map(Key::of)
                            .collect(),
                    ))
                }
                Request::Canvas {
                    center,
                    back,
                    forward,
                } => Ok(Reply::Canvas(crate::canvas::view_model(
                    &self.book,
                    cell(center)?,
                    back,
                    forward,
                ))),
                Request::Ledger {
                    sheet,
                    filter,
                    text,
                    sort,
                    page,
                    active,
                } => Ok(Reply::Ledger(crate::ledger::view_model(
                    &self.book,
                    sheet.map(Sheet::id),
                    &filter,
                    &text,
                    &sort,
                    page as usize,
                    active.and_then(Key::cell),
                ))),
                Request::ValidationInvalid(sheet) => {
                    if self.book.host().model().sheet_by_id(sheet.id()).is_none() {
                        return Err("The worksheet no longer exists".into());
                    }
                    let report = self.book.host().scan_validation(sheet.id());
                    Ok(Reply::ValidationInvalid(wire::ValidationScan {
                        sheet,
                        revision: self.book.host().revision().get(),
                        invalid: report.invalid.into_iter().map(Key::of).collect(),
                        checked: report.checked,
                        valid: report.valid,
                        unavailable: report.unavailable,
                        truncated: report.truncated,
                        reasons: report.reasons,
                    }))
                }
                Request::Validation(at) => {
                    let at = cell(at)?;
                    Ok(Reply::Validation(self.book.validation(at).map(|rule| {
                        Validation {
                            dropdown: rule.dropdown,
                            prompt: rule.prompt,
                            items: self.book.validation_items(at),
                        }
                    })))
                }
            }
        })();
        self.collect();
        Answer { reply, bytes: out }
    }

    /// The image of the picture `id` on `sheet`, its bytes beside the reply; `None` when there is none the page can decode.
    fn shape_picture(&self, sheet: Sheet, id: u32) -> Answer {
        fn find(shapes: &[calc_host::Shape], id: u32) -> Option<&calc_host::Shape> {
            shapes.iter().find_map(|s| {
                // A picture, or a shape filled with a picture (dc-zjsg.9).
                if s.id == id && (s.kind == calc_host::ShapeKind::Picture || matches!(s.fill, calc_host::ShapeFill::Picture(_))) {
                    Some(s)
                } else {
                    find(&s.children, id)
                }
            })
        }
        let found = find(self.book.host().shapes(sheet.id()), id)
            .and_then(|shape| shape.media.as_ref())
            .and_then(|media| crate::picture::source(media).ok());
        match found {
            Some((content_type, bytes)) => Answer { reply: Ok(Reply::ShapePicture(Some(content_type.to_owned()))), bytes: Some(bytes) },
            None => Answer { reply: Ok(Reply::ShapePicture(None)), bytes: None },
        }
    }

    /// A new workbook in place of the open one: the page's subscriptions end with it.
    /// The picture `at` holds, its bytes travelling beside the reply as a saved package's do.
    fn cell_image(&self, at: Key) -> Answer {
        let Some(cell) = at.cell() else {
            return Answer {
                reply: Ok(Reply::CellImage(None)),
                bytes: None,
            };
        };
        let cells = vec![cell];
        let projected =
            calc_host::project(self.book.host(), &calc_host::ProjectionRequest { cells });
        let value = match projected.as_ref().map(|page| page.cells()[0].value()) {
            Ok(calc_host::ProjectedValue::Calculated { value, .. }) => Some(value.clone()),
            Ok(calc_host::ProjectedValue::FileCached(cache)) => Some(cache.value().to_value()),
            Ok(calc_host::ProjectedValue::Stale(stale)) => stale
                .last_value()
                .cloned()
                .or_else(|| stale.file_cache().map(|cache| cache.value().to_value())),
            _ => None,
        };
        let Some(image) = value.as_ref().and_then(|value| value.image_value()) else {
            return Answer {
                reply: Ok(Reply::CellImage(None)),
                bytes: None,
            };
        };
        Answer {
            reply: Ok(Reply::CellImage(Some(wire::CellImage {
                source: image.source.clone(),
                alt_text: image.alt_text.clone(),
                sizing: image.sizing,
                height: image.height,
                width: image.width,
                content_type: image.format.content_type().to_owned(),
            }))),
            bytes: Some(image.bytes.to_vec()),
        }
    }

    /// Attach a native observer before the next open/calculation, retaining the VBA owner.
    pub fn set_workbook_event_observer(
        &mut self,
        observer: Option<std::rc::Rc<dyn calc_host::WorkbookEventObserver>>,
    ) {
        self.book.set_workbook_event_observer(observer.clone());
        self.workbook_observer = observer;
    }
    pub fn set_open_file_location(&mut self, full_name: Option<String>) {
        self.open_file_location = full_name;
    }
    pub fn set_file_location(&mut self, full_name: String) {
        self.book.set_file_location(full_name);
    }

    /// Bind WEBSERVICE and IMAGE of this and every later workbook to `route`.
    pub fn set_fetch_route(&mut self, route: std::rc::Rc<dyn calc_func::FetchRoute>) {
        self.book.set_fetch_route(route.clone());
        self.fetch_route = Some(route);
        self.collect();
    }

    /// Bind IMPORTCSV and IMPORTTEXT of this and every later workbook to `route`.
    pub fn set_import_route(&mut self, route: std::rc::Rc<dyn calc_func::ImportRoute>) {
        self.book.set_import_route(route.clone());
        self.import_route = Some(route);
        self.collect();
    }

    /// Bind a native host's user functions (an XLL's) to this and every later workbook.
    /// Binding changes invalidate their callers, which recalculate in automatic mode.
    pub fn set_user_functions(&mut self, functions: calc_host::CallableTable) {
        self.book.set_user_functions(functions.clone());
        self.user_functions = Some(functions);
        self.collect();
    }

    /// The macro security level in force (dc-15dk). A native host sets what the person stored, or
    /// Unguarded pinned by `--trust-macros` or `DNACALC_TRUST_ALL_MACROS=1`, before the first open.
    pub fn set_macro_policy(&mut self, policy: dna_calc_model::MacroPolicy) {
        self.macro_policy = policy;
    }

    pub fn macro_policy(&self) -> dna_calc_model::MacroPolicy {
        self.macro_policy
    }

    /// Under the level Unguarded (or its pin) a macro workbook's project runs as it opens, with
    /// an information notice when its origin would have blocked it. A project that does not
    /// compile stays held and the notice says so.
    fn run_macros_on_open(&mut self) {
        let Some(policy) = self.macro_policy.runs_on_open() else {
            return;
        };
        let by = self.macro_policy.trust_source();
        match self
            .book
            .enable_macros_as(crate::workbook::EnableMode::Trusted { by, policy })
        {
            Ok(()) | Err(crate::workbook::EnableRefused::NoProject) => self.collect(),
            Err(refused) => self
                .book
                .set_macro_note(format!("Macros did not run: {refused}")),
        }
    }

    /// Where the file the next open request names came from (the desktop host read its path):
    /// its Mark of the Web or a network path blocks its VBA project (dc-0f1k.22). Without one the
    /// origin is unknown, as in a browser, and Full execution requires Unblock.
    pub fn set_open_origin(&mut self, origin: crate::workbook::FileOrigin) {
        self.open_origin.set(Some(origin));
    }

    /// Bind origin to one native request, including a request rejected before opening.
    pub fn with_open_origin<R>(
        &mut self,
        origin: crate::workbook::FileOrigin,
        call: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let slot = self.open_origin.clone();
        let earlier = slot.replace(Some(origin));
        let _scope = OpenOriginScope { slot, earlier };
        call(self)
    }

    /// Bind a native host's RTD connector to this and every later workbook.
    pub fn set_rtd_connector(&mut self, connector: std::rc::Rc<dyn calc_host::RtdConnector>) {
        if let Some(clock) = &self.model_clock {
            clock.set_fallback(connector.clone());
            self.book.set_rtd_connector(clock.clone());
        } else {
            self.book.set_rtd_connector(connector.clone());
        }
        self.rtd_connector = Some(connector);
    }

    /// Install the local DnaCalc.Clock RTD provider for this workbook and its replacements.
    pub fn enable_model_clock(&mut self) {
        if self.model_clock.is_some() {
            return;
        }
        let clock = std::rc::Rc::new(crate::model_clock::ModelClock::default());
        if let Some(fallback) = &self.rtd_connector {
            clock.set_fallback(fallback.clone());
        }
        self.book.set_rtd_connector(clock.clone());
        self.model_clock = Some(clock);
    }

    /// Drive the local clock using monotonic elapsed time. The normal RTD path admits each tick
    /// and schedules its recalculation.
    pub fn advance_model_clock(&mut self, elapsed: std::time::Duration) {
        let Some(clock) = &self.model_clock else {
            return;
        };
        let ticks = clock.advance(elapsed, !self.has_pending_calculation());
        for tick in ticks {
            self.update_rtd(tick.topic, tick.lease, tick.sequence, tick.value);
        }
    }

    /// A fetch has its reply: resumes the waiting cell, or does nothing when the calculation
    /// that asked has been superseded.
    pub fn complete_fetch(&mut self, request: u64, value: calc_host::value::CalcValue) -> bool {
        let token = self
            .pending_async()
            .into_iter()
            .find(|token| token.request == request);
        match token {
            Some(token) => {
                self.complete_async(&token, value) == calc_host::AsyncDisposition::Accepted
            }
            None => false,
        }
    }

    fn install(&mut self, mut book: Workbook) {
        book.set_workbook_event_observer(self.workbook_observer.clone());
        if let Some(route) = &self.fetch_route {
            book.set_fetch_route(route.clone());
        }
        if let Some(route) = &self.import_route {
            book.set_import_route(route.clone());
        }
        if let Some(functions) = &self.user_functions {
            book.set_user_functions(functions.clone());
        }
        if let Some(clock) = &self.model_clock {
            clock.reset();
            book.set_rtd_connector(clock.clone());
        } else if let Some(connector) = &self.rtd_connector {
            book.set_rtd_connector(connector.clone());
        }
        self.pending_name = None;
        self.pending_save = None;
        self.save_as_target = None;
        let interrupted = self.book.has_pending_calculation();
        if interrupted {
            self.book.cancel_calculation();
        }
        self.collect();
        self.release_calculation_events(true);
        book.set_calculation_budget(self.calculation_budget);
        self.validation_pending = None;
        self.workbook = self
            .workbook
            .checked_add(1)
            .expect("workbook incarnations exhausted");
        self.book = book;
        self.subscriptions.clear();
        self.subscription_interests.clear();
        self.overhang.clear();
        self.last_subscription = None;
        self.events
            .retain(|event| matches!(event, Event::Calculation { .. }));
        self.tile_streams.clear();
        self.snapshots.clear();
        self.paste_plan = None;
    }

    fn prepare_paste(
        &mut self,
        source: wire::PasteSource,
        to: GridArea,
        mode: wire::PasteMode,
        transpose: bool,
    ) -> Result<Reply, String> {
        use wire::PasteSource;
        self.paste_plan = None;
        let flash = matches!(source, PasteSource::FlashFill);
        let source = match source {
            PasteSource::Internal { token, cut } => calc_host::PasteSource::Snapshot {
                snapshot: self
                    .snapshots
                    .get(&token)
                    .ok_or("The copied snapshot expired; copy again")?,
                cut,
            },
            PasteSource::Fill { from, down, series } => calc_host::PasteSource::Fill {
                from: area(from)?,
                down,
                series,
            },
            PasteSource::External(rows) => calc_host::PasteSource::External(rows),
            PasteSource::FlashFill => calc_host::PasteSource::FlashFill,
        };
        let plan = calc_host::prepare_paste_oriented(
            self.book.host(),
            source,
            to,
            wire::host_paste_mode(mode),
            transpose,
        )?;
        let samples = if flash {
            plan.text_preview()
                .into_iter()
                .map(|(at, text)| format!("{} → {text}", cells::address(at.row(), at.column())))
                .collect()
        } else {
            Vec::new()
        };
        let target = plan.target();
        let replaces = plan.replaces();
        let id = self.next_clip;
        self.next_clip += 1;
        self.paste_plan = Some((id, plan));
        Ok(Reply::PastePreview(wire::PastePreview {
            samples,
            id,
            area: Area::of(target),
            replaces,
        }))
    }

    fn summary(&self) -> Summary {
        let iteration = self.book.host().model().definitions().iteration();
        let circular = self.book.host().circular_cells();
        Summary {
            workbook: self.workbook,
            session: self.clipboard_session.clone(),
            sheet_view_modes: self
                .book
                .host()
                .sheets()
                .iter()
                .filter_map(|s| {
                    use calc_host::SheetViewMode::{Normal, PageBreakPreview, PageLayout};
                    let mode = match self.book.host().sheet_view(s.id())?.mode? {
                        Normal => return None,
                        PageBreakPreview => wire::SheetViewMode::PageBreakPreview,
                        PageLayout => wire::SheetViewMode::PageLayout,
                    };
                    Some((Sheet::of(s.id()), mode))
                })
                .collect(),
            sheet_views: self
                .book
                .host()
                .sheets()
                .iter()
                .filter(|s| s.visibility() == calc_host::SheetVisibility::Visible)
                .filter_map(|s| {
                    let view = self.book.host().sheet_view(s.id())?;
                    let active = view.active_cell.filter(|at| *at != (1, 1));
                    (active.is_some() || view.selection.is_some()).then(|| {
                        (
                            Sheet::of(s.id()),
                            active.unwrap_or((1, 1)),
                            view.selection,
                        )
                    })
                })
                .collect(),
            initial_selection: self
                .book
                .host()
                .loaded()
                .active_sheet()
                .filter(|id| {
                    self.book.host().sheets().iter().any(|s| {
                        s.id() == *id && s.visibility() == calc_host::SheetVisibility::Visible
                    })
                })
                .or_else(|| {
                    self.book
                        .host()
                        .sheets()
                        .iter()
                        .find(|s| s.visibility() == calc_host::SheetVisibility::Visible)
                        .map(|s| s.id())
                })
                .and_then(|id| self.book.host().sheet_view(id))
                .and_then(|v| v.selection),
            initial_cell: self
                .book
                .host()
                .loaded()
                .active_sheet()
                .and_then(|id| {
                    self.book.host().sheets().iter().find(|s| {
                        s.id() == id && s.visibility() == calc_host::SheetVisibility::Visible
                    })
                })
                .or_else(|| {
                    self.book
                        .host()
                        .sheets()
                        .iter()
                        .find(|s| s.visibility() == calc_host::SheetVisibility::Visible)
                })
                .and_then(|s| {
                    let (r, c) = self
                        .book
                        .host()
                        .sheet_view(s.id())
                        .and_then(|v| v.active_cell)
                        .unwrap_or((1, 1));
                    CellKey::new(s.id(), r, c).ok().map(Key::of)
                }),
            name: self.book.name().to_owned(),
            changed: self.book.changed(),
            unsaved: self.book.unsaved(),
            macro_enabled: self.book.output_format().macro_enabled(),
            macro_policy: self.macro_policy,
            macro_note: self.book.macro_note().unwrap_or_default().to_owned(),
            macros: match self.book.macros() {
                crate::workbook::Macros::None => wire::MacroStatus::None,
                crate::workbook::Macros::Disabled(_) => wire::MacroStatus::Disabled,
                crate::workbook::Macros::Blocked(block) => wire::MacroStatus::Blocked {
                    reason: block.to_string(),
                },
                crate::workbook::Macros::Enabled => wire::MacroStatus::Enabled,
            },
            template: self.book.output_format().is_template(),
            undo: self.book.session().history_depth().0,
            redo: self.book.session().history_depth().1,
            sheets: self
                .book
                .host()
                .sheets()
                .iter()
                .filter(|s| s.visibility() == calc_host::SheetVisibility::Visible)
                .map(|s| (Sheet::of(s.id()), s.name().text().to_string_lossy()))
                .collect(),
            sheet_order: self
                .book
                .host()
                .sheets()
                .iter()
                .map(|s| Sheet::of(s.id()))
                .collect(),
            hidden_sheets: self
                .book
                .host()
                .sheets()
                .iter()
                .filter(|s| s.visibility() != calc_host::SheetVisibility::Visible)
                .map(|s| {
                    (
                        Sheet::of(s.id()),
                        s.name().text().to_string_lossy(),
                        s.visibility() == calc_host::SheetVisibility::VeryHidden,
                    )
                })
                .collect(),
            manual: self.book.session().calc_mode() == CalcMode::Manual,
            revision: self.book.host().revision().get(),
            iteration: iteration.enabled(),
            max_iterations: iteration.max_iterations(),
            max_change: iteration.max_change(),
            circular: self.book.host().circular_reported().map(Key::of),
            circular_count: circular.len(),
            open_repairs: self
                .book
                .host()
                .open_report()
                .repairs
                .iter()
                .map(|repair| match repair {
                    calc_host::LoadRepair::SheetRelationshipMissing { sheet, id } => {
                        let name = self
                            .book
                            .host()
                            .sheets()
                            .iter()
                            .find(|entry| entry.id() == *sheet)
                            .map(|entry| entry.name().text().to_string_lossy())
                            .unwrap_or_default();
                        format!("sheet {name} named a missing part ({id}); kept empty")
                    }
                    calc_host::LoadRepair::FormulaInsideArray { row, column, .. } => {
                        format!("formula inside an array dropped at row {row}, column {column}")
                    }
                    calc_host::LoadRepair::RejectedFormulaValue { at, .. } => format!(
                        "kept the saved value of a rejected formula at row {}, column {}",
                        at.row(),
                        at.column()
                    ),
                    calc_host::LoadRepair::DuplicateSheetId { id, now, .. } => {
                        format!("sheet id {id} repeated; the later sheet is now {now}")
                    }
                })
                .collect(),
        }
    }

    fn outcome(&self, accepted: bool, error: Option<String>, elapsed_ms: f64) -> Outcome {
        let selection = self.book.session().selection();
        Outcome {
            notebook_placement: None,
            validation: None,
            accepted,
            committed: false,
            recalculated: false,
            error,
            recalc: None,
            note: None,
            elapsed_ms,
            selection: selection.active.map(|at| {
                (
                    Key::of(at),
                    selection.areas.iter().copied().map(Area::of).collect(),
                )
            }),
            summary: self.summary(),
        }
    }

    fn done(&self, accepted: bool, error: Option<String>, elapsed_ms: f64) -> Reply {
        Reply::Done {
            outcome: self.outcome(accepted, error, elapsed_ms),
            opened: None,
            saved: None,
        }
    }

    fn fit_contents(&self, target: GridArea) -> Result<Reply, String> {
        let host = self.book.host();
        let metrics = dna_calc_model::geometry::sheet_metrics(
            host.sheet_geometry(target.sheet())
                .as_ref()
                .map(wire::geometry_of)
                .as_ref(),
        );
        let keys = calc_host::autofit_cells(host, target, |at| {
            metrics.width(at.column()) != 0. && metrics.height(at.row()) != 0.
        })?;
        let projected = calc_host::project(host, &calc_host::ProjectionRequest { cells: keys })
            .map_err(|e| e.to_string())?;
        let mut cells = Vec::new();
        for p in projected.cells() {
            let shown = self.book.shown(p);
            if let Some(reason) = &shown.style.unsupported {
                return Err(reason.clone());
            }
            cells.push(self.book.wire_cell(p, &shown));
        }
        Ok(Reply::FitContents {
            revision: host.revision().get(),
            cells,
        })
    }

    fn review(&self, at: CellKey) -> wire::Review {
        let host = self.book.host();
        let source = host.model().definitions().conditional_rules(at.sheet());
        let explanations: BTreeMap<_, _> = self
            .book
            .conditional_formats(at.sheet())
            .explain(host, at)
            .into_iter()
            .collect();
        let mut rules = source
            .iter()
            .enumerate()
            .filter_map(|(index, s)| {
                let r = calc_host::conditional_rule(s).ok()?;
                Some(wire::Rule {
                    index: index as u32,
                    kind: r.kind.clone(),
                    priority: s.priority,
                    applies: s
                        .areas
                        .iter()
                        .map(|a| {
                            format!(
                                "{}:{}",
                                dna_formula_ui::display::column_letters(a.first().column())
                                    + &a.first().row().to_string(),
                                dna_formula_ui::display::column_letters(a.last().column())
                                    + &a.last().row().to_string()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                    formulas: s.formulas.clone(),
                    stop: s.stop,
                    state: explanations
                        .get(&index)
                        .cloned()
                        .unwrap_or_else(|| "Rule unavailable".into()),
                    editable: r.extension_id.is_none(),
                    formula_editable: matches!(r.kind.as_str(), "expression" | "cellIs"),
                    visual: matches!(r.kind.as_str(), "colorScale" | "dataBar" | "iconSet"),
                })
            })
            .collect::<Vec<_>>();
        rules.sort_by_key(|r| r.priority);
        let review = host.sheet_review(at.sheet());
        let count = review.notes.iter().filter(|n| !n.compatibility).count() + review.links.len();
        let mut unsupported = review.unsupported;
        if host
            .loaded()
            .sheet(at.sheet())
            .is_some_and(|s| s.conditional_formats().iter().any(|b| b.unread_container))
        {
            unsupported.push("Additional rule metadata is preserved; conditional-rule editing is unavailable on this worksheet (dc-971p.8)".into());
        }

        if count > 5000 {
            unsupported.push(
                "Only the first 5000 review markers are drawn; cell details remain available here"
                    .into(),
            );
        }
        let validation=host.validation(at).map(|v|format!("{} validation · {:?}. Entry alerts check candidate values through the workbook evaluator. Circle Invalid Data checks current published values.",v.kind,v.style));
        wire::Review {
            next_index: review.notes.len() as u32,
            at: Some(Key::of(at)),
            revision: host.revision().get(),
            rules,
            notes: review
                .notes
                .into_iter()
                .enumerate()
                .filter(|(_, n)| n.at == at && !n.compatibility)
                .map(|(index, n)| {
                    let (threaded, reply, resolved) = match &n.source {
                        Some(calc_host::ReviewSource::Thread {
                            parent, resolved, ..
                        }) => (true, parent.is_some(), *resolved),
                        _ => (false, false, false),
                    };
                    let runs = calc_host::note_segments(&n)
                        .into_iter()
                        .map(|(text, s)| wire::ReviewRun {
                            text,
                            bold: s.bold,
                            italic: s.italic,
                            underline: s.underline,
                            strike: s.strike,
                            color: s.color.filter(|c| {
                                matches!(c.len(), 6 | 8) && c.bytes().all(|b| b.is_ascii_hexdigit())
                            }),
                            size: s.size.filter(|s| {
                                s.parse::<f64>()
                                    .is_ok_and(|n| n.is_finite() && n > 0. && n <= 500.)
                            }),
                        })
                        .collect();
                    wire::ReviewItem {
                        runs,
                        index: index as u32,
                        author: n.author,
                        text: n.text,
                        threaded,
                        reply,
                        resolved,
                        editable: n.editable,
                    }
                })
                .collect(),
            links: review
                .links
                .into_iter()
                .filter(|l| l.area.contains(at))
                .map(|l| wire::ReviewLink {
                    display: l.display.clone().unwrap_or_else(|| {
                        l.target
                            .clone()
                            .or_else(|| l.location.clone())
                            .unwrap_or_else(|| "Unavailable link".into())
                    }),
                    target: l.target,
                    location: l.location,
                    tooltip: l.tooltip.unwrap_or_default(),
                })
                .collect(),
            unsupported,
            validation,
        }
    }

    fn intent(&mut self, intent: Intent) -> Result<Reply, String> {
        let intent = match intent {
            Intent::CreateReview {
                at,
                revision,
                parent,
                threaded,
                author,
                text,
                timestamp,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ReviewAction {
                at: cell(at)?,
                revision,
                action: calc_host::ReviewAction::Create {
                    parent,
                    threaded,
                    author,
                    text,
                    timestamp,
                },
            }),
            Intent::FormatReview {
                at,
                revision,
                index,
                text,
                start,
                end,
                patch,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ReviewAction {
                at: cell(at)?,
                revision,
                action: calc_host::ReviewAction::Format {
                    index,
                    text,
                    start,
                    end,
                    patch: calc_host::NoteStylePatch {
                        bold: patch.bold,
                        italic: patch.italic,
                        underline: patch.underline,
                        color: patch.color,
                        clear: patch.clear,
                    },
                },
            }),
            Intent::ResolveReview {
                at,
                revision,
                index,
                resolved,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ReviewAction {
                at: cell(at)?,
                revision,
                action: calc_host::ReviewAction::Resolve { index, resolved },
            }),
            Intent::EditReview {
                at,
                revision,
                index,
                text,
            } => SkinIntent::Command(calc_host::WorkbookCommand::Review {
                at: cell(at)?,
                revision,
                index,
                text,
            }),
            Intent::EditName {
                id,
                version,
                name,
                scope,
                formula,
            } => SkinIntent::Command(calc_host::WorkbookCommand::Name(calc_host::NameEdit {
                id: id.map(|(a, b)| calc_host::value::NameId::from_parts(a, b)),
                version,
                name,
                scope: scope
                    .map(|s| calc_host::NameScope::Sheet(s.id()))
                    .unwrap_or(calc_host::NameScope::Workbook),
                formula,
            })),
            Intent::FormatCells {
                areas,
                revision,
                patch,
            } => {
                if revision != self.book.host().revision().get() {
                    return Err("The workbook changed; reopen Format Cells".into());
                }
                SkinIntent::Command(calc_host::WorkbookCommand::FormatCells {
                    areas: areas.into_iter().map(area).collect::<Result<Vec<_>, _>>()?,
                    patch: crate::format::host_patch(patch),
                })
            }
            Intent::AcceptValidation { token } => {
                let (held, revision, selection, intent) = self
                    .validation_pending
                    .take()
                    .ok_or("This validation decision has expired")?;
                if held != token
                    || revision != self.book.host().revision().get()
                    || &selection != self.book.session().selection()
                {
                    return Err("The workbook or selection changed; enter the draft again".into());
                }
                return self.dispatch_entry(intent, true);
            }
            Intent::NotebookLink {
                anchor,
                prose,
                text,
            } => return self.notebook_link(anchor, prose, text),
            Intent::NotebookOrder {
                placement,
                direction,
            } => return self.notebook_order(placement, direction),

            Intent::RefreshAll { revision } => {
                SkinIntent::Command(calc_host::WorkbookCommand::RefreshAll { revision })
            }
            Intent::PivotCreate {
                revision,
                source,
                destination,
                row,
                value,
                subtotal,
            } => SkinIntent::Command(calc_host::WorkbookCommand::PivotCreate {
                revision,
                source: source.area().ok_or("Invalid pivot source range")?,
                destination,
                row,
                value,
                subtotal,
            }),
            Intent::PivotCreateLayout {
                revision,
                source,
                destination,
                rows,
                columns,
                values,
            } => SkinIntent::Command(calc_host::WorkbookCommand::PivotCreateLayout {
                revision,
                source: source.area().ok_or("Invalid pivot source range")?,
                destination,
                rows,
                columns,
                values,
            }),
            Intent::PivotSetLayout {
                sheet,
                revision,
                part,
                rows,
                columns,
            } => SkinIntent::Command(calc_host::WorkbookCommand::PivotSetLayout {
                sheet: sheet.id(),
                revision,
                part,
                rows,
                columns,
            }),
            Intent::PivotRefresh {
                sheet,
                revision,
                part,
            } => SkinIntent::Command(calc_host::WorkbookCommand::PivotRefresh {
                sheet: sheet.id(),
                revision,
                part,
            }),
            Intent::ChartCreate {
                revision,
                area,
                kind,
                rows,
                headers,
                categories,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ChartCreate {
                revision,
                area: area.area().ok_or("Invalid chart selection")?,
                kind,
                rows,
                headers,
                categories,
            }),
            Intent::ChartSeriesStyle {
                sheet,
                revision,
                part,
                plot,
                series,
                fill,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ChartSeriesStyle {
                sheet: sheet.id(),
                revision,
                part,
                plot,
                series,
                fill,
            }),
            Intent::ChartElementTextStyle {
                sheet,
                revision,
                part,
                target,
                style,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ChartElementTextStyle {
                sheet: sheet.id(),
                revision,
                part,
                target,
                style: calc_host::chart::ChartTextStyle {
                    family: style.family,
                    size: style.size,
                    bold: style.bold,
                    italic: style.italic,
                    color: style.color,
                },
            }),
            Intent::ChartTextStyle {
                sheet,
                revision,
                part,
                style,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ChartTextStyle {
                sheet: sheet.id(),
                revision,
                part,
                style: calc_host::chart::ChartTextStyle {
                    family: style.family,
                    size: style.size,
                    bold: style.bold,
                    italic: style.italic,
                    color: style.color,
                },
            }),
            Intent::ChartType {
                sheet,
                revision,
                part,
                plot,
                kind,
                stacked,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ChartType {
                sheet: sheet.id(),
                revision,
                part,
                plot,
                kind,
                stacked,
            }),
            Intent::PageSetup {
                sheet,
                revision,
                change,
            } => SkinIntent::Command(calc_host::WorkbookCommand::PageSetup {
                sheet: sheet.id(),
                revision,
                change: crate::printing::change(change),
            }),
            Intent::ShapePlace { sheet, revision, id, left, top, width, height } => {
                let points = |v: i64| Some(calc_host::Points::new(v as f64 / 100.0));
                SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                    sheet: sheet.id(),
                    revision,
                    change: calc_host::ShapeChange::Place { id, left: points(left), top: points(top), width: points(width), height: points(height) },
                })
            }
            Intent::ShapeDelete { sheet, revision, id } => SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                sheet: sheet.id(),
                revision,
                change: calc_host::ShapeChange::Delete { id },
            }),
            Intent::ShapeAdd { sheet, revision, at, kind, geometry, width, height } => {
                let at = cell(at)?;
                let kind = match kind.as_str() {
                    "shape" => calc_host::ShapeKind::Shape,
                    "textbox" => calc_host::ShapeKind::TextBox,
                    "connector" => calc_host::ShapeKind::Connector,
                    _ => return Err("Insert a shape, a text box or a line".into()),
                };
                let (left, top) = self.book.host().cell_corner(sheet.id(), at.row(), at.column());
                let points = |v: f64| calc_host::Points::new(v);
                SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                    sheet: sheet.id(),
                    revision,
                    change: calc_host::ShapeChange::Add {
                        kind,
                        geometry,
                        left: points(left),
                        top: points(top),
                        width: points(width as f64 / 100.0),
                        height: points(height as f64 / 100.0),
                        flips: (false, false),
                        name: None,
                    },
                })
            }
            Intent::SparklineAdd { sheet, revision, at, data, kind } => {
                let at = cell(at)?;
                let data = match self.book.host().resolve_reference_text(at, data.trim()) {
                    calc_host::ReferenceTarget::Area(area) => area,
                    _ => return Err("Give the sparkline's data as a range, such as A2:E2".into()),
                };
                SkinIntent::Command(calc_host::WorkbookCommand::Sparkline {
                    sheet: sheet.id(),
                    revision,
                    change: calc_host::SparklineChange::Add { data, cell: at, kind: Some(kind).filter(|k| !k.is_empty()) },
                })
            }
            Intent::SparklineKind { sheet, revision, at, kind } => SkinIntent::Command(calc_host::WorkbookCommand::Sparkline {
                sheet: sheet.id(),
                revision,
                change: calc_host::SparklineChange::Kind { cell: cell(at)?, kind: Some(kind).filter(|k| !k.is_empty()) },
            }),
            Intent::SparklineShow { sheet, revision, at, point, on } => SkinIntent::Command(calc_host::WorkbookCommand::Sparkline {
                sheet: sheet.id(),
                revision,
                change: calc_host::SparklineChange::Show { cell: cell(at)?, point, on },
            }),
            Intent::SparklineDelete { sheet, revision, at } => SkinIntent::Command(calc_host::WorkbookCommand::Sparkline {
                sheet: sheet.id(),
                revision,
                change: calc_host::SparklineChange::Delete { cell: cell(at)? },
            }),
            Intent::ShapePicture { sheet, revision, at, name } => {
                let at = cell(at)?;
                let bytes = self.intent_bytes.take().unwrap_or_default();
                let (content_type, (width, height)) = crate::picture::inserted(&bytes)?;
                let (left, top) = self.book.host().cell_corner(sheet.id(), at.row(), at.column());
                let points = |v: f64| calc_host::Points::new(v);
                SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                    sheet: sheet.id(),
                    revision,
                    change: calc_host::ShapeChange::AddPicture {
                        left: points(left),
                        top: points(top),
                        // Pixels at 96 to the inch.
                        width: points(width * 0.75),
                        height: points(height * 0.75),
                        content_type: content_type.to_owned(),
                        bytes: bytes.into(),
                        description: (!name.is_empty()).then_some(name),
                    },
                })
            }
            Intent::ShapeText { sheet, revision, id, text } => SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                sheet: sheet.id(),
                revision,
                change: calc_host::ShapeChange::Text { id, text: text.replace("\r\n", "\r").replace('\n', "\r") },
            }),
            Intent::ShapeFill { sheet, revision, id, fill } => {
                let fill = match shape_colour(&fill)? {
                    Some(Some(colour)) => calc_host::ShapeFill::Solid(colour),
                    Some(None) => calc_host::ShapeFill::None,
                    None => calc_host::ShapeFill::Style,
                };
                SkinIntent::Command(calc_host::WorkbookCommand::Shape { sheet: sheet.id(), revision, change: calc_host::ShapeChange::Fill { id, fill } })
            }
            Intent::ShapeLine { sheet, revision, id, colour, weight } => {
                let current = self.book.host().shapes(sheet.id()).iter().find(|s| s.id == id).map(|s| s.line.clone()).unwrap_or_default();
                let mut line = current;
                match shape_colour(&colour)? {
                    Some(Some(c)) => (line.hidden, line.colour) = (false, Some(c)),
                    Some(None) => (line.hidden, line.colour) = (true, None),
                    None => (line.hidden, line.colour) = (false, None),
                }
                if weight.is_some_and(|w| !(0..=158_400).contains(&w)) {
                    return Err("Use an outline weight from 0 to 1584 points".into());
                }
                line.weight = weight.map(|w| w * 127);
                SkinIntent::Command(calc_host::WorkbookCommand::Shape { sheet: sheet.id(), revision, change: calc_host::ShapeChange::Line { id, line } })
            }
            Intent::ShapeRotate { sheet, revision, id, degrees } => SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                sheet: sheet.id(),
                revision,
                change: calc_host::ShapeChange::Rotate { id, rotation: degrees.rem_euclid(360) * 60_000 },
            }),
            Intent::ShapeOrder { sheet, revision, id, order } => {
                let order = match order.as_str() {
                    "forward" => calc_host::ShapeOrder::Forward,
                    "backward" => calc_host::ShapeOrder::Backward,
                    "front" => calc_host::ShapeOrder::Front,
                    "back" => calc_host::ShapeOrder::Back,
                    _ => return Err("Bring forward, send backward, bring to front or send to back".into()),
                };
                SkinIntent::Command(calc_host::WorkbookCommand::Shape { sheet: sheet.id(), revision, change: calc_host::ShapeChange::Order { id, order } })
            }
            Intent::ShapeDuplicate { sheet, revision, id, dx, dy } => SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                sheet: sheet.id(),
                revision,
                change: calc_host::ShapeChange::Duplicate { id, dx: calc_host::Points::new(dx as f64 / 100.0), dy: calc_host::Points::new(dy as f64 / 100.0) },
            }),
            Intent::ShapeNudge { sheet, revision, id, dx, dy } => SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                sheet: sheet.id(),
                revision,
                change: calc_host::ShapeChange::Nudge { id, dx: calc_host::Points::new(dx as f64 / 100.0), dy: calc_host::Points::new(dy as f64 / 100.0) },
            }),
            Intent::ShapeFlip { sheet, revision, id, horizontal } => SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                sheet: sheet.id(),
                revision,
                change: calc_host::ShapeChange::Flip { id, horizontal },
            }),
            Intent::ShapeGroup { sheet, revision, ids } => SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                sheet: sheet.id(),
                revision,
                change: calc_host::ShapeChange::Group { ids },
            }),
            Intent::ShapeUngroup { sheet, revision, id } => SkinIntent::Command(calc_host::WorkbookCommand::Shape {
                sheet: sheet.id(),
                revision,
                change: calc_host::ShapeChange::Ungroup { id },
            }),
            Intent::ChartLegend {
                sheet,
                revision,
                part,
                position,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ChartLegend {
                sheet: sheet.id(),
                revision,
                part,
                position,
            }),
            Intent::ChartLogAxis {
                sheet,
                revision,
                part,
                axis,
                base,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ChartLogAxis {
                sheet: sheet.id(),
                revision,
                part,
                axis,
                base,
            }),
            Intent::ChartAxis {
                sheet,
                revision,
                part,
                axis,
                minimum,
                maximum,
                major_unit,
                reversed,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ChartAxis {
                sheet: sheet.id(),
                revision,
                part,
                axis,
                minimum,
                maximum,
                major_unit,
                reversed,
            }),
            Intent::ChartSeries {
                sheet,
                revision,
                part,
                plot,
                series,
                name,
                categories,
                values,
                remove,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ChartSeries {
                sheet: sheet.id(),
                revision,
                part,
                plot,
                series,
                name,
                categories,
                values,
                remove,
            }),
            Intent::ChartProperties {
                sheet,
                revision,
                part,
                title,
                set_title,
                legend,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ChartProperties {
                sheet: sheet.id(),
                revision,
                part,
                title,
                set_title,
                legend,
            }),
            Intent::Table { revision, change } => {
                use calc_host::TableChange as H;
                use wire::TableChange as W;
                let change = match change {
                    W::Create { area: a, name } => H::Create {
                        area: area(a)?,
                        name,
                    },
                    W::Resize { part, area: a } => H::Resize {
                        part,
                        area: area(a)?,
                    },
                    W::Options {
                        part,
                        headers,
                        totals,
                        style,
                        row_stripes,
                        column_stripes,
                        first_column,
                        last_column,
                    } => H::Options {
                        part,
                        headers,
                        totals,
                        style,
                        row_stripes,
                        column_stripes,
                        first_column,
                        last_column,
                    },
                    W::Total {
                        part,
                        column,
                        function,
                    } => H::Total {
                        part,
                        column,
                        function,
                    },
                    W::RenameColumn { part, column, name } => {
                        H::RenameColumn { part, column, name }
                    }
                    W::CalculatedColumn {
                        part,
                        column,
                        formula,
                    } => H::CalculatedColumn {
                        part,
                        column,
                        formula,
                    },
                };
                SkinIntent::Command(calc_host::WorkbookCommand::Table { revision, change })
            }
            Intent::AutoFilter {
                sheet,
                revision,
                table,
                change,
            } => {
                use calc_host::{
                    FilterChange as H, FilterCriteria, FilterOperator, FilterPredicate,
                };
                use wire::FilterChange as W;
                let change = match change {
                    W::Enable(a) => H::Enable { area: area(a)? },
                    W::Values {
                        column,
                        values,
                        blank,
                    } => H::Column {
                        column,
                        criteria: Some(FilterCriteria::Values { values, blank }),
                    },
                    W::Custom {
                        column,
                        and,
                        predicates,
                    } => H::Column {
                        column,
                        criteria: Some(FilterCriteria::Custom {
                            and,
                            predicates: predicates
                                .into_iter()
                                .map(|(operator, value)| {
                                    Ok(FilterPredicate {
                                        operator: FilterOperator::from_xml(&operator)
                                            .ok_or("Unknown filter operator")?,
                                        value,
                                    })
                                })
                                .collect::<Result<Vec<_>, &str>>()?,
                        }),
                    },
                    W::Dates {
                        column,
                        groups,
                        values,
                        blank,
                    } => H::Column {
                        column,
                        criteria: Some(FilterCriteria::Dates {
                            groups,
                            values,
                            blank,
                        }),
                    },
                    W::Top {
                        column,
                        top,
                        percent,
                        value,
                    } => H::Column {
                        column,
                        criteria: Some(FilterCriteria::Top {
                            top,
                            percent,
                            value,
                        }),
                    },
                    W::Calendar { column, period } => H::Column {
                        column,
                        criteria: Some(FilterCriteria::Calendar {
                            period,
                            val: None,
                            max_val: None,
                        }),
                    },
                    W::Average { column, above } => H::Column {
                        column,
                        criteria: Some(FilterCriteria::Average { above, val: None }),
                    },
                    W::Color {
                        column,
                        cell_color,
                        color,
                    } => H::Color {
                        column,
                        cell_color,
                        color,
                    },
                    W::ColorReference {
                        column,
                        dxf_id,
                        cell_color,
                    } => H::Column {
                        column,
                        criteria: Some(FilterCriteria::Color { dxf_id, cell_color }),
                    },
                    W::ClearColumn(column) => H::Column {
                        column,
                        criteria: None,
                    },
                    W::Clear => H::Clear,
                    W::Remove => H::Remove,
                    W::Reapply => H::Reapply,
                };
                SkinIntent::Command(if let Some(part) = table {
                    calc_host::WorkbookCommand::TableFilter {
                        sheet: sheet.id(),
                        revision,
                        part,
                        change,
                    }
                } else {
                    calc_host::WorkbookCommand::AutoFilter {
                        sheet: sheet.id(),
                        revision,
                        change,
                    }
                })
            }
            Intent::SortRowsWithOptions {
                area: a,
                keys,
                header,
                revision,
                case_sensitive,
                custom_order,
            } => SkinIntent::Command(calc_host::WorkbookCommand::SortRowsWithOptions {
                area: area(a)?,
                keys: keys
                    .into_iter()
                    .map(|(column, descending)| calc_host::SortKey { column, descending })
                    .collect(),
                header,
                revision,
                options: calc_host::SortOptions {
                    case_sensitive,
                    custom_order,
                },
            }),
            Intent::SortRows {
                area: a,
                keys,
                header,
                revision,
            } => SkinIntent::Command(calc_host::WorkbookCommand::SortRows {
                area: area(a)?,
                keys: keys
                    .into_iter()
                    .map(|(column, descending)| calc_host::SortKey { column, descending })
                    .collect(),
                header,
                revision,
            }),
            Intent::Replace {
                options,
                replacement,
                revision,
                only,
            } => SkinIntent::Command(calc_host::WorkbookCommand::Replace {
                options: crate::search::host_options(&options)?,
                replacement,
                revision,
                only: only.map(cell).transpose()?,
            }),
            Intent::EditConditional {
                sheet,
                revision,
                index,
                change,
            } => SkinIntent::Command(calc_host::WorkbookCommand::Conditional {
                sheet: sheet.id(),
                revision,
                index,
                change: conditional_change(change),
            }),
            Intent::ShiftCells {
                area: a,
                down,
                insert,
            } => SkinIntent::Command(calc_host::WorkbookCommand::ShiftCells {
                area: area(a)?,
                down,
                insert,
                from_after: false,
            }),
            Intent::EditSheet { sheet, change } => {
                SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
                    sheet: sheet.map(Sheet::id),
                    change: sheet_change(change),
                })
            }
            Intent::FitContents {
                sheet,
                rows,
                revision,
                sizes,
            } => SkinIntent::Command(calc_host::WorkbookCommand::FitContents {
                sheet: sheet.id(),
                rows,
                revision,
                sizes,
            }),
            Intent::Outline {
                sheet,
                rows,
                revision,
                change,
            } => SkinIntent::Command(calc_host::WorkbookCommand::Outline {
                sheet: sheet.id(),
                rows,
                revision,
                change: match change {
                    wire::OutlineChange::Group { first, last } => {
                        calc_host::OutlineChange::Group { first, last }
                    }
                    wire::OutlineChange::Ungroup { first, last } => {
                        calc_host::OutlineChange::Ungroup { first, last }
                    }
                    wire::OutlineChange::Detail {
                        first,
                        last,
                        level,
                        show,
                    } => calc_host::OutlineChange::Detail {
                        first,
                        last,
                        level,
                        show,
                    },
                    wire::OutlineChange::ShowLevel(level) => {
                        calc_host::OutlineChange::ShowLevel(level)
                    }
                    wire::OutlineChange::SummaryAfter(after) => {
                        calc_host::OutlineChange::SummaryAfter(after)
                    }
                },
            }),
            Intent::AxisFormat {
                sheet,
                rows,
                first,
                count,
                size_bits,
                hidden,
            } => SkinIntent::Command(calc_host::WorkbookCommand::AxisFormat {
                sheet: sheet.id(),
                rows,
                first,
                count,
                size_bits,
                hidden,
            }),
            Intent::SplitPanes {
                sheet,
                revision,
                x,
                y,
                row,
                column,
            } => SkinIntent::Command(calc_host::WorkbookCommand::SplitPanes {
                sheet: sheet.id(),
                revision,
                x,
                y,
                row,
                column,
            }),
            Intent::FreezePanes {
                sheet,
                rows,
                columns,
            } => SkinIntent::Command(calc_host::WorkbookCommand::FreezePanes {
                sheet: sheet.id(),
                rows,
                columns,
            }),
            Intent::ApplyPaste { id, select_target } => {
                let Some((held, plan)) = self.paste_plan.take() else {
                    return Err("The paste preview expired; paste again".into());
                };
                if held != id {
                    return Err("The workbook changed after the preview; paste again".into());
                }
                SkinIntent::Command(calc_host::WorkbookCommand::Paste(if select_target {
                    plan.with_target_selection()
                } else {
                    plan
                }))
            }
            Intent::Navigate {
                nav,
                extend,
                page_height,
            } => {
                let selection = self.book.session().selection();
                let at = selection.active.ok_or("no active cell")?;
                if matches!(
                    nav,
                    dna_calc_model::selection::Navigation::Column
                        | dna_calc_model::selection::Navigation::Row
                ) && selection.areas.len() > 1
                {
                    return Ok(self.done(false,Some("Axis expansion for disjoint selections is not modelled; select one rectangle".to_owned()),0.0));
                }
                let merges = self.book.host().merged_areas(at.sheet());
                let active_merge = merges.iter().copied().find(|a| a.contains(at));
                if extend
                    && active_merge.is_none()
                    && selection.areas.first().is_some_and(|area| {
                        (at.row() != area.first().row() && at.row() != area.last().row())
                            || (at.column() != area.first().column()
                                && at.column() != area.last().column())
                    })
                {
                    return Ok(self.done(false,Some("Range extension from an interior active cell awaits Excel measurement; select an anchor corner first".to_owned()),0.0));
                }
                let host = self.book.host();
                if matches!(
                    nav,
                    dna_calc_model::selection::Navigation::All
                        | dna_calc_model::selection::Navigation::Column
                        | dna_calc_model::selection::Navigation::Row
                ) && host
                    .model()
                    .definitions()
                    .tables()
                    .iter()
                    .any(|table| table.area.contains(at))
                {
                    return Ok(self.done(false,Some("Table selection stages are not modelled; use the name box to select an explicit range".to_owned()),0.0));
                }
                let occupied = if matches!(
                    nav,
                    dna_calc_model::selection::Navigation::Edge(_)
                        | dna_calc_model::selection::Navigation::All
                ) {
                    host.occupied_areas(at.sheet())
                } else {
                    Vec::new()
                };
                let mut used = if matches!(nav, dna_calc_model::selection::Navigation::UsedEnd) {
                    host.used_area(at.sheet())
                } else {
                    None
                };
                if matches!(nav, dna_calc_model::selection::Navigation::UsedEnd) {
                    for (key, _) in host
                        .model()
                        .definitions()
                        .cell_formats()
                        .filter(|(key, _)| key.sheet() == at.sheet())
                    {
                        used = Some(match used {
                            None => GridArea::cell(key),
                            Some(area) => GridArea::new(
                                CellKey::new(
                                    at.sheet(),
                                    area.first().row().min(key.row()),
                                    area.first().column().min(key.column()),
                                )
                                .expect("bounded format"),
                                CellKey::new(
                                    at.sheet(),
                                    area.last().row().max(key.row()),
                                    area.last().column().max(key.column()),
                                )
                                .expect("bounded format"),
                            )
                            .expect("one sheet"),
                        });
                    }
                }
                let metrics = dna_calc_model::geometry::sheet_metrics(
                    host.sheet_geometry(at.sheet())
                        .as_ref()
                        .map(wire::geometry_of)
                        .as_ref(),
                );
                let mut selection = selection.clone();
                let mut nav = nav;
                if !extend && let Some(merged) = merges.iter().copied().find(|a| a.contains(at)) {
                    use dna_calc_model::selection::{NavAction, Navigation};
                    if let Navigation::Walk {
                        horizontal,
                        reverse,
                    } = nav
                        && selection.areas.as_slice() == [merged]
                    {
                        nav = Navigation::Step(match (horizontal, reverse) {
                            (true, false) => NavAction::Right,
                            (true, true) => NavAction::Left,
                            (false, false) => NavAction::Down,
                            (false, true) => NavAction::Up,
                        });
                    }
                    let edge = match nav {
                        Navigation::Step(NavAction::Down | NavAction::CommitDown) => {
                            Some((merged.last().row(), merged.first().column()))
                        }
                        Navigation::Step(
                            NavAction::Right | NavAction::NextColumn | NavAction::CommitRight,
                        ) => Some((merged.first().row(), merged.last().column())),
                        Navigation::Step(
                            NavAction::Up | NavAction::Left | NavAction::PrevColumn,
                        ) => Some((merged.first().row(), merged.first().column())),
                        _ => None,
                    };
                    if let Some((row, col)) = edge {
                        selection.active =
                            Some(CellKey::new(at.sheet(), row, col).expect("merge edge"));
                    }
                }
                let mut next = if extend
                    && active_merge.is_some()
                    && !matches!(
                        nav,
                        dna_calc_model::selection::Navigation::Row
                            | dna_calc_model::selection::Navigation::Column
                            | dna_calc_model::selection::Navigation::All
                    ) {
                    dna_calc_model::selection::extend_merged(
                        &selection,
                        active_merge.unwrap(),
                        nav,
                        &metrics,
                        f64::from(page_height),
                        &occupied,
                        used,
                    )
                } else {
                    dna_calc_model::selection::navigate(
                        &selection,
                        nav,
                        extend,
                        &metrics,
                        f64::from(page_height),
                        &occupied,
                        used,
                    )
                };
                // Walk visits each merged identity once within a larger selected rectangle.
                // Jump to the covered run's edge instead of snapping back to its anchor.
                if !extend
                    && let dna_calc_model::selection::Navigation::Walk {
                        horizontal,
                        reverse,
                    } = nav
                {
                    let mut visited = std::collections::BTreeSet::new();
                    while let Some(target) = next.active {
                        let Some(merge) = merges
                            .iter()
                            .find(|a| a.contains(target) && a.first() != target)
                        else {
                            break;
                        };
                        if !visited.insert(target) {
                            return Err("Merged selection navigation did not advance".into());
                        }
                        let area = next
                            .areas
                            .iter()
                            .find(|a| a.contains(target))
                            .copied()
                            .unwrap_or_else(|| GridArea::cell(target));
                        let full_line = if horizontal {
                            area.first().column() == merge.first().column()
                                && area.last().column() == merge.last().column()
                        } else {
                            area.first().row() == merge.first().row()
                                && area.last().row() == merge.last().row()
                        };
                        if reverse
                            && (full_line
                                || if horizontal {
                                    target.row() == merge.first().row()
                                } else {
                                    target.column() == merge.first().column()
                                })
                        {
                            next.active = Some(merge.first());
                            break;
                        }
                        let edge = if full_line {
                            merge.last()
                        } else if horizontal {
                            CellKey::new(
                                target.sheet(),
                                target.row(),
                                if reverse {
                                    merge.first().column()
                                } else {
                                    merge.last().column()
                                },
                            )
                            .expect("merged walk column")
                        } else {
                            CellKey::new(
                                target.sheet(),
                                if reverse {
                                    merge.first().row()
                                } else {
                                    merge.last().row()
                                },
                                target.column(),
                            )
                            .expect("merged walk row")
                        };
                        next.active = Some(edge);
                        next = dna_calc_model::selection::navigate(
                            &next,
                            nav,
                            false,
                            &metrics,
                            f64::from(page_height),
                            &occupied,
                            used,
                        );
                    }
                }
                SkinIntent::Select(next)
            }
            Intent::EnterCell { at, text } => SkinIntent::EnterCell {
                at: cell(at)?,
                text,
            },
            Intent::EnterSelection { text } => SkinIntent::EnterSelection { text },
            Intent::Clear { areas } => SkinIntent::Edit {
                edits: areas
                    .into_iter()
                    .map(|a| Ok(CellEdit::Clear { area: area(a)? }))
                    .collect::<Result<_, String>>()?,
            },
            Intent::EditAxis {
                sheet,
                rows,
                insert,
                start,
                count,
            } => {
                let axis = if rows { Axis::Rows } else { Axis::Columns };
                let change = if insert {
                    AxisChange::Insert
                } else {
                    AxisChange::Delete
                };
                SkinIntent::EditAxis(
                    AxisEdit::new(sheet.id(), axis, change, start, count)
                        .map_err(|e| format!("{e:?}"))?,
                )
            }
            Intent::Copy { from, to, cut } => SkinIntent::Copy {
                from: area(from)?,
                to: area(to)?,
                cut,
            },
            Intent::Undo => SkinIntent::Undo,
            Intent::Redo => SkinIntent::Redo,
            Intent::Recalculate => SkinIntent::Recalculate,
            Intent::Rebuild => SkinIntent::Rebuild,
            Intent::Calculate { scope, selection } => SkinIntent::Calculate {
                scope: match scope {
                    wire::CalculationScope::Workbook => calc_host::CalcScope::Workbook,
                    wire::CalculationScope::Sheet(sheet) => calc_host::CalcScope::Sheet(sheet.id()),
                    wire::CalculationScope::Range(areas) => calc_host::CalcScope::Range(
                        areas
                            .into_iter()
                            .map(|area| area.area().ok_or("The calculation range is invalid"))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                },
                selection: match selection {
                    wire::CalculationSelection::DirtyAndVolatile => {
                        calc_host::EvalSelection::DirtyAndVolatile
                    }
                    wire::CalculationSelection::ForceAll => calc_host::EvalSelection::ForceAll,
                    wire::CalculationSelection::FormulaEntry => {
                        calc_host::EvalSelection::FormulaEntry
                    }
                },
            },
            Intent::CalculateRange { areas } => SkinIntent::CalculateRange {
                areas: areas
                    .into_iter()
                    .map(|area| area.area().ok_or("The calculation range is invalid"))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            Intent::SetCalcMode { manual } => SkinIntent::SetCalcMode(if manual {
                CalcMode::Manual
            } else {
                CalcMode::Automatic
            }),
            Intent::SetIteration {
                enabled,
                max_iterations,
                max_change_bits,
            } => SkinIntent::Edit {
                edits: vec![calc_host::CellEdit::SetIteration(
                    calc_host::IterationSettings::new(
                        enabled,
                        max_iterations,
                        f64::from_bits(max_change_bits),
                    ),
                )],
            },
            Intent::Select { active, areas } => SkinIntent::Select(Selection {
                active: Some(cell(active)?),
                areas: areas.into_iter().map(area).collect::<Result<_, _>>()?,
            }),
        };
        let intent = if let SkinIntent::Select(mut selection) = intent {
            if let Some(at) = selection.active {
                let merges = self.book.host().merged_areas(at.sheet());
                if let Some(merged) = merges.iter().find(|a| a.contains(at)) {
                    selection.active = Some(merged.first());
                }
                for area in &mut selection.areas {
                    loop {
                        let before = *area;
                        for merge in &merges {
                            if area.intersection(*merge).is_some() {
                                *area = GridArea::new(
                                    CellKey::new(
                                        area.sheet(),
                                        area.first().row().min(merge.first().row()),
                                        area.first().column().min(merge.first().column()),
                                    )
                                    .expect("merge start"),
                                    CellKey::new(
                                        area.sheet(),
                                        area.last().row().max(merge.last().row()),
                                        area.last().column().max(merge.last().column()),
                                    )
                                    .expect("merge end"),
                                )
                                .expect("one sheet");
                            }
                        }
                        if *area == before {
                            break;
                        }
                    }
                }
            }
            SkinIntent::Select(selection)
        } else {
            intent
        };
        self.dispatch_entry(intent, false)
    }

    fn dispatch_entry(&mut self, intent: SkinIntent, confirmed: bool) -> Result<Reply, String> {
        if !confirmed {
            self.validation_pending = None;
            let targets = match &intent {
                SkinIntent::EnterCell { at, text }
                | SkinIntent::EnterCellAndLayout { at, text, .. } => vec![(*at, text.as_str())],
                SkinIntent::EnterSelection { text } => {
                    let selection = self.book.session().selection();
                    if !selection.areas.iter().any(|a| {
                        self.book
                            .host()
                            .loaded()
                            .sheet(a.sheet())
                            .is_some_and(|s| !s.validations().is_empty())
                    }) {
                        Vec::new()
                    } else {
                        if selection.areas.iter().map(|a| a.cell_count()).sum::<u64>() > 10000 {
                            return Ok(self.done(
                                false,
                                Some("Validate at most 10000 selected cells at once".into()),
                                0.0,
                            ));
                        }
                        selection
                            .areas
                            .iter()
                            .flat_map(|a| {
                                (a.first().row()..=a.last().row()).flat_map(move |r| {
                                    (a.first().column()..=a.last().column())
                                        .map(move |c| CellKey::new(a.sheet(), r, c).unwrap())
                                })
                            })
                            .map(|at| (at, text.as_str()))
                            .collect()
                    }
                }
                _ => Vec::new(),
            };
            let mut failures = targets
                .iter()
                .filter_map(|(at, text)| self.book.validation_failure(*at, text))
                .collect::<Vec<_>>();
            failures.sort_by_key(|f| match f.style {
                calc_host::ErrorStyle::Stop => 0,
                calc_host::ErrorStyle::Warning => 1,
                _ => 2,
            });
            if let Some(failure) = failures.first() {
                let token = self.next_clip;
                self.next_clip += 1;
                let style = match failure.style {
                    calc_host::ErrorStyle::Stop => "Stop",
                    calc_host::ErrorStyle::Warning => "Warning",
                    _ => "Information",
                };
                let message = if failures.len() > 1 {
                    format!(
                        "{} ({} selected cells reject this entry)",
                        failure.message,
                        failures.len()
                    )
                } else {
                    failure.message.clone()
                };
                let decision = wire::ValidationDecision {
                    token,
                    style: style.into(),
                    title: failure.title.clone(),
                    message,
                };
                if failure.style != calc_host::ErrorStyle::Stop {
                    self.validation_pending = Some((
                        token,
                        self.book.host().revision().get(),
                        self.book.session().selection().clone(),
                        intent,
                    ));
                }
                let mut outcome = self.outcome(false, Some(failure.to_string()), 0.0);
                outcome.validation = Some(decision);
                return Ok(Reply::Done {
                    outcome,
                    opened: None,
                    saved: None,
                });
            }
        }
        // What a warning or information rule says about an entry, worked out before it is taken.
        let note = match &intent {
            SkinIntent::EnterCell { at, text }
            | SkinIntent::EnterCellAndLayout { at, text, .. } => {
                self.book.validation_note(*at, text).map(|failure| {
                    let kind = match failure.style {
                        calc_host::ErrorStyle::Warning => "Warning",
                        _ => "Information",
                    };
                    (kind.to_owned(), failure.to_string())
                })
            }
            _ => None,
        };
        let locate = |edit: &calc_host::SkinLayoutEdit| match edit {
            calc_host::SkinLayoutEdit::AddPlacement {
                placement,
                spot: calc_host::Spot::Notebook { .. },
                ..
            } => Some(placement.to_string()),
            _ => None,
        };
        let notebook_placement = match &intent {
            SkinIntent::EnterCellAndLayout { layout, .. } => layout.iter().find_map(locate),
            SkinIntent::Edit { edits } => edits.iter().find_map(|e| {
                if let CellEdit::Layout(edit) = e {
                    locate(edit)
                } else {
                    None
                }
            }),
            _ => None,
        };
        // Preserve the request's response shape while the host owns all command rules.
        let rule_error_is_route = matches!(
            &intent,
            SkinIntent::Command(
                calc_host::WorkbookCommand::Sheet { .. }
                    | calc_host::WorkbookCommand::FitContents { .. }
                    | calc_host::WorkbookCommand::AxisFormat { .. }
                    | calc_host::WorkbookCommand::Outline { .. }
                    | calc_host::WorkbookCommand::FreezePanes { .. }
                    | calc_host::WorkbookCommand::SplitPanes { .. }
                    | calc_host::WorkbookCommand::Paste(_)
                    | calc_host::WorkbookCommand::ShiftCells { .. }
            )
        );
        let started = crate::services::now_ms();
        let receipt = self.book.dispatch(intent);
        if rule_error_is_route
            && let Some(calc_host::IntentError::Workbook(reason)) = &receipt.error
        {
            return Err(reason.clone());
        }
        let elapsed = crate::services::now_ms() - started;
        let mut outcome = self.outcome(
            receipt.accepted(),
            receipt.error.as_ref().map(ToString::to_string),
            elapsed,
        );
        outcome.notebook_placement = notebook_placement.filter(|_| receipt.accepted());
        outcome.committed = receipt.committed.is_some();
        outcome.recalculated = receipt.recalc.is_some();
        outcome.recalc = receipt.recalc.as_ref().map(Workbook::recalc_text);
        outcome.note = note.filter(|_| receipt.accepted());
        Ok(Reply::Done {
            outcome,
            opened: None,
            saved: None,
        })
    }

    fn push_calculation_event(&mut self, event: calc_host::CalcEvent, terminal: bool) {
        use calc_host::CalcEvent as E;
        let scope = |scope: calc_host::CalcScope| match scope {
            calc_host::CalcScope::Workbook => wire::CalculationScope::Workbook,
            calc_host::CalcScope::Sheet(sheet) => wire::CalculationScope::Sheet(Sheet::of(sheet)),
            calc_host::CalcScope::Range(areas) => {
                wire::CalculationScope::Range(areas.into_iter().map(Area::of).collect())
            }
        };
        let receipt = match event {
            E::TargetsRefused {
                command,
                targets,
                construct,
                owner,
            } => wire::CalculationEvent::TargetsRefused {
                domain: command.domain_incarnation,
                command: command.sequence,
                targets: targets.into_iter().map(Key::of).collect(),
                construct: construct.into(),
                owner: owner.into(),
            },
            E::Reevaluated {
                previous: _,
                next,
                invocations,
            } => wire::CalculationEvent::Reevaluated {
                domain: next.pass.command.domain_incarnation,
                command: next.pass.command.sequence,
                pass: next.pass.ordinal,
                at: Key::of(next.locus),
                evaluation: next.evaluation,
                reason: format!("{:?}", next.reason),
                previous_calls: invocations.into_iter().map(|i| i.runtime).collect(),
            },
            E::AwaitingAsync(token) => wire::CalculationEvent::AwaitingAsync {
                domain: token.invocation.pass.command.domain_incarnation,
                command: token.invocation.pass.command.sequence,
                pass: token.invocation.pass.ordinal,
                token: token.id,
                at: Key::of(token.target.locus),
                request: token.request,
            },
            E::AsyncReturned { token, disposition } => wire::CalculationEvent::AsyncReturned {
                token,
                disposition: match disposition {
                    calc_host::AsyncDisposition::Accepted => wire::AsyncReturnDisposition::Accepted,
                    calc_host::AsyncDisposition::Duplicate => {
                        wire::AsyncReturnDisposition::Duplicate
                    }
                    calc_host::AsyncDisposition::Rejected(reason) => {
                        wire::AsyncReturnDisposition::Rejected(format!("{reason:?}"))
                    }
                },
            },
            E::Committed { revision } => wire::CalculationEvent::Committed {
                revision: revision.get(),
            },
            E::Admitted(r) => wire::CalculationEvent::Admitted {
                domain: r.command.domain_incarnation,
                command: r.command.sequence,
                pass: r.pass.ordinal,
                revision: r.admitted.model.get(),
                scope: scope(r.scope),
                selection: match r.selection {
                    calc_host::EvalSelection::DirtyAndVolatile => {
                        wire::CalculationSelection::DirtyAndVolatile
                    }
                    calc_host::EvalSelection::ForceAll => wire::CalculationSelection::ForceAll,
                    calc_host::EvalSelection::FormulaEntry => {
                        wire::CalculationSelection::FormulaEntry
                    }
                },
                range_post_state: match r.range_post_state {
                    calc_host::RangePostState::Satisfied => {
                        wire::CalculationRangePostState::Satisfied
                    }
                    calc_host::RangePostState::StillRequired => {
                        wire::CalculationRangePostState::StillRequired
                    }
                },
                predecessor: r.predecessor.map(|id| id.sequence),
            },
            E::Yielded { pass, remaining } => wire::CalculationEvent::Yielded {
                domain: pass.command.domain_incarnation,
                command: pass.command.sequence,
                pass: pass.ordinal,
                remaining,
            },
            E::Superseded(r) => {
                let by = match r.outcome {
                    calc_host::CompletionOutcome::Superseded { by } => by.sequence,
                    _ => unreachable!("a supersession receipt names its successor"),
                };
                wire::CalculationEvent::Superseded {
                    domain: r.command.domain_incarnation,
                    command: r.command.sequence,
                    by,
                    revision: r.admitted.model.get(),
                }
            }
            E::Reused {
                at,
                computed_by,
                certified_for,
            } => wire::CalculationEvent::Reused {
                at: Key::of(at),
                computed_by: computed_by.sequence,
                certified_for: certified_for.sequence,
            },
            E::Rejected { at } => wire::CalculationEvent::Rejected { at: Key::of(at) },
            E::Published(unit) => wire::CalculationEvent::Published(wire::Publication {
                domain: unit.command.domain_incarnation,
                command: unit.command.sequence,
                sequence: unit.sequence,
            }),
            E::Completed(r) => wire::CalculationEvent::Completed {
                domain: r.command.domain_incarnation,
                command: r.command.sequence,
                revision: r.admitted.model.get(),
                outcome: match r.outcome {
                    calc_host::CompletionOutcome::Complete => wire::CalculationOutcome::Complete,
                    calc_host::CompletionOutcome::Incomplete => {
                        wire::CalculationOutcome::Incomplete
                    }
                    calc_host::CompletionOutcome::Cancelled => wire::CalculationOutcome::Cancelled,
                    calc_host::CompletionOutcome::Failed => wire::CalculationOutcome::Failed,
                    calc_host::CompletionOutcome::Superseded { .. } => {
                        wire::CalculationOutcome::Superseded
                    }
                },
                required: r.coverage.required,
                calculated: r.coverage.calculated,
                refused: r.coverage.refused,
                held: r.coverage.held,
                uncertified: r.coverage.uncertified,
                boundary_cached: r.coverage.boundary_cached,
                imported_cached: r.coverage.imported_cached,
            },
        };
        let event = Event::Calculation {
            workbook: self.workbook,
            receipt,
            summary: None,
            failed_projections: Vec::new(),
        };
        if terminal {
            self.deferred_calculation.push(event);
        } else {
            self.events.push(event);
        }
    }

    fn retire_tile_events(&mut self, id: u64) {
        self.events.retain(|event| match event {
            Event::TileAdmission(stamp) => stamp.id != id,
            Event::Tiles(batch) => batch.stamp.id != id,
            Event::TileCompletion(barrier) => barrier.id != id,
            Event::ProjectionFailed { id: failed, .. } => *failed != id,
            Event::Calculation { .. }
            | Event::SaveReady { .. }
            | Event::WorkbookEventFailed { .. } => true,
        });
    }

    fn save_window(&self, input: &wire::SaveWindow) -> Result<calc_host::SaveViews, String> {
        let host = self.book.host();
        if input.revision != host.revision().get() {
            return Err("The workbook changed before saving its window".into());
        }
        let mut sheets = BTreeMap::new();
        for saved in &input.sheets {
            if host.model().sheet_by_id(saved.sheet.id()).is_none()
                || sheets.contains_key(&saved.sheet.id())
            {
                return Err("Invalid saved worksheet".into());
            }
            let g = self.geometry(saved.sheet);
            if g != saved.geometry {
                return Err("The worksheet view is still updating; save again after it settles (dc-971p.16)".into());
            }
            let m = dna_calc_model::geometry::sheet_metrics(Some(&g));
            let mut v = host.sheet_view(saved.sheet.id()).unwrap_or_default();
            let mask = if let Some((x, y)) = g.split {
                u8::from(f64::from_bits(x) > 0.) | (u8::from(f64::from_bits(y) > 0.) << 1)
            } else {
                u8::from(g.frozen_columns > 0) | (u8::from(g.frozen_rows > 0) << 1)
            };
            let first = g.top_left.unwrap_or((1, 1));
            let bias = (
                m.col_start(first.1.saturating_add(g.frozen_columns)) - m.col_start(first.1),
                m.row_start(first.0.saturating_add(g.frozen_rows)) - m.row_start(first.0),
            );
            let mut starts = BTreeMap::new();
            let mut zoom = None;
            for pane in &saved.panes {
                if pane.index > 3
                    || starts.contains_key(&pane.index)
                    || !pane.x.is_finite()
                    || !pane.y.is_finite()
                    || pane.x < 0.
                    || pane.y < 0.
                    || !pane.zoom.is_finite()
                    || !(dna_calc_model::viewport::ZOOM_MIN..=dna_calc_model::viewport::ZOOM_MAX)
                        .contains(&pane.zoom)
                {
                    return Err("Invalid saved pane position or zoom".into());
                }
                if pane.index & !mask != 0 {
                    continue;
                }
                let cell = (
                    m.row_at(pane.y / pane.zoom + bias.1, 1_048_576),
                    m.col_at(pane.x / pane.zoom + bias.0, 16_384),
                );
                starts.insert(pane.index, cell);
                if pane.index == saved.active_pane || zoom.is_none() {
                    zoom = Some((pane.zoom * 100.).round() as u16);
                }
            }
            if g.split_window {
                let a = starts.get(&0).copied().unwrap_or(first);
                let old = g.pane_top_left.unwrap_or(a);
                let b = (
                    starts
                        .iter()
                        .find(|(i, _)| **i & 2 != 0)
                        .map_or(old.0, |(_, c)| c.0),
                    starts
                        .iter()
                        .find(|(i, _)| **i & 1 != 0)
                        .map_or(old.1, |(_, c)| c.1),
                );
                v.top_left = Some(a);
                v.pane_top_left = Some(b);
            } else if let Some(&cell) = starts.get(&0) {
                v.top_left = Some((
                    if g.frozen_rows > 0 { first.0 } else { cell.0 },
                    if g.frozen_columns > 0 {
                        first.1
                    } else {
                        cell.1
                    },
                ));
                if mask != 0 {
                    v.pane_top_left = Some(cell);
                }
            }
            v.zoom = zoom.or(v.zoom);
            v.active_cell = Some(saved.active);
            v.selection = saved.selection;
            v.active_pane = if g.split_window {
                saved.active_pane & mask
            } else {
                u8::from(g.frozen_columns > 0 && saved.active.1 >= first.1 + g.frozen_columns)
                    | (u8::from(g.frozen_rows > 0 && saved.active.0 >= first.0 + g.frozen_rows)
                        << 1)
            };
            sheets.insert(saved.sheet.id(), v);
        }
        let mut seen = std::collections::BTreeSet::new();
        for &(sheet, mode) in &input.sheet_view_modes {
            let mut view = host.sheet_view(sheet.id()).ok_or("Invalid saved worksheet")?;
            if !seen.insert(sheet.id()) {
                return Err("Duplicate saved worksheet mode".into());
            }
            let mode = Some(match mode {
                wire::SheetViewMode::Normal => calc_host::SheetViewMode::Normal,
                wire::SheetViewMode::PageBreakPreview => calc_host::SheetViewMode::PageBreakPreview,
                wire::SheetViewMode::PageLayout => calc_host::SheetViewMode::PageLayout,
            });
            if let Some(saved) = sheets.get_mut(&sheet.id()) {
                saved.mode = mode;
            } else if view.mode != mode {
                view.mode = mode;
                sheets.insert(sheet.id(), view);
            }
        }
        Ok(calc_host::SaveViews {
            revision: input.revision,
            active_sheet: Some(input.active_sheet.id()),
            sheets,
        })
    }
    /// The sheet's geometry as it goes on the wire (the display's column and row sizes).
    pub fn geometry(&self, sheet: Sheet) -> wire::Geometry {
        let mut geometry = self
            .book
            .host()
            .sheet_geometry(sheet.id())
            .map(|g| wire::geometry_of(&g))
            .unwrap_or_default();
        // The default font sets the digit width columns are counted in and the default row height.
        geometry.default_digit_px = Some(self.book.host().default_digit_px());
        geometry.default_font = self.book.host().default_font_name().map(str::to_owned);
        geometry.default_row_height.get_or_insert(self.book.host().default_font_row_points());
        let layouts = self.book.host().model().definitions().axis_layouts();
        geometry.outline = [calc_host::Axis::Rows, calc_host::Axis::Columns]
            .into_iter()
            .flat_map(|axis| {
                layouts
                    .outline_groups(sheet.id(), axis)
                    .into_iter()
                    .map(move |g| wire::OutlineGroup {
                        rows: axis == calc_host::Axis::Rows,
                        first: g.first,
                        last: g.last,
                        level: g.level,
                        summary: g.summary,
                        collapsed: g.collapsed,
                    })
            })
            .collect();
        geometry.auto_filter = self
            .book
            .host()
            .model()
            .definitions()
            .auto_filter(sheet.id())
            .map(|f| wire::FilterView {
                table: None,
                area: Area::of(f.area),
                columns: f.columns.keys().copied().collect(),
                filtered: f.filtered_rows.len() as u32,
                unsupported: f.unsupported,
            });
        geometry.tables = self
            .book
            .host()
            .model()
            .definitions()
            .tables()
            .iter()
            .filter(|t| t.sheet == sheet.id())
            .map(|t| wire::TableView {
                part: t.part.clone(),
                name: t.display_name.clone(),
                area: Area::of(t.area),
                columns: t.columns.clone(),
                column_formulas: t.column_formulas.clone(),
                headers: t.header_rows > 0,
                totals: t.totals_rows > 0,
                style: t.style.name.clone().unwrap_or_default(),
                row_stripes: t.style.row_stripes,
                column_stripes: t.style.column_stripes,
                first_column: t.style.first_column,
                last_column: t.style.last_column,
                opaque: t.axis_opaque,
                filter: t.filter.as_ref().map(|f| wire::FilterView {
                    table: Some(t.part.clone()),
                    area: Area::of(f.area),
                    columns: f.columns.keys().copied().collect(),
                    filtered: f.filtered_rows.len() as u32,
                    unsupported: f.unsupported,
                }),
            })
            .collect();
        geometry.merges = self
            .book
            .host()
            .merged_areas(sheet.id())
            .into_iter()
            .map(wire::Area::of)
            .collect();
        geometry.edit_refusal = calc_host::can_edit(
            self.book.host(),
            CellKey::new(sheet.id(), 1, 1).expect("sheet origin"),
        )
        .err();
        if let Some(panes) = self
            .book
            .host()
            .model()
            .definitions()
            .frozen_panes(sheet.id())
        {
            geometry.frozen_rows = panes.rows();
            geometry.frozen_columns = panes.columns();
        }
        if let Some(view) = self.book.host().sheet_view(sheet.id()) {
            geometry.zoom = view.zoom;
            geometry.top_left = view.top_left;
            geometry.pane_top_left = view.pane_top_left;
            geometry.active_cell = view.active_cell;
            geometry.active_pane = view.active_pane;
            geometry.split = view.split;
            geometry.split_window = view.split_window;
        }
        geometry
    }

    /// The cells outside a bounded area subscription whose text can overflow into it: in each
    /// subscribed row, the nearest authored cell to the left and to the right, within
    /// OVERHANG_COLUMNS. A nearer one stops the text of one beyond it, so these are the only
    /// candidates. The page lays the overflow out, but only from cells it was sent.
    fn overhang_of(&self, interest: &calc_host::Interest) -> Vec<GridArea> {
        let areas = match interest {
            calc_host::Interest::Area(a) => vec![*a],
            calc_host::Interest::Areas(areas) => areas.clone(),
            _ => return Vec::new(),
        };
        if areas.iter().map(|a| a.cell_count()).sum::<u64>() > OVERHANG_CELLS {
            return Vec::new();
        }
        let model = self.book.host().model();
        let mut found = std::collections::BTreeSet::new();
        for area in &areas {
            let (sheet, top, bottom) = (area.sheet(), area.first().row(), area.last().row());
            let (left, right) = (area.first().column(), area.last().column());
            let strip = |first: u32, last: u32| {
                let first = CellKey::new(sheet, top, first).ok()?;
                let last = CellKey::new(sheet, bottom, last).ok()?;
                GridArea::new(first, last).ok()
            };
            let mut nearest: BTreeMap<u32, u32> = BTreeMap::new();
            if left > 1
                && let Some(strip) = strip(left.saturating_sub(OVERHANG_COLUMNS).max(1), left - 1)
            {
                for (at, _) in model.cells(strip).into_iter().flatten() {
                    nearest.insert(at.row(), at.column());
                }
                found.extend(
                    std::mem::take(&mut nearest)
                        .into_iter()
                        .map(|(row, column)| (sheet, row, column)),
                );
            }
            if right < 16_384
                && let Some(strip) = strip(
                    right + 1,
                    right.saturating_add(OVERHANG_COLUMNS).min(16_384),
                )
            {
                for (at, _) in model.cells(strip).into_iter().flatten() {
                    nearest.entry(at.row()).or_insert(at.column());
                }
                found.extend(
                    std::mem::take(&mut nearest)
                        .into_iter()
                        .map(|(row, column)| (sheet, row, column)),
                );
            }
        }
        found
            .into_iter()
            .filter_map(|(sheet, row, column)| CellKey::new(sheet, row, column).ok())
            .filter(|at| !areas.iter().any(|a| a.contains(*at)))
            .map(GridArea::cell)
            .collect()
    }

    /// Finds each subscription's overflow anchors again after an authored change and moves the
    /// host's interest only when the set changed.
    fn refresh_overhang(&mut self) {
        let revision = self.book.host().revision().get();
        let stale: Vec<u64> = self
            .overhang
            .iter()
            .filter(|(_, (at, _))| *at != revision)
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            let (Some(interest), Some(&subscription)) = (
                self.subscription_interests.get(&id).cloned(),
                self.subscriptions.get(&id),
            ) else {
                continue;
            };
            let extras = self.overhang_of(&interest);
            let changed = self.overhang.get(&id).is_none_or(|(_, old)| *old != extras);
            if changed {
                self.book.dispatch(SkinIntent::SetInterest {
                    subscription,
                    interest: widened(&interest, &extras),
                });
            }
            self.overhang.insert(id, (revision, extras));
        }
    }

    /// Adapt installed host deltas into bounded projection publications. There is no new
    /// evaluation here. Geometry-only revisions also publish, keeping value/style/layout roots
    /// related even when no displayed text changed.
    fn collect(&mut self) {
        let failures = self.book.take_workbook_event_failures();
        for (index, failure) in failures.failures.into_iter().enumerate() {
            let event = match failure.event {
                calc_host::WorkbookEvent::Open => "Workbook_Open",
                calc_host::WorkbookEvent::AfterCalculate => "AfterCalculate",
                calc_host::WorkbookEvent::WorksheetChange { .. } => "Worksheet_Change",
                calc_host::WorkbookEvent::WorksheetCalculate { .. } => "Worksheet_Calculate",
                calc_host::WorkbookEvent::SelectionChanged { .. } => "Worksheet_SelectionChange",
                calc_host::WorkbookEvent::BeforeSave { .. } => "Workbook_BeforeSave",
            };
            self.events.push(Event::WorkbookEventFailed {
                workbook: self.workbook,
                sequence: failure.sequence,
                event: event.into(),
                reason: failure.reason,
                omitted: if index == 0 { failures.omitted } else { 0 },
            });
        }
        let mut terminal = Vec::new();
        for event in self.book.take_calculation_events() {
            if matches!(
                event,
                calc_host::CalcEvent::Published(_) | calc_host::CalcEvent::Completed(_)
            ) {
                terminal.push(event);
            } else {
                self.push_calculation_event(event, false);
            }
        }
        self.refresh_overhang();
        let deltas = self.book.take_deltas();
        let revision = self.book.host().revision().get();
        let geometry: Vec<_> = self
            .book
            .sheets()
            .into_iter()
            .map(|s| {
                let sheet = Sheet::of(s.id);
                (sheet, self.geometry(sheet))
            })
            .collect();
        let geometry: BTreeMap<_, Vec<_>> = self
            .subscription_interests
            .iter()
            .map(|(&id, interest)| {
                let areas = self.book.session().interest_areas(interest);
                (
                    id,
                    geometry
                        .iter()
                        .map(|(sheet, g)| {
                            let mut g = g.clone();
                            g.coverage = areas
                                .iter()
                                .filter(|a| a.sheet() == sheet.id())
                                .copied()
                                .map(wire::Area::of)
                                .collect();
                            (*sheet, g)
                        })
                        .collect(),
                )
            })
            .collect();
        let ids: BTreeMap<SubscriptionId, u64> =
            self.subscriptions.iter().map(|(&p, &h)| (h, p)).collect();
        for delta in deltas {
            let Some(&id) = ids.get(&delta.subscription) else {
                continue;
            };
            let mut cells = Vec::new();
            let mut removed = Vec::new();
            let mut retained = 0usize;
            let mut oversized = false;
            for projected in &delta.cells {
                if cells.len() + removed.len() >= wire::MAX_PUBLICATION_RECORDS {
                    oversized = true;
                    break;
                }
                let shown = self.book.shown(projected);
                if projected.input().is_none()
                    && projected.value() == &ProjectedValue::Empty
                    && shown.conditional.is_none()
                    && self.book.host().cell_style(projected.at()) == 0
                {
                    retained = retained.saturating_add(16);
                    if retained > wire::MAX_PUBLICATION_BYTES - wire::MAX_TILE_BYTES {
                        oversized = true;
                        break;
                    }
                    removed.push(Key::of(projected.at()));
                    continue;
                }
                let cell = self.book.wire_cell(projected, &shown);
                retained = retained.saturating_add(cell.retained_bytes());
                if cells.len() >= wire::MAX_TILE_CELLS
                    || retained > wire::MAX_PUBLICATION_BYTES - wire::MAX_TILE_BYTES
                {
                    oversized = true;
                    break;
                }
                cells.push(cell);
            }
            if oversized {
                if let Some(stream) = self.tile_streams.get_mut(&id) {
                    stream.reject();
                    stream.revision = revision;
                }
                self.events.push(Event::ProjectionFailed {
                    id,
                    reason: "This projection exceeds the tile budget; narrow its visible area"
                        .into(),
                });
                continue;
            }
            let publication = match &delta.cause {
                calc_host::DeltaCause::Recalculated { publication, .. } => {
                    Some(wire::Publication {
                        domain: publication.command.domain_incarnation,
                        command: publication.command.sequence,
                        sequence: publication.sequence,
                    })
                }
                _ => None,
            };
            if let Some(stream) = self.tile_streams.get_mut(&id) {
                if let Err(reason) = stream.update(
                    delta.reset,
                    cells,
                    removed,
                    revision,
                    geometry.get(&id).cloned().unwrap_or_default(),
                    publication,
                ) {
                    self.events.push(Event::ProjectionFailed { id, reason });
                }
            }
        }
        for (&id, stream) in &mut self.tile_streams {
            if stream.revision != revision {
                if let Err(reason) = stream.update(
                    false,
                    Vec::new(),
                    Vec::new(),
                    revision,
                    geometry.get(&id).cloned().unwrap_or_default(),
                    None,
                ) {
                    self.events.push(Event::ProjectionFailed { id, reason });
                }
            }
            let transaction = self.next_publication;
            self.next_publication = self
                .next_publication
                .checked_add(1)
                .expect("Tile transaction identities exhausted");
            match stream.publish(id, self.workbook, transaction) {
                Ok(events) => self.events.extend(events),
                Err(reason) => self.events.push(Event::ProjectionFailed { id, reason }),
            }
        }
        for event in terminal {
            self.push_calculation_event(event, true);
        }
        if self
            .tile_streams
            .values()
            .all(|stream| !stream.has_pending())
        {
            self.release_calculation_events(false);
        }
    }

    /// Receipt frontiers stay immutable. The summary describes current host state at
    /// delivery, including same-revision changes such as mode switches and successful saves.
    fn release_calculation_events(&mut self, closing: bool) {
        if self.deferred_calculation.is_empty() {
            return;
        }
        let current = self.summary();
        let failed: Vec<_> = self
            .tile_streams
            .iter()
            .filter_map(|(&id, stream)| {
                (stream.failed() || closing && stream.has_pending()).then_some(id)
            })
            .collect();
        for event in &mut self.deferred_calculation {
            if let Event::Calculation {
                summary,
                failed_projections,
                ..
            } = event
            {
                *summary = Some(current.clone());
                *failed_projections = failed.clone();
            }
        }
        self.events.append(&mut self.deferred_calculation);
    }

    fn notebook(
        &self,
        sheet: Sheet,
        requested: usize,
        reveal: Option<CellKey>,
        placement: Option<&str>,
    ) -> wire::Notebook {
        use calc_host::{BlockKind, ViewKind};
        let host = self.book.host();
        let layout = host.model().layout();
        let mut linked = Vec::new();
        let mut placed = Vec::new();
        for placement in layout.view(ViewKind::Notebook) {
            if let Some(block) = layout.block(placement.block()) {
                let anchor = block.anchor();
                if anchor.is_none_or(|a| a.sheet() == sheet.id()) {
                    if let Some(a) = anchor {
                        placed.push(a);
                    }
                    linked.push((
                        Some(placement.id().to_string()),
                        anchor,
                        block.kind() == BlockKind::Prose,
                    ));
                }
            }
        }
        let reveal = reveal.filter(|at| {
            host.occupied_areas(sheet.id())
                .iter()
                .any(|a| a.contains(*at))
        });
        let (keys, omitted) =
            crate::tree::outline_keys(host, sheet.id(), wire::NOTEBOOK_PAGE, reveal);
        // Containment against the placed anchors: a one-cell anchor is a set lookup, the few
        // larger ones a scan.
        let single: std::collections::HashSet<CellKey> = placed
            .iter()
            .filter(|a| a.first() == a.last())
            .map(|a| a.first())
            .collect();
        let wide: Vec<GridArea> = placed
            .iter()
            .filter(|a| a.first() != a.last())
            .copied()
            .collect();
        linked.extend(
            keys.into_iter()
                .filter(|at| !single.contains(at) && !wide.iter().any(|a| a.contains(*at)))
                .map(|at| (None, Some(GridArea::cell(at)), false)),
        );
        let total = linked.len();
        let pages = total.div_ceil(wire::NOTEBOOK_PAGE).max(1);
        let page = placement
            .and_then(|id| linked.iter().position(|(p, _, _)| p.as_deref() == Some(id)))
            .or_else(|| {
                reveal.and_then(|at| {
                    linked
                        .iter()
                        .position(|(_, a, _)| a.is_some_and(|a| a.contains(at)))
                })
            })
            .map(|i| i / wire::NOTEBOOK_PAGE)
            .unwrap_or(requested)
            .min(pages - 1);
        let blocks = linked
            .into_iter()
            .skip(page * wire::NOTEBOOK_PAGE)
            .take(wire::NOTEBOOK_PAGE)
            .map(|(placement, anchor, prose)| {
                let cell = anchor
                    .and_then(|a| self.book.cell(a.first()))
                    .map(|p| self.book.wire_cell(&p, &self.book.shown(&p)));
                wire::NotebookBlock {
                    placement,
                    anchor: anchor.map(Area::of),
                    prose,
                    cell,
                }
            })
            .collect();
        wire::Notebook {
            total: total as u64,
            page: page as u64,
            omitted,
            blocks,
        }
    }

    fn notebook_link(
        &mut self,
        target: Area,
        prose: bool,
        text: Option<String>,
    ) -> Result<Reply, String> {
        let intent =
            calc_host::prepare_notebook_link(self.book.host(), area(target)?, prose, text)?;
        self.dispatch_entry(intent, false)
    }
    fn notebook_order(&mut self, id: String, direction: i8) -> Result<Reply, String> {
        let id = calc_host::PlacementId::parse(&id).ok_or("Invalid notebook placement")?;
        match calc_host::prepare_notebook_order(self.book.host(), id, direction)? {
            Some(intent) => self.dispatch_entry(intent, false),
            None => Ok(self.done(true, None, 0.)),
        }
    }
    fn block_detail(&self, linked: GridArea) -> wire::BlockDetail {
        let host = self.book.host();
        let at = linked.first();
        let owner = host.spill_owner(at);
        let blocked = host.blocked_spill_area(at);
        let area = blocked
            .or_else(|| owner.and_then(|a| host.spill_area(a)))
            .unwrap_or(linked);
        let (blockers, more) = host.spill_blockers(at, 32);
        let keys = (area.first().row()..=area.last().row())
            .flat_map(|row| {
                (area.first().column()..=area.last().column())
                    .map(move |col| CellKey::new(area.sheet(), row, col).unwrap())
            })
            .take(32)
            .collect::<Vec<_>>();
        let preview = self
            .book
            .project(keys)
            .iter()
            .map(|p| self.book.wire_cell(p, &self.book.shown(p)))
            .collect::<Vec<_>>();
        let label = cells::area_label(host, area);
        let summary = if blocked.is_some() {
            format!(
                "{} cannot spill into {label}. {} blocking areas shown{}; preview is current worksheet contents.",
                cells::cell_label(host, at),
                blockers.len(),
                if more { " (more not shown)" } else { "" }
            )
        } else if let Some(owner) = owner {
            format!(
                "Array anchor {}; actual range {label}. Showing {} of {} cells.",
                cells::cell_label(host, owner),
                preview.len(),
                area.cell_count()
            )
        } else {
            format!(
                "Worksheet source {label}. Showing {} of {} cells.",
                preview.len(),
                area.cell_count()
            )
        };
        wire::BlockDetail {
            summary,
            anchor: owner.map(Key::of),
            cells: preview,
            blockers: blockers.into_iter().map(Area::of).collect(),
        }
    }

    /// The tree's outline ([`crate::tree::outline_sheet`] per sheet) with the open sheets' shown
    /// cells.
    fn outline(
        &self,
        shown: &[(Sheet, u64)],
        open: &[Sheet],
        reveal: Option<CellKey>,
        query: &str,
    ) -> wire::Outline {
        let query = query.trim().to_lowercase();
        let mut omitted = 0;
        let host = self.book.host();
        let names = host
            .defined_names()
            .iter()
            .filter(|name| {
                !host
                    .model()
                    .layout()
                    .blocks()
                    .iter()
                    .any(|b| b.locator().as_str() == name.name)
            })
            .map(|name| {
                let formula = name.formula.strip_prefix('=').unwrap_or(&name.formula);
                format!("{} = {formula}", name.name)
            })
            .filter(|text| query.is_empty() || text.to_lowercase().contains(&query))
            .collect();
        let mut sheets = Vec::new();
        let mut wanted = Vec::new();
        for tab in self.book.sheets() {
            let sheet = Sheet::of(tab.id);
            let limit = shown
                .iter()
                .find(|(s, _)| *s == sheet)
                .map_or(crate::tree::ROW_PAGE, |(_, n)| *n as usize);
            let (keys, hidden) = crate::tree::outline_keys(host, tab.id, limit, reveal);
            omitted += hidden;
            let keys: Vec<_> = keys
                .into_iter()
                .filter(|at| {
                    query.is_empty()
                        || format!(
                            "{}!{} {}",
                            tab.name,
                            cells::address(at.row(), at.column()),
                            cells::input_text(host.model().cell(*at))
                        )
                        .to_lowercase()
                        .contains(&query)
                })
                .collect();
            if !query.is_empty() && keys.is_empty() {
                continue;
            }
            let mut outline =
                crate::tree::outline_sheet(tab.id, tab.name, keys.iter().copied(), limit);
            if query.is_empty()
                && let Some(at) = reveal.filter(|at| at.sheet() == tab.id)
            {
                if !outline.rows.iter().any(|(row, _)| *row == at.row()) {
                    outline.rows.push((
                        at.row(),
                        keys.iter()
                            .copied()
                            .filter(|key| key.row() == at.row())
                            .collect(),
                    ));
                    outline.rows.sort_by_key(|(row, _)| *row);
                    outline.more_rows = outline.more_rows.saturating_sub(1);
                }
            }
            if open.contains(&sheet) {
                for (_, row) in &outline.rows {
                    wanted.extend(row.iter().take(crate::tree::ROW_CELLS).copied());
                    if let Some(at) = reveal.filter(|at| row.contains(at)) {
                        wanted.push(at);
                    }
                }
            }
            sheets.push(wire::OutlineSheet {
                sheet,
                name: outline.name,
                cells: outline.cells as u64,
                rows: outline
                    .rows
                    .iter()
                    .map(|(row, cells)| (*row, cells.iter().copied().map(Key::of).collect()))
                    .collect(),
                more_rows: outline.more_rows as u64,
            });
        }
        wanted.sort();
        wanted.dedup();
        let cells = self
            .book
            .project(wanted)
            .iter()
            .map(|p| self.book.wire_cell(p, &self.book.shown(p)))
            .collect();
        wire::Outline {
            omitted: omitted as u64,
            workbook: self.book.name().to_owned(),
            names,
            functions: host
                .user_function_descriptors()
                .into_iter()
                .map(|d| wire::FunctionDoc {
                    name: d.name,
                    origin: d.origin,
                    category: d.category,
                    description: d.description,
                    parameters: d
                        .parameters
                        .into_iter()
                        .map(|p| (p.name, p.description))
                        .collect(),
                    volatile: d.volatile,
                    thread_safe: d.thread_safe,
                    asynchronous: d.asynchronous,
                })
                .collect(),
            sheets,
            cells,
        }
    }

    /// The sheet's charts, laid out with its sizes at 100% and drawn.
    /// Describe saved definitions; cache contents never stand in for a refresh.
    fn pivots(&self, sheet: Sheet) -> (Vec<wire::SavedPivot>, Vec<String>) {
        let state = self.book.host().model().definitions().pivots();
        let caches = &state.caches;
        let mut issues = caches
            .issues
            .iter()
            .map(|e| format!("{}: {} ({})", e.part, e.detail, e.owner))
            .collect::<Vec<_>>();
        let Some(source) = state.sheet(sheet.id()) else {
            return (Vec::new(), Vec::new());
        };
        issues.extend(
            source
                .issues
                .iter()
                .map(|e| format!("{}: {} ({})", e.part, e.detail, e.owner)),
        );
        let tables = source
            .tables
            .iter()
            .map(|p| {
                let saved_cache = caches.caches.get(&p.cache_id);
                let (source, fields, cache) = match saved_cache {
                    Some(c) => {
                        let field = |index: i32| {
                            if index == wire::SavedPivotLayout::VALUES {
                                "Values".to_owned()
                            } else {
                                usize::try_from(index)
                                    .ok()
                                    .and_then(|i| c.fields.get(i))
                                    .map(|f| f.name.clone())
                                    .unwrap_or_else(|| format!("Field {index} (unread)"))
                            }
                        };
                        let mut fields = Vec::new();
                        if !p.row_fields.is_empty() {
                            fields.push(format!(
                                "Rows: {}",
                                p.row_fields
                                    .iter()
                                    .map(|i| field(*i))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                        }
                        if !p.column_fields.is_empty() {
                            fields.push(format!(
                                "Columns: {}",
                                p.column_fields
                                    .iter()
                                    .map(|i| field(*i))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                        }
                        if !p.page_fields.is_empty() {
                            fields.push(format!(
                                "Filters: {}",
                                p.page_fields
                                    .iter()
                                    .map(|i| field(i.field as i32))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                        }
                        if !p.data_fields.is_empty() {
                            fields.push(format!(
                                "Values: {}",
                                p.data_fields
                                    .iter()
                                    .map(|v| format!(
                                        "{} ({})",
                                        v.name.clone().unwrap_or_else(|| field(v.field as i32)),
                                        v.subtotal
                                    ))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                        }
                        let source = if let Some(name) = &c.source_name {
                            format!("Source name: {name}")
                        } else if let (Some(sheet), Some(range)) =
                            (&c.source_sheet, &c.source_range)
                        {
                            format!("Source: {sheet}!{range}")
                        } else {
                            format!("Saved {} source", c.source_type)
                        };
                        (
                            source,
                            fields.join(" · "),
                            format!(
                                "Saved cache {} · {} fields · {} records declared",
                                c.id,
                                c.fields.len(),
                                c.record_count
                                    .map(|n| n.to_string())
                                    .unwrap_or_else(|| "unknown".into())
                            ),
                        )
                    }
                    None => {
                        issues.push(format!(
                            "{}: cache {} was not read (dc-8wp)",
                            p.name, p.cache_id
                        ));
                        (
                            "Source unavailable".into(),
                            "Fields unavailable".into(),
                            format!("Saved cache {} is unread", p.cache_id),
                        )
                    }
                };
                let address = |cell: CellKey| {
                    format!(
                        "{}{}",
                        dna_calc_model::address::column_letters(cell.column()),
                        cell.row()
                    )
                };
                let layout = saved_cache.and_then(|cache| {
                    (cache.source_type == "worksheet"
                        && cache.fields.len() == p.fields.len()
                        && cache.unsupported.is_empty()
                        && (cache.records_part.is_some() || cache.snapshot.is_some())
                        && p.page_fields.is_empty()
                        && p.filters.is_empty()
                        && p.unsupported.is_empty()
                        && !p.data_fields.is_empty())
                    .then(|| wire::SavedPivotLayout {
                        source_fields: cache.fields.iter().map(|field| field.name.clone()).collect(),
                        rows: p.row_fields.clone(),
                        columns: p.column_fields.clone(),
                    })
                });
                wire::SavedPivot {
                    refreshed_revision: saved_cache
                        .and_then(|cache| cache.snapshot.as_ref())
                        .filter(|snapshot| {
                            !matches!(
                                snapshot.origin,
                                calc_host::PivotSnapshotOrigin::FileRecords { .. }
                            )
                        })
                        .map(|snapshot| snapshot.source_revision),
                    name: p.name.clone(),
                    part: p.part.clone(),
                    area: Area::of(p.area),
                    location: format!("{}:{}", address(p.area.first()), address(p.area.last())),
                    source,
                    fields,
                    cache,
                    layout,
                }
            })
            .collect();
        (tables, issues)
    }

    /// The sheet's shapes and text boxes as the page draws them, in drawing order (dc-zjsg.2).
    /// The sheet's sparklines with their axis ends worked out and colours resolved (dc-zjsg.7).
    fn drawn_sparklines(&self, sheet: Sheet) -> Vec<dna_calc_model::sparklines::DrawnSparkline> {
        let host = self.book.host();
        let mut out = Vec::new();
        for (group, lines) in host.sparklines(sheet.id()) {
            let colour = |name: &str| {
                group.colour(name).and_then(|c| match host.sparkline_colour(c) {
                    Some(calc_host::LookColor::Rgb(rgb)) => Some(rgb.to_ascii_uppercase()),
                    _ => None,
                })
            };
            let finite = |values: &[Option<f64>]| values.iter().flatten().copied().filter(|v| v.is_finite()).collect::<Vec<_>>();
            let group_values: Vec<f64> = lines.iter().flat_map(|l| finite(&l.values)).collect();
            let extreme = |values: &[f64], max: bool| {
                values.iter().copied().fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| if max { a.max(v) } else { a.min(v) })))
            };
            let weight = group.line_weight.as_deref().and_then(|w| w.parse::<f64>().ok()).unwrap_or(0.75) * 96.0 / 72.0;
            for line in lines {
                let own = finite(&line.values);
                let end = |kind: Option<&str>, manual: Option<&str>, max: bool| match kind {
                    Some("custom") => manual.and_then(|m| m.parse::<f64>().ok()),
                    Some("group") => extreme(&group_values, max),
                    _ => extreme(&own, max),
                };
                let (Some(min), Some(max)) = (
                    end(group.min_axis.as_deref(), group.manual_min.as_deref(), false).or_else(|| extreme(&own, false)),
                    end(group.max_axis.as_deref(), group.manual_max.as_deref(), true).or_else(|| extreme(&own, true)),
                ) else {
                    continue;
                };
                let flagged = |on: bool, name: &str| if on { colour(name) } else { None };
                // On a date axis, each point's share of the way from the earliest date to the latest.
                let dated = finite(&line.dates);
                let positions = match (extreme(&dated, false), extreme(&dated, true)) {
                    (Some(first), Some(last)) => line
                        .dates
                        .iter()
                        .map(|d| d.filter(|d| d.is_finite()).map(|d| if last > first { (d - first) / (last - first) } else { 0.5 }))
                        .collect(),
                    _ => Vec::new(),
                };
                out.push(dna_calc_model::sparklines::DrawnSparkline {
                    positions,
                    cell: Key::of(line.cell),
                    kind: group.kind.clone().unwrap_or_else(|| "line".into()),
                    values: line.values,
                    min,
                    max,
                    empty: group.empty_cells.clone().unwrap_or_else(|| "zero".into()),
                    weight,
                    series: colour("colorSeries").unwrap_or_else(|| "#000000".into()),
                    negative: flagged(group.negative, "colorNegative"),
                    axis: flagged(group.axis, "colorAxis"),
                    markers: flagged(group.markers, "colorMarkers"),
                    first: flagged(group.first, "colorFirst"),
                    last: flagged(group.last, "colorLast"),
                    high: flagged(group.high, "colorHigh"),
                    low: flagged(group.low, "colorLow"),
                    right_to_left: group.right_to_left,
                });
            }
        }
        out
    }

    fn drawn_shapes(&self, sheet: Sheet) -> Vec<wire::DrawnShape> {
        self.drawn_shapes_where(sheet, |_| true)
    }

    /// The sheet's objects `keep` picks, drawn (the print route keeps the ones printed with the sheet).
    fn drawn_shapes_where(&self, sheet: Sheet, keep: impl Fn(&calc_host::Shape) -> bool) -> Vec<wire::DrawnShape> {
        let host = self.book.host();
        let theme = host.theme_colours();
        let lines = host.theme_line_widths();
        host.shapes(sheet.id())
            .iter()
            .filter(|shape| keep(shape))
            .filter_map(|shape| crate::drawn::drawn_with(shape, self.place_as(sheet, &shape.anchor, shape.edit_as.as_deref())?, theme, lines))
            .collect()
    }

    /// Where an anchor lands on the grid, in the page's terms.
    fn place_of(&self, sheet: Sheet, anchor: &calc_host::ChartAnchor) -> Option<ChartPlace> {
        self.place_as(sheet, anchor, None)
    }

    /// [`Self::place_of`] for an object with an anchor's `editAs` (a two-cell anchor that moves as one cell).
    fn place_as(&self, sheet: Sheet, anchor: &calc_host::ChartAnchor, edit_as: Option<&str>) -> Option<ChartPlace> {
        Some(match self.book.host().drawing_place(sheet.id(), anchor, edit_as)? {
            calc_host::ChartPlace::Cells { from, to } => ChartPlace::Cells {
                from: (Key::of(from.cell), from.dx, from.dy),
                to: (Key::of(to.cell), to.dx, to.dy),
            },
            calc_host::ChartPlace::Sized { from, width, height } => ChartPlace::Sized {
                from: (Key::of(from.cell), from.dx, from.dy),
                width,
                height,
            },
            calc_host::ChartPlace::Absolute { x, y, width, height } => ChartPlace::Absolute { x, y, width, height },
        })
    }

    /// The sheet's form controls with the state each shows now (dc-zjsg.8): a check box from its linked cell (TRUE
    /// checked, FALSE unchecked, an error mixed), else its stored state; the radio buttons of the sheet as one group
    /// (their place by identity, the group's linked cell the first one names; a group box's own group not yet
    /// modelled); a list or combo box's selection and a scroll bar's or spinner's value from the linked cell, else
    /// as stored.
    fn drawn_controls(&self, sheet: Sheet) -> Vec<dna_calc_model::controls::DrawnControl> {
        use calc_host::FormControlView;
        use calc_value::CalcValue;
        let views = self.book.host().form_controls(sheet.id());
        let number = |v: &Option<CalcValue>| match v.as_ref().map(|v| v.core_ref()) {
            Some(calc_value::CoreRef::Number(n)) => Some(n),
            Some(calc_value::CoreRef::Logical(b)) => Some(if b { 1.0 } else { 0.0 }),
            _ => None,
        };
        // Option buttons go in groups: those inside one group box are a group, those inside none are the sheet's
        // group. A button is inside the smallest group box its whole rectangle lies in (ASSUMED until Excel answers).
        // Within a group the buttons count from 1 in the order they were made (their identities), and the group
        // shares the first linked cell any of them states.
        let host = self.book.host();
        let rect = |v: &FormControlView| host.anchor_rectangle(sheet.id(), &v.control.anchor);
        let boxes: Vec<(u32, (f64, f64, f64, f64))> = views.iter().filter(|v| v.control.kind == "GBox").map(|v| (v.control.id, rect(v))).collect();
        let group_of = |v: &FormControlView| -> Option<u32> {
            let (l, t, w, h) = rect(v);
            boxes
                .iter()
                .filter(|(_, (bl, bt, bw, bh))| l >= *bl && t >= *bt && l + w <= bl + bw && t + h <= bt + bh)
                .min_by(|a, b| (a.1.2 * a.1.3).total_cmp(&(b.1.2 * b.1.3)))
                .map(|(id, _)| *id)
        };
        let mut radios: Vec<(&FormControlView, Option<u32>)> = views.iter().filter(|v| v.control.kind == "Radio").map(|v| (v, group_of(v))).collect();
        radios.sort_by_key(|(v, _)| v.control.id);
        let group = |id: u32| -> (Vec<&FormControlView>, Option<CellKey>, Option<f64>) {
            let of = radios.iter().find(|(v, _)| v.control.id == id).and_then(|(_, g)| *g);
            let members: Vec<&FormControlView> = radios.iter().filter(|(_, g)| *g == of).map(|(v, _)| *v).collect();
            let link = members.iter().find_map(|v| v.link);
            let value = members.iter().find(|v| v.link.is_some()).and_then(|v| number(&v.linked));
            (members, link, value)
        };
        views
            .iter()
            .filter_map(|v| {
                let c = &v.control;
                let place = self.place_of(sheet, &c.anchor)?;
                let (mut link, mut position) = (v.link, 0);
                let checked = match c.kind.as_str() {
                    "CheckBox" => match v.linked.as_ref().map(|x| x.core_ref()) {
                        Some(calc_value::CoreRef::Logical(true)) => "Checked".to_owned(),
                        Some(calc_value::CoreRef::Logical(false)) => "Unchecked".to_owned(),
                        Some(calc_value::CoreRef::Error(_)) => "Mixed".to_owned(),
                        Some(calc_value::CoreRef::Number(n)) => if n != 0.0 { "Checked" } else { "Unchecked" }.to_owned(),
                        _ => c.checked.clone().unwrap_or_else(|| "Unchecked".into()),
                    },
                    "Radio" => {
                        let (members, group_link, group_value) = group(c.id);
                        position = members.iter().position(|r| r.control.id == c.id).map_or(1, |p| p + 1);
                        link = group_link;
                        match group_value {
                            Some(n) => if n == position as f64 { "Checked" } else { "Unchecked" }.to_owned(),
                            None => c.checked.clone().unwrap_or_else(|| "Unchecked".into()),
                        }
                    }
                    _ => "Unchecked".to_owned(),
                };
                let stored = |value: Option<i64>| number(&v.linked).map(|n| n as i64).or(value);
                let selected = match c.kind.as_str() {
                    "List" | "Drop" => stored(c.selected).filter(|&n| n >= 1).map(|n| (n - 1) as usize),
                    _ => None,
                };
                Some(dna_calc_model::controls::DrawnControl {
                    id: c.id,
                    name: c.name.clone(),
                    kind: c.kind.clone(),
                    place,
                    caption: c.caption.clone(),
                    checked,
                    selected,
                    value: stored(c.value).unwrap_or(0),
                    min: c.min.unwrap_or(0),
                    max: c.max.unwrap_or(100),
                    increment: c.increment.unwrap_or(1),
                    page: c.page.unwrap_or(10),
                    items: v.items.clone(),
                    horizontal: c.horizontal,
                    link: link.map(Key::of),
                    position,
                    macro_name: c.macro_name.clone(),
                })
            })
            .collect()
    }

    fn charts(&self, sheet: Sheet) -> Vec<Chart> {
        let host = self.book.host();
        let geometry = host.sheet_geometry(sheet.id());
        let m = dna_calc_model::geometry::sheet_metrics(
            geometry.as_ref().map(wire::geometry_of).as_ref(),
        );
        let corner = |c: &calc_host::ChartCorner| {
            let x = m.col_start(c.cell.column()) + c.dx;
            let y = m.row_start(c.cell.row()) + c.dy;
            (x, y)
        };
        host.charts(sheet.id())
            .0
            .into_iter()
            .map(|view| {
                let (place, width, height) = match view.place {
                    calc_host::ChartPlace::Cells { from, to } => {
                        let ((x0, y0), (x1, y1)) = (corner(&from), corner(&to));
                        (
                            ChartPlace::Cells {
                                from: (Key::of(from.cell), from.dx, from.dy),
                                to: (Key::of(to.cell), to.dx, to.dy),
                            },
                            x1 - x0,
                            y1 - y0,
                        )
                    }
                    calc_host::ChartPlace::Sized {
                        from,
                        width,
                        height,
                    } => (
                        ChartPlace::Sized {
                            from: (Key::of(from.cell), from.dx, from.dy),
                            width,
                            height,
                        },
                        width,
                        height,
                    ),
                    calc_host::ChartPlace::Absolute {
                        x,
                        y,
                        width,
                        height,
                    } => (
                        ChartPlace::Absolute {
                            x,
                            y,
                            width,
                            height,
                        },
                        width,
                        height,
                    ),
                };
                let kind = view
                    .plots
                    .first()
                    .map(|plot| format!("{:?}", plot.kind).to_lowercase())
                    .unwrap_or_default();
                let source = host
                    .model()
                    .definitions()
                    .charts(sheet.id())
                    .iter()
                    .find(|chart| chart.part == view.part);
                let plots = source
                    .map(|chart| {
                        chart
                            .plots
                            .iter()
                            .enumerate()
                            .map(|(p, plot)| wire::ChartPlotSource {
                                kind: plot.kind.clone(),
                                bar_direction: plot.bar_direction.clone(),
                                grouping: plot.grouping.clone(),
                                series: plot
                                    .series
                                    .iter()
                                    .enumerate()
                                    .map(|(i, s)| wire::ChartSeriesSource {
                                        index: s.index,
                                        fill: s.fill.clone(),
                                        label: view
                                            .plots
                                            .get(p)
                                            .and_then(|p| p.series.get(i))
                                            .map(|s| s.name.clone())
                                            .unwrap_or_default(),
                                        name: s
                                            .name
                                            .as_ref()
                                            .map(|d| {
                                                d.formula
                                                    .as_ref()
                                                    .map(|f| format!("={f}"))
                                                    .unwrap_or_else(|| {
                                                        d.cache
                                                            .first()
                                                            .cloned()
                                                            .flatten()
                                                            .unwrap_or_default()
                                                    })
                                            })
                                            .unwrap_or_default(),
                                        categories: s
                                            .categories
                                            .as_ref()
                                            .and_then(|d| d.formula.clone()),
                                        values: s.values.as_ref().and_then(|d| d.formula.clone()),
                                    })
                                    .collect(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Chart {
                    axes: source
                        .map(|c| {
                            c.axes
                                .iter()
                                .filter(|a| a.kind == "valAx")
                                .map(|a| wire::ChartAxisSource {
                                    id: a.id,
                                    position: a.position.clone().unwrap_or_default(),
                                    minimum: a.minimum.clone(),
                                    maximum: a.maximum.clone(),
                                    major_unit: a.major_unit.clone(),
                                    reversed: a.reversed,
                                    logarithmic: a.log_base.is_some(),
                                    log_base: a.log_base.clone(),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    plots,
                    text_elements: source
                        .map(|c| {
                            let mut targets = Vec::new();
                            if c.title.is_some() || c.title_formula.is_some() {
                                targets.push(("title".into(), "Title".into()));
                            }
                            if c.legend.is_some() {
                                targets.push(("legend".into(), "Legend".into()));
                            }
                            for a in &c.axes {
                                targets.push((
                                    format!("axis:{}", a.id),
                                    format!(
                                        "{} axis labels",
                                        match a.position.as_deref() {
                                            Some("l") => "Left",
                                            Some("r") => "Right",
                                            Some("t") => "Top",
                                            Some("b") => "Bottom",
                                            _ => "Other",
                                        }
                                    ),
                                ));
                            }
                            targets
                                .into_iter()
                                .map(|(target, label)| {
                                    let f =
                                        c.element_text.get(&target).cloned().unwrap_or_default();
                                    wire::ChartTextElement {
                                        target,
                                        label,
                                        style: wire::ChartTextStyle {
                                            family: f.family,
                                            size: f.size,
                                            bold: f.bold,
                                            italic: f.italic,
                                            color: f.color,
                                        },
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    text_style: source
                        .map(|c| wire::ChartTextStyle {
                            family: c.text_style.family.clone(),
                            size: c.text_style.size,
                            bold: c.text_style.bold,
                            italic: c.text_style.italic,
                            color: c.text_style.color.clone(),
                        })
                        .unwrap_or_default(),
                    part: view.part.clone(),
                    title: view.title.clone(),
                    legend: view.legend.clone(),
                    title_linked: host
                        .model()
                        .definitions()
                        .charts(sheet.id())
                        .iter()
                        .any(|chart| chart.part == view.part && chart.title_formula.is_some()),
                    name: view.name.clone(),
                    kind,
                    place,
                    svg: if width > 0.0 && height > 0.0 {
                        crate::chart::chart_svg(&view, width, height)
                    } else {
                        String::new()
                    },
                }
            })
            .collect()
    }
}


/// A colour the page sends: `#RRGGBB` (`Some(Some)`), `none` (`Some(None)`), or empty for the style's (`None`).
fn shape_colour(text: &str) -> Result<Option<Option<calc_host::ShapeColour>>, String> {
    match text.trim() {
        "" => Ok(None),
        "none" => Ok(Some(None)),
        hex => {
            let digits = hex.strip_prefix('#').unwrap_or(hex);
            match u32::from_str_radix(digits, 16) {
                Ok(v) if digits.len() == 6 => Ok(Some(Some(calc_host::ShapeColour::Rgb(v)))),
                _ => Err("Use a colour as #RRGGBB".into()),
            }
        }
    }
}

fn cell(at: Key) -> Result<CellKey, String> {
    at.cell().ok_or_else(|| "not a cell".to_owned())
}

fn area(area: Area) -> Result<GridArea, String> {
    area.area().ok_or_else(|| "not an area".to_owned())
}

/// How far from a subscribed area an authored cell is looked for as an overflow anchor, and the
/// largest subscription that is searched. Larger ones, such as a whole sheet, hold every anchor.
/// What a worker without a native host answers to an add-in load or unload (dc-0f1k.23).
pub const ADDINS_DESKTOP_ONLY: &str =
    "Add-ins (.xll, .xlam) load in the DNA Calc desktop app; a browser page cannot load them";

const OVERHANG_COLUMNS: u32 = 2048;
const OVERHANG_CELLS: u64 = 32_768;

/// `interest` with the overflow anchors `extras` added; the page's requested interest and the
/// coverage it reports stay as asked.
fn widened(interest: &calc_host::Interest, extras: &[GridArea]) -> calc_host::Interest {
    let mut areas = match interest {
        _ if extras.is_empty() => return interest.clone(),
        calc_host::Interest::Area(a) => vec![*a],
        calc_host::Interest::Areas(areas) => areas.clone(),
        other => return other.clone(),
    };
    areas.extend_from_slice(extras);
    calc_host::Interest::Areas(areas)
}
fn host_interest(interest: Interest) -> Result<calc_host::Interest, String> {
    Ok(match interest {
        Interest::Area(a) => calc_host::Interest::Area(area(a)?),
        Interest::Areas(areas) => {
            calc_host::Interest::Areas(areas.into_iter().map(area).collect::<Result<_, _>>()?)
        }
        Interest::Sheet(sheet) => calc_host::Interest::Sheet(sheet.id()),
        Interest::Selection => calc_host::Interest::Selection,
    })
}

/// Answers a message's text as the worker does: the reply, or the reason there is none.
pub fn answer_text(
    server: &mut Server,
    text: &str,
    bytes: Option<Vec<u8>>,
) -> (String, Option<Vec<u8>>) {
    crate::message::answer(server, crate::message::PageMessage::decode(text, bytes))
}

#[cfg(test)]
mod fit_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fitted_contents_on_an_added_sheet_survive_history_and_save() {
        let mut server = Server::new();
        let reply = server
            .execute_current(
                Request::Intent(Intent::EditSheet {
                    sheet: None,
                    change: wire::SheetChange::Add {
                        name: "Fit me".into(),
                    },
                }),
                None,
            )
            .reply
            .unwrap();
        assert!(matches!(
            reply,
            Reply::Done {
                outcome: Outcome { accepted: true, .. },
                ..
            }
        ));
        let sheet = server.book.host().sheets().last().unwrap().id();
        let at = CellKey::new(sheet, 1, 1).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(at),
                    text: "fit me".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        let Reply::FitContents { revision, cells } = server
            .execute_current(
                Request::FitContents {
                    area: Area::of(GridArea::cell(at)),
                },
                None,
            )
            .reply
            .unwrap()
        else {
            panic!("the content reply")
        };
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].text, "fit me");
        let reply = server
            .execute_current(
                Request::Intent(Intent::FitContents {
                    sheet: Sheet::of(sheet),
                    rows: false,
                    revision,
                    sizes: vec![(1, 20f64.to_bits())],
                }),
                None,
            )
            .reply
            .unwrap();
        assert!(matches!(
            reply,
            Reply::Done {
                outcome: Outcome { accepted: true, .. },
                ..
            }
        ));
        let width = |server: &Server| server.geometry(Sheet::of(sheet)).columns.first().copied();
        let fitted = width(&server).expect("a fitted column");
        assert_eq!(fitted, (1, 1, Some(5302. / 256.), false));
        server
            .execute_current(Request::Intent(Intent::Undo), None)
            .reply
            .unwrap();
        assert_eq!(width(&server), None);
        server
            .execute_current(Request::Intent(Intent::Redo), None)
            .reply
            .unwrap();
        assert_eq!(width(&server), Some(fitted));
        let bytes = server.execute_current(Request::Save, None).bytes.unwrap();
        server
            .execute_current(
                Request::Open {
                    name: "fit.xlsx".into(),
                },
                Some(bytes),
            )
            .reply
            .unwrap();
        let reopened = server.book.host().sheets().last().unwrap().id();
        assert_eq!(
            server.geometry(Sheet::of(reopened)).columns.first(),
            Some(&fitted)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fitted_contents_come_from_projection_and_a_stale_measurement_cannot_edit() {
        let mut server = Server::new();
        let sheet = server.book.host().sheets()[0].id();
        let at = CellKey::new(sheet, 1, 1).unwrap();
        let enter = |at, text: &str| {
            Request::Intent(Intent::EnterCell {
                at: Key::of(at),
                text: text.to_owned(),
            })
        };
        assert!(
            server
                .execute_current(enter(at, "original"), None)
                .reply
                .is_ok()
        );
        let Reply::FitContents { revision, cells } = server
            .execute_current(
                Request::FitContents {
                    area: Area::of(GridArea::cell(at)),
                },
                None,
            )
            .reply
            .unwrap()
        else {
            panic!("the content reply")
        };
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].text, "original");
        assert!(
            server
                .execute_current(enter(CellKey::new(sheet, 2, 1).unwrap(), "later"), None)
                .reply
                .is_ok()
        );
        let answer = server.execute_current(
            Request::Intent(Intent::FitContents {
                sheet: Sheet::of(sheet),
                rows: false,
                revision,
                sizes: vec![(1, 20f64.to_bits())],
            }),
            None,
        );
        assert!(answer.reply.unwrap_err().contains("changed while fitting"));
        assert_eq!(
            server
                .book
                .host()
                .model()
                .definitions()
                .axis_layouts()
                .get(sheet, Axis::Columns, 1)
                .size(),
            None
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fitting_refuses_a_large_spill_before_expanding_or_projecting_its_cells() {
        let mut server = Server::new();
        let sheet = server.book.host().sheets()[0].id();
        let at = CellKey::new(sheet, 1, 1).unwrap();
        let reply = server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(at),
                    text: "=SEQUENCE(10001)".to_owned(),
                }),
                None,
            )
            .reply
            .unwrap();
        assert!(matches!(
            reply,
            Reply::Done {
                outcome: Outcome { accepted: true, .. },
                ..
            }
        ));
        let target = GridArea::new(at, CellKey::new(sheet, 10001, 1).unwrap()).unwrap();
        let answer = server.execute_current(
            Request::FitContents {
                area: Area::of(target),
            },
            None,
        );
        assert!(answer.reply.unwrap_err().contains("10000 occupied"));
        assert_eq!(
            server
                .book
                .host()
                .model()
                .definitions()
                .axis_layouts()
                .get(sheet, Axis::Columns, 1)
                .size(),
            None
        );
    }
}

#[cfg(test)]
mod outline_tests {
    use super::*;
    /// The outline reply carries what the registered functions' providers documented, so the formula
    /// bar completes them and shows their signature and help (dc-0f1k milestone 4).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_outline_carries_the_registered_functions_descriptors() {
        #[derive(Debug)]
        struct Add;
        impl calc_fml::UserFunction for Add {
            fn call(&self, _: &[calc_value::CalcValue]) -> Result<calc_value::CalcValue, String> {
                Ok(calc_value::CalcValue::number(0.))
            }
        }
        let mut server = Server::new();
        assert!(server.outline(&[], &[], None, "").functions.is_empty());
        let mut table = calc_fml::CallableTable::new();
        table
            .register_described(
                None,
                "MyAdd",
                std::rc::Rc::new(Add),
                calc_fml::CallableDescriptor {
                    name: "MyAdd".into(),
                    origin: "xll My.xll".into(),
                    category: "Math".into(),
                    description: "Adds.".into(),
                    parameters: vec![calc_fml::CallableParameter {
                        name: "first".into(),
                        description: "One.".into(),
                    }],
                    thread_safe: true,
                    ..calc_fml::CallableDescriptor::default()
                },
            )
            .unwrap();
        server.book.set_user_functions(table);
        let functions = server.outline(&[], &[], None, "").functions;
        assert_eq!(functions.len(), 1);
        let function = &functions[0];
        assert_eq!(
            (function.name.as_str(), function.origin.as_str()),
            ("MyAdd", "xll My.xll")
        );
        assert_eq!(
            (function.category.as_str(), function.description.as_str()),
            ("Math", "Adds.")
        );
        assert_eq!(
            function.parameters,
            [("first".to_owned(), "One.".to_owned())]
        );
        assert!(function.thread_safe && !function.volatile && !function.asynchronous);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_1nbp_13_the_open_reply_carries_what_excel_would_repair() {
        let mut server = Server::new();
        for (name, data, repairs) in [
            (
                "dangling-rid.xlsx",
                include_bytes!("../../../crates/calc-doc/tests/fixtures/reader_tolerance/dangling-rid.xlsx").as_slice(),
                1,
            ),
            (
                "dangling-rid-control.xlsx",
                include_bytes!("../../../crates/calc-doc/tests/fixtures/reader_tolerance/dangling-rid-control.xlsx").as_slice(),
                0,
            ),
        ] {
            let reply = server.execute_current(Request::Open { name: name.into() }, Some(data.to_vec()));
            assert!(reply.reply.is_ok(), "{name}");
            assert_eq!(server.summary().open_repairs.len(), repairs, "{name}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_971p_6__format_cells_reads_and_previews_superscript_and_subscript() {
        let mut server = Server::new();
        assert!(server.execute_current(Request::Open { name: "blank.xlsx".into() }, Some(calc_host::blank_package())).reply.is_ok());
        let sheet = server.book.host().sheets()[0].id();
        let a1 = calc_host::value::CellKey::new(sheet, 1, 1).unwrap();
        let selection = crate::format::selection(&server.book, vec![calc_host::value::GridArea::cell(a1)]).unwrap();
        assert_eq!((selection.common.superscript, selection.common.subscript), (Some(false), Some(false)));
        // The preview of the dialog's choice draws the cell in the chosen script.
        let preview = |superscript, subscript| {
            let patch = dna_calc_model::format::FormatPatch { superscript, subscript, ..Default::default() };
            crate::format::preview(&server.book, a1, patch).map(|cell| cell.style.script.clone())
        };
        assert_eq!(preview(Some(true), None).unwrap().as_deref(), Some("superscript"));
        assert_eq!(preview(None, Some(true)).unwrap().as_deref(), Some("subscript"));
        assert_eq!(preview(None, None).unwrap(), None);
        // Both on is refused before anything is drawn.
        assert!(preview(Some(true), Some(true)).is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_2__the_server_answers_a_sheets_shapes_drawn() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/shapes-paint.xlsx");
        assert!(
            server
                .execute_current(Request::Open { name: "shapes-paint.xlsx".into() }, Some(data.to_vec()))
                .reply
                .is_ok()
        );
        let sheet = server.book.host().sheets()[0].id();
        let Reply::Shapes(shapes) = server
            .execute_current(Request::Shapes(Sheet::of(sheet)), None)
            .reply
            .unwrap()
        else {
            panic!("the shapes reply")
        };
        let names: Vec<_> = shapes.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["Rectangle 1", "Rounded 2", "Oval 3", "Line 4", "Straight Connector 5", "TextBox 6", "TextBox 7", "Rectangle 8", "Oval 9", "Rectangle 10"]
        );
        let by = |name: &str| shapes.iter().find(|s| s.name == name).unwrap();
        let rect = by("Rectangle 1");
        assert_eq!(rect.fill.as_deref(), Some("#FFC000"));
        let line = rect.line.as_ref().unwrap();
        assert_eq!((line.colour.as_str(), line.width), ("#1F4E79", 25_400.0 / 9_525.0));
        let rounded = by("Rounded 2");
        assert_eq!((rounded.fill.clone(), rounded.line.as_ref().unwrap().dash.as_str()), (None, "dash"));
        assert!(by("Oval 3").line.is_none());
        assert!(by("Line 4").flip_vertical && by("Line 4").fill.is_none());
        let text = by("TextBox 6").text.as_ref().unwrap();
        assert_eq!(text.inset, [19.2; 4]);
        assert_eq!((text.anchor.as_str(), text.wrap, text.bold), ("ctr", true, true));
        assert_eq!(text.paragraphs.iter().map(|p| p.align.as_str()).collect::<Vec<_>>(), ["l", "ctr", "r"]);
        assert_eq!(by("TextBox 7").text.as_ref().unwrap().wrap, false);
        // The cell anchors cross as the grid places them, from the sheet's cells.
        assert!(matches!(by("Rectangle 1").place, ChartPlace::Cells { .. }));
        assert!(matches!(by("Oval 9").place, ChartPlace::Sized { .. }));
        assert!(matches!(by("Rectangle 10").place, ChartPlace::Absolute { .. }));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_2__the_server_answers_rotation_arrowheads_gradients_groups_and_presets() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/shapes-paint2.xlsx");
        assert!(
            server
                .execute_current(Request::Open { name: "shapes-paint2.xlsx".into() }, Some(data.to_vec()))
                .reply
                .is_ok()
        );
        let sheet = server.book.host().sheets()[0].id();
        let Reply::Shapes(shapes) = server
            .execute_current(Request::Shapes(Sheet::of(sheet)), None)
            .reply
            .unwrap()
        else {
            panic!("the shapes reply")
        };
        assert_eq!(shapes.len(), 4 + 4 + 12 + 8 + 3 + 28);
        let by = |name: &str| shapes.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"));
        // Rotation, in degrees: a quarter turn's anchor is the shape's box turned (the page swaps it back).
        let turned = by("Rot 90 roundRect");
        assert_eq!(turned.rotation, 90.0);
        assert!(dna_calc_model::drawn_shapes::quarter_turned(turned.rotation));
        assert!(!dna_calc_model::drawn_shapes::quarter_turned(by("Rot 180 rightArrow").rotation));
        let flip = by("FlipH rightArrow");
        assert!(flip.flip_horizontal && !flip.flip_vertical && flip.rotation == 0.0);
        assert_eq!(by("Rot 30 rect").rotation, 30.0);
        // Arrowheads: kind and sizes, on the end the file names.
        let tail = by("Arrow triangle sm 1pt").line.as_ref().unwrap();
        assert!(tail.head.is_none());
        let end = tail.tail.as_ref().unwrap();
        assert_eq!((end.kind.as_str(), end.width.as_str(), end.length.as_str()), ("triangle", "sm", "sm"));
        let both = by("Arrow both triangle lg 1pt").line.as_ref().unwrap();
        assert!(both.head.is_some() && both.tail.is_some());
        assert_eq!(by("Arrow oval wide short 3pt").line.as_ref().unwrap().head.as_ref().unwrap().length, "sm");
        // Gradients: stops as fractions, the angle in degrees, the scaled flag and a path's focus.
        let g = by("Grad linear 45 scaled").gradient.as_ref().unwrap();
        assert_eq!((g.angle, g.scaled, g.path.clone()), (45.0, true, None));
        assert_eq!(g.stops, [(0.0, "#FF0000".to_owned()), (0.5, "#FFFF00".to_owned()), (1.0, "#0070C0".to_owned())]);
        assert!(!by("Grad linear 45 unscaled").gradient.as_ref().unwrap().scaled);
        assert_eq!(by("Grad linear 45 unscaled").fill, None);
        let corner = by("Grad path rect top-left").gradient.as_ref().unwrap();
        assert_eq!((corner.path.as_deref(), corner.focus), (Some("rect"), [0.0, 0.0]));
        assert_eq!(by("Grad path circle centre").gradient.as_ref().unwrap().focus, [0.5, 0.5]);
        // Presets keep their adjust values.
        assert_eq!(by("rightArrow adj1=25000,adj2=80000").adjust, [("adj1".to_owned(), 25_000), ("adj2".to_owned(), 80_000)]);
        assert!(by("triangle").adjust.is_empty());
        // A group's children are boxes inside it through its child space (100000, 100000, 2000000 by 1000000).
        let group = by("Group plain");
        assert_eq!(group.kind, "group");
        let rels: Vec<[f64; 4]> = group.children.iter().map(|c| c.rel).collect();
        assert_eq!(rels, [[0.0, 0.0, 0.5, 1.0], [0.5, 0.2, 0.45, 0.6], [0.0, 0.9, 0.4, 0.1]]);
        assert_eq!(group.children[2].shape.rotation, 30.0);
        // A flipped group mirrors its children's boxes and flips them.
        let mirrored = by("Group flipH");
        let first = &mirrored.children[0];
        assert_eq!(first.rel, [0.5, 0.0, 0.5, 1.0]);
        assert!(first.shape.flip_horizontal);
        assert_eq!(mirrored.children[2].shape.rotation, -30.0);
        assert_eq!(by("Group rot 20").rotation, 20.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_4__drawings_follow_inserted_and_deleted_rows_on_screen() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx");
        assert!(server.execute_current(Request::Open { name: "shapes-paint4.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = Sheet::of(server.book.host().sheets()[0].id());
        let rows_of = |server: &mut Server, name: &str| {
            let Reply::Shapes(drawn) = server.execute_current(Request::Shapes(sheet), None).reply.unwrap() else { panic!("shapes") };
            match drawn.iter().find(|d| d.name == name).unwrap().place {
                ChartPlace::Cells { from, to } => (from.0.2, to.0.2),
                ref other => panic!("{other:?}"),
            }
        };
        // "Freeform triangle" is anchored from row 4 to row 9 (1-based), editAs oneCell.
        assert_eq!(rows_of(&mut server, "Freeform triangle"), (4, 9));
        // Two rows inserted at row 2: it moves down two rows, its span kept.
        assert!(server.execute_current(Request::Intent(Intent::EditAxis { sheet, rows: true, insert: true, start: 2, count: 2 }), None).reply.is_ok());
        assert_eq!(rows_of(&mut server, "Freeform triangle"), (6, 11));
        // Rows 1 to 3 deleted: it moves up three.
        assert!(server.execute_current(Request::Intent(Intent::EditAxis { sheet, rows: true, insert: false, start: 1, count: 3 }), None).reply.is_ok());
        assert_eq!(rows_of(&mut server, "Freeform triangle"), (3, 8));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_4__the_page_moves_resizes_and_deletes_a_stored_shape() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx");
        assert!(server.execute_current(Request::Open { name: "shapes-paint4.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = Sheet::of(server.book.host().sheets()[0].id());
        let id = |server: &Server, name: &str| server.book.host().shapes(sheet.id()).iter().find(|s| s.name == name).unwrap().id;
        let (triangle, curves) = (id(&server, "Freeform triangle"), id(&server, "Freeform curves"));
        let revision = server.book.host().revision().get();
        // 96 pt by 30 pt, 60 pt by 45 pt, in hundredths of a point.
        let moved = server.execute_current(
            Request::Intent(Intent::ShapePlace { sheet, revision, id: triangle, left: 9_600, top: 3_000, width: 6_000, height: 4_500 }),
            None,
        );
        assert!(moved.reply.is_ok());
        let shape = server.book.host().shapes(sheet.id()).iter().find(|s| s.id == triangle).unwrap().clone();
        assert_eq!(server.book.host().anchor_rectangle(sheet.id(), &shape.anchor), (96.0, 30.0, 60.0, 45.0));
        let revision = server.book.host().revision().get();
        assert!(server.execute_current(Request::Intent(Intent::ShapeDelete { sheet, revision, id: curves }), None).reply.is_ok());
        let Reply::Shapes(drawn) = server.execute_current(Request::Shapes(sheet), None).reply.unwrap() else { panic!("shapes") };
        assert_eq!(drawn.len(), 19);
        // Undo brings the deleted shape back.
        assert!(server.execute_current(Request::Intent(Intent::Undo), None).reply.is_ok());
        assert_eq!(server.book.host().shapes(sheet.id()).len(), 20);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_4__the_page_inserts_edits_turns_orders_and_groups_shapes() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx");
        assert!(server.execute_current(Request::Open { name: "shapes-paint4.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = Sheet::of(server.book.host().sheets()[0].id());
        let revision = |server: &Server| server.book.host().revision().get();
        let run = |server: &mut Server, make: &dyn Fn(u64) -> Intent| {
            let revision = server.book.host().revision().get();
            let reply = server.execute_current(Request::Intent(make(revision)), None).reply;
            assert!(matches!(&reply, Ok(Reply::Done { outcome, .. }) if outcome.accepted), "{reply:?}");
        };
        // A rectangle at C3: one inch square at the cell's top left (column C at 48 pt a column, row 3 at 15 pt).
        let at = Key::of(calc_value::CellKey::new(sheet.id(), 3, 3).unwrap());
        run(&mut server, &|revision| Intent::ShapeAdd { sheet, revision, at, kind: "shape".into(), geometry: "ellipse".into(), width: 7_200, height: 7_200 });
        let added = server.book.host().shapes(sheet.id()).last().unwrap().clone();
        assert_eq!((added.name.as_str(), added.offset, added.extent), ("Oval 21", (96 * 12_700, 30 * 12_700), (72 * 12_700, 72 * 12_700)));
        let id = added.id;
        run(&mut server, &|revision| Intent::ShapeText { sheet, revision, id, text: "One\nTwo".into() });
        run(&mut server, &|revision| Intent::ShapeFill { sheet, revision, id, fill: "#FF8000".into() });
        run(&mut server, &|revision| Intent::ShapeLine { sheet, revision, id, colour: "#000080".into(), weight: Some(150) });
        run(&mut server, &|revision| Intent::ShapeRotate { sheet, revision, id, degrees: 45 });
        run(&mut server, &|revision| Intent::ShapeOrder { sheet, revision, id, order: "back".into() });
        let shape = server.book.host().shapes(sheet.id())[0].clone();
        assert_eq!(shape.id, id);
        assert_eq!(shape.text, Some(vec!["One".to_owned(), "Two".to_owned()]));
        assert_eq!(shape.fill, calc_host::ShapeFill::Solid(calc_host::ShapeColour::Rgb(0xFF8000)));
        assert_eq!((shape.line.colour.clone(), shape.line.weight), (Some(calc_host::ShapeColour::Rgb(0x80)), Some(19_050)));
        assert_eq!(shape.rotation, 45 * 60_000);
        let stored = server.book.host().shapes(sheet.id())[1].id;
        run(&mut server, &|revision| Intent::ShapeGroup { sheet, revision, ids: vec![id, stored] });
        let group = server.book.host().shapes(sheet.id())[0].clone();
        assert_eq!((group.kind, group.children.len()), (calc_host::ShapeKind::Group, 2));
        let reply = server.execute_current(Request::Intent(Intent::ShapeUngroup { sheet, revision: revision(&server), id: group.id }), None).reply;
        assert!(matches!(&reply, Ok(Reply::Done { outcome, .. }) if outcome.accepted), "{reply:?}");
        assert_eq!(server.book.host().shapes(sheet.id())[0].id, id);
        // A refused step names why and changes nothing.
        let refused = server.execute_current(Request::Intent(Intent::ShapeOrder { sheet, revision: revision(&server), id, order: "back".into() }), None).reply;
        assert!(matches!(&refused, Ok(Reply::Done { outcome, .. }) if !outcome.accepted && outcome.error.as_deref().is_some_and(|e| e.contains("already at the back"))), "{refused:?}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_4__the_page_inserts_a_picture_at_its_own_size() {
        let mut server = Server::new();
        let sheet = Sheet::of(server.book.host().sheets()[0].id());
        let at = Key::of(calc_value::CellKey::new(sheet.id(), 2, 2).unwrap());
        let pictures: [(&str, &[u8], &str); 4] = [
            ("card.png", include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/pictures/card.png"), "image/png"),
            ("card.gif", include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/pictures/card.gif"), "image/gif"),
            ("card.bmp", include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/pictures/card.bmp"), "image/bmp"),
            ("card.jpg", include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/pictures/card.jpg"), "image/jpeg"),
        ];
        for (name, bytes, content_type) in pictures {
            let revision = server.book.host().revision().get();
            let reply = server.execute_current(Request::Intent(Intent::ShapePicture { sheet, revision, at, name: name.into() }), Some(bytes.to_vec())).reply;
            assert!(matches!(&reply, Ok(Reply::Done { outcome, .. }) if outcome.accepted), "{name}: {reply:?}");
            let shape = server.book.host().shapes(sheet.id()).last().unwrap().clone();
            // 160 by 100 pixels: 120 by 75 points at B2's top left (column A 48 pt, row 1 15 pt).
            assert_eq!((shape.kind, shape.offset, shape.extent), (calc_host::ShapeKind::Picture, (48 * 12_700, 15 * 12_700), (120 * 12_700, 75 * 12_700)), "{name}");
            let media = shape.media.unwrap().image.unwrap();
            assert_eq!((media.content_type.as_str(), &*media.bytes), (content_type, bytes), "{name}");
            let Answer { reply, bytes: shown } = server.handle(Request::ShapePicture { sheet, id: shape.id }, None);
            assert!(matches!(reply, Ok(Reply::ShapePicture(Some(_)))) && shown.is_some(), "{name}");
        }
        assert_eq!(server.book.host().shapes(sheet.id())[0].name, "Picture 1");
        // Without its image, or with bytes that are no picture, nothing is inserted.
        let revision = server.book.host().revision().get();
        let reply = server.execute_current(Request::Intent(Intent::ShapePicture { sheet, revision, at, name: "x.png".into() }), Some(b"not a picture".to_vec())).reply;
        assert!(reply.is_err() || matches!(&reply, Ok(Reply::Done { outcome, .. }) if !outcome.accepted), "{reply:?}");
        assert_eq!(server.book.host().shapes(sheet.id()).len(), 4);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_8__form_controls_show_their_linked_cells_and_clicks_write_them() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/form-controls.xlsx");
        assert!(server.execute_current(Request::Open { name: "form-controls.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = Sheet::of(server.book.host().sheets()[0].id());
        let controls = |server: &mut Server| {
            let Reply::Controls(c) = server.execute_current(Request::Controls(sheet), None).reply.unwrap() else { panic!("controls") };
            c
        };
        let drawn = controls(&mut server);
        assert_eq!(drawn.len(), 12);
        let by = |c: &[dna_calc_model::controls::DrawnControl], name: &str| c.iter().find(|x| x.name == name).unwrap().clone();
        // B2 TRUE: checked; B3 2: the second option button; B4 3: the third item; B5 30; B6 5; B7 2: the second item.
        assert_eq!(by(&drawn, "Check Box 2").checked, "Checked");
        assert_eq!((by(&drawn, "Option Button 3").checked.as_str(), by(&drawn, "Option Button 4").checked.as_str()), ("Unchecked", "Checked"));
        assert_eq!(by(&drawn, "Option Button 4").link, by(&drawn, "Option Button 3").link);
        // The two option buttons inside the group box are a group of their own: linked to B8 (2), numbered 1 and 2.
        let (weekly, daily) = (by(&drawn, "Option Button 11"), by(&drawn, "Option Button 12"));
        assert_eq!((weekly.position, daily.position, weekly.checked.as_str(), daily.checked.as_str()), (1, 2, "Unchecked", "Checked"));
        assert_eq!((weekly.link, daily.link), (Some(Key(sheet.0, sheet.1, 8, 2)), Some(Key(sheet.0, sheet.1, 8, 2))));
        let list = by(&drawn, "List Box 5");
        assert_eq!((list.items.clone(), list.selected), (vec!["North".to_owned(), "South".into(), "East".into(), "West".into()], Some(2)));
        assert_eq!(by(&drawn, "Drop Down 8").selected, Some(1));
        assert_eq!((by(&drawn, "Scroll Bar 6").value, by(&drawn, "Spinner 7").value), (30, 5));
        // A click writes the linked cell through the ordinary edit; the next answer shows it.
        use dna_calc_model::controls::{ControlAction, control_action};
        let click = |server: &mut Server, control: &dna_calc_model::controls::DrawnControl, act: &str, index: Option<usize>| {
            let Some(ControlAction::Write(at, text)) = control_action(control, act, index) else { panic!("a write") };
            assert!(server.execute_current(Request::Intent(Intent::EnterCell { at, text }), None).reply.is_ok());
        };
        click(&mut server, &by(&drawn, "Check Box 2"), "toggle", None);
        click(&mut server, &by(&drawn, "Option Button 3"), "toggle", None);
        click(&mut server, &by(&drawn, "List Box 5"), "item", Some(0));
        click(&mut server, &by(&drawn, "Spinner 7"), "inc", None);
        let after = controls(&mut server);
        assert_eq!(by(&after, "Check Box 2").checked, "Unchecked");
        assert_eq!(by(&after, "Option Button 3").checked, "Checked");
        // Clicking an option button of the sheet's group leaves the group box's group as it was.
        assert_eq!(by(&after, "Option Button 12").checked, "Checked");
        click(&mut server, &by(&after, "Option Button 11"), "toggle", None);
        let again = controls(&mut server);
        assert_eq!((by(&again, "Option Button 11").checked.as_str(), by(&again, "Option Button 3").checked.as_str()), ("Checked", "Checked"));
        assert_eq!(by(&after, "List Box 5").selected, Some(0));
        assert_eq!(by(&after, "Spinner 7").value, 6);
        assert!(matches!(control_action(&by(&after, "Button 1"), "run", None), Some(ControlAction::RunMacro(_))));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_6__a_chart_sheet_shows_its_chart() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/preserved_sheets/chart-and-dialog-sheets.xlsx");
        assert!(server.execute_current(Request::Open { name: "chart-and-dialog-sheets.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let chart_sheet = server.book.host().sheets()[1].id();
        let Reply::Charts(charts) = server.execute_current(Request::Charts(Sheet::of(chart_sheet)), None).reply.unwrap() else { panic!("charts") };
        assert_eq!(charts.len(), 1);
        assert!(charts[0].svg.contains("data-point=\"0\""), "{}", charts[0].svg);
        assert!(!charts[0].svg.contains("chart-notice"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_7__the_page_inserts_changes_and_deletes_sparklines() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/sparklines.xlsx");
        assert!(server.execute_current(Request::Open { name: "sparklines.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = Sheet::of(server.book.host().sheets()[0].id());
        let at = |row, column| Key::of(calc_value::CellKey::new(sheet.id(), row, column).unwrap());
        let run = |server: &mut Server, make: &dyn Fn(u64) -> Intent| {
            let revision = server.book.host().revision().get();
            let reply = server.execute_current(Request::Intent(make(revision)), None).reply;
            assert!(matches!(&reply, Ok(Reply::Done { outcome, .. }) if outcome.accepted), "{reply:?}");
        };
        run(&mut server, &|revision| Intent::SparklineAdd { sheet, revision, at: at(3, 7), data: "A3:E3".into(), kind: "stacked".into() });
        run(&mut server, &|revision| Intent::SparklineShow { sheet, revision, at: at(3, 7), point: "high".into(), on: true });
        run(&mut server, &|revision| Intent::SparklineKind { sheet, revision, at: at(3, 7), kind: "column".into() });
        let Reply::Sparklines(lines) = server.execute_current(Request::Sparklines(sheet), None).reply.unwrap() else { panic!() };
        let g3 = lines.iter().find(|l| l.cell == at(3, 7)).unwrap();
        assert_eq!((g3.kind.as_str(), g3.values.len(), g3.high.is_some()), ("column", 5, true));
        run(&mut server, &|revision| Intent::SparklineDelete { sheet, revision, at: at(3, 7) });
        assert_eq!(server.book.host().model().definitions().sparklines(sheet.id()).len(), 5);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_7__the_server_answers_sparklines_with_values_axes_and_colours() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/sparklines.xlsx");
        assert!(server.execute_current(Request::Open { name: "sparklines.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = server.book.host().sheets()[0].id();
        let Reply::Sparklines(lines) = server.execute_current(Request::Sparklines(Sheet::of(sheet)), None).reply.unwrap() else {
            panic!("the sparklines reply")
        };
        assert_eq!(lines.len(), 7);
        let at = |row: u32| lines.iter().find(|l| l.cell.2 == row).unwrap_or_else(|| panic!("row {row}"));
        // F8: on a date axis, each point at its date's share of 45000..45008 (dc-zjsg.7).
        assert_eq!(at(8).positions, vec![Some(0.0), Some(0.125), Some(0.5), Some(0.625), Some(1.0)]);
        assert!(at(2).positions.is_empty());
        // F2: a line over A2:E2, its own axis, accent 1 tint -0.5 as a cell colour takes it (203764), markers, high and low.
        let f2 = at(2);
        assert_eq!((f2.kind.as_str(), f2.values.clone()), ("line", vec![Some(3.0), Some(5.0), Some(2.0), Some(8.0), Some(6.0)]));
        assert_eq!((f2.min, f2.max), (2.0, 8.0));
        assert_eq!(f2.series, "#203764");
        assert_eq!((f2.high.as_deref(), f2.low.as_deref()), (Some("#00B050"), Some("#FF0000")));
        assert!(f2.markers.is_some() && f2.negative.is_none());
        // F3: an empty cell is an empty point, shown as a gap.
        assert_eq!((at(3).values[1], at(3).empty.as_str()), (None, "gap"));
        // F4 and F5 share the group's axis, -2 to 15.
        assert_eq!((at(4).min, at(4).max, at(5).min, at(5).max), (-2.0, 15.0, -2.0, 15.0));
        assert_eq!((at(4).kind.as_str(), at(4).negative.as_deref(), at(4).axis.as_deref()), ("column", Some("#ED7D31"), Some("#000000")));
        assert_eq!(at(6).kind, "stacked");
        // F7: the custom axis and the weight in pixels; empty cells spanned.
        let f7 = at(7);
        assert_eq!((f7.min, f7.max, f7.empty.as_str()), (0.0, 10.0, "span"));
        assert!((f7.weight - 3.0).abs() < 1e-9);
        // An edit to the data is in the next answer.
        server.execute_current(Request::Intent(Intent::EnterCell { at: Key(Sheet::of(sheet).0, Sheet::of(sheet).1, 2, 1), text: "20".into() }), None);
        let Reply::Sparklines(lines) = server.execute_current(Request::Sparklines(Sheet::of(sheet)), None).reply.unwrap() else { panic!() };
        let f2 = lines.iter().find(|l| l.cell.2 == 2).unwrap();
        assert_eq!((f2.values[0], f2.max), (Some(20.0), 20.0));
        assert!(dna_calc_model::sparklines::sparkline_svg(f2, 64.0, 20.0, 1.0).contains("data-marker"));
        // Column B hidden: its value is left out of F2, as the group does not show hidden cells.
        assert!(server.execute_current(Request::Intent(Intent::AxisFormat { sheet: Sheet::of(sheet), rows: false, first: 2, count: 1, size_bits: None, hidden: Some(true) }), None).reply.is_ok());
        assert!(server.book.host().sparklines(sheet).iter().any(|(g, _)| !g.hidden));
        let Reply::Sparklines(lines) = server.execute_current(Request::Sparklines(Sheet::of(sheet)), None).reply.unwrap() else { panic!() };
        let f2 = lines.iter().find(|l| l.cell.2 == 2).unwrap();
        assert_eq!(f2.values, vec![Some(20.0), Some(2.0), Some(8.0), Some(6.0)]);
        let f8 = lines.iter().find(|l| l.cell.2 == 8).unwrap();
        assert_eq!((f8.values.len(), f8.positions.len()), (4, 4));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_9__the_server_draws_kept_objects_from_their_fallbacks() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/kept-objects.xlsx");
        assert!(server.execute_current(Request::Open { name: "kept-objects.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = server.book.host().sheets()[0].id();
        let Reply::Shapes(shapes) = server.execute_current(Request::Shapes(Sheet::of(sheet)), None).reply.unwrap() else {
            panic!("the shapes reply")
        };
        // The hidden shape Excel writes for the OLE object is not drawn; the four kept objects are.
        let kept: Vec<_> = shapes.iter().map(|s| (s.name.as_str(), s.kind.as_str(), s.kept.as_deref())).collect();
        assert_eq!(
            kept,
            [
                ("TextBox 1", "textbox", Some("equation")),
                ("Ink 2", "picture", Some("ink")),
                ("Diagram 3", "group", Some("SmartArt")),
                ("Object 1", "picture", Some("OLE object")),
            ]
        );
        // The equation is in the subset: drawn from its OMML (dc-zjsg.11), not its fallback picture, no text, its outline.
        let equation = &shapes[0];
        assert!(equation.picture.is_none() && equation.text.is_none() && equation.fill.is_none() && equation.line.is_some());
        let math = equation.math.as_ref().unwrap();
        assert_eq!((math.align.as_str(), math.colour.as_str()), ("center", "#000000"));
        for element in ["<mfrac>", "<msup>", "<msqrt>", "<munderover>", "<mtable>", "<mo fence=\"true\">(</mo>"] {
            assert!(math.markup.contains(element), "{element}: {}", math.markup);
        }
        let markup = dna_calc_model::drawn_shapes::shape_markup(equation, 256.0, 80.0, 1.0).unwrap();
        assert!(markup.contains("dc-shape-math") && !markup.contains("math-style"), "{markup}");
        assert!(markup.contains("<math>"), "{markup}");
        // The picture fill's bytes are still answered by the shape's identity like a picture's.
        let answer = server.handle(Request::ShapePicture { sheet: Sheet::of(sheet), id: equation.id }, None);
        assert!(answer.bytes.is_some());
        // SmartArt: five children, the boxes with their text in white.
        let diagram = &shapes[2];
        assert_eq!(diagram.children.len(), 5);
        let build = &diagram.children[2];
        assert_eq!(build.shape.text.as_ref().unwrap().paragraphs[0].text, "Build");
        assert!((build.rel[0] - 192.0 / 512.0).abs() < 1e-9 && (build.rel[2] - 0.25).abs() < 1e-9, "{:?}", build.rel);
        // The OLE object's picture, its bytes by its shape id.
        let answer = server.handle(Request::ShapePicture { sheet: Sheet::of(sheet), id: 1025 }, None);
        assert!(matches!(answer.reply, Ok(Reply::ShapePicture(Some(ref t))) if t == "image/png"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_2__the_server_answers_custom_geometry_patterns_and_style_references() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/shapes-paint4.xlsx");
        assert!(server.execute_current(Request::Open { name: "shapes-paint4.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = server.book.host().sheets()[0].id();
        let Reply::Shapes(shapes) = server.execute_current(Request::Shapes(Sheet::of(sheet)), None).reply.unwrap() else {
            panic!("the shapes reply")
        };
        // Every object is drawn, none left out.
        assert_eq!(shapes.len(), 20);
        let by = |name: &str| shapes.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"));
        use dna_calc_model::drawn_shapes::{Seg, shape_markup};
        // A custom geometry: its path in fractions of the box, its stated fill, the style's 1 pt outline.
        let triangle = by("Freeform triangle");
        assert_eq!(triangle.geometry, "custom");
        assert_eq!(triangle.custom[0].segments[..2], [Seg::Move(0.5, 0.0), Seg::Line(1.0, 1.0)]);
        assert_eq!(triangle.fill.as_deref(), Some("#C00000"));
        assert_eq!(triangle.line.as_ref().map(|l| l.width), Some(12_700.0 / 9_525.0));
        assert_eq!(triangle.custom_text, Some([0.0, 0.0, 1.0, 1.0]));
        assert!(shape_markup(triangle, 128.0, 100.0, 1.0).unwrap().contains("M64 0L128 100L0 100Z"));
        // An unfilled custom path is a line with its arrowhead.
        let open = by("Freeform open arrow");
        let markup = shape_markup(open, 128.0, 100.0, 1.0).unwrap();
        assert!(markup.contains("fill=\"none\"") && markup.contains("#7030A0"), "{markup}");
        // The arc from guides: a half disc of two quarter curves.
        assert_eq!(by("Freeform half disc from guides").custom[0].segments.len(), 4);
        // Two paths, one unfilled: the fill beneath, the strokes over it.
        let two = shape_markup(by("Freeform two paths, one unfilled"), 100.0, 100.0, 1.0).unwrap();
        assert!(two.contains("stroke=\"none\"/><path"), "{two}");
        // Patterns: preset and colours, drawn as an SVG pattern.
        let pattern = by("Pattern dkUpDiag").pattern.as_ref().unwrap();
        assert_eq!((pattern.preset.as_str(), pattern.foreground.as_str(), pattern.background.as_str()), ("dkUpDiag", "#C00000", "#FFFF00"));
        assert!(shape_markup(by("Pattern dkUpDiag"), 128.0, 80.0, 1.0).unwrap().contains("<pattern "));
        // Style references: accent 2 fill with the shaded outline, white text.
        let accent2 = by("Style accent2");
        assert_eq!(accent2.fill.as_deref(), Some("#ED7D31"));
        assert_eq!(accent2.text.as_ref().unwrap().colour, "#FFFFFF");
        // Line style 3 (the theme's 1.5 pt) in accent 6, no fill, dark text.
        let line_only = by("Style accent6 line only");
        assert_eq!(line_only.fill, None);
        let line = line_only.line.as_ref().unwrap();
        assert_eq!((line.colour.as_str(), line.width), ("#70AD47", 19_050.0 / 9_525.0));
        assert_eq!(line_only.text.as_ref().unwrap().colour, "#000000");
        // A fill reference's colour with its modifiers: accent 4 lighter 60%.
        assert_eq!(by("Style accent4 dark text").fill.as_deref(), Some("#FFE699"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_2__the_server_answers_colour_modifiers_shadows_connectors_and_group_pictures() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/shapes-paint3.xlsx");
        assert!(
            server
                .execute_current(Request::Open { name: "shapes-paint3.xlsx".into() }, Some(data.to_vec()))
                .reply
                .is_ok()
        );
        let sheet = server.book.host().sheets()[0].id();
        let Reply::Shapes(shapes) = server.execute_current(Request::Shapes(Sheet::of(sheet)), None).reply.unwrap() else {
            panic!("the shapes reply")
        };
        let by = |name: &str| shapes.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"));
        // Excel's "Accent 1, Lighter 40%" and "Darker 25%" and the shaded outline of accent 1, from the workbook's theme.
        assert_eq!(by("Mod accent1 lumMod60 lumOff40").fill.as_deref(), Some("#8FAADC"));
        assert_eq!(by("Mod accent1 lumMod75").fill.as_deref(), Some("#2F5597"));
        assert_eq!(by("Mod accent1 shade50").fill.as_deref(), Some("#2F528F"));
        assert_eq!(by("Mod srgb red alpha50").fill.as_deref(), Some("rgba(255,0,0,0.5)"));
        // The outline's accent 3 at 60% opacity is a translucent grey.
        assert!(by("Mod accent1 tint40").line.as_ref().unwrap().colour.starts_with("rgba(165,165,165,0.6"));
        // Shadows reach the page with their offset along the direction, in pixels.
        let shadow = by("Shadow offset BR blur 4pt").shadow.as_ref().unwrap();
        // 38100 EMU is 4 px, along 45 degrees.
        assert!((shadow.dx - 2.0 * 2f64.sqrt()).abs() < 1e-9 && (shadow.dy - 2.0 * 2f64.sqrt()).abs() < 1e-9, "{shadow:?}");
        assert_eq!((shadow.align.as_str(), shadow.rotate), ("tl", false));
        assert!(by("Shadow rot 30 with shape").shadow.as_ref().unwrap().rotate);
        // Connectors with adjusts are drawn (they were refused before) and keep their flips.
        assert_eq!(by("Conn bentConnector3 flipH").geometry, "bentConnector3");
        assert_eq!(by("Conn curvedConnector5 adj1=20000,adj2=40000,adj3=80000").adjust.len(), 3);
        // A flipped picture keeps its flips; a group's pictures are in its children with their images (no placeholder).
        let flipped = by("Pic flipH+V");
        assert!(flipped.flip_horizontal && flipped.flip_vertical);
        let group = by("Group flipH with picture");
        let pictures: Vec<_> = group.children.iter().filter(|c| c.shape.kind == "picture").collect();
        assert_eq!(pictures.len(), 2);
        assert!(pictures.iter().all(|c| c.shape.picture.as_ref().unwrap().placeholder.is_none()));
        // The picture's bytes are answered by its identity, inside a group too.
        let answer = server.handle(Request::ShapePicture { sheet: Sheet::of(sheet), id: pictures[0].shape.id }, None);
        assert!(matches!(answer.reply, Ok(Reply::ShapePicture(Some(ref t))) if t == "image/png"));
        assert!(answer.bytes.is_some());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_3__the_server_draws_pictures_and_answers_their_bytes() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/pictures-paint.xlsx");
        assert!(
            server
                .execute_current(Request::Open { name: "pictures-paint.xlsx".into() }, Some(data.to_vec()))
                .reply
                .is_ok()
        );
        let sheet = Sheet::of(server.book.host().sheets()[0].id());
        let Reply::Shapes(shapes) = server.execute_current(Request::Shapes(sheet), None).reply.unwrap() else {
            panic!("the shapes reply")
        };
        assert_eq!(shapes.len(), 11);
        let by = |name: &str| shapes.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("{name}"));
        assert!(shapes.iter().all(|s| s.kind == "picture" && s.picture.is_some()));
        // The crop as fractions, the description kept, an image the page decodes has no placeholder.
        let crop = by("Pic PNG crop").picture.as_ref().unwrap();
        assert_eq!(crop.crop, [0.25, 0.1, 0.25, 0.4]);
        assert_eq!(crop.description.as_deref(), Some("Pic PNG crop (test card)"));
        assert_eq!(crop.placeholder, None);
        assert_eq!(by("Pic PNG rot 30 outlined").rotation, 30.0);
        assert_eq!(by("Pic PNG rot 30 outlined").line.as_ref().unwrap().dash, "dash");
        // The metafiles: a placeholder with the size the header gives, unless a PNG is inside.
        assert_eq!(by("Pic EMF bare").picture.as_ref().unwrap().placeholder.as_deref(), Some("EMF picture 160 × 100"));
        assert_eq!(by("Pic WMF bare").picture.as_ref().unwrap().placeholder.as_deref(), Some("WMF picture 96 × 48"));
        assert_eq!(by("Pic EMF with PNG").picture.as_ref().unwrap().placeholder, None);
        // The bytes travel beside the reply by the picture's identity: each format as itself.
        let mut seen = Vec::new();
        for shape in &shapes {
            let answer = server.handle(Request::ShapePicture { sheet, id: shape.id }, None);
            match answer.reply.unwrap() {
                Reply::ShapePicture(Some(content_type)) => {
                    let bytes = answer.bytes.unwrap();
                    seen.push((shape.name.clone(), content_type, bytes.len()));
                }
                Reply::ShapePicture(None) => assert!(shape.picture.as_ref().unwrap().placeholder.is_some(), "{}", shape.name),
                other => panic!("{other:?}"),
            }
        }
        let kind = |name: &str| seen.iter().find(|(n, ..)| n == name).map(|(_, c, _)| c.as_str());
        assert_eq!(kind("Pic PNG"), Some("image/png"));
        assert_eq!(kind("Pic JPEG"), Some("image/jpeg"));
        assert_eq!(kind("Pic GIF"), Some("image/gif"));
        assert_eq!(kind("Pic BMP"), Some("image/bmp"));
        assert_eq!(kind("Pic SVG with PNG fallback"), Some("image/svg+xml"));
        // The PNG inside the EMF is sent, not the EMF.
        assert_eq!(kind("Pic EMF with PNG"), Some("image/png"));
        assert_eq!(kind("Pic EMF bare"), None);
        assert_eq!(seen.len(), 9);
        // An identity that is no picture has no bytes.
        let none = server.handle(Request::ShapePicture { sheet, id: 9999 }, None);
        assert!(matches!(none.reply.unwrap(), Reply::ShapePicture(None)) && none.bytes.is_none());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn outline_search_reveal_and_spill_windows_use_the_application_server() {
        let mut server = Server::new();
        let data = include_bytes!("../../../crates/calc-host/tests/fixtures/dc_gf0/cut-base.xlsx");
        assert!(
            server
                .execute_current(
                    Request::Open {
                        name: "cut-base.xlsx".into()
                    },
                    Some(data.to_vec())
                )
                .reply
                .is_ok()
        );
        let sheet = server.book.host().sheets()[0].id();
        let distant = CellKey::new(sheet, 10000, 52).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(distant),
                    text: "distant source".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        let outline = server.outline(&[], &[Sheet::of(sheet)], Some(distant), "distant source");
        assert_eq!(outline.cells.len(), 1);
        assert_eq!(outline.cells[0].at, Key::of(distant));
        assert_eq!(outline.cells[0].input, "distant source");
        server.execute_current(Request::New, None).reply.unwrap();
        let sheet = server.book.host().sheets()[0].id();
        let anchor = CellKey::new(sheet, 1, 3).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(anchor),
                    text: "=SEQUENCE(1000,2)".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        let outline = server.outline(&[], &[Sheet::of(sheet)], Some(anchor), "");
        assert_eq!(outline.omitted, 1900);
        assert_eq!(outline.cells.len(), 100);
        let last = outline
            .cells
            .iter()
            .find(|c| c.at == Key::of(CellKey::new(sheet, 50, 4).unwrap()))
            .unwrap();
        assert_eq!(last.text, "100");
        assert!(last.input.is_empty());
        assert_eq!(last.provenance, cells::Provenance::Calculated);
    }
}

#[cfg(test)]
mod notebook_tests {
    use super::*;
    fn apply(server: &mut Server, intent: Intent) {
        let Reply::Done { outcome, .. } = server
            .execute_current(Request::Intent(intent), None)
            .reply
            .unwrap()
        else {
            panic!("an edit receipt")
        };
        assert!(outcome.accepted, "{:?}", outcome.error);
    }
    fn page(server: &mut Server, sheet: calc_host::value::SheetId, n: u64) -> wire::Notebook {
        let Reply::Notebook(page) = server
            .execute_current(
                Request::Notebook {
                    sheet: Sheet::of(sheet),
                    page: n,
                    reveal: None,
                    placement: None,
                },
                None,
            )
            .reply
            .unwrap()
        else {
            panic!("a notebook page")
        };
        page
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn newly_linked_duplicate_is_revealed_by_placement_after_the_first_page() {
        let mut server = Server::new();
        let sheet = server.book.host().sheets()[0].id();
        let at = CellKey::new(sheet, 1, 1).unwrap();
        apply(
            &mut server,
            Intent::EnterCell {
                at: Key::of(at),
                text: "7".into(),
            },
        );
        let mut last = None;
        for _ in 0..45 {
            let Reply::Done { outcome, .. } = server
                .execute_current(
                    Request::Intent(Intent::NotebookLink {
                        anchor: Area::of(GridArea::cell(at)),
                        prose: false,
                        text: None,
                    }),
                    None,
                )
                .reply
                .unwrap()
            else {
                panic!("link receipt")
            };
            assert!(outcome.accepted);
            last = outcome.notebook_placement;
        }
        let Reply::Notebook(page) = server
            .execute_current(
                Request::Notebook {
                    sheet: Sheet::of(sheet),
                    page: 0,
                    reveal: Some(Key::of(at)),
                    placement: last.clone(),
                },
                None,
            )
            .reply
            .unwrap()
        else {
            panic!("notebook page")
        };
        assert_eq!(page.page, 1);
        assert_eq!(page.blocks.len(), 5);
        assert_eq!(page.blocks.last().unwrap().placement, last);
        assert_eq!(page.blocks.last().unwrap().cell.as_ref().unwrap().text, "7");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn links_prose_order_structural_moves_and_orphans_round_trip_without_moving_sources() {
        let mut server = Server::new();
        let sheet = server.book.host().sheets()[0].id();
        let key = |row, col| CellKey::new(sheet, row, col).unwrap();
        apply(
            &mut server,
            Intent::NotebookLink {
                anchor: Area::of(GridArea::cell(key(1, 1))),
                prose: true,
                text: Some("=visible prose".into()),
            },
        );
        let first = page(&mut server, sheet, 0);
        assert_eq!(first.blocks.len(), 1);
        assert!(first.blocks[0].prose);
        assert_eq!(
            first.blocks[0].cell.as_ref().unwrap().text,
            "=visible prose"
        );
        assert!(!first.blocks[0].cell.as_ref().unwrap().formula);
        apply(
            &mut server,
            Intent::NotebookLink {
                anchor: Area::of(GridArea::cell(key(2, 1))),
                prose: false,
                text: Some("=3*7".into()),
            },
        );
        let id = page(&mut server, sheet, 0).blocks[1]
            .placement
            .clone()
            .unwrap();
        apply(
            &mut server,
            Intent::NotebookOrder {
                placement: id.clone(),
                direction: -1,
            },
        );
        assert_eq!(
            page(&mut server, sheet, 0).blocks[0]
                .cell
                .as_ref()
                .unwrap()
                .text,
            "21"
        );
        assert_eq!(
            server
                .book
                .shown(&server.book.cell(key(1, 1)).unwrap())
                .text,
            "=visible prose"
        );
        apply(&mut server, Intent::Undo);
        assert!(page(&mut server, sheet, 0).blocks[0].prose);
        apply(&mut server, Intent::Redo);
        apply(
            &mut server,
            Intent::EditAxis {
                sheet: Sheet::of(sheet),
                rows: true,
                insert: true,
                start: 1,
                count: 1,
            },
        );
        assert_eq!(
            page(&mut server, sheet, 0).blocks[0]
                .anchor
                .unwrap()
                .area()
                .unwrap()
                .first(),
            key(3, 1)
        );
        let bytes = server.book.save().unwrap().0;
        server
            .execute_current(
                Request::Open {
                    name: "notebook.xlsx".into(),
                },
                Some(bytes),
            )
            .reply
            .unwrap();
        let sheet = server.book.host().sheets()[0].id();
        let reopened = page(&mut server, sheet, 0);
        assert_eq!(reopened.blocks[0].placement.as_deref(), Some(id.as_str()));
        assert_eq!(reopened.blocks[0].cell.as_ref().unwrap().text, "21");
        assert_eq!(
            reopened.blocks[1].cell.as_ref().unwrap().text,
            "=visible prose"
        );
        apply(
            &mut server,
            Intent::EditAxis {
                sheet: Sheet::of(sheet),
                rows: true,
                insert: false,
                start: 3,
                count: 1,
            },
        );
        assert!(page(&mut server, sheet, 0).blocks[0].anchor.is_none());
        let bytes = server.book.save().unwrap().0;
        server
            .execute_current(
                Request::Open {
                    name: "orphan.xlsx".into(),
                },
                Some(bytes),
            )
            .reply
            .unwrap();
        let sheet = server.book.host().sheets()[0].id();
        let orphan = page(&mut server, sheet, 0);
        assert!(orphan.blocks[0].anchor.is_none());
        assert_eq!(
            orphan.blocks[1].cell.as_ref().unwrap().text,
            "=visible prose"
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn bounded_pages_and_current_array_details_do_not_turn_caches_into_calculated_values() {
        let mut server = Server::new();
        let sheet = server.book.host().sheets()[0].id();
        let key = |row, col| CellKey::new(sheet, row, col).unwrap();
        for row in 1..=85 {
            apply(
                &mut server,
                Intent::EnterCell {
                    at: Key::of(key(row, 1)),
                    text: row.to_string(),
                },
            );
        }
        let first = page(&mut server, sheet, 0);
        assert_eq!(first.total, 85);
        assert_eq!(first.blocks.len(), 40);
        assert_eq!(page(&mut server, sheet, 2).blocks.len(), 5);
        assert!(server.subscriptions.is_empty());
        apply(
            &mut server,
            Intent::EnterCell {
                at: Key::of(key(2, 4)),
                text: "99".into(),
            },
        );
        apply(
            &mut server,
            Intent::EnterCell {
                at: Key::of(key(1, 3)),
                text: "=SEQUENCE(3,2)".into(),
            },
        );
        let detail = server.block_detail(GridArea::cell(key(1, 3)));
        assert_eq!(detail.blockers, vec![Area::of(GridArea::cell(key(2, 4)))]);
        assert!(detail.summary.contains("cannot spill"));
        assert_eq!(detail.cells[3].text, "99");
        apply(
            &mut server,
            Intent::Clear {
                areas: vec![Area::of(GridArea::cell(key(2, 4)))],
            },
        );
        assert!(
            server
                .notebook_link(
                    Area::of(GridArea::cell(key(2, 4))),
                    false,
                    Some("overwrite".into())
                )
                .is_err()
        );
        assert!(
            server
                .notebook_link(Area::of(GridArea::cell(key(2, 4))), true, None)
                .is_err()
        );
        let detail = server.block_detail(GridArea::cell(key(2, 4)));
        assert_eq!(detail.anchor, Some(Key::of(key(1, 3))));
        assert!(detail.blockers.is_empty());
        assert_eq!(detail.cells.len(), 6);
        assert_eq!(detail.cells[5].text, "6");
        apply(&mut server, Intent::SetCalcMode { manual: true });
        apply(
            &mut server,
            Intent::EnterCell {
                at: Key::of(key(2, 4)),
                text: "99".into(),
            },
        );
        assert!(
            server
                .block_detail(GridArea::cell(key(1, 3)))
                .blockers
                .is_empty()
        );
    }
}

fn sheet_change(change: wire::SheetChange) -> calc_host::SheetChange {
    use calc_host::SheetChange as H;
    use wire::SheetChange as W;
    match change {
        W::Add { name } => H::Add { name },
        W::Rename { name } => H::Rename { name },
        W::Move { position } => H::Move { position },
        W::Remove => H::Remove,
        W::Hide => H::Hide,
        W::Show => H::Show,
    }
}
fn conditional_change(change: wire::ConditionalChange) -> calc_host::ConditionalChange {
    use calc_host::ConditionalChange as H;
    use wire::ConditionalChange as W;
    match change {
        W::Condition {
            kind,
            applies,
            color,
            formulas,
            operator,
            rank,
            percent,
            stop,
        } => H::Condition {
            kind,
            applies,
            color,
            formulas,
            operator,
            rank,
            percent,
            stop,
        },
        W::Add {
            kind,
            applies,
            color,
        } => H::Add {
            kind,
            applies,
            color,
        },
        W::Update {
            applies,
            formulas,
            stop,
        } => H::Update {
            applies,
            formulas,
            stop,
        },
        W::Move { up } => H::Move { up },
        W::Remove => H::Remove,
    }
}

#[cfg(test)]
mod tile_tests {
    use super::*;
    use dna_calc_model::{TileApply, ViewMirror};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn number_as_text_markers_match_excel_through_the_tile_stream() {
        use crate::workbook::zip_fixture_kit::{RawEntry, build_zip, parse_zip};
        let blank = calc_host::blank_package();
        let zip = parse_zip(&blank);
        // ww2 grid-header probe, Excel 16.0.20527. Apostrophe, Text format, numeric General,
        // formula text, and the workbook's ignoredErrors control.
        let cells = r#"<sheetData><row r="1">
            <c r="A1" s="1" t="inlineStr"><is><t>123</t></is></c>
            <c r="B1" s="2" t="inlineStr"><is><t>123</t></is></c>
            <c r="C1"><v>123</v></c>
            <c r="D1" t="str"><f>"123"</f><v>123</v></c>
            <c r="E1" s="2" t="inlineStr"><is><t>123</t></is></c>
            </row></sheetData><ignoredErrors>
            <ignoredError sqref="E1" numberStoredAsText="1"/>
            </ignoredErrors>"#;
        let entries: Vec<_> = zip
            .entries
            .iter()
            .map(|entry| {
                let payload = entry.payload(&blank);
                if !matches!(entry.name.as_str(), "xl/worksheets/sheet1.xml" | "xl/styles.xml") {
                    return RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32);
                }
                let bytes = miniz_oxide::inflate::decompress_to_vec(payload).unwrap();
                let xml = String::from_utf8(bytes).unwrap();
                let xml = if entry.name == "xl/worksheets/sheet1.xml" {
                    xml.replace("<sheetData/>", cells)
                } else {
                    xml.replace(r#"<cellXfs count="1">"#, r#"<cellXfs count="3">"#)
                        .replace(
                            "</cellXfs>",
                            concat!(
                                r#"<xf xfId="0" quotePrefix="1"/>"#,
                                r#"<xf xfId="0" numFmtId="49" applyNumberFormat="1"/>"#,
                                "</cellXfs>"
                            ),
                        )
                };
                RawEntry::stored(&entry.name, xml.as_bytes())
            })
            .collect();
        let mut server = Server::new();
        let opened = server.execute_current(
            Request::Open {
                name: "number-as-text.xlsx".into(),
            },
            Some(build_zip(&entries)),
        );
        assert!(
            matches!(opened.reply, Ok(Reply::Done { ref outcome, .. }) if outcome.accepted),
            "{:?}",
            opened.reply
        );
        while server.has_pending_calculation() {
            server.advance_calculation();
        }
        server.take_events();
        let sheet = server.book().sheets()[0].id;
        subscribe(&mut server, 1);
        let mut mirror = ViewMirror::default();
        receive(&mut mirror, projection_events(&mut server));
        let observed: Vec<_> = (1..=5)
            .map(|column| {
                let at = CellKey::new(sheet, 1, column).unwrap();
                mirror.cell(at).unwrap().number_as_text
            })
            .collect();
        assert_eq!(observed, [true, true, false, false, false]);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_blank_formatted_cell_survives_the_wire_and_renders_until_undo() {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let at = CellKey::new(sheet, 2, 2).unwrap();
        subscribe(&mut server, 1);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        server
            .handle(Request::TileAck(barrier), None)
            .reply
            .unwrap();
        let answer = server.execute_current(
            Request::Intent(Intent::FormatCells {
                areas: vec![Area::of(GridArea::cell(at))],
                revision: server.book().host().revision().get(),
                patch: dna_calc_model::format::FormatPatch {
                    fill: Some("#ffeedd".into()),
                    ..Default::default()
                },
            }),
            None,
        );
        assert!(
            matches!(
                answer.reply,
                Ok(Reply::Done {
                    outcome: Outcome { accepted: true, .. },
                    ..
                })
            ),
            "{:?}",
            answer.reply
        );
        let barrier = receive(&mut mirror, projection_events(&mut server));
        let cell = mirror
            .cell(at)
            .expect("style-only cell is a projected identity");
        assert_eq!(cell.provenance, cells::Provenance::Empty);
        assert!(cell.text.is_empty());
        assert!(cell.input.is_empty());
        assert_eq!(cell.style.fill.as_deref(), Some("#ffeedd"));
        assert_eq!(
            render(&mirror, &server)
                .cells
                .iter()
                .find(|c| c.address == "B2")
                .unwrap()
                .shown
                .style
                .fill
                .as_deref(),
            Some("#ffeedd")
        );
        server
            .handle(Request::TileAck(barrier), None)
            .reply
            .unwrap();
        server
            .execute_current(Request::Intent(Intent::Undo), None)
            .reply
            .unwrap();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        assert!(mirror.cell(at).is_none());
        server
            .handle(Request::TileAck(barrier), None)
            .reply
            .unwrap();
        server
            .execute_current(Request::Intent(Intent::Redo), None)
            .reply
            .unwrap();
        receive(&mut mirror, projection_events(&mut server));
        assert_eq!(
            mirror.cell(at).unwrap().style.fill.as_deref(),
            Some("#ffeedd")
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_subscription_to_a_covered_merge_cell_observes_its_source_anchor_and_edits() {
        let mut server = Server::new();
        server
            .execute_current(
                Request::Open {
                    name: "features.xlsx".into(),
                },
                Some(
                    include_bytes!("../../../crates/calc-doc/tests/fixtures/dc_5sr/features.xlsx")
                        .to_vec(),
                ),
            )
            .reply
            .unwrap();
        let sheet = server.book().sheets()[0].id;
        let anchor = CellKey::new(sheet, 1, 8).unwrap();
        let covered = CellKey::new(sheet, 1, 9).unwrap();
        let enter_anchor = |server: &mut Server, text: &str| {
            let Reply::Done { outcome, .. } = server
                .execute_current(
                    Request::Intent(Intent::EnterCell {
                        at: Key::of(anchor),
                        text: text.into(),
                    }),
                    None,
                )
                .reply
                .unwrap()
            else {
                panic!("entry receipt")
            };
            assert!(outcome.accepted, "{:?}", outcome.error);
        };
        enter_anchor(&mut server, "merge source");
        server.take_events();
        let workbook = server.workbook_id();
        server
            .execute_current(
                Request::Subscribe {
                    workbook,
                    id: 1,
                    interest: Interest::Area(Area::of(GridArea::cell(covered))),
                },
                None,
            )
            .reply
            .unwrap();
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        let coverage = &mirror.geometry(sheet).unwrap().coverage;
        assert!(
            coverage
                .iter()
                .filter_map(|a| a.area())
                .any(|a| a.contains(anchor))
        );
        assert!(
            !coverage
                .iter()
                .filter_map(|a| a.area())
                .any(|a| a.contains(CellKey::new(sheet, 99, 99).unwrap()))
        );
        assert_eq!(mirror.cell(anchor).unwrap().text, "merge source");
        assert_eq!(
            mirror.cell(anchor).unwrap().provenance,
            cells::Provenance::Literal
        );
        assert_eq!(
            mirror.geometry(sheet).unwrap().merges,
            [Area::of(GridArea::new(anchor, covered).unwrap())]
        );
        server
            .handle(Request::TileAck(barrier), None)
            .reply
            .unwrap();
        enter_anchor(&mut server, "changed source");
        receive(&mut mirror, projection_events(&mut server));
        assert_eq!(mirror.cell(anchor).unwrap().text, "changed source");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dense_publication_reaches_the_mirror() {
        let mut server = Server::new();
        enter(&mut server, "1");
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        server.handle(Request::TileAck(barrier), None);
        let marker = mirror.cells().next().unwrap().1.clone();
        eprintln!(
            "cell inline bytes: {}, accounted bytes: {}",
            std::mem::size_of_val(&marker),
            marker.retained_bytes()
        );
        let sheet = Sheet(marker.at.0, marker.at.1);
        let cells = (1..=7900)
            .map(|row| {
                let mut cell = marker.clone();
                cell.at = Key(sheet.0, sheet.1, row, 1);
                cell
            })
            .collect();
        let geometry = vec![(sheet, server.geometry(sheet))];
        let stream = server.tile_streams.get_mut(&11).unwrap();
        stream
            .update(true, cells, vec![], 1, geometry, None)
            .unwrap();
        let events = stream
            .publish(11, barrier.workbook, barrier.transaction + 1)
            .unwrap();
        receive(&mut mirror, events);
        assert_eq!(mirror.cells().count(), 7900);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn distant_overflow_anchor_is_in_the_subscription() {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let anchor = CellKey::new(sheet, 1, 1).unwrap();
        let visible = CellKey::new(sheet, 1, 17).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(anchor),
                    text: "wide text ".repeat(100),
                }),
                None,
            )
            .reply
            .unwrap();
        server.take_events();
        server
            .execute_current(
                Request::Subscribe {
                    workbook: server.workbook_id(),
                    id: 1,
                    interest: Interest::Area(Area::of(GridArea::cell(visible))),
                },
                None,
            )
            .reply
            .unwrap();
        let mut mirror = ViewMirror::default();
        receive(&mut mirror, projection_events(&mut server));
        assert!(
            mirror.cell(anchor).is_some(),
            "overflow source must be projected"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_overflow_anchor_authored_after_the_subscription_reaches_the_mirror_and_leaves_with_its_text()
     {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let anchor = CellKey::new(sheet, 2, 1).unwrap();
        let near = CellKey::new(sheet, 2, 3).unwrap();
        let visible = CellKey::new(sheet, 1, 17).unwrap();
        let area = GridArea::new(visible, CellKey::new(sheet, 4, 20).unwrap()).unwrap();
        server
            .execute_current(
                Request::Subscribe {
                    workbook: server.workbook_id(),
                    id: 1,
                    interest: Interest::Area(Area::of(area)),
                },
                None,
            )
            .reply
            .unwrap();
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        server.handle(Request::TileAck(barrier), None);
        let enter = |server: &mut Server, at: CellKey, text: &str| {
            server
                .execute_current(
                    Request::Intent(Intent::EnterCell {
                        at: Key::of(at),
                        text: text.into(),
                    }),
                    None,
                )
                .reply
                .unwrap();
        };
        enter(&mut server, anchor, &"wide text ".repeat(100));
        let barrier = receive(&mut mirror, projection_events(&mut server));
        server.handle(Request::TileAck(barrier), None);
        assert!(
            mirror.cell(anchor).is_some(),
            "an anchor authored later must be projected"
        );
        // A nearer cell in the same row stops the overflow, so it replaces the far anchor.
        enter(&mut server, near, "blocker");
        let barrier = receive(&mut mirror, projection_events(&mut server));
        server.handle(Request::TileAck(barrier), None);
        assert!(mirror.cell(near).is_some());
        assert!(
            mirror.cell(anchor).is_none(),
            "a blocked anchor is no longer needed"
        );
    }
    fn projection_events(server: &mut Server) -> Vec<Event> {
        server
            .take_events()
            .into_iter()
            .filter(|e| !matches!(e, Event::Calculation { .. }))
            .collect()
    }
    fn enter(server: &mut Server, text: &str) {
        let sheet = server.book().sheets()[0].id;
        let answer = server.execute_current(
            Request::Intent(Intent::EnterCell {
                at: Key::of(CellKey::new(sheet, 1, 1).unwrap()),
                text: text.into(),
            }),
            None,
        );
        assert!(
            matches!(
                answer.reply,
                Ok(Reply::Done {
                    outcome: Outcome { accepted: true, .. },
                    ..
                })
            ),
            "{:?}",
            answer.reply
        );
    }
    fn subscribe(server: &mut Server, id: u64) {
        let sheet = server.book().sheets()[0].id;
        assert!(
            server
                .handle(
                    Request::Subscribe {
                        workbook: server.workbook_id(),
                        id,
                        interest: Interest::Sheet(Sheet::of(sheet))
                    },
                    None
                )
                .reply
                .is_ok()
        );
    }
    fn render(mirror: &ViewMirror, server: &Server) -> dna_calc_model::render_plan::RenderPlan {
        let sheet = server.book().sheets()[0].id;
        let shown: Vec<_> = mirror.cells().filter_map(|(_, c)| c.shown()).collect();
        let metrics = dna_calc_model::geometry::sheet_metrics(mirror.geometry(sheet));
        let view = dna_calc_model::geometry::Viewport {
            width: 800.,
            height: 16_000.,
            ..Default::default()
        };
        let mut plan = dna_calc_model::render_plan::build_render_plan_with(
            &shown,
            &metrics,
            &view,
            1000,
            20,
            &dna_calc_model::render_plan::PlanOptions {
                geometry: mirror.geometry(sheet),
                ..Default::default()
            },
        );
        plan.attach_semantics(
            mirror,
            sheet,
            &[GridArea::cell(CellKey::new(sheet, 1, 1).unwrap())],
            (1, 1),
        );
        plan
    }
    fn receive(mirror: &mut ViewMirror, events: Vec<Event>) -> wire::TileBarrier {
        let mut barrier = None;
        assert!(matches!(events.first(), Some(Event::TileAdmission(_))));
        assert!(matches!(events.last(), Some(Event::TileCompletion(_))));
        let original = mirror
            .cells()
            .map(|(at, c)| (*at, c.clone()))
            .collect::<Vec<_>>();
        for event in events {
            match event {
                Event::TileAdmission(stamp) => {
                    mirror.admit(stamp).unwrap();
                }
                Event::Tiles(batch) => {
                    let batch = wire::decode_tiles(&wire::encode_tiles(&batch).unwrap()).unwrap();
                    assert_eq!(mirror.stage(&batch), Ok(TileApply::Staged));
                    assert_eq!(
                        mirror
                            .cells()
                            .map(|(at, c)| (*at, c.clone()))
                            .collect::<Vec<_>>(),
                        original
                    );
                }
                Event::TileCompletion(b) => {
                    assert_eq!(mirror.complete(b), Ok(TileApply::Applied(b)));
                    barrier = Some(b);
                }
                Event::ProjectionFailed { reason, .. } => panic!("{reason}"),
                Event::Calculation { .. }
                | Event::SaveReady { .. }
                | Event::WorkbookEventFailed { .. } => {
                    unreachable!("projection helper removes owner controls")
                }
            }
        }
        barrier.expect("the completion barrier")
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fragmented_spills_shrink_atomically_through_the_application_codec() {
        let mut server = Server::new();
        enter(&mut server, "=SEQUENCE(600)");
        subscribe(&mut server, 11);
        let events = projection_events(&mut server);
        assert!(
            events
                .iter()
                .filter(|e| matches!(e, Event::Tiles(_)))
                .count()
                >= 3
        );
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, events);
        assert_eq!(mirror.cells().count(), 600);
        let plan = render(&mirror, &server);
        assert_eq!(plan.cells.len(), 600);
        assert_eq!(plan.publication, mirror.stamp());
        assert_eq!(plan.cells[599].shown.text, "600");
        assert_eq!(
            plan.cells[599].shown.provenance,
            wire::Provenance::Calculated
        );
        assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
        enter(&mut server, "=SEQUENCE(2)");
        let events = projection_events(&mut server);
        assert!(
            events
                .iter()
                .any(|e| matches!(e,Event::Tiles(b) if !b.removed.is_empty()))
        );
        receive(&mut mirror, events);
        assert_eq!(mirror.cells().count(), 2);
        let plan = render(&mirror, &server);
        assert_eq!(plan.cells.len(), 2);
        assert_eq!(plan.cells[1].shown.text, "2");
        assert!(plan.semantic_rows[0].cells[0].selected);
        let cleared = plan
            .semantic_rows
            .iter()
            .flat_map(|r| &r.cells)
            .find(|c| c.address == "A3")
            .unwrap();
        assert_eq!(cleared.provenance, "empty");
        assert!(cleared.text.is_empty());
        assert_eq!(
            mirror
                .cells()
                .map(|(_, c)| c.text.as_str())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_slow_subscriber_keeps_one_flight_and_coalesces_missed_revisions_into_a_reset() {
        let mut server = Server::new();
        enter(&mut server, "1");
        subscribe(&mut server, 12);
        let mut mirror = ViewMirror::default();
        let first = receive(&mut mirror, projection_events(&mut server));
        for n in 2..=75 {
            enter(&mut server, &n.to_string());
            assert!(projection_events(&mut server).is_empty());
        }
        let mut forged = first;
        forged.transaction += 1;
        server.handle(Request::TileAck(forged), None);
        assert!(projection_events(&mut server).is_empty());
        server.handle(Request::TileAck(first), None);
        let events = projection_events(&mut server);
        assert!(
            matches!(events.first(),Some(Event::TileAdmission(s)) if s.reset && s.base==first.through)
        );
        let latest = receive(&mut mirror, events);
        assert_eq!(mirror.cells().next().unwrap().1.text, "75");
        assert_eq!(latest.through, first.through + 1);
        server.handle(Request::TileAck(first), None); // duplicate ack cannot release the new flight
        enter(&mut server, "76");
        assert!(projection_events(&mut server).is_empty());
        server.handle(Request::TileAck(latest), None);
        receive(&mut mirror, projection_events(&mut server));
        assert_eq!(mirror.cells().next().unwrap().1.text, "76");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn lost_fragments_recover_by_explicit_reset_and_old_workbook_acks_cannot_release_new_flights() {
        let mut server = Server::new();
        enter(&mut server, "=SEQUENCE(300)");
        subscribe(&mut server, 13);
        let events = projection_events(&mut server);
        let Event::TileAdmission(stamp) = events[0] else {
            panic!("admission")
        };
        let mut mirror = ViewMirror::default();
        mirror.admit(stamp).unwrap();
        assert!(mirror.complete(stamp.barrier()).is_err());
        server.handle(
            Request::ResetTiles {
                workbook: server.workbook_id(),
                id: 13,
            },
            None,
        );
        let reset = receive(&mut mirror, projection_events(&mut server));
        assert_eq!(mirror.cells().count(), 300);
        let old = reset;
        assert!(server.execute_current(Request::New, None).reply.is_ok());
        subscribe(&mut server, 13);
        let mut replacement = ViewMirror::default();
        let new = receive(&mut replacement, projection_events(&mut server));
        assert_ne!(old.workbook, new.workbook);
        server.handle(Request::TileAck(old), None);
        enter(&mut server, "42");
        assert!(projection_events(&mut server).is_empty());
        server.handle(Request::TileAck(new), None);
        receive(&mut replacement, projection_events(&mut server));
        assert_eq!(replacement.cells().next().unwrap().1.text, "42");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn subscription_budget_is_enforced_and_unsubscribe_releases_it() {
        let mut server = Server::new();
        for id in 0..wire::MAX_SUBSCRIPTIONS as u64 {
            subscribe(&mut server, id);
        }
        let sheet = server.book().sheets()[0].id;
        let workbook = server.workbook_id();
        let request = || Request::Subscribe {
            workbook,
            id: 100,
            interest: Interest::Sheet(Sheet::of(sheet)),
        };
        assert!(
            server
                .handle(request(), None)
                .reply
                .unwrap_err()
                .contains("Too many")
        );
        server.handle(Request::Unsubscribe { workbook, id: 0 }, None);
        assert!(server.handle(request(), None).reply.is_ok());
        assert!(!server.tile_streams.contains_key(&0));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn delayed_controls_cannot_reach_a_replacement_or_reuse_a_retired_identity() {
        let mut server = Server::new();
        subscribe(&mut server, 1);
        let old = server.workbook_id();
        server.execute_current(Request::New, None).reply.unwrap();
        let current = server.workbook_id();
        assert!(current > old);
        subscribe(&mut server, 1);
        let sheet = server.book().sheets()[0].id;
        let interest = Interest::Sheet(Sheet::of(sheet));
        assert!(
            server
                .handle(
                    Request::SetInterest {
                        workbook: old,
                        id: 1,
                        interest: interest.clone()
                    },
                    None
                )
                .reply
                .is_err()
        );
        assert!(
            server
                .handle(
                    Request::ResetTiles {
                        workbook: old,
                        id: 1
                    },
                    None
                )
                .reply
                .is_err()
        );
        assert!(
            server
                .handle(
                    Request::Subscribe {
                        workbook: old,
                        id: 2,
                        interest: interest.clone()
                    },
                    None
                )
                .reply
                .is_err()
        );
        server
            .handle(
                Request::Unsubscribe {
                    workbook: old,
                    id: 1,
                },
                None,
            )
            .reply
            .unwrap();
        assert!(server.subscriptions.contains_key(&1));
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        assert_eq!(barrier.workbook, current);
        server
            .handle(
                Request::Unsubscribe {
                    workbook: current,
                    id: 1,
                },
                None,
            )
            .reply
            .unwrap();
        assert!(
            server
                .handle(
                    Request::Subscribe {
                        workbook: current,
                        id: 1,
                        interest: interest.clone()
                    },
                    None
                )
                .reply
                .unwrap_err()
                .contains("cannot be reused")
        );
        assert!(
            server
                .handle(
                    Request::Subscribe {
                        workbook: current,
                        id: 2,
                        interest
                    },
                    None
                )
                .reply
                .is_ok()
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn retiring_subscriptions_discards_undelivered_frames() {
        let mut server = Server::new();
        for id in 0..200 {
            subscribe(&mut server, id);
            assert!(server.events.iter().any(|e| matches!(e, Event::Tiles(_))));
            let controls: Vec<_> = server
                .events
                .iter()
                .filter(|e| matches!(e, Event::Calculation { .. }))
                .cloned()
                .collect();
            server
                .handle(
                    Request::Unsubscribe {
                        workbook: server.workbook_id(),
                        id,
                    },
                    None,
                )
                .reply
                .unwrap();
            assert_eq!(server.take_events(), controls);
            assert!(server.tile_streams.is_empty());
            assert!(server.subscription_interests.is_empty());
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn recalculation_versions_presentation_roots_without_an_authored_revision() {
        let mut server = Server::new();
        enter(&mut server, "=RAND()");
        server.take_events();
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let first = receive(&mut mirror, projection_events(&mut server));
        let previous = mirror.stamp().unwrap();
        server.handle(Request::TileAck(first), None).reply.unwrap();
        server.take_events();
        server
            .execute_current(Request::Intent(Intent::Recalculate), None)
            .reply
            .unwrap();
        receive(&mut mirror, projection_events(&mut server));
        let current = mirror.stamp().unwrap();
        assert_eq!(current.revision, previous.revision);
        assert!(current.transaction > previous.transaction);
        assert_eq!(current.geometry_version, current.transaction);
        assert_eq!(current.style_version, current.transaction);
        assert_ne!(current.style_version, previous.style_version);
        let cell = mirror.cells().next().unwrap().1;
        assert_eq!(cell.input, "=RAND()");
        assert_eq!(cell.provenance, wire::Provenance::Calculated);
        assert_eq!(cell.kind, "number");
        assert!((0.0..1.0).contains(&cell.text.parse::<f64>().unwrap()));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calculation_completion_waits_for_its_projection_under_backpressure() {
        let mut server = Server::new();
        enter(&mut server, "1");
        server.take_events();
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        enter(&mut server, "=1+1");
        let held = server.take_events();
        let command = held
            .iter()
            .find_map(|e| match e {
                Event::Calculation {
                    receipt: wire::CalculationEvent::Admitted { command, .. },
                    ..
                } => Some(*command),
                _ => None,
            })
            .unwrap();
        assert!(!held.iter().any(|e| matches!(
            e,
            Event::Tiles(_)
                | Event::TileCompletion(_)
                | Event::Calculation {
                    receipt: wire::CalculationEvent::Completed { .. },
                    ..
                }
        )));
        assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
        let released = server.take_events();
        let unit = released
            .iter()
            .find_map(|e| match e {
                Event::TileAdmission(stamp) => stamp.calculation_through,
                _ => None,
            })
            .unwrap();
        assert_eq!(unit.command, command);
        let tiles = released
            .iter()
            .position(|e| matches!(e, Event::TileCompletion(_)))
            .unwrap();
        let completed = released
            .iter()
            .position(|e| {
                matches!(e, Event::Calculation {
            receipt: wire::CalculationEvent::Completed { command: id, .. }, .. } if *id == command)
            })
            .unwrap();
        assert!(tiles < completed);
        receive(
            &mut mirror,
            released
                .into_iter()
                .filter(|e| !matches!(e, Event::Calculation { .. }))
                .collect(),
        );
        let sheet = server.book().sheets()[0].id;
        assert_eq!(
            mirror
                .cell(CellKey::new(sheet, 1, 1).unwrap())
                .unwrap()
                .text,
            "2"
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_oversized_calculation_reports_projection_failure_without_a_tile_barrier() {
        // Real accounting charges each 32,767-character result its text, so the whole column must
        // hold more than MAX_PUBLICATION_BYTES of it to be refused.
        const ROWS: u32 = 3_000;
        let mut server = Server::new();
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
        server.take_events();
        let sheet = server.book().sheets()[0].id;
        let first = CellKey::new(sheet, 1, 1).unwrap();
        let area = GridArea::new(first, CellKey::new(sheet, ROWS, 1).unwrap()).unwrap();
        let selected = server.execute_current(
            Request::Intent(Intent::Select {
                active: Key::of(first),
                areas: vec![Area::of(area)],
            }),
            None,
        );
        assert!(selected.reply.is_ok());
        let result = server.execute_current(
            Request::Intent(Intent::EnterSelection {
                text: "=REPT(\"x\",32767)".into(),
            }),
            None,
        );
        assert!(matches!(
            result.reply,
            Ok(Reply::Done {
                outcome: Outcome { accepted: true, .. },
                ..
            })
        ));
        let events = server.take_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ProjectionFailed { id: 11, .. }))
        );
        assert!(!events.iter().any(|e| matches!(e, Event::TileCompletion(_))));
        assert!(events.iter().any(|e| matches!(e,Event::Calculation { receipt: wire::CalculationEvent::Completed { outcome: wire::CalculationOutcome::Complete, calculated, .. }, failed_projections, .. } if failed_projections == &[11] && *calculated as u64 == u64::from(ROWS))));
        assert!(server.tile_streams[&11].failed());
        assert!(mirror.cells().next().is_none());
        let marker = CellKey::new(sheet, 1, 2).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(marker),
                    text: "preserved".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        let deleted = GridArea::new(
            CellKey::new(sheet, 2, 1).unwrap(),
            CellKey::new(sheet, ROWS, 1).unwrap(),
        )
        .unwrap();
        server
            .execute_current(
                Request::Intent(Intent::Clear {
                    areas: vec![Area::of(deleted)],
                }),
                None,
            )
            .reply
            .unwrap();
        assert!(server.tile_streams[&11].failed());
        assert!(
            !server
                .take_events()
                .iter()
                .any(|e| matches!(e, Event::Tiles(_) | Event::TileCompletion(_)))
        );
        mirror.invalidate();
        server
            .handle(
                Request::ResetTiles {
                    workbook: server.workbook_id(),
                    id: 11,
                },
                None,
            )
            .reply
            .unwrap();
        receive(&mut mirror, projection_events(&mut server));
        assert!(!server.tile_streams[&11].failed());
        assert!(!mirror.needs_reset());
        assert_eq!(mirror.cells().count(), 2);
        assert_eq!(mirror.cell(first).unwrap().text.len(), 32767);
        assert_eq!(mirror.cell(marker).unwrap().text, "preserved");
        assert!(mirror.cell(CellKey::new(sheet, 2, 1).unwrap()).is_none());
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_delayed_manual_completion_retains_its_original_revision_after_an_edit() {
        let mut server = Server::new();
        assert!(
            server
                .execute_current(Request::Intent(Intent::SetCalcMode { manual: true }), None)
                .reply
                .is_ok()
        );
        enter(&mut server, "=1+1");
        server.take_events();
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        let sheet = server.book().sheets()[0].id;
        let at = CellKey::new(sheet, 1, 1).unwrap();
        assert!(
            server
                .execute_current(
                    Request::Intent(Intent::CalculateRange {
                        areas: vec![Area::of(GridArea::cell(at))]
                    }),
                    None
                )
                .reply
                .is_ok()
        );
        let original_revision = server.book().host().revision().get();
        let held = server.take_events();
        assert!(held.iter().any(|e| matches!(
            e,
            Event::Calculation {
                receipt: wire::CalculationEvent::Admitted { .. },
                ..
            }
        )));
        enter(&mut server, "9");
        let after_edit = server.take_events();
        assert!(!after_edit.iter().any(|e| matches!(
            e,
            Event::Calculation {
                receipt: wire::CalculationEvent::Admitted { .. },
                ..
            }
        )));
        assert!(server.book().host().revision().get() > original_revision);
        assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
        let events = server.take_events();
        assert!(events.iter().any(|e| matches!(e,Event::Calculation {
            receipt: wire::CalculationEvent::Completed { revision, .. }, summary: Some(summary), ..
        } if *revision == original_revision && summary.revision == server.book().host().revision().get())));
        receive(
            &mut mirror,
            events
                .into_iter()
                .filter(|e| !matches!(e, Event::Calculation { .. }))
                .collect(),
        );
        assert_eq!(mirror.cell(at).unwrap().text, "9");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn delayed_controls_keep_current_mode_and_saved_status_at_the_same_revision() {
        let mut server = Server::new();
        enter(&mut server, "1");
        server.take_events();
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let barrier = receive(&mut mirror, projection_events(&mut server));
        enter(&mut server, "=1+1");
        let revision = server.book().host().revision().get();
        let held = server.take_events();
        assert!(held.iter().any(|e| matches!(
            e,
            Event::Calculation {
                receipt: wire::CalculationEvent::Admitted { .. },
                ..
            }
        )));
        assert!(
            server
                .execute_current(Request::Intent(Intent::SetCalcMode { manual: true }), None)
                .reply
                .is_ok()
        );
        assert!(server.execute_current(Request::Save, None).reply.is_ok());
        assert_eq!(server.book().host().revision().get(), revision);
        assert!(server.summary().manual);
        assert!(!server.summary().changed);
        assert!(server.handle(Request::TileAck(barrier), None).reply.is_ok());
        let events = server.take_events();
        let mut completed = 0;
        for event in events {
            if let Event::Calculation {
                receipt,
                summary: Some(summary),
                ..
            } = event
            {
                assert_eq!(summary.revision, revision);
                assert!(summary.manual);
                assert!(!summary.changed);
                if let wire::CalculationEvent::Completed {
                    revision: admitted, ..
                } = receipt
                {
                    assert_eq!(admitted, revision);
                    completed += 1;
                }
            }
        }
        assert_eq!(completed, 1);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn removal_pressure_coalesces_to_the_complete_root_and_never_admits_an_oversized_delta() {
        let mut server = Server::new();
        enter(&mut server, "retained");
        subscribe(&mut server, 11);
        let mut mirror = ViewMirror::default();
        let first = receive(&mut mirror, projection_events(&mut server));
        server.handle(Request::TileAck(first), None);
        let marker = mirror.cells().next().unwrap().1.clone();
        let sheet = Sheet(marker.at.0, marker.at.1);
        let geometry = vec![(sheet, server.geometry(sheet))];
        let revision = server.book().host().revision().get();

        // Individually legal disjoint removal lists exceed the pending record budget together.
        // The untouched marker must survive replacing those operations with the current root.
        let split = wire::MAX_PUBLICATION_RECORDS / 2;
        let removed = |start: usize, count: usize| {
            (start..start + count)
                .map(|row| Key(sheet.0, sheet.1, row as u32, 1))
                .collect()
        };
        {
            let stream = server.tile_streams.get_mut(&11).unwrap();
            stream
                .update(
                    false,
                    Vec::new(),
                    removed(2, split),
                    revision,
                    geometry.clone(),
                    None,
                )
                .unwrap();
            stream
                .update(
                    false,
                    Vec::new(),
                    removed(2 + split, split + 1),
                    revision,
                    geometry.clone(),
                    None,
                )
                .unwrap();
        }
        server.collect();
        let events = projection_events(&mut server);
        assert!(matches!(events.first(), Some(Event::TileAdmission(s)) if s.reset));
        let second = receive(&mut mirror, events);
        assert_eq!(mirror.cells().count(), 1);
        assert_eq!(mirror.cell(marker.at.cell().unwrap()), Some(&marker));
        server.handle(Request::TileAck(second), None);

        // Byte pressure also resets a pending delta before its count limit. Every fragment
        // and the aggregate still pass the shipping codec and atomic mirror route.
        let mut large = marker.clone();
        large.text = "x".repeat(16_384);
        large.input = large.text.clone();
        let geometry_bytes = geometry
            .iter()
            .map(|(_, g)| wire::geometry_bytes(g))
            .sum::<usize>();
        let count = (wire::MAX_PUBLICATION_BYTES
            - wire::MAX_TILE_BYTES
            - geometry_bytes
            - marker.retained_bytes())
            / large.retained_bytes();
        let cells = (0..count)
            .map(|row| {
                let mut cell = large.clone();
                cell.at = Key(sheet.0, sheet.1, row as u32 + 1, 2);
                cell
            })
            .collect();
        assert!(count + 16_384 < wire::MAX_PUBLICATION_RECORDS);
        {
            let stream = server.tile_streams.get_mut(&11).unwrap();
            stream
                .update(false, cells, Vec::new(), revision, geometry.clone(), None)
                .unwrap();
            stream
                .update(
                    false,
                    Vec::new(),
                    removed(2, 16_384),
                    revision,
                    geometry.clone(),
                    None,
                )
                .unwrap();
        }
        server.collect();
        let events = projection_events(&mut server);
        assert!(matches!(events.first(), Some(Event::TileAdmission(s)) if s.reset));
        let (bytes, records) = events
            .iter()
            .fold((0, 0), |(bytes, records), event| match event {
                Event::Tiles(batch) => (
                    bytes + batch.retained_bytes(),
                    records + batch.cells.len() + batch.removed.len(),
                ),
                _ => (bytes, records),
            });
        assert!(bytes <= wire::MAX_PUBLICATION_BYTES);
        assert!(records <= wire::MAX_PUBLICATION_RECORDS);
        let third = receive(&mut mirror, events);
        assert_eq!(mirror.cells().count(), count + 1);
        assert_eq!(mirror.cell(marker.at.cell().unwrap()), Some(&marker));
        assert_eq!(
            mirror
                .cell(Key(sheet.0, sheet.1, count as u32, 2).cell().unwrap())
                .unwrap()
                .text,
            large.text
        );
        server.handle(Request::TileAck(third), None);

        // An individual oversized list is refused before retaining it or advancing delivery.
        let stream = server.tile_streams.get_mut(&11).unwrap();
        let error = stream
            .update(
                false,
                Vec::new(),
                removed(2, wire::MAX_PUBLICATION_RECORDS + 1),
                revision,
                geometry,
                None,
            )
            .unwrap_err();
        assert!(error.contains("delta exceeds"));
        assert!(stream.failed());
        assert!(!stream.has_pending());
        assert!(
            stream
                .publish(11, first.workbook, third.transaction + 1)
                .unwrap()
                .is_empty()
        );
        assert_eq!(mirror.cells().count(), count + 1);
        assert_eq!(mirror.stamp().unwrap().barrier(), third);
    }
}

#[cfg(test)]
#[path = "../../../crates/calc-engine/tests/fixtures/seven_cell_async.rs"]
mod async_provider;
#[cfg(test)]
mod async_contract_tests {
    use super::async_provider::{Counts, Echo};
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    #[derive(Debug, Default)]
    struct MockFetch(RefCell<Vec<calc_func::FetchRequest>>);
    impl calc_func::FetchRoute for MockFetch {
        fn begin(&self, request: calc_func::FetchRequest) -> u64 {
            let mut seen = self.0.borrow_mut();
            seen.push(request);
            seen.len() as u64
        }
    }

    /// A formula entered in the worker fetches through the server's route, parks, and takes its
    /// reply; a replacement workbook keeps the route.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_fetch_f0tp__a_formula_fetches_through_the_server() {
        let mut server = Server::new();
        let route = Rc::new(MockFetch::default());
        server.set_fetch_route(route.clone());
        let s = server.book.host().sheets()[0].id();
        let b = CellKey::new(s, 1, 2).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(b),
                    text: "=LEN(WEBSERVICE(\"http://mock/x\"))".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        drain(&mut server);
        assert_eq!(route.0.borrow().len(), 1);
        assert_eq!(route.0.borrow()[0].url, "http://mock/x");
        let reply =
            calc_func::FetchReply::Body(b"abc".to_vec()).into_value(calc_func::FetchKind::Text);
        assert!(server.complete_fetch(1, reply.clone()));
        assert!(!server.complete_fetch(1, reply), "one reply per request");
        drain(&mut server);
        let projection = calc_host::project(
            server.book().host(),
            &calc_host::ProjectionRequest { cells: vec![b] },
        )
        .unwrap();
        match projection.cells()[0].value() {
            calc_host::ProjectedValue::Calculated { value, .. } => {
                assert_eq!(value, &calc_host::value::CalcValue::number(3.0));
            }
            other => panic!("{other:?}"),
        }
    }

    /// A picture reaches the page with its bytes, and the cell shows its alt text meanwhile.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_fetch_f0tp__a_picture_reaches_the_page() {
        let mut server = Server::new();
        let route = Rc::new(MockFetch::default());
        server.set_fetch_route(route.clone());
        let s = server.book.host().sheets()[0].id();
        let b = CellKey::new(s, 1, 2).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(b),
                    text: "=IMAGE(\"https://mock/p.png\",\"a picture\")".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        drain(&mut server);
        let bytes = b"\x89PNG\r\n\x1a\n a picture".to_vec();
        let reply =
            calc_func::FetchReply::Body(bytes.clone()).into_value(calc_func::FetchKind::Picture);
        assert!(server.complete_fetch(1, reply));
        drain(&mut server);
        let projected = calc_host::project(
            server.book().host(),
            &calc_host::ProjectionRequest { cells: vec![b] },
        )
        .unwrap();
        let shown = server.book().shown(&projected.cells()[0]);
        assert_eq!(shown.kind, "image");
        let answer = server.handle(Request::CellImage { at: Key::of(b) }, None);
        match answer.reply.unwrap() {
            Reply::CellImage(Some(image)) => {
                assert_eq!(image.source, "https://mock/p.png");
                assert_eq!(image.alt_text.as_deref(), Some("a picture"));
                assert_eq!(image.content_type, "image/png");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(answer.bytes, Some(bytes.clone()));

        server
            .execute_current(Request::Intent(Intent::SetCalcMode { manual: true }), None)
            .reply
            .unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(b),
                    text: "=IMAGE(\"https://mock/other.png\")".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        let stale = server.handle(Request::CellImage { at: Key::of(b) }, None);
        assert!(matches!(stale.reply.unwrap(), Reply::CellImage(Some(_))));
        assert_eq!(stale.bytes, Some(bytes));
    }

    /// `=FILTERXML(WEBSERVICE(url),xpath)` over the body Excel fetched from a public URL
    /// (https://httpbin.org/xml) gives Excel's spill: its two titles, and the author attribute.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_fetch_f0tp__filterxml_over_webservice_matches_live_excel() {
        const BODY: &str = include_str!("../tests/fixtures/httpbin-xml.txt");
        let mut server = Server::new();
        let route = Rc::new(MockFetch::default());
        server.set_fetch_route(route.clone());
        let s = server.book.host().sheets()[0].id();
        let enter = |server: &mut Server, column: u32, formula: &str| {
            server
                .execute_current(
                    Request::Intent(Intent::EnterCell {
                        at: Key::of(CellKey::new(s, 1, column).unwrap()),
                        text: formula.into(),
                    }),
                    None,
                )
                .reply
                .unwrap();
            for _ in 0..100 {
                drain(server);
                let Some(token) = server.pending_async().into_iter().next() else {
                    return;
                };
                let reply = calc_func::FetchReply::Body(BODY.as_bytes().to_vec())
                    .into_value(calc_func::FetchKind::Text);
                assert_eq!(
                    server.complete_async(&token, reply),
                    calc_host::AsyncDisposition::Accepted
                );
            }
            panic!("the fetches did not settle");
        };
        enter(
            &mut server,
            1,
            "=FILTERXML(WEBSERVICE(\"http://mock/xml\"),\"//slide/title\")",
        );
        enter(
            &mut server,
            3,
            "=FILTERXML(WEBSERVICE(\"http://mock/xml\"),\"/slideshow/@author\")",
        );
        let cells = [(1, 1), (2, 1), (1, 3)]
            .map(|(row, column)| CellKey::new(s, row, column).unwrap())
            .to_vec();
        let projection = calc_host::project(
            server.book().host(),
            &calc_host::ProjectionRequest { cells },
        )
        .unwrap();
        let text = |at: usize| match projection.cells()[at].value() {
            calc_host::ProjectedValue::Calculated { value, .. } => match value.core_ref() {
                calc_host::value::CoreRef::Text(text) => text.to_string_lossy(),
                other => format!("{other:?}"),
            },
            other => format!("{other:?}"),
        };
        assert_eq!(text(0), "Wake up to WonderWidgets!");
        assert_eq!(text(1), "Overview");
        assert_eq!(text(2), "Yours Truly");
    }

    fn configured(formula: &str) -> (Server, CellKey, Rc<RefCell<Counts>>) {
        let mut server = Server::new();
        let s = server.book.host().sheets()[0].id();
        let b = CellKey::new(s, 1, 2).unwrap();
        server
            .execute_current(Request::Intent(Intent::SetCalcMode { manual: true }), None)
            .reply
            .unwrap();
        let counts = Rc::new(RefCell::new(Counts::default()));
        let mut functions = calc_host::CallableTable::new();
        functions
            .register("AsyncEcho", Rc::new(Echo(counts.clone())))
            .unwrap();
        server.book_mut().set_user_functions(functions);
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(b),
                    text: formula.into(),
                }),
                None,
            )
            .reply
            .unwrap();
        server.set_calculation_budget(calc_host::WorkBudget::evaluations(1));
        (server, b, counts)
    }
    fn drain(server: &mut Server) {
        for _ in 0..1000 {
            if !server.has_pending_calculation() {
                return;
            }
            server.advance_calculation();
        }
        panic!("bounded worker calculation did not park or finish");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p06_worker_ordinary_save_waits_once_and_notifies_without_polling() {
        for existing in [false, true] {
            let (mut server, b, counts) = configured("=AsyncEcho(1)+2");
            let c = CellKey::new(b.sheet(), 1, 3).unwrap();
            server.book_mut().dispatch(SkinIntent::EnterCell {
                at: c,
                text: "=B1*2".into(),
            });
            if existing {
                server
                    .execute_current(Request::Intent(Intent::Recalculate), None)
                    .reply
                    .unwrap();
                drain(&mut server);
            }
            let request = server.envelope(Request::SaveWithContext {
                views: None,
                calculate_before_save: Some(true),
            });
            let first = server.handle(request.clone(), None);
            let retry = server.handle(request, None);
            assert_eq!(wire::encode(&first.reply), wire::encode(&retry.reply));
            let token = match first.reply.unwrap() {
                Reply::SavePending { token, .. } => token,
                other => panic!("{other:?}"),
            };
            assert!(first.bytes.is_none());
            assert_eq!(counts.borrow().prefix, usize::from(existing));
            drain(&mut server);
            assert!(!server.has_pending_calculation());
            assert_eq!(counts.borrow().prefix, 1);
            assert!(
                !server
                    .take_events()
                    .iter()
                    .any(|e| matches!(e, Event::SaveReady { .. }))
            );
            let provider = server.pending_async()[0].clone();
            assert_eq!(provider.destination, calc_host::ResultDestination::Live);
            let waiting = server.execute_current(Request::PollSave { token }, None);
            assert!(matches!(
                waiting.reply,
                Ok(Reply::SavePending { waiting: true, .. })
            ));
            assert!(waiting.bytes.is_none());
            assert_eq!(
                server.complete_async(&provider, calc_host::value::CalcValue::number(40.0)),
                calc_host::AsyncDisposition::Accepted
            );
            assert_eq!(counts.borrow().suffix, 0);
            drain(&mut server);
            assert_eq!(
                server
                    .take_events()
                    .iter()
                    .filter(|e| matches!(e,Event::SaveReady { token: id,.. } if *id==token))
                    .count(),
                1
            );
            let request = server.envelope(Request::PollSave { token });
            let image = server.handle(request.clone(), None);
            let retry = server.handle(request, None);
            assert_eq!(wire::encode(&image.reply), wire::encode(&retry.reply));
            assert_eq!(image.bytes, retry.bytes);
            assert!(matches!(image.reply,Ok(Reply::SaveImage { outcome,.. }) if outcome.accepted));
            assert_eq!((counts.borrow().prefix, counts.borrow().suffix), (1, 1));
            let reopened = calc_host::HostState::open(
                std::sync::Arc::new(calc_host::MemorySource::new(image.bytes.unwrap())),
                &calc_host::OpenPolicy::default(),
                calc_host::HostOptions {
                    calc_mode: Some(CalcMode::Manual),
                    ..Default::default()
                },
            )
            .unwrap();
            let s = reopened.sheets()[0].id();
            for (column, expected) in [(2, 42.0), (3, 84.0)] {
                let at = CellKey::new(s, 1, column).unwrap();
                let page = calc_host::project(
                    &reopened,
                    &calc_host::ProjectionRequest { cells: vec![at] },
                )
                .unwrap();
                assert!(
                    matches!(page.cells()[0].value(),ProjectedValue::FileCached(cache) if cache.value().to_value().as_number()==Some(expected))
                );
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_rebuild_is_public_forced_and_deduplicated_without_authored_edits() {
        let mut server = Server::new();
        let s = server.book.host().sheets()[0].id();
        let at = CellKey::new(s, 1, 1).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(at),
                    text: "=2+3".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        drain(&mut server);
        let revision = server.book.host().revision();
        let history = server.book.session().history_depth();
        server.take_events();
        let command = server.envelope(Request::Intent(Intent::Rebuild));
        let first = server.handle(command.clone(), None);
        let retry = server.handle(command, None);
        assert_eq!(wire::encode(&first.reply), wire::encode(&retry.reply));
        drain(&mut server);
        assert_eq!(server.book.host().revision(), revision);
        assert_eq!(server.book.session().history_depth(), history);
        let events = server.take_events();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Calculation {
                        receipt: wire::CalculationEvent::Admitted {
                            selection: wire::CalculationSelection::ForceAll,
                            ..
                        },
                        ..
                    }
                ))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Calculation {
                        receipt: wire::CalculationEvent::Completed {
                            calculated: 1,
                            required: 1,
                            outcome: wire::CalculationOutcome::Complete,
                            ..
                        },
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(
            matches!(server.book.cell(at).unwrap().value(),ProjectedValue::Calculated { value,.. } if value.as_number()==Some(5.0))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_10__worker_save_regroups_authored_sources_in_the_actual_bytes() {
        let mut server = Server::new();
        server
            .execute_current(
                Request::Open {
                    name: "shared.xlsx".into(),
                },
                Some(calc_host::blank_package()),
            )
            .reply
            .unwrap();
        let sheet = server.book().host().sheets()[0].id();
        for (r, c, text) in [
            (1, 1, "1"),
            (2, 1, "2"),
            (4, 1, "4"),
            (1, 2, "10"),
            (1, 3, "=A1+$B$1"),
            (2, 3, "=A2+$B$1"),
            (4, 3, "=A4+$B$1"),
        ] {
            let request = server.envelope(Request::Intent(Intent::EnterCell {
                at: Key::of(CellKey::new(sheet, r, c).unwrap()),
                text: text.into(),
            }));
            let outcome = server.handle(wire::decode(&wire::encode(&request)).unwrap(), None);
            assert!(
                matches!(
                    outcome.reply,
                    Ok(Reply::Done {
                        outcome: wire::Outcome { accepted: true, .. },
                        ..
                    })
                ),
                "{:?}",
                outcome.reply
            );
            drain(&mut server);
        }
        let before = server.book().host().model().formula_source_stats();
        assert_eq!((before.inline_sources, before.retained_sources), (3, 0));
        let revision = server.book().host().revision();
        let request = server.envelope(Request::Save);
        let first = server.handle(wire::decode(&wire::encode(&request)).unwrap(), None);
        let retry = server.handle(request, None);
        assert_eq!(wire::encode(&first.reply), wire::encode(&retry.reply));
        assert_eq!(first.bytes, retry.bytes);
        assert!(
            matches!(&first.reply,Ok(Reply::Done {outcome,saved:Some(_),..}) if outcome.accepted),
            "{:?}",
            first.reply
        );
        assert_eq!(server.book().host().revision(), revision);
        assert_eq!(
            server.book().host().model().formula_source_stats(),
            before,
            "serialization does not replace live storage"
        );
        let reopened = calc_host::HostState::open(
            std::sync::Arc::new(calc_host::MemorySource::new(first.bytes.unwrap())),
            &Default::default(),
            calc_host::HostOptions {
                calc_mode: Some(calc_host::CalcMode::Manual),
                ..Default::default()
            },
        )
        .unwrap();
        let stats = reopened.model().formula_source_stats();
        assert_eq!(
            (
                stats.retained_sources,
                stats.retained_members,
                stats.formula_cells
            ),
            (1, 3, 3)
        );
        let sheet = reopened.sheets()[0].id();
        assert!(
            reopened
                .model()
                .cell(CellKey::new(sheet, 3, 3).unwrap())
                .is_none()
        );
        for (row, expected) in [(1, 11.), (2, 12.), (4, 14.)] {
            let at = CellKey::new(sheet, row, 3).unwrap();
            let cells =
                calc_host::project(&reopened, &calc_host::ProjectionRequest { cells: vec![at] })
                    .unwrap();
            let calc_host::ProjectedValue::FileCached(cache) = cells.cells()[0].value() else {
                panic!("actual saved cache")
            };
            assert_eq!(
                cache.value().to_value(),
                calc_value::CalcValue::number(expected)
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p06_worker_save_writes_live_table_results_and_journals_the_bytes() {
        let mut server = Server::new();
        server
            .execute_current(
                Request::Open {
                    name: "oracle.xlsx".into(),
                },
                Some(
                    include_bytes!("../../../crates/calc-engine/tests/fixtures/dc_10y/oracle.xlsx")
                        .to_vec(),
                ),
            )
            .reply
            .unwrap();
        drain(&mut server);
        let s = server
            .book
            .host()
            .sheets()
            .iter()
            .find(|sheet| String::from_utf16_lossy(sheet.name().text().utf16_code_units()) == "DT")
            .unwrap()
            .id();
        let cell = |r, c| CellKey::new(s, r, c).unwrap();
        server
            .book_mut()
            .dispatch(SkinIntent::SetCalcMode(CalcMode::AutomaticExceptTables));
        for (at, text) in [
            (cell(1, 26), "1"),
            (cell(3, 2), "=B1*Z1"),
            (cell(4, 1), "2"),
            (cell(5, 1), "3"),
        ] {
            server.book_mut().dispatch(SkinIntent::EnterCell {
                at,
                text: text.into(),
            });
            drain(&mut server);
        }
        server.book_mut().dispatch(SkinIntent::Recalculate);
        drain(&mut server);
        assert!(
            matches!(server.book.cell(cell(4,2)).unwrap().value(),ProjectedValue::Calculated {value,..} if value.as_number()==Some(2.0))
        );
        server.book_mut().dispatch(SkinIntent::EnterCell {
            at: cell(1, 26),
            text: "10".into(),
        });
        drain(&mut server);
        assert!(matches!(
            server.book.cell(cell(4, 2)).unwrap().value(),
            ProjectedValue::Stale(_)
        ));
        let unrelated = CellKey::new(server.book.host().sheets()[0].id(), 6, 4).unwrap();
        let before_unrelated = server.book.cell(unrelated).unwrap().value().clone();
        // Open now supplies the file's own name. This measured malformed local
        // INDIRECT reference calculates #REF! rather than refusing a missing file identity.
        assert!(
            matches!(
                &before_unrelated,
                ProjectedValue::Calculated { value, .. }
                    if value.as_error() == Some(calc_value::WorksheetErrorCode::Ref)
            ),
            "{before_unrelated:?}"
        );
        server.set_calculation_budget(calc_host::WorkBudget::evaluations(1));
        let request = server.envelope(Request::Save);
        let answer = server.handle(request.clone(), None);
        let retry = server.handle(request, None);
        assert_eq!(wire::encode(&answer.reply), wire::encode(&retry.reply));
        assert_eq!(answer.bytes, retry.bytes);
        let token = match answer.reply.unwrap() {
            Reply::SavePending {
                token,
                waiting: false,
            } => token,
            other => panic!("{other:?}"),
        };
        assert!(answer.bytes.is_none());
        assert!(
            matches!(
                server.book.cell(cell(4, 2)).unwrap().value(),
                ProjectedValue::Stale(_)
            ),
            "Save admission does not run TABLE synchronously"
        );
        drain(&mut server);
        assert_eq!(
            server
                .take_events()
                .iter()
                .filter(|event| matches!(event, Event::SaveReady { token: id, .. } if *id == token))
                .count(),
            1
        );
        let poll = server.envelope(Request::PollSave { token });
        let answer = server.handle(poll.clone(), None);
        let retry = server.handle(poll, None);
        assert_eq!(wire::encode(&answer.reply), wire::encode(&retry.reply));
        assert_eq!(answer.bytes, retry.bytes);
        assert!(
            matches!(&answer.reply, Ok(Reply::SaveImage { outcome, .. }) if outcome.accepted),
            "{:?}",
            answer.reply
        );
        assert_eq!(
            server.book.cell(unrelated).unwrap().value(),
            &before_unrelated
        );
        for (r, expected) in [(4, 20.0), (5, 30.0)] {
            assert!(
                matches!(server.book.cell(cell(r,2)).unwrap().value(),ProjectedValue::Calculated {value,..} if value.as_number()==Some(expected))
            );
        }
        let bytes = answer
            .bytes
            .expect("the ordinary Save returned its actual package");
        let reopened = calc_host::HostState::open(
            std::sync::Arc::new(calc_host::MemorySource::new(bytes)),
            &calc_host::OpenPolicy::default(),
            calc_host::HostOptions {
                calc_mode: Some(CalcMode::Manual),
                ..Default::default()
            },
        )
        .unwrap();
        let s = reopened
            .sheets()
            .iter()
            .find(|sheet| String::from_utf16_lossy(sheet.name().text().utf16_code_units()) == "DT")
            .unwrap()
            .id();
        let page = calc_host::project(
            &reopened,
            &calc_host::ProjectionRequest {
                cells: vec![
                    CellKey::new(s, 4, 2).unwrap(),
                    CellKey::new(s, 5, 2).unwrap(),
                ],
            },
        )
        .unwrap();
        for (cell, expected) in page.cells().iter().zip([20.0, 30.0]) {
            let ProjectedValue::FileCached(cache) = cell.value() else {
                panic!("saved file provenance");
            };
            assert!(
                matches!(cache.value(),calc_host::CachedValue::Number(number) if number.get()==expected)
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p07_worker_wait_return_and_completion_use_the_existing_server() {
        for formula in [
            "=AsyncEcho(1)+2",
            "=LET(x,2,IF(TRUE,AsyncEcho(1)+x,0))",
            "=LAMBDA(x,IF(TRUE,AsyncEcho(1)+x,0))(2)",
            "=SUM(MAP({1},LAMBDA(x,AsyncEcho(x)+2)))",
            "=REDUCE(2,{1},LAMBDA(acc,x,AsyncEcho(x)+acc))",
            "=SUM(SCAN(2,{1},LAMBDA(acc,x,AsyncEcho(x)+acc)))",
            "=SUM(BYROW({1},LAMBDA(row,AsyncEcho(SUM(row))+2)))",
            "=SUM(BYCOL({1},LAMBDA(col,AsyncEcho(SUM(col))+2)))",
            "=SUM(MAKEARRAY(1,1,LAMBDA(r,c,AsyncEcho(r)+c+1)))",
        ] {
            let (mut server, b, counts) = configured(formula);
            server
                .execute_current(Request::Intent(Intent::Recalculate), None)
                .reply
                .unwrap();
            for _ in 0..100 {
                if !server.has_pending_calculation() {
                    break;
                }
                server.advance_calculation();
            }
            assert!(!server.has_pending_calculation());
            let token = server.book().host().pending_async()[0].clone();
            let before = server.take_events();
            assert!(before.iter().any(|e|matches!(e,Event::Calculation{receipt:wire::CalculationEvent::AwaitingAsync{token:id,..},..}if *id==token.id)));
            assert!(!before.iter().any(|e|matches!(e,Event::Calculation{receipt:wire::CalculationEvent::Completed{domain,command,..},..}if *domain==token.invocation.pass.command.domain_incarnation && *command==token.invocation.pass.command.sequence)));
            assert_eq!(
                server.complete_async(&token, calc_value::CalcValue::number(40.0)),
                calc_host::AsyncDisposition::Accepted
            );
            assert_eq!(counts.borrow().suffix, 0);
            let returned = server.take_events();
            assert!(returned.iter().any(|e|matches!(e,Event::Calculation{receipt:wire::CalculationEvent::AsyncReturned{token:id,disposition:wire::AsyncReturnDisposition::Accepted},..}if *id==token.id)));
            for _ in 0..100 {
                if !server.has_pending_calculation() {
                    break;
                }
                server.advance_calculation();
            }
            let after = server.take_events();
            assert!(after.iter().any(|e|matches!(e,Event::Calculation{receipt:wire::CalculationEvent::Completed{domain,command,outcome:wire::CalculationOutcome::Complete,calculated:1,required:1,..},..}if *domain==token.invocation.pass.command.domain_incarnation && *command==token.invocation.pass.command.sequence)));
            assert_eq!((counts.borrow().prefix, counts.borrow().suffix), (1, 1));
            let value = calc_host::project(
                server.book().host(),
                &calc_host::ProjectionRequest { cells: vec![b] },
            )
            .unwrap();
            assert!(
                matches!(value.cells()[0].value(),calc_host::ProjectedValue::Calculated{value,..}if value.as_number()==Some(42.0))
            );
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p07_worker_calculated_save_delivers_only_the_pinned_bytes() {
        let (mut server, b, counts) = configured("=LET(x,2,IF(TRUE,AsyncEcho(1)+x,0))");
        let a = CellKey::new(b.sheet(), 1, 1).unwrap();
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(a),
                    text: "1".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        server.take_events();
        let revision = server.book.host().revision().get();
        let request = server.envelope(Request::SaveCalculated);
        let first = server.handle(request.clone(), None);
        let retry = server.handle(request, None);
        assert_eq!(wire::encode(&first.reply), wire::encode(&retry.reply));
        let token = match first.reply.unwrap() {
            Reply::SavePending { token, .. } => token,
            reply => panic!("{reply:?}"),
        };
        assert_eq!(counts.borrow().prefix, 0);
        drain(&mut server);
        assert!(
            !server.has_pending_calculation(),
            "a waiting save posts no polling task"
        );
        assert!(server.book.host().pending_async().is_empty());
        let pending = server.pending_async();
        assert_eq!(pending.len(), 1);
        let provider = pending[0].clone();
        assert!(matches!(
            provider.destination,
            calc_host::ResultDestination::PinnedSave { .. }
        ));
        assert!(
            !server.take_events().iter().any(|event| matches!(
                event,
                Event::Calculation {
                    receipt: wire::CalculationEvent::Admitted { .. }
                        | wire::CalculationEvent::Published { .. }
                        | wire::CalculationEvent::Completed { .. },
                    ..
                }
            )),
            "save receipts never enter the live publication stream"
        );
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(a),
                    text: "9".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        assert_eq!(
            server.complete_async(&provider, calc_value::CalcValue::number(40.0)),
            calc_host::AsyncDisposition::Accepted
        );
        assert_eq!(counts.borrow().suffix, 0);
        drain(&mut server);
        let request = server.envelope(Request::PollSave { token });
        let answer = server.handle(request.clone(), None);
        assert!(
            matches!(answer.reply, Ok(Reply::SaveImage { model_revision, .. }) if model_revision == revision)
        );
        let retry = server.handle(request, None);
        assert_eq!(wire::encode(&answer.reply), wire::encode(&retry.reply));
        assert_eq!(answer.bytes, retry.bytes);
        assert!(
            server.book.changed(),
            "an older image cannot clear later live edits"
        );
        assert_eq!(
            server.complete_async(&provider, calc_value::CalcValue::number(40.0)),
            calc_host::AsyncDisposition::Duplicate
        );
        assert_eq!((counts.borrow().prefix, counts.borrow().suffix), (1, 1));
        let reopened = Workbook::open("pinned.xlsx", answer.bytes.unwrap()).unwrap();
        let s = reopened.host().sheets()[0].id();
        assert_eq!(
            reopened
                .shown(&reopened.cell(CellKey::new(s, 1, 1).unwrap()).unwrap())
                .text,
            "1"
        );
        let cell = reopened.cell(CellKey::new(s, 1, 2).unwrap()).unwrap();
        assert!(
            matches!(cell.value(), calc_host::ProjectedValue::FileCached(_)),
            "a reopened cache is never calculated provenance"
        );
        assert_eq!(reopened.shown(&cell).text, "42");
    }
}
#[cfg(test)]
mod friendly_formula_tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn friendly_sources_share_worker_projections_and_preserve_xlsx_syntax() {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let at = CellKey::new(sheet, 1, 1).unwrap();
        let raw = "=_xlfn.SEQUENCE(2,2)";
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(at),
                    text: raw.into(),
                }),
                None,
            )
            .reply
            .unwrap();
        for _ in 0..100 {
            if !server.has_pending_calculation() {
                break;
            }
            server.advance_calculation();
        }
        let projected = server.book().cell(at).unwrap();
        assert_eq!(cells::input_text(projected.input()), raw);
        assert_eq!(calc_host::search::entry_text(projected.input()), raw);
        let captured = match server
            .execute_current(
                Request::Capture {
                    area: Area::of(GridArea::cell(at)),
                    keep: None,
                },
                None,
            )
            .reply
            .unwrap()
        {
            Reply::Clipboard(c) => {
                assert_eq!(c.text, "1");
                c
            }
            other => panic!("{other:?}"),
        };
        let pasted = CellKey::new(sheet, 1, 5).unwrap();
        let preview = match server
            .execute_current(
                Request::PreparePaste {
                    source: wire::PasteSource::Internal {
                        token: captured.token,
                        cut: false,
                    },
                    to: Area::of(GridArea::cell(pasted)),
                    mode: wire::PasteMode::Formulas,
                    transpose: false,
                },
                None,
            )
            .reply
            .unwrap()
        {
            Reply::PastePreview(p) => p,
            other => panic!("{other:?}"),
        };
        server
            .execute_current(
                Request::Intent(Intent::ApplyPaste {
                    id: preview.id,
                    select_target: false,
                }),
                None,
            )
            .reply
            .unwrap();
        assert_eq!(
            cells::input_text(server.book().host().model().cell(pasted)),
            raw
        );
        match server
            .execute_current(
                Request::Find(wire::FindOptions {
                    sheet: Sheet::of(sheet),
                    areas: Vec::new(),
                    text: "_xlfn.SEQUENCE".into(),
                    case_sensitive: true,
                    whole: false,
                    wildcards: false,
                    formulas: true,
                    scope: wire::FindScope::Sheet,
                }),
                None,
            )
            .reply
            .unwrap()
        {
            Reply::Found(Ok(found)) => {
                assert_eq!(found.rows.len(), 2);
                assert!(found.rows.iter().all(|r| r.source == "=SEQUENCE(2,2)"));
            }
            other => panic!("{other:?}"),
        }
        let shown = wire::cell_of(
            &server.book().shown(&projected),
            cells::input_text(projected.input()),
            cells::provenance_text(server.book().host(), &projected),
            &projected,
        );
        assert_eq!(shown.input, "=SEQUENCE(2,2)");
        assert_eq!(shown.text, "1");
        assert_eq!(shown.provenance, cells::Provenance::Calculated);
        match server
            .execute_current(
                Request::Notebook {
                    sheet: Sheet::of(sheet),
                    page: 0,
                    reveal: Some(Key::of(at)),
                    placement: None,
                },
                None,
            )
            .reply
            .unwrap()
        {
            Reply::Notebook(n) => assert_eq!(n.blocks[0].cell.as_ref().unwrap().input, shown.input),
            other => panic!("{other:?}"),
        }
        match server
            .execute_current(
                Request::Canvas {
                    center: Key::of(at),
                    back: 1,
                    forward: 1,
                },
                None,
            )
            .reply
            .unwrap()
        {
            Reply::Canvas(c) => {
                assert_eq!(c.inspector.input, shown.input);
                let center = c
                    .chips
                    .iter()
                    .find(|chip| chip.cell == Some(Key::of(at)))
                    .expect("the trace includes its selected center cell");
                assert_eq!(center.input, shown.input);
            }
            other => panic!("{other:?}"),
        }
        match server
            .execute_current(
                Request::Ledger {
                    sheet: Some(Sheet::of(sheet)),
                    filter: "all".into(),
                    text: String::new(),
                    sort: "address".into(),
                    page: 0,
                    active: Some(Key::of(at)),
                },
                None,
            )
            .reply
            .unwrap()
        {
            Reply::Ledger(l) => assert_eq!(l.page[0].formula, shown.input),
            other => panic!("{other:?}"),
        }
        let saved = server
            .execute_current(Request::Save, None)
            .bytes
            .expect("actual xlsx bytes");
        let mut reopened = Server::new();
        reopened
            .execute_current(
                Request::Open {
                    name: "Book1.xlsx".into(),
                },
                Some(saved),
            )
            .reply
            .unwrap();
        let sheet = reopened.book().sheets()[0].id;
        let at = CellKey::new(sheet, 1, 1).unwrap();
        assert_eq!(
            cells::input_text(reopened.book().host().model().cell(at)),
            raw
        );
        assert_eq!(
            cells::display_input(reopened.book().host().model().cell(at)),
            shown.input
        );
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    /// The skin's Save sends no setting: the worker uses the workbook's CalculateBeforeSave
    /// (true for a file silent about `calcOnSave`) in Manual mode, and an explicit setting wins.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_1nbp_14__skin_save_uses_the_workbooks_calculate_before_save_in_manual_mode() {
        for (setting, calculates) in [(None, true), (Some(true), true), (Some(false), false)] {
            let mut server = Server::new();
            let s = server.book.host().sheets()[0].id();
            server.book_mut().dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
            for (address, text) in [(1, "5"), (2, "=A1*2")] {
                assert!(
                    server
                        .book_mut()
                        .dispatch(SkinIntent::EnterCell {
                            at: CellKey::new(s, address, 1).unwrap(),
                            text: text.into()
                        })
                        .accepted()
                );
            }
            let answer = server.execute_current(
                Request::SaveWithContext {
                    calculate_before_save: setting,
                    views: None,
                },
                None,
            );
            let mut reply = answer.reply;
            let mut bytes = answer.bytes;
            for _ in 0..1000 {
                if bytes.is_some() {
                    break;
                }
                let Ok(Reply::SavePending { token, .. }) = reply else {
                    panic!("{reply:?}");
                };
                server.advance_calculation();
                let next = server.execute_current(Request::PollSave { token }, None);
                reply = next.reply;
                bytes = next.bytes;
            }
            let bytes = bytes.expect("the save finishes");
            let mut reopened = Server::new();
            assert!(
                reopened
                    .execute_current(Request::Open { name: "b.xlsx".into() }, Some(bytes.clone()))
                    .reply
                    .is_ok()
            );
            assert_eq!(reopened.book().host().calculate_before_save(), calculates, "{setting:?}");
            let r = reopened.book().host().sheets()[0].id();
            let shown = calc_host::project(
                reopened.book().host(),
                &calc_host::ProjectionRequest {
                    cells: vec![CellKey::new(r, 2, 1).unwrap()],
                },
            )
            .unwrap();
            // The file's cache of A2 is 10 only if the save calculated before writing.
            let cached = matches!(
                shown.cells()[0].value(),
                calc_host::ProjectedValue::FileCached(cache)
                    if format!("{:?}", cache.value()).contains("10.0")
            );
            assert_eq!(cached, calculates, "{setting:?}");
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn window_save_converts_zoomed_offsets_and_pins_the_ordinary_save_route() {
        for zoom in [0.1, 0.25, 0.5, 1., 1.75, 4.] {
            let mut server = Server::new();
            let s = server.book.host().sheets()[0].id();
            let sheet = Sheet::of(s);
            let geometry = server.geometry(sheet);
            let m = dna_calc_model::geometry::sheet_metrics(Some(&geometry));
            let request = Request::SaveWithContext {
                calculate_before_save: Some(false),
                views: Some(wire::SaveWindow {
                    revision: server.book.host().revision().get(),
                    active_sheet: sheet,
                    sheet_view_modes: Vec::new(),
                    sheets: vec![wire::SheetWindow {
                        geometry: geometry.clone(),
                        sheet,
                        active: (101, 12),
                        selection: Some((101, 12, 103, 14)),
                        active_pane: 0,
                        panes: vec![wire::PaneWindow {
                            index: 0,
                            x: m.col_start(10) * zoom,
                            y: m.row_start(100) * zoom,
                            zoom,
                        }],
                    }],
                }),
            };
            let before = (
                server.book.host().revision(),
                server.book.session().history_depth(),
            );
            let answer = server.execute_current(request, None);
            assert!(answer.reply.is_ok(), "{:?}", answer.reply);
            let bytes = answer
                .bytes
                .expect("ordinary save returns the prepared image");
            assert_eq!(
                before,
                (
                    server.book.host().revision(),
                    server.book.session().history_depth()
                )
            );
            let mut reopened = Server::new();
            assert!(
                reopened
                    .execute_current(
                        Request::Open {
                            name: "view.xlsx".into()
                        },
                        Some(bytes)
                    )
                    .reply
                    .is_ok()
            );
            let s = reopened.book.host().sheets()[0].id();
            let view = reopened.book.host().sheet_view(s).unwrap();
            assert_eq!(view.top_left, Some((100, 10)));
            assert_eq!(view.active_cell, Some((101, 12)));
            assert_eq!(view.selection, Some((101, 12, 103, 14)));
            assert_eq!(reopened.summary().initial_selection, Some((101, 12, 103, 14)));
            assert_eq!(view.zoom, Some((zoom * 100.).round() as u16));
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn worksheet_view_mode_saves_without_a_mounted_grid_and_reopens() {
        let mut server = Server::new();
        let sheet = Sheet::of(server.book.host().sheets()[0].id());
        for mode in [
            wire::SheetViewMode::PageBreakPreview,
            wire::SheetViewMode::PageLayout,
            wire::SheetViewMode::Normal,
        ] {
            let saved = server.execute_current(
                Request::SaveWithContext {
                    calculate_before_save: Some(false),
                    views: Some(wire::SaveWindow {
                        revision: server.book.host().revision().get(),
                        active_sheet: sheet,
                        sheets: Vec::new(),
                        sheet_view_modes: vec![(sheet, mode)],
                    }),
                },
                None,
            );
            assert!(saved.reply.is_ok(), "{:?}", saved.reply);
            let reopened = server.execute_current(
                Request::Open { name: "view.xlsx".into() },
                saved.bytes,
            );
            assert!(reopened.reply.is_ok(), "{:?}", reopened.reply);
            let expected = if mode == wire::SheetViewMode::Normal {
                Vec::new()
            } else {
                vec![(sheet, mode)]
            };
            assert_eq!(server.summary().sheet_view_modes, expected);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn window_save_refuses_stale_geometry_and_converts_frozen_offsets() {
        let mut server = Server::new();
        let s = server.book.host().sheets()[0].id();
        let sheet = Sheet::of(s);
        let old = server.geometry(sheet);
        assert!(
            server
                .book
                .dispatch(SkinIntent::Command(
                    calc_host::WorkbookCommand::FreezePanes {
                        sheet: s,
                        rows: 2,
                        columns: 1
                    }
                ))
                .accepted()
        );
        let input = wire::SaveWindow {
            revision: server.book.host().revision().get(),
            active_sheet: sheet,
            sheet_view_modes: Vec::new(),
            sheets: vec![wire::SheetWindow {
                geometry: old,
                sheet,
                active: (101, 12),
                selection: None,
                active_pane: 0,
                panes: vec![wire::PaneWindow {
                    index: 0,
                    x: 0.,
                    y: 0.,
                    zoom: 2.,
                }],
            }],
        };
        let refused = server.execute_current(
            Request::SaveWithContext {
                calculate_before_save: Some(false),
                views: Some(input.clone()),
            },
            None,
        );
        assert!(refused.bytes.is_none());
        assert!(
            refused
                .reply
                .unwrap_err()
                .contains("view is still updating")
        );
        let g = server.geometry(sheet);
        let m = dna_calc_model::geometry::sheet_metrics(Some(&g));
        let mut valid = input;
        valid.sheets[0].geometry = g;
        valid.sheets[0].panes[0].x = (m.col_start(10) - m.col_start(2)) * 2.;
        valid.sheets[0].panes[0].y = (m.row_start(100) - m.row_start(3)) * 2.;
        let answer = server.execute_current(
            Request::SaveWithContext {
                calculate_before_save: Some(false),
                views: Some(valid),
            },
            None,
        );
        let bytes = answer.bytes.unwrap();
        let mut reopened = Server::new();
        reopened
            .execute_current(
                Request::Open {
                    name: "frozen.xlsx".into(),
                },
                Some(bytes),
            )
            .reply
            .unwrap();
        let view = reopened
            .book
            .host()
            .sheet_view(reopened.book.host().sheets()[0].id())
            .unwrap();
        assert_eq!(view.top_left, Some((1, 1)));
        assert_eq!(view.pane_top_left, Some((100, 10)));
        assert_eq!(view.active_pane, 3);
        assert_eq!(view.zoom, Some(200));
    }
}

#[cfg(test)]
mod template_save_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__template_basename_counters_survive_replacement_without_counting_edits_or_failures()
    {
        let mut source = Server::new();
        let sheet = source.book().sheets()[0].id;
        for (column, text) in [(1, "6"), (2, "=A1*3")] {
            source
                .execute_current(
                    Request::Intent(Intent::EnterCell {
                        at: Key::of(CellKey::new(sheet, 1, column).unwrap()),
                        text: text.into(),
                    }),
                    None,
                )
                .reply
                .unwrap();
        }
        let plain = source
            .execute_current(
                Request::SaveAs {
                    name: "Budget.xltx".into(),
                    format: wire::FileFormat::Xltx,
                    views: None,
                },
                None,
            )
            .bytes
            .unwrap();
        let macros = source
            .execute_current(
                Request::SaveAs {
                    name: "Budget.xltm".into(),
                    format: wire::FileFormat::Xltm,
                    views: None,
                },
                None,
            )
            .bytes
            .unwrap();
        let mut server = Server::new();
        let open = |server: &mut Server,
                    request: Request,
                    bytes: Vec<u8>,
                    expected: &str,
                    unsaved: bool| {
            let answer = server.execute_current(request, Some(bytes));
            assert!(
                matches!(
                    answer.reply,
                    Ok(Reply::Done {
                        outcome: Outcome { accepted: true, .. },
                        ..
                    })
                ),
                "{:?}",
                answer.reply
            );
            assert_eq!(server.book().name(), expected);
            assert_eq!(server.book().save_name(), expected);
            assert_eq!(
                server.book().host().model().definitions().workbook_name(),
                Some(expected)
            );
            assert_eq!(server.book().unsaved(), unsaved);
            let at = CellKey::new(server.book().sheets()[0].id, 1, 2).unwrap();
            assert_eq!(
                server.book().shown(&server.book().cell(at).unwrap()).text,
                "18"
            );
        };
        open(
            &mut server,
            Request::Open {
                name: "Budget.xltx".into(),
            },
            plain.clone(),
            "Budget1.xlsx",
            true,
        );
        let before = server.summary();
        let failed = server.execute_current(
            Request::Open {
                name: "Budget.xltx".into(),
            },
            Some(vec![0, 1, 2]),
        );
        assert!(matches!(
            failed.reply,
            Ok(Reply::Done {
                outcome: Outcome {
                    accepted: false,
                    ..
                },
                ..
            })
        ));
        assert_eq!(server.summary(), before);
        open(
            &mut server,
            Request::OpenForEditing {
                name: "Budget.xltx".into(),
            },
            plain.clone(),
            "Budget.xltx",
            false,
        );
        server
            .execute_current(Request::InspectTemplate, Some(plain.clone()))
            .reply
            .unwrap();
        open(
            &mut server,
            Request::OpenTemplate {
                name: "C:\\Templates\\budget.XLTX".into(),
            },
            plain.clone(),
            "budget2.xlsx",
            true,
        );
        open(
            &mut server,
            Request::Open {
                name: "Forecast.xltx".into(),
            },
            plain.clone(),
            "Forecast1.xlsx",
            true,
        );
        server.execute_current(Request::New, None).reply.unwrap();
        open(
            &mut server,
            Request::Open {
                name: "/templates/Budget.xltm".into(),
            },
            macros,
            "Budget3.xlsm",
            true,
        );
        server
            .execute_current(
                Request::SaveAs {
                    name: "Chosen.xlsm".into(),
                    format: wire::FileFormat::Xlsm,
                    views: None,
                },
                None,
            )
            .reply
            .unwrap();
        assert_eq!(server.book().name(), "Chosen.xlsm");
        assert!(!server.book().unsaved());
        open(
            &mut server,
            Request::Open {
                name: "Budget.xltx".into(),
            },
            plain.clone(),
            "Budget4.xlsx",
            true,
        );
        let mut other = Server::new();
        open(
            &mut other,
            Request::Open {
                name: "Budget.xltx".into(),
            },
            plain,
            "Budget1.xlsx",
            true,
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__inspection_is_read_only_and_insertion_keeps_the_worker_session() {
        let mut source = Server::new();
        let sheet = source.book().sheets()[0].id;
        source
            .execute_current(
                Request::Intent(wire::Intent::EnterCell {
                    at: Key::of(CellKey::new(sheet, 1, 1).unwrap()),
                    text: "6".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        source
            .execute_current(
                Request::Intent(wire::Intent::EnterCell {
                    at: Key::of(CellKey::new(sheet, 1, 2).unwrap()),
                    text: "=A1*3".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        let template = source
            .execute_current(
                Request::SaveAs {
                    name: "Test.xltx".into(),
                    format: wire::FileFormat::Xltx,
                    views: None,
                },
                None,
            )
            .bytes
            .unwrap();
        let mut server = Server::new();
        let before = server.summary();
        assert!(matches!(
            server
                .execute_current(Request::InspectTemplate, Some(template.clone()))
                .reply,
            Ok(Reply::TemplateFormat(wire::FileFormat::Xltx))
        ));
        assert!(
            server
                .execute_current(Request::InspectTemplate, Some(calc_host::blank_package()))
                .reply
                .is_err()
        );
        assert_eq!(server.summary(), before);
        let result = server.execute_current(
            Request::InsertTemplate {
                name: None,
                revision: before.revision,
            },
            Some(template),
        );
        assert!(
            matches!(
                result.reply,
                Ok(Reply::Done {
                    outcome: Outcome { accepted: true, .. },
                    ..
                })
            ),
            "{:?}",
            result.reply
        );
        let after = server.summary();
        assert_eq!(after.session, before.session);
        assert_eq!(after.workbook, before.workbook);
        assert_eq!(after.undo, before.undo + 1);
        assert_eq!(after.sheets.len(), 2);
        let sheet = after.sheets[1].0.id();
        assert_eq!(
            server
                .book()
                .shown(
                    &server
                        .book()
                        .cell(CellKey::new(sheet, 1, 2).unwrap())
                        .unwrap()
                )
                .text,
            "18"
        );
        server
            .execute_current(Request::Intent(wire::Intent::Undo), None)
            .reply
            .unwrap();
        assert_eq!(server.summary().sheets.len(), 1);
        server
            .execute_current(Request::Intent(wire::Intent::Redo), None)
            .reply
            .unwrap();
        assert_eq!(server.summary().sheets[1].0.id(), sheet);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__save_as_updates_actual_reference_identity_and_undo_retains_it() {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        let at = |r, c| Key::of(CellKey::new(sheet, r, c).unwrap());
        server
            .execute_current(
                Request::Intent(wire::Intent::EnterCell {
                    at: at(1, 1),
                    text: "42".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        server
            .execute_current(
                Request::Intent(wire::Intent::EditSheet {
                    sheet: None,
                    change: wire::SheetChange::Add {
                        name: "Extra".into(),
                    },
                }),
                None,
            )
            .reply
            .unwrap();
        let reply = server.execute_current(
            Request::SaveAs {
                name: "Budget.xltm".into(),
                format: wire::FileFormat::Xltm,
                views: None,
            },
            None,
        );
        assert!(reply.bytes.is_some());
        assert!(matches!(
            reply.reply,
            Ok(Reply::Done { saved: Some(_), .. })
        ));
        assert_eq!(
            server.book().host().model().definitions().workbook_name(),
            Some("Budget.xltm")
        );
        assert!(server.book().host().model().definitions().macro_enabled());
        server
            .execute_current(
                Request::Intent(wire::Intent::EnterCell {
                    at: at(1, 2),
                    text: "=INDIRECT(\"[Budget.xltm]Sheet1!A1\")".into(),
                }),
                None,
            )
            .reply
            .unwrap();
        assert_eq!(
            server
                .book()
                .shown(
                    &server
                        .book()
                        .cell(CellKey::new(sheet, 1, 2).unwrap())
                        .unwrap()
                )
                .text,
            "42"
        );
        // A structural snapshot undo must not restore an obsolete file identity.
        server
            .execute_current(Request::Intent(wire::Intent::Undo), None)
            .reply
            .unwrap();
        server
            .execute_current(Request::Intent(wire::Intent::Undo), None)
            .reply
            .unwrap();
        assert_eq!(server.book().sheets().len(), 1);
        assert_eq!(
            server.book().host().model().definitions().workbook_name(),
            Some("Budget.xltm")
        );
        assert!(server.book().host().model().definitions().macro_enabled());
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__macro_loss_answers_with_a_terminal_refusal_and_no_save_token() {
        let mut server = Server::new();
        server
            .execute_current(
                Request::Open {
                    name: "local.xlsm".into(),
                },
                Some(
                    include_bytes!(
                        "../../../crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm"
                    )
                    .to_vec(),
                ),
            )
            .reply
            .unwrap();
        let revision = server.book().host().revision();
        let history = server.book().session().history_depth();
        let answer = server.execute_current(
            Request::SaveAs {
                name: "bad.xltx".into(),
                format: wire::FileFormat::Xltx,
                views: None,
            },
            None,
        );
        assert!(answer.bytes.is_none());
        assert!(
            matches!(answer.reply,Ok(Reply::Done{outcome:Outcome{accepted:false,error:Some(ref e),..},saved:None,..}) if e.contains("would be lost"))
        );
        assert!(server.pending_save.is_none());
        assert!(server.save_as_target.is_none());
        assert_eq!(server.book().name(), "local.xlsm");
        assert_eq!(server.book().host().revision(), revision);
        assert_eq!(server.book().session().history_depth(), history);
    }
}

#[cfg(test)]
mod template_pending_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__save_as_retires_old_calculation_before_rebinding_file_identity() {
        let mut server = Server::new();
        let sheet = server.book().sheets()[0].id;
        for (c, text) in [
            (1, "42"),
            (2, "=INDIRECT(\"[NewName.xlsx]Sheet1!A1\")"),
            (3, "=B1+1"),
            (4, "=C1+1"),
        ] {
            server
                .execute_current(
                    Request::Intent(wire::Intent::EnterCell {
                        at: Key::of(CellKey::new(sheet, 1, c).unwrap()),
                        text: text.into(),
                    }),
                    None,
                )
                .reply
                .unwrap();
        }
        server.set_calculation_budget(calc_host::WorkBudget::evaluations(1));
        server
            .execute_current(Request::Intent(wire::Intent::Recalculate), None)
            .reply
            .unwrap();
        assert!(server.book().has_pending_calculation());
        server.take_events();
        let answer = server.execute_current(
            Request::SaveAs {
                name: "NewName.xlsx".into(),
                format: wire::FileFormat::Xlsx,
                views: None,
            },
            None,
        );
        assert!(
            matches!(
                answer.reply,
                Ok(Reply::Done {
                    outcome: Outcome { accepted: true, .. },
                    saved: Some(_),
                    ..
                })
            ),
            "{:?}",
            answer.reply
        );
        assert!(answer.bytes.is_some());
        let mut events = server.take_events();
        for _ in 0..100 {
            if !server.has_pending_calculation() {
                break;
            }
            server.advance_calculation();
            events.extend(server.take_events());
        }
        assert!(!server.has_pending_calculation());
        assert_eq!(
            server
                .book()
                .shown(
                    &server
                        .book()
                        .cell(CellKey::new(sheet, 1, 4).unwrap())
                        .unwrap()
                )
                .text,
            "44"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Calculation {
                    receipt: wire::CalculationEvent::Completed {
                        outcome: wire::CalculationOutcome::Cancelled,
                        ..
                    },
                    ..
                }
            )),
            "{events:?}"
        );
    }
}

#[cfg(test)]
mod printing_tests {
    use super::*;
    use dna_calc_model::printing::{PageBreak, PageSetupChange};
    fn state(server: &mut Server, sheet: Sheet) -> dna_calc_model::printing::PageSetupState {
        let Reply::PageSetup(Ok(s)) = server
            .execute_current(Request::PageSetup(sheet), None)
            .reply
            .unwrap()
        else {
            panic!("Page Setup reply")
        };
        s
    }
    fn preview(
        server: &mut Server,
        sheet: Sheet,
    ) -> Result<dna_calc_model::printing::PrintDocument, String> {
        let revision = server.book.host().revision().get();
        let Reply::PrintPreview(p) = server
            .execute_current(Request::PrintPreview { sheet, revision }, None)
            .reply
            .unwrap()
        else {
            panic!("Print preview reply")
        };
        p
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_3__print_preview_carries_the_shapes_and_the_pictures_images() {
        let mut s = Server::new();
        // pictures-paint holds a bare EMF and WMF: printing it is refused, not printed without them.
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/pictures-paint.xlsx");
        assert!(s.execute_current(Request::Open { name: "pictures-paint.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = Sheet::of(s.book.host().sheets()[0].id());
        assert!(preview(&mut s, sheet).unwrap_err().contains("EMF or WMF"));
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/shapes-paint3.xlsx");
        assert!(s.execute_current(Request::Open { name: "shapes-paint3.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = Sheet::of(s.book.host().sheets()[0].id());
        let document = preview(&mut s, sheet).unwrap();
        let Reply::Shapes(drawn) = s.execute_current(Request::Shapes(sheet), None).reply.unwrap() else { panic!("shapes") };
        // The same objects the grid draws, and an image for every picture the page can decode (not the bare EMF/WMF).
        assert_eq!(document.shapes, drawn);
        let decodable = drawn.iter().filter(|d| d.picture.as_ref().is_some_and(|p| p.placeholder.is_none())).count();
        // Pictures inside groups too.
        assert!(decodable >= 1 && document.pictures.len() > decodable, "{decodable} {}", document.pictures.len());
        assert!(document.pictures.iter().all(|(_, url)| url.starts_with("data:image/png;base64,iVBORw0KGgo")));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_zjsg_8__a_sheet_with_form_controls_prints_them_as_the_grid_draws_them() {
        let mut s = Server::new();
        let data = include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/form-controls.xlsx");
        assert!(s.execute_current(Request::Open { name: "form-controls.xlsx".into() }, Some(data.to_vec())).reply.is_ok());
        let sheet = Sheet::of(s.book.host().sheets()[0].id());
        let document = preview(&mut s, sheet).unwrap();
        let Reply::Controls(drawn) = s.execute_current(Request::Controls(sheet), None).reply.unwrap() else { panic!("controls") };
        // Every control prints (none states print="0"), in the state the grid shows.
        assert_eq!(document.controls, drawn);
        assert_eq!(document.controls.len(), 12);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn page_setup_and_print_preview_use_the_host_values_history_and_saved_document() {
        let mut s = Server::new();
        let sheet = Sheet::of(s.book.host().sheets()[0].id());
        for (row, col, text) in [(1, 1, "6"), (1, 2, "=A1*3"), (60, 4, "end")] {
            s.execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key(sheet.0, sheet.1, row, col),
                    text: text.into(),
                }),
                None,
            )
            .reply
            .unwrap();
        }
        let old = state(&mut s, sheet);
        let change = PageSetupChange {
            attributes: vec![
                (
                    "pageSetup".into(),
                    "orientation".into(),
                    Some("portrait".into()),
                ),
                ("pageSetup".into(), "paperSize".into(), Some("1".into())),
            ],
            print_area: Some("A1:D60".into()),
            print_titles: Some("1:2".into()),
            headers: vec![("oddHeader".into(), "&CPage &P of &N".into())],
            row_breaks: Some(vec![PageBreak {
                id: 20,
                min: 0,
                max: 16383,
                manual: true,
                pivot: false,
            }]),
            ..Default::default()
        };
        let reply = s
            .execute_current(
                Request::Intent(Intent::PageSetup {
                    sheet,
                    revision: old.revision,
                    change,
                }),
                None,
            )
            .reply
            .unwrap();
        assert!(
            matches!(
                reply,
                Reply::Done {
                    outcome: Outcome { accepted: true, .. },
                    ..
                }
            ),
            "{reply:?}"
        );
        let settings = state(&mut s, sheet);
        let doc = preview(&mut s, sheet).unwrap();
        assert_eq!(doc.plan.pages.len(), 2);
        assert_eq!(doc.plan.pages[0].body.1.2, 20);
        assert_eq!(doc.plan.pages[1].regions[0].area.1.2, 2);
        let b1 = doc
            .cells
            .iter()
            .find(|c| c.at.2 == 1 && c.at.3 == 2)
            .unwrap();
        assert_eq!(b1.text, "18");
        assert_eq!(b1.provenance, wire::Provenance::Calculated);
        let Reply::PrintPreview(Err(stale)) = s
            .execute_current(
                Request::PrintPreview {
                    sheet,
                    revision: old.revision,
                },
                None,
            )
            .reply
            .unwrap()
        else {
            panic!("Stale preview must refuse")
        };
        assert!(stale.contains("changed"));
        s.execute_current(Request::Intent(Intent::Undo), None)
            .reply
            .unwrap();
        assert_eq!(state(&mut s, sheet).settings, old.settings);
        s.execute_current(Request::Intent(Intent::Redo), None)
            .reply
            .unwrap();
        assert_eq!(state(&mut s, sheet).settings, settings.settings);
        let bytes = s.execute_current(Request::Save, None).bytes.unwrap();
        let mut reopened = Server::new();
        reopened
            .execute_current(
                Request::Open {
                    name: "Layout.xlsx".into(),
                },
                Some(bytes),
            )
            .reply
            .unwrap();
        let reopened_sheet = Sheet::of(reopened.book.host().sheets()[0].id());
        let saved = state(&mut reopened, reopened_sheet);
        assert_eq!(saved.settings, settings.settings);
        assert_eq!(saved.print_area, settings.print_area);
        assert_eq!(saved.print_titles, settings.print_titles);
        assert_eq!(
            preview(&mut reopened, reopened_sheet).unwrap().plan.pages,
            doc.plan.pages
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn print_preview_refuses_stale_values_and_large_areas_before_expansion() {
        let mut s = Server::new();
        let sheet = Sheet::of(s.book.host().sheets()[0].id());
        s.execute_current(Request::Intent(Intent::SetCalcMode { manual: true }), None)
            .reply
            .unwrap();
        s.execute_current(
            Request::Intent(Intent::EnterCell {
                at: Key(sheet.0, sheet.1, 1, 1),
                text: "=1+2".into(),
            }),
            None,
        )
        .reply
        .unwrap();
        assert!(preview(&mut s, sheet).unwrap_err().contains("uncalculated"));
        s.execute_current(Request::Intent(Intent::Recalculate), None)
            .reply
            .unwrap();
        assert_eq!(preview(&mut s, sheet).unwrap().cells[0].text, "3");
        let revision = s.book.host().revision().get();
        let change = PageSetupChange {
            print_area: Some("A1:XFD1048576".into()),
            ..Default::default()
        };
        s.execute_current(
            Request::Intent(Intent::PageSetup {
                sheet,
                revision,
                change,
            }),
            None,
        )
        .reply
        .unwrap();
        assert!(preview(&mut s, sheet).unwrap_err().contains("200 pages"));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn print_preview_preserves_circular_held_values_after_reopen() {
        // Excel 16.0.20527.20196, ww2 print-holds-active: iteration-off circular cells
        // displayed 12 and 11 after reopen and PDF export.
        let mut s = Server::new();
        let sheet = Sheet::of(s.book.host().sheets()[0].id());
        assert!(!s.book.host().recalc_context().iteration);
        let enter = |s: &mut Server, col, text: &str| {
            let reply = s
                .execute_current(
                    Request::Intent(Intent::EnterCell {
                        at: Key(sheet.0, sheet.1, 1, col),
                        text: text.into(),
                    }),
                    None,
                )
                .reply
                .unwrap();
            assert!(matches!(reply, Reply::Done { outcome, .. } if outcome.accepted));
        };
        enter(&mut s, 1, "=IF(C1=0,12,B1+1)");
        enter(&mut s, 2, "=A1-1");
        enter(&mut s, 3, "1");

        let bytes = s.execute_current(Request::Save, None).bytes.unwrap();
        let mut reopened = Server::new();
        reopened
            .execute_current(
                Request::Open { name: "Circular.xlsx".into() },
                Some(bytes),
            )
            .reply
            .unwrap();
        let sheet = Sheet::of(reopened.book.host().sheets()[0].id());
        assert!(!reopened.book.host().recalc_context().iteration);
        for (col, expected) in [(1, 12.0f64), (2, 11.0f64)] {
            let at = CellKey::new(sheet.id(), 1, col).unwrap();
            let projected = reopened.book.cell(at).unwrap();
            let calc_host::ProjectedValue::Calculated { value, calculation } = projected.value()
            else {
                panic!("{at:?}: {:?}", projected.value());
            };
            assert!(calculation.held_by_circular_rule(), "{at:?}");
            let calc_value::CoreRef::Number(value) = value.core_ref() else {
                panic!("{at:?}: {:?}", projected.value());
            };
            assert_eq!(value.to_bits(), expected.to_bits(), "{at:?}");
        }
        let doc = preview(&mut reopened, sheet).unwrap();
        assert_eq!(
            doc.cells.iter().map(|cell| cell.text.as_str()).collect::<Vec<_>>(),
            ["12", "11", "1"],
        );
        assert!(doc.cells[..2].iter().all(|cell| cell.why.contains("held by a circular reference")));
        assert!(doc.notices.iter().any(|notice| notice.contains("held by circular references")));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn print_preview_uses_the_closed_workbook_link_cache() {
        use crate::workbook::zip_fixture_kit::{RawEntry, build_zip, parse_zip};

        // Excel 16.0.20527.20196, ww2 print-holds-active: the closed source changed
        // from 7 to 11, and the linked result remained 8 when updates were declined.
        // Reuse the retained external-link package with the probe's cache value and formula.
        let original =
            include_bytes!("../../../crates/calc-host/tests/fixtures/dc_ylr/oracle.xlsx");
        let archive = parse_zip(original);
        let entries = archive
            .entries
            .iter()
            .map(|part| {
                let payload = part.payload(original);
                let replacements = match part.name.as_str() {
                    "xl/externalLinks/externalLink1.xml" => Some((
                        r#"<cell r="A1"><v>1</v></cell>"#,
                        r#"<cell r="A1"><v>7</v></cell>"#,
                    )),
                    "xl/worksheets/sheet1.xml" => Some((
                        "<f>[1]Sheet1!A1</f><v>1</v>",
                        "<f>[1]Sheet1!A1+1</f><v>8</v>",
                    )),
                    _ => None,
                };
                if let Some((old, new)) = replacements {
                    let bytes = if part.method == 0 {
                        payload.to_vec()
                    } else {
                        miniz_oxide::inflate::decompress_to_vec(payload).unwrap()
                    };
                    let xml = String::from_utf8(bytes).unwrap();
                    assert_eq!(xml.matches(old).count(), 1, "{}", part.name);
                    RawEntry::stored(&part.name, xml.replace(old, new).as_bytes())
                } else if part.method == 0 {
                    RawEntry::stored(&part.name, payload)
                } else {
                    RawEntry::deflated(&part.name, payload, part.size, part.crc32)
                }
            })
            .collect::<Vec<_>>();
        let mut s = Server::new();
        s.execute_current(
            Request::Open {
                name: "ClosedLink.xlsx".into(),
            },
            Some(build_zip(&entries)),
        )
        .reply
        .unwrap();
        let sheet = Sheet::of(s.book.host().sheets()[0].id());
        let revision = state(&mut s, sheet).revision;
        let reply = s
            .execute_current(
                Request::Intent(Intent::PageSetup {
                    sheet,
                    revision,
                    change: PageSetupChange {
                        print_area: Some("A1".into()),
                        ..Default::default()
                    },
                }),
                None,
            )
            .reply
            .unwrap();
        assert!(matches!(reply, Reply::Done { outcome, .. } if outcome.accepted));
        let at = CellKey::new(sheet.id(), 1, 1).unwrap();
        let projected = s.book.cell(at).unwrap();
        let calc_host::ProjectedValue::Calculated { value, calculation } = projected.value() else {
            panic!("{at:?}: {:?}", projected.value());
        };
        assert_eq!(value.as_number().unwrap().to_bits(), 8.0f64.to_bits());
        assert!(calculation.reads_external_links());
        let doc = preview(&mut s, sheet).unwrap();
        assert_eq!(doc.cells.len(), 1);
        assert_eq!(doc.cells[0].text, "8");
        assert!(doc.cells[0].why.contains("saved external link cache"));
        assert!(
            doc.notices
                .iter()
                .any(|notice| notice.contains("external link caches"))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn print_preview_accepts_an_unsaved_added_sheet_and_waits_for_calculation() {
        let mut s = Server::new();
        let reply = s
            .execute_current(
                Request::Intent(Intent::EditSheet {
                    sheet: None,
                    change: wire::SheetChange::Add {
                        name: "Print this sheet".into(),
                    },
                }),
                None,
            )
            .reply
            .unwrap();
        assert!(
            matches!(
                reply,
                Reply::Done {
                    outcome: Outcome { accepted: true, .. },
                    ..
                }
            ),
            "{reply:?}"
        );
        let sheet = Sheet::of(s.book.host().sheets().last().unwrap().id());
        for (col, text) in [
            (1, "42"),
            (2, "=INDIRECT(\"A1\")+1"),
            (3, "=B1+1"),
            (4, "=C1+1"),
        ] {
            s.execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key(sheet.0, sheet.1, 1, col),
                    text: text.into(),
                }),
                None,
            )
            .reply
            .unwrap();
        }
        let doc = preview(&mut s, sheet).unwrap();
        assert_eq!(doc.sheet_name, "Print this sheet");
        assert_eq!(doc.cells.iter().find(|c| c.at.3 == 4).unwrap().text, "45");
        s.set_calculation_budget(calc_host::WorkBudget::evaluations(1));
        s.execute_current(Request::Intent(Intent::Recalculate), None)
            .reply
            .unwrap();
        assert!(s.book.has_pending_calculation());
        assert!(
            preview(&mut s, sheet)
                .unwrap_err()
                .contains("still running")
        );
        for _ in 0..100 {
            if !s.has_pending_calculation() {
                break;
            }
            s.advance_calculation();
            s.take_events();
        }
        assert!(!s.book.has_pending_calculation());
        assert_eq!(
            preview(&mut s, sheet)
                .unwrap()
                .cells
                .iter()
                .find(|c| c.at.3 == 4)
                .unwrap()
                .text,
            "45"
        );
    }
}
