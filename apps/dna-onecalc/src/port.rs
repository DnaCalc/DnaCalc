//! The message set between OneCalc's surface and its [`Session`].
//!
//! Today the browser surface calls the session in its own page. The desktop must run the session
//! on the native thread that owns add-in calls (XLL and COM calls need the STA), with the page in
//! WebView2 beside it. Both routes are one message set: authored state goes in, a calculation
//! view and the function names come out, and nothing else crosses. [`LocalPort`] answers in
//! process (the browser's route, and the test's); a bridge port answers over the WebView2 bridge
//! (milestone 2 of the OneCalc desktop note), and both are to be tested by the same cases.
//!
//! The vocabulary is the one authored files already use: an input is a cell address and an
//! entry under a kind (`number`, `text`, `logical`, `error`, `blank`), as in `FormulaInput`. A
//! reply carries text the surface would show, never a calc-func value, so a reply is plain data
//! that serialises. Nothing here reaches a calculation engine, a workbook or a document codec.
//!
//! A request that cannot be honoured is a `Refused` reply with the reason; the session is left as
//! it was.

use crate::display;
use crate::grid;
use crate::session::{InputValue, Session};
use calc_fml::{CalcEnvironment, DateSystem, FormatProfile, FormulaForm};
use serde::{Deserialize, Serialize};

/// What a surface asks of its session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// The formula text, with or without its leading `=`.
    SetFormula { text: String },
    /// The cell the formula is entered in.
    SetFormulaCell { address: String },
    /// Dynamic-array entry (`false`, as Excel 365) or legacy entry (`true`).
    SetForm { legacy: bool },
    /// One input: the cell, the kind of entry and the entry text.
    SetInput {
        address: String,
        kind: String,
        entry: String,
    },
    RemoveInput { address: String },
    ClearInputs,
    /// The locale profile (a culture name) and the clock (a serial date number) the date, time and
    /// number-format functions read. The clock comes from the owner of the page: a session has none,
    /// and without one those functions are refused.
    SetEnvironment { culture: String, now: Option<f64> },
    /// Compiles a VBA standard module and publishes its functions, replacing the session's
    /// callable table; a module that does not compile changes nothing.
    LoadVba { name: String, source: String },
    /// The user functions the session can call, by name.
    Functions,
    /// Calculates the formula over the inputs.
    Calculate,
}

/// What a calculation gave, as text for people.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalculationView {
    /// The value as a cell shows it; `None` when diagnostics stopped the calculation.
    pub value: Option<String>,
    /// Why there is no value, or what a user function reported.
    pub diagnostics: Vec<String>,
    /// The areas read, in order, with what each returned.
    pub reads: Vec<String>,
    /// Where the value came from; `None` when nothing was calculated.
    pub provenance: Option<String>,
}

/// What a session answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    /// The request changed the session and has nothing to report.
    Accepted,
    /// The cell a request named, as the session spells it (`Sheet1!B3`).
    Cell { address: String },
    Calculated(CalculationView),
    /// The names of the functions a VBA module published, or all user functions.
    Functions { names: Vec<String> },
    Refused { message: String },
}

/// A session behind a message boundary. `&mut self` and no async: an answer is the thread's own
/// turn; what arrives later (RTD, native async returns) is pushed by the owner, not asked for.
pub trait SessionPort {
    fn send(&mut self, request: Request) -> Reply;
}

/// The session in the same process as its surface.
#[derive(Debug)]
pub struct LocalPort {
    session: Session,
}

impl LocalPort {
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// For the owner that publishes add-in functions (`Session::set_user_functions`) between
    /// messages; a surface only sends requests.
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    pub fn into_session(self) -> Session {
        self.session
    }
}

