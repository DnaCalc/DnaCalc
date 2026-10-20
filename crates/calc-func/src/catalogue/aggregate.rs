//! The aggregate family fragment: SUM and its kernel.

use super::{ArgumentDemand, CatalogueEntry, Family, FunctionId, Kernel, Notation, Parameter};
use crate::call::{CellReader, EvalServices, FunctionFault, Unreadable};
use crate::coerce;
use crate::kernel::{Exit, Site, Step};
use crate::value::{CalcValue, CallableArityShape, CoreValue, ReferenceLike};
use std::ops::ControlFlow;

const CODE: u16 = 4;

const SUM: FunctionId = FunctionId::new(CODE, 1);

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[CatalogueEntry {
        id: SUM,
        spelling: "SUM",
        notation: Notation::Call,
        arity: CallableArityShape::range(1, 255),
        parameters: &[
            Parameter::value_or_reference("number1"),
            Parameter::value_or_reference("number2"),
        ],
        demand: ArgumentDemand::Eager,
        kernel: Kernel::Eager(sum),
    }],
};

/// SUM: the numbers of its arguments added left to right from 0, each addition an arithmetic
/// site. A direct argument is read through worksheet number coercion, so direct text and logicals
/// count (and non-numeric text is #VALUE!). A reference's cells, read through the services' cell
/// reader area by area in row-major order, and an array's elements, in row-major order, count
/// only their numbers: text, logicals and blanks there are skipped. The first error met in that
/// order is the result. A total that overflows is #NUM!.
fn sum(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(SUM, services);
    Site::finish(total(&site, arguments, services))
}

fn total(
    site: &Site<'_>,
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Step<CalcValue> {
    let mut total = 0.0;
    for (index, argument) in arguments.iter().enumerate() {
        match argument.core() {
            CoreValue::Reference(reference) => {
                let cells =
                    services
                        .cells()
                        .ok_or(Exit::Fault(FunctionFault::ReferenceUnreadable {
                            id: site.id(),
                            index,
                        }))?;
                add_reference(site, &mut total, cells, reference, index)?;
            }
            CoreValue::Array(array) => {
                for element in array.iter_row_major() {
                    add_held(site, &mut total, element, index)?;
                }
            }
            _ => {
                let number = site.number_argument(index, argument)?;
                total = site.add(total, number)?;
            }
        }
    }
    site.number(total)
}

/// Adds the cells of every area of `reference`, in its order, as values a reference holds.
fn add_reference(
    site: &Site<'_>,
    total: &mut f64,
    cells: &mut dyn CellReader,
    reference: &ReferenceLike,
    index: usize,
) -> Step<()> {
    for area in reference.areas() {
        let mut stopped = Ok(());
        let read = cells.read_area(
            *area,
            &mut |_, value| match add_held(site, total, value, index) {
                Ok(()) => ControlFlow::Continue(()),
                Err(exit) => {
                    stopped = Err(exit);
                    ControlFlow::Break(())
                }
            },
        );
        read.map_err(|Unreadable| {
            Exit::Fault(FunctionFault::ReferenceUnreadable {
                id: site.id(),
                index,
            })
        })?;
        stopped?;
    }
    Ok(())
}

/// Adds a value that a reference or an array holds: a number is added, an error stops the sum,
/// and text, a logical or a blank is skipped. A nested array or reference is not modelled.
fn add_held(site: &Site<'_>, total: &mut f64, value: &CalcValue, index: usize) -> Step<()> {
    match value.core() {
        CoreValue::Number(n) => {
            let n = site.argument(index, coerce::operand(*n))?;
            *total = site.add(*total, n)?;
            Ok(())
        }
        CoreValue::Error(code) => Err(Exit::Error(*code)),
        CoreValue::Text(_) | CoreValue::Logical(_) | CoreValue::Empty | CoreValue::Missing => {
            Ok(())
        }
        CoreValue::Array(_) | CoreValue::Reference(_) => Err(site.not_modelled(Some(index))),
    }
}
