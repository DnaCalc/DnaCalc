use crate::value::WorksheetErrorCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RectShape {
    rows: usize,
    cols: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct NumericRect {
    shape: RectShape,
    values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
struct OptionalNumericRect {
    shape: RectShape,
    values: Vec<Option<f64>>,
}

fn shapes_match<T>(rects: &[T], shape_of: impl Fn(&T) -> RectShape) -> bool {
    let Some(first) = rects.first() else {
        return true;
    };
    let base = shape_of(first);
    rects.iter().all(|r| shape_of(r) == base)
}
