use crate::{invalid, query::QueryWork};
use calc_doc_model::CellInput;
use calc_host::{HostState, value::CellKey};
use calc_skin::{OperandSpan, ReferenceOperand, ReferenceOperands, Refusal, grid::Key};

pub(crate) fn operands(
    host: &HostState,
    at: CellKey,
    meter: &mut QueryWork,
) -> Result<ReferenceOperands, Refusal> {
    meter.work(1, 0, 0)?;
    let Some(CellInput::Formula(input)) = host.model().cell(at) else {
        return Err(invalid("Choose a formula cell"));
    };
    let source = input.rendered();
    let body = String::from_utf16(source.utf16_code_units())
        .map_err(|_| invalid("This formula cannot be edited losslessly"))?;
    let formula = format!("={body}");
    let units: Vec<_> = formula.encode_utf16().collect();
    let mut operands = Vec::new();
    for (span, target) in host.scalar_reference_sites(at, &formula) {
        meter.work(0, 0, 1)?;
        let text = String::from_utf16(&units[span.start..span.end()])
            .map_err(|_| invalid("This reference cannot be edited losslessly"))?;
        operands.push(ReferenceOperand {
            span: OperandSpan { start: span.start, end: span.end() },
            text,
            at: Key::of(target),
        });
    }
    Ok(ReferenceOperands { formula, operands })
}

pub(crate) fn replace(
    host: &HostState,
    at: CellKey,
    before: &str,
    occurrence: OperandSpan,
    with: &str,
) -> Result<(String, CellKey), Refusal> {
    let with = with.trim();
    if let Some(CellInput::Formula(input)) = host.model().cell(at)
        && String::from_utf16(input.rendered().utf16_code_units()).is_err()
    {
        return Err(invalid("This formula cannot be edited losslessly"));
    }
    if with.is_empty() || with.encode_utf16().take(8193).count() > 8192 {
        return Err(invalid("Enter a single cell reference or a Defined Name"));
    }
    if !host.scalar_reference_sites(at, before).iter().any(|(span, _)| {
        span.start == occurrence.start && span.end() == occurrence.end
    }) {
        return Err(invalid("Choose one scalar reference in the current formula"));
    }
    let mut units: Vec<_> = before.encode_utf16().collect();
    units.splice(occurrence.start..occurrence.end, with.encode_utf16());
    let after = String::from_utf16(&units)
        .map_err(|_| invalid("This formula cannot be edited losslessly"))?;
    let end = occurrence.start + with.encode_utf16().count();
    let target = host.scalar_reference_sites(at, &after).into_iter()
        .find(|(span, _)| span.start == occurrence.start && span.end() == end)
        .map(|(_, target)| target)
        .ok_or_else(|| invalid("Enter a single cell reference or a Defined Name that resolves to one cell here"))?;
    Ok((after, target))
}
