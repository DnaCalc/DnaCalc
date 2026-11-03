//! Authored validation rules; the evaluator and document writer share these coordinates.
/// One `<dataValidation>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Validation {
    /// Original worksheet fragment retaining namespaces and unedited rule attributes.
    pub source: Option<std::sync::Arc<[u8]>>,
    /// The areas, as (top, left, bottom, right).
    pub areas: Vec<(u32, u32, u32, u32)>,
    /// `none`, `whole`, `decimal`, `list`, `date`, `time`, `textLength` or `custom`.
    pub kind: String,
    /// `between` (the default), `notBetween`, `equal`, `notEqual`, `greaterThan`, `lessThan`,
    /// `greaterThanOrEqual` or `lessThanOrEqual`.
    pub operator: String,
    pub allow_blank: bool,
    pub show_input: bool,
    pub show_error: bool,
    /// Whether a list shows its drop-down in the cell (the file's `showDropDown="1"` hides it).
    pub in_cell_dropdown: bool,
    /// `stop` (the default), `warning` or `information`.
    pub error_style: String,
    pub error_title: Option<String>,
    pub error: Option<String>,
    pub prompt_title: Option<String>,
    pub prompt: Option<String>,
    /// The formulas, as written (a literal list as `"a,b,c"` with its quotes).
    pub formula1: Option<String>,
    pub formula2: Option<String>,
}