/// The typed value an entry gives under its kind, or why it gives none.
pub(crate) fn entry_value(
    session: &Session,
    kind: &str,
    entry: &str,
) -> Result<InputValue, String> {
    let text = entry.trim();
    let refused = |error| display::input_error(session, &error);
    match kind {
        "number" => match text.parse::<f64>() {
            Ok(number) => InputValue::number(number).map_err(refused),
            Err(_) => Err(format!("{entry:?} is not a number")),
        },
        "text" => InputValue::text(entry).map_err(refused),
        "logical" if text.eq_ignore_ascii_case("TRUE") => Ok(InputValue::logical(true)),
        "logical" if text.eq_ignore_ascii_case("FALSE") => Ok(InputValue::logical(false)),
        "logical" => Err(format!("{entry:?} is neither TRUE nor FALSE")),
        "error" => grid::error_literal(text)
            .map(InputValue::error)
            .ok_or_else(|| format!("{entry:?} is not a worksheet error such as #N/A")),
        "blank" => Ok(InputValue::blank()),
        other => Err(format!("{other:?} is not a kind of input")),
    }
}

/// The calculation environment a surface gives its session: the 1900 date system, the profile of
/// `culture`, and the owner's clock.
pub(crate) fn environment(culture: &str, now: Option<f64>) -> CalcEnvironment {
    CalcEnvironment {
        compatibility_version: 1,
        date_system: DateSystem::System1900,
        profile: FormatProfile::for_culture(culture),
        now,
    }
}

fn refused(message: impl Into<String>) -> Reply {
    Reply::Refused {
        message: message.into(),
    }
}

