//! The reference-text family fragment: INDIRECT and its kernel.
//!
//! INDIRECT returns the reference its text names. The kernel reads its arguments as values and
//! hands the text to the services' [`ReferenceText`](crate::ReferenceText) resolution, which the
//! caller supplies from the calling formula's workbook context (calc-fml's reference binder);
//! calc-func parses no reference text itself. Without a resolution the call is refused as not
//! modelled.
//!
//! The catalogue declares INDIRECT volatile (`EntryTraits::NONE.volatile()`, in calc-catalogue's
//! `reference_text.rs`), as Microsoft's documentation of recalculation lists it among the
//! volatile functions: a plan holding one is volatile (`CompiledPlan::volatile`), and the engine
//! evaluates its formula on every recalculation. What an evaluation resolved from the text is a
//! separate matter, which a caller learns from the evaluator (the observer's `resolved`), and the
//! engine files as the formula's dynamic prerequisites.

#[cfg(test)]
use super::Notation;
use crate::call::{EvalServices, FunctionFault, TextReference};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, ReferenceLike, WorksheetErrorCode};

use calc_catalogue::reference_text::{CODE, ENTRIES as DECLARATIONS, INDIRECT};

attach_family! {
    code: CODE; declarations: DECLARATIONS;
    INDIRECT => Eager(indirect);
}

fn indirect(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(INDIRECT, services);
    Site::finish(resolved(&site, arguments, services))
}

/// INDIRECT's result. `ref_text` is read as text (a number or a logical as its text, a blank
/// cell or an omitted argument as empty text) and `a1` as a logical (omitted from the call: TRUE;
/// written but omitted, or a blank cell: FALSE); an error of `ref_text`, and then of `a1`, is the
/// result before any text is resolved. Text that names no reference is #REF!.
fn resolved(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    site.scalar_operands(arguments)?;
    let text = site.argument(0, coerce::text(&arguments[0]))?;
    let a1 = match arguments.get(1) {
        None => true,
        Some(a1) => site.argument(1, coerce::logical(a1))?,
    };
    let Some(resolution) = services.reference_text() else {
        return Err(site.not_modelled(Some(0)));
    };
    match resolution.resolve(&text, a1) {
        TextReference::Area(area) => Ok(CalcValue::reference(ReferenceLike::area(area))),
        TextReference::Invalid => Err(Exit::Error(WorksheetErrorCode::Ref)),
        TextReference::NotModelled => Err(site.not_modelled(Some(0))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call::{FunctionCallScratch, PreparedArguments, ReferenceText, invoke};
    use crate::catalogue::catalogue;
    use crate::value::{CellKey, ExcelText, GridArea, IdAllocator, NumericMode, SheetId};

    struct Given(Vec<CalcValue>);

    impl PreparedArguments for Given {
        fn count(&self) -> usize {
            self.0.len()
        }

        fn argument(&mut self, index: usize) -> CalcValue {
            self.0[index].clone()
        }
    }

    /// A resolution that logs what it is asked and gives a fixed answer.
    struct Logged {
        asked: Vec<(String, bool)>,
        answer: TextReference,
    }

    impl ReferenceText for Logged {
        fn resolve(&mut self, text: &[u16], a1: bool) -> TextReference {
            self.asked
                .push((String::from_utf16(text).expect("UTF-16 text"), a1));
            self.answer
        }
    }

    fn area() -> GridArea {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        GridArea::cell(CellKey::new(sheets.issue().unwrap(), 2, 3).unwrap())
    }

    fn text(s: &str) -> CalcValue {
        CalcValue::text(ExcelText::from_interop_assignment(s))
    }

    fn call(
        arguments: Vec<CalcValue>,
        resolution: Option<&mut Logged>,
    ) -> Result<CalcValue, FunctionFault> {
        let target = catalogue()
            .resolve_surface(Notation::Call, "INDIRECT")
            .expect("INDIRECT is catalogued");
        let mode = NumericMode::Fast;
        let mut services = EvalServices::new(&mode);
        if let Some(resolution) = resolution {
            services = services.with_reference_text(resolution);
        }
        invoke(
            target,
            &mut Given(arguments),
            &mut services,
            &mut FunctionCallScratch::new(),
        )
    }

    /// The text and notation reach the resolution as the arguments give them, and its answer is
    /// the result: an area as a reference, no reference as #REF!, a refusal as not modelled.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_resolution_decides() {
        let mut logged = Logged {
            asked: Vec::new(),
            answer: TextReference::Area(area()),
        };
        assert_eq!(
            call(vec![text("B2")], Some(&mut logged)),
            Ok(CalcValue::reference(ReferenceLike::area(area())))
        );
        assert_eq!(
            call(
                vec![text("R2C3"), CalcValue::logical(false)],
                Some(&mut logged)
            ),
            Ok(CalcValue::reference(ReferenceLike::area(area())))
        );
        // An omitted a1 is FALSE, a number is its text, a blank cell is empty text.
        let _ = call(vec![text("A1"), CalcValue::missing()], Some(&mut logged));
        let _ = call(vec![CalcValue::number(12.0)], Some(&mut logged));
        let _ = call(
            vec![CalcValue::empty(), CalcValue::number(2.0)],
            Some(&mut logged),
        );
        let _ = call(
            vec![CalcValue::logical(true), CalcValue::empty()],
            Some(&mut logged),
        );
        assert_eq!(
            logged.asked,
            [
                ("B2".to_owned(), true),
                ("R2C3".to_owned(), false),
                ("A1".to_owned(), false),
                ("12".to_owned(), true),
                (String::new(), true),
                ("TRUE".to_owned(), false),
            ]
        );
        logged.answer = TextReference::Invalid;
        assert_eq!(
            call(vec![text("nowhere")], Some(&mut logged)),
            Ok(CalcValue::error(WorksheetErrorCode::Ref))
        );
        logged.answer = TextReference::NotModelled;
        assert_eq!(
            call(vec![text("Name")], Some(&mut logged)),
            Err(FunctionFault::NotImplemented {
                id: INDIRECT,
                argument: Some(0)
            })
        );
    }

    /// Errors come before any resolution, ref_text's first; without a resolution the call is
    /// refused once its arguments are read.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn errors_and_refusals() {
        use WorksheetErrorCode as W;
        let mut logged = Logged {
            asked: Vec::new(),
            answer: TextReference::Area(area()),
        };
        let cases = [
            (
                vec![CalcValue::error(W::Div0), CalcValue::error(W::NA)],
                Ok(CalcValue::error(W::Div0)),
            ),
            (
                vec![text("A1"), CalcValue::error(W::NA)],
                Ok(CalcValue::error(W::NA)),
            ),
            (
                vec![CalcValue::number(1.0), CalcValue::error(W::NA)],
                Ok(CalcValue::error(W::NA)),
            ),
            (vec![text("A1"), text("x")], Ok(CalcValue::error(W::Value))),
            // Digit text is no logical either: Excel reads no number in a logical position.
            (vec![text("A1"), text("1")], Ok(CalcValue::error(W::Value))),
        ];
        for (arguments, expected) in cases {
            assert_eq!(
                call(arguments.clone(), Some(&mut logged)),
                expected,
                "{arguments:?}"
            );
        }
        assert!(logged.asked.is_empty(), "{:?}", logged.asked);
        assert_eq!(
            call(vec![text("A1")], None),
            Err(FunctionFault::NotImplemented {
                id: INDIRECT,
                argument: Some(0)
            })
        );
        assert_eq!(
            call(vec![CalcValue::error(W::NA)], None),
            Ok(CalcValue::error(W::NA))
        );
    }
}
