use crate::value::WorksheetErrorCode;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AggregateOperation {
    Average,
    Count,
    CountA,
    Max,
    Min,
    Product,
    StdevS,
    StdevP,
    Sum,
    VarS,
    VarP,
    Median,
    ModeSngl,
    Large,
    Small,
    PercentileInc,
    QuartileInc,
    PercentileExc,
    QuartileExc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AggregateFilterRules {
    ignore_nested_aggregate: bool,
    ignore_manual_hidden_rows: bool,
    ignore_filtered_rows: bool,
    ignore_errors: bool,
}