impl SessionPort for LocalPort {
    fn send(&mut self, request: Request) -> Reply {
        let session = &mut self.session;
        match request {
            Request::SetFormula { text } => {
                session.set_formula(&text);
                Reply::Accepted
            }
            Request::SetFormulaCell { address } => match session.set_formula_cell(&address) {
                Ok(cell) => Reply::Cell {
                    address: display::cell(session, cell),
                },
                Err(error) => refused(display::input_error(session, &error)),
            },
            Request::SetForm { legacy } => {
                session.set_form(if legacy {
                    FormulaForm::Legacy
                } else {
                    FormulaForm::Dynamic
                });
                Reply::Accepted
            }
            Request::SetInput {
                address,
                kind,
                entry,
            } => {
                let value = match entry_value(session, &kind, &entry) {
                    Ok(value) => value,
                    Err(message) => return refused(message),
                };
                match session.set_input(&address, value) {
                    Ok(cell) => Reply::Cell {
                        address: display::cell(session, cell),
                    },
                    Err(error) => refused(display::input_error(session, &error)),
                }
            }
            Request::RemoveInput { address } => match session.remove_input(&address) {
                Ok(_) => Reply::Accepted,
                Err(error) => refused(display::input_error(session, &error)),
            },
            Request::ClearInputs => {
                session.clear_inputs();
                Reply::Accepted
            }
            Request::SetEnvironment { culture, now } => {
                session.set_environment(Some(environment(&culture, now)));
                Reply::Accepted
            }
            Request::LoadVba { name, source } => {
                match session.load_vba_module(&name, &source, calc_vba::DEFAULT_STEP_BUDGET) {
                    Ok(names) => Reply::Functions { names },
                    Err(message) => refused(message),
                }
            }
            Request::Functions => Reply::Functions {
                names: session.user_function_names(),
            },
            Request::Calculate => {
                let calculation = session.calculate();
                Reply::Calculated(CalculationView {
                    value: calculation.result.as_ref().map(display::value),
                    diagnostics: calculation
                        .diagnostics
                        .iter()
                        .map(|d| display::diagnostic(session, d))
                        .collect(),
                    reads: calculation
                        .reads
                        .iter()
                        .map(|r| display::read(session, r))
                        .collect(),
                    provenance: crate::result::provenance(session, &calculation),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port() -> LocalPort {
        LocalPort::new(Session::new("Sheet1").unwrap())
    }

    fn calculated(reply: Reply) -> CalculationView {
        match reply {
            Reply::Calculated(view) => view,
            other => panic!("not a calculation: {other:?}"),
        }
    }

    /// The same session, driven directly, is what the port's view reports.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_calculation_through_the_port_is_the_sessions_own() {
        let mut port = port();
        assert_eq!(
            port.send(Request::SetFormulaCell {
                address: "C3".into()
            }),
            Reply::Cell {
                address: "Sheet1!C3".into()
            }
        );
        port.send(Request::SetFormula {
            text: "=A1*2+B1".into(),
        });
        for (address, entry) in [("A1", "20"), ("B1", "2")] {
            assert_eq!(
                port.send(Request::SetInput {
                    address: address.into(),
                    kind: "number".into(),
                    entry: entry.into(),
                }),
                Reply::Cell {
                    address: format!("Sheet1!{address}")
                }
            );
        }
        let view = calculated(port.send(Request::Calculate));

        let mut direct = Session::new("Sheet1").unwrap();
        direct.set_formula_cell("C3").unwrap();
        direct.set_formula("=A1*2+B1");
        direct
            .set_input("A1", InputValue::number(20.).unwrap())
            .unwrap();
        direct
            .set_input("B1", InputValue::number(2.).unwrap())
            .unwrap();
        let expected = direct.calculate();
        assert_eq!(view.value, expected.result.as_ref().map(display::value));
        assert!(view.value.as_deref().is_some_and(|v| v.contains("42")));
        assert!(view.diagnostics.is_empty());
        assert_eq!(view.reads.len(), expected.reads.len());
        assert_eq!(
            view.provenance,
            crate::result::provenance(&direct, &expected)
        );
    }

    /// A refused request says why and leaves the session as it was; a formula that does not parse
    /// is a calculation with diagnostics, not a refusal.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn refusals_name_the_reason_and_change_nothing() {
        let mut port = port();
        for (request, reason) in [
            (
                Request::SetInput {
                    address: "A1".into(),
                    kind: "number".into(),
                    entry: "x".into(),
                },
                "is not a number",
            ),
            (
                Request::SetInput {
                    address: "A1".into(),
                    kind: "colour".into(),
                    entry: "red".into(),
                },
                "is not a kind of input",
            ),
            (
                Request::SetInput {
                    address: "A1:A2".into(),
                    kind: "blank".into(),
                    entry: String::new(),
                },
                "A1:A2",
            ),
            (
                Request::SetFormulaCell {
                    address: "Nowhere!A1".into(),
                },
                "Nowhere",
            ),
        ] {
            match port.send(request) {
                Reply::Refused { message } => assert!(message.contains(reason), "{message}"),
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(port.session().inputs().count(), 0);
        port.send(Request::SetFormula { text: "=1+".into() });
        let view = calculated(port.send(Request::Calculate));
        assert!(view.value.is_none() && !view.diagnostics.is_empty());
    }

    /// VBA functions arrive as names; the set of user functions is asked for, never reached into;
    /// a module that does not compile is a refusal that keeps the earlier functions.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn vba_modules_publish_names_and_a_broken_module_changes_nothing() {
        let mut port = port();
        assert_eq!(
            port.send(Request::Functions),
            Reply::Functions { names: vec![] }
        );
        let loaded = port.send(Request::LoadVba {
            name: "Math".into(),
            source: calc_vba::EXAMPLE_MODULE.into(),
        });
        assert_eq!(
            loaded,
            Reply::Functions {
                names: vec!["ADD2".into(), "MATH.ADD2".into()]
            }
        );
        port.send(Request::SetFormula {
            text: "=Math.Add2(40,2)".into(),
        });
        let view = calculated(port.send(Request::Calculate));
        assert!(view.value.as_deref().is_some_and(|v| v.contains("42")), "{view:?}");
        assert!(matches!(
            port.send(Request::LoadVba {
                name: "Broken".into(),
                source: "Function (".into()
            }),
            Reply::Refused { .. }
        ));
        assert_eq!(port.send(Request::Functions), loaded);
    }

    /// Requests and replies are plain data: they survive the bridge as JSON.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn messages_round_trip_as_json() {
        let requests = vec![
            Request::SetFormula { text: "=A1".into() },
            Request::SetInput {
                address: "A1".into(),
                kind: "error".into(),
                entry: "#N/A".into(),
            },
            Request::SetEnvironment {
                culture: "de-DE".into(),
                now: Some(45000.5),
            },
            Request::Calculate,
        ];
        for request in requests {
            let text = serde_json::to_string(&request).unwrap();
            assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
        }
        let mut port = port();
        port.send(Request::SetFormula { text: "=1/0".into() });
        let reply = port.send(Request::Calculate);
        let text = serde_json::to_string(&reply).unwrap();
        assert_eq!(serde_json::from_str::<Reply>(&text).unwrap(), reply);
    }
}
