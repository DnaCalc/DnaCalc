//! Portable `ObjectModel` for hosts without `calc-host` (Formula Explorer / OneCalc,
//! standalone `load_module`, `load_office_project`, and `calc-vba` unit tests).
//!
//! Routes member calls through the active scoped `SpreadsheetContext` (worksheet UDF
//! invocation), `EventReadContext` (standalone document events), or `ValuesOnly` fallback.
//! Never depends on `calc-host` or `calc-engine`.
use crate::events::CalculationCommand;
use crate::runtime::{ContextError, Range, SpreadsheetContext, Value};
use calc_func::value::{CalcValue, ReferenceLike, SheetId};
use calc_object_model::{
    AutoArray, AutoError, AutoValue, Call, Descriptor, Handlers, InvokeKind, ObjectKey,
    ObjectModel, ObjectRef,
    geometry::{self, AddressOptions, RangeAxis},
    profile::{
        ApplicationHost,
        values_only::{ValuesOnly, object},
        worksheet_function,
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const OWNER: &str = "dc-piw0";
type Reply = Result<AutoValue, AutoError>;

struct PortableAppHost {
    events_enabled: Cell<bool>,
}

impl ApplicationHost for PortableAppHost {
    fn hwnd(&self) -> isize {
        0
    }
    fn run(&self, _: &str, _: &[AutoValue]) -> Reply {
        Err(AutoError::not_modelled("_Application.Run", OWNER))
    }
    fn events_enabled(&self) -> bool {
        self.events_enabled.get()
    }
    fn set_events_enabled(&self, enabled: bool) {
        self.events_enabled.set(enabled);
    }
}

/// Portable `ObjectModel` backed by the active scoped `SpreadsheetContext` (UDFs) and
/// `EventReadContext` (standalone `EventSource` tests and `load_office_project`).
pub(crate) struct InvocationModel {
    fallback: ValuesOnly<PortableAppHost>,
    handlers: Handlers<Self>,
    /// Optional document name cell for standalone `EventSource::worksheet`/`workbook`.
    document_name: Option<Rc<RefCell<String>>>,
    /// Standalone event-range registry (used when `EventSource::change` runs without `calc-host`).
    event_ranges: RefCell<Vec<ReferenceLike>>,
    /// Values-only host (Formula Explorer / OneCalc).
    values_only: bool,
}

impl InvocationModel {
    pub(crate) fn new(values_only: bool) -> Self {
        Self::with_document_name(values_only, None)
    }

    pub(crate) fn with_document_name(
        values_only: bool,
        document_name: Option<Rc<RefCell<String>>>,
    ) -> Self {
        let mut handlers = Handlers::new();
        use InvokeKind::{Get, Method, Put};
        handlers.on("Application", "Application", Get, Self::delegate);
        handlers.on("Application", "Caller", Get, Self::app_caller);
        handlers.on("Application", "WorksheetFunction", Get, Self::app_functions);
        handlers.on("Application", "Volatile", Method, Self::app_volatile);
        handlers.on("Application", "ActiveSheet", Get, Self::app_active_sheet);
        handlers.on("Application", "Selection", Get, Self::app_selection);
        handlers.on("Application", "Range", Get, Self::app_range);
        handlers.on("Application", "Calculate", Method, Self::app_calculate);
        handlers.on("Application", "CalculateFull", Method, Self::app_calculate_full);
        handlers.on(
            "Application",
            "CalculateFullRebuild",
            Method,
            Self::app_calculate_rebuild,
        );
        handlers.on("Application", "EnableEvents", Get, Self::app_events_get);
        handlers.on("Application", "EnableEvents", Put, Self::app_events_put);
        handlers.on("Application", "ActiveWorkbook", Get, Self::not_modelled_member);
        handlers.on("Application", "ActiveWindow", Get, Self::not_modelled_member);

        handlers.on("Workbook", "Name", Get, Self::document_name_get);
        handlers.on("Worksheet", "Name", Get, Self::worksheet_name_get);
        handlers.on("Worksheet", "Range", Get, Self::worksheet_range);

        handlers.on("Range", "Value2", Get, Self::range_value2_get);
        handlers.on("Range", "Value2", Put, Self::range_value_put);
        handlers.on("Range", "Value", Get, Self::range_value_get);
        handlers.on("Range", "Value", Put, Self::range_value_put);
        handlers.on("Range", "_Default", Get, Self::range_default_get);
        handlers.on("Range", "_Default", Put, Self::range_value_put);
        handlers.on("Range", "Formula", Get, Self::not_modelled_member);
        handlers.on("Range", "Formula", Put, Self::range_value_put);
        handlers.on("Range", "Formula2", Put, Self::range_value_put);
        handlers.on("Range", "FormulaR1C1", Put, Self::range_value_put);
        handlers.on("Range", "Formula2R1C1", Put, Self::range_value_put);
        handlers.on("Range", "Count", Get, Self::range_count);
        handlers.on("Range", "Row", Get, Self::range_row);
        handlers.on("Range", "Column", Get, Self::range_column);
        handlers.on("Range", "Address", Get, Self::range_address);
        handlers.on("Range", "Rows", Get, Self::range_rows);
        handlers.on("Range", "Columns", Get, Self::range_columns);
        handlers.on("Range", "Cells", Get, Self::range_cells);
        handlers.on("Range", "Item", Get, Self::range_item);
        handlers.on("Range", "Areas", Get, Self::range_areas);
        handlers.on("Range", "Calculate", Method, Self::range_calculate);

        handlers.on("Areas", "Count", Get, Self::areas_count);
        handlers.on("Areas", "Item", Get, Self::areas_item);
        handlers.on("Areas", "_Default", Get, Self::areas_item);

        worksheet_function::register(
            &mut handlers,
            Self::worksheet_func,
            Self::delegate,
            Self::delegate,
        );

        Self {
            fallback: ValuesOnly::new(PortableAppHost {
                events_enabled: Cell::new(true),
            }),
            handlers,
            document_name,
            event_ranges: RefCell::new(Vec::new()),
            values_only,
        }
    }

    pub(crate) fn register_event_range(&self, reference: ReferenceLike) -> ObjectRef {
        let mut ranges = self.event_ranges.borrow_mut();
        let index = ranges.len() as u64;
        ranges.push(reference);
        object("Range", ObjectKey::Host(EVENT_RANGE_TAG | index))
    }

    fn range_object(range: Range, axis: RangeAxis) -> ObjectRef {
        let key = ObjectKey::Host(encode_range(range, axis));
        if axis == RangeAxis::Cells {
            object("Range", key)
        } else {
            object("Range", key)
        }
    }

    fn areas_object(range: Range) -> ObjectRef {
        object("Areas", ObjectKey::Host(encode_areas(range)))
    }

    fn event_reference(&self, target: &ObjectRef) -> Option<ReferenceLike> {
        let ObjectKey::Host(tag) = target.key else {
            return None;
        };
        if (tag & TAG_MASK) != EVENT_RANGE_TAG {
            return None;
        }
        let idx = (tag & !TAG_MASK) as usize;
        self.event_ranges.borrow().get(idx).cloned()
    }

    fn context_range(target: &ObjectRef) -> Option<(Range, RangeAxis)> {
        let ObjectKey::Host(tag) = target.key else {
            return None;
        };
        decode_range(tag)
    }

    fn delegate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.fallback.invoke(target, call)
    }

    fn not_modelled_member(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        Err(AutoError::not_modelled(call.label(), OWNER))
    }

    fn with_ctx<T>(
        call: impl FnOnce(&dyn SpreadsheetContext) -> Result<T, ContextError>,
    ) -> Result<T, AutoError> {
        crate::runtime::spreadsheet::with_active_context(call)
    }

    fn app_caller(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 1 {
            return Err(geometry::bad_count());
        }
        if !call.arg(0).is_missing() {
            return Err(AutoError::not_modelled("_Application.Caller Index", OWNER));
        }
        if self.values_only {
            return Err(AutoError::not_modelled(crate::NEEDS_A_WORKBOOK, "dc-0f1k"));
        }
        match Self::with_ctx(|ctx| Ok(ctx.caller()))? {
            Some(range) => Ok(AutoValue::Object(Self::range_object(range, RangeAxis::Cells))),
            None => Ok(AutoValue::Error(2023)),
        }
    }

    fn app_functions(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::Object(object(
            "WorksheetFunction",
            ObjectKey::WorksheetFunction,
        )))
    }

    fn app_volatile(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 1 {
            return Err(geometry::bad_count());
        }
        let enabled = if call.arg(0).is_missing() {
            true
        } else {
            geometry::boolean(call.arg(0))?
        };
        Self::with_ctx(|ctx| ctx.set_volatile(enabled))?;
        Ok(AutoValue::Empty)
    }

    fn app_active_sheet(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        // Validate the host models an active sheet before returning the wrapper.
        Self::with_ctx(|ctx| ctx.active_sheet_name())?;
        Ok(AutoValue::Object(object(
            "Worksheet",
            ObjectKey::Worksheet {
                workbook: 0,
                sheet: SheetId::from_parts(0, 0),
            },
        )))
    }

    fn app_selection(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let range = Self::with_ctx(|ctx| ctx.selection())?;
        Ok(AutoValue::Object(Self::range_object(range, RangeAxis::Cells)))
    }

    fn app_range(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let text = call
            .arg(0)
            .as_text()
            .ok_or_else(geometry::type_mismatch)?;
        let reference = crate::events::with_event_read(|ctx| ctx.resolve_reference(&text))?;
        Ok(AutoValue::Object(self.register_event_range(reference)))
    }

    fn app_calculate(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        crate::events::with_event_read(|ctx| ctx.calculate(CalculationCommand::Dirty))?;
        Ok(AutoValue::Empty)
    }

    fn app_calculate_full(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        crate::events::with_event_read(|ctx| ctx.calculate(CalculationCommand::Full))?;
        Ok(AutoValue::Empty)
    }

    fn app_calculate_rebuild(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        crate::events::with_event_read(|ctx| ctx.calculate(CalculationCommand::Rebuild))?;
        Ok(AutoValue::Empty)
    }

    fn app_events_get(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let enabled = crate::events::with_event_read(|ctx| ctx.events_enabled())?;
        Ok(AutoValue::Bool(enabled))
    }

    fn app_events_put(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let enabled = geometry::boolean(call.arg(0))?;
        crate::events::with_event_read(|ctx| ctx.set_events_enabled(enabled))?;
        Ok(AutoValue::Empty)
    }

    fn document_name_get(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(name) = &self.document_name {
            return Ok(AutoValue::text(&name.borrow()));
        }
        Err(AutoError::not_modelled(call.label(), OWNER))
    }

    fn worksheet_name_get(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(name) = &self.document_name {
            return Ok(AutoValue::text(&name.borrow()));
        }
        let name = Self::with_ctx(|ctx| ctx.active_sheet_name())?;
        Ok(AutoValue::text(&name))
    }

    fn worksheet_range(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        if self.document_name.is_some() {
            return Err(AutoError::not_modelled(
                "a document member outside Name without a workbook",
                OWNER,
            ));
        }
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let address = call
            .arg(0)
            .as_text()
            .ok_or_else(geometry::type_mismatch)?;
        let range = Self::with_ctx(|ctx| ctx.active_range(&address))?;
        Ok(AutoValue::Object(Self::range_object(range, RangeAxis::Cells)))
    }

    fn range_value2_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(reference) = self.event_reference(target) {
            let value = crate::events::with_event_read(|ctx| ctx.read_reference(&reference))?;
            return calc_to_auto(&value);
        }
        let (range, _) = Self::context_range(target).ok_or_else(geometry::type_mismatch)?;
        let value = Self::read_context_range(range)?;
        vba_to_auto(&value)
    }

    fn range_value_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if self.event_reference(target).is_some() {
            return Err(AutoError::not_modelled(call.label(), OWNER));
        }
        Err(AutoError::not_modelled(
            "a declared Range member outside the supported host surface",
            OWNER,
        ))
    }

    fn range_default_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.is_empty() || (call.arg(0).is_missing() && call.arg(1).is_missing()) {
            return self.range_value2_get(
                target,
                &Call {
                    interface: call.interface,
                    member: call.member,
                    args: vec![],
                    lcid: 0,
                },
            );
        }
        self.range_item(target, call)
    }

    fn range_value_put(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if let Some(reference) = self.event_reference(target) {
            let rhs = call.args.last().ok_or_else(geometry::bad_count)?;
            let value = auto_to_calc(rhs)?;
            crate::events::with_event_read(|ctx| ctx.write_reference(&reference, value))?;
            return Ok(AutoValue::Empty);
        }
        if crate::runtime::spreadsheet::worksheet_function_call_active() {
            return Err(AutoError::excel(
                1004,
                "Application-defined or object-defined error",
            ));
        }
        Err(AutoError::not_modelled(
            "spreadsheet objects are read-only",
            OWNER,
        ))
    }

    fn range_calculate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(reference) = self.event_reference(target) {
            crate::events::with_event_read(|ctx| {
                ctx.calculate(CalculationCommand::Range(reference))
            })?;
            return Ok(AutoValue::Empty);
        }
        Err(AutoError::not_modelled(call.label(), OWNER))
    }

    fn range_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(reference) = self.event_reference(target) {
            return Ok(geometry::count_value(geometry::count(
                &reference,
                RangeAxis::Cells,
            )));
        }
        let (range, axis) = Self::context_range(target).ok_or_else(geometry::type_mismatch)?;
        let info = Self::with_ctx(|ctx| ctx.metadata(range))?;
        let count = match axis {
            RangeAxis::Rows => u64::from(info.rows),
            RangeAxis::Columns => u64::from(info.columns),
            RangeAxis::Cells => info.cells,
        };
        Ok(geometry::count_value(count))
    }

    fn range_row(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(reference) = self.event_reference(target) {
            return Ok(AutoValue::I4(geometry::row(&reference) as i32));
        }
        let (range, _) = Self::context_range(target).ok_or_else(geometry::type_mismatch)?;
        let info = Self::with_ctx(|ctx| ctx.metadata(range))?;
        Ok(AutoValue::I4(info.row as i32))
    }

    fn range_column(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(reference) = self.event_reference(target) {
            return Ok(AutoValue::I4(geometry::column(&reference) as i32));
        }
        let (range, _) = Self::context_range(target).ok_or_else(geometry::type_mismatch)?;
        let info = Self::with_ctx(|ctx| ctx.metadata(range))?;
        Ok(AutoValue::I4(info.column as i32))
    }

    fn range_address(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if let Some(reference) = self.event_reference(target) {
            let options = AddressOptions::from_call(call)?;
            return Ok(AutoValue::text(&geometry::address(
                &reference,
                options,
                None,
            )?));
        }
        let (range, _) = Self::context_range(target).ok_or_else(geometry::type_mismatch)?;
        let options = AddressOptions::from_call(call)?;
        if !options.row_absolute || !options.column_absolute {
            return Err(AutoError::not_modelled(
                "relative Range.Address coordinates",
                OWNER,
            ));
        }
        if options.style != geometry::ReferenceStyle::A1 {
            return Err(AutoError::not_modelled(
                "a non-A1 Range.Address reference style",
                OWNER,
            ));
        }
        if !geometry::address_default(call.arg(4)) {
            return Err(AutoError::not_modelled("Range.Address RelativeTo", OWNER));
        }
        let address = if options.external {
            Self::with_ctx(|ctx| ctx.external_address(range))?
        } else {
            Self::with_ctx(|ctx| ctx.metadata(range))?.address
        };
        Ok(AutoValue::text(&address))
    }

    fn range_rows(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (range, _) = Self::context_range(target).ok_or_else(geometry::type_mismatch)?;
        Ok(AutoValue::Object(Self::range_object(range, RangeAxis::Rows)))
    }

    fn range_columns(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (range, _) = Self::context_range(target).ok_or_else(geometry::type_mismatch)?;
        Ok(AutoValue::Object(Self::range_object(
            range,
            RangeAxis::Columns,
        )))
    }

    fn range_cells(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (range, _) = Self::context_range(target).ok_or_else(geometry::type_mismatch)?;
        Ok(AutoValue::Object(Self::range_object(range, RangeAxis::Cells)))
    }

    fn range_areas(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (range, _) = Self::context_range(target).ok_or_else(geometry::type_mismatch)?;
        Ok(AutoValue::Object(Self::areas_object(range)))
    }

    fn range_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (range, axis) = Self::context_range(target).ok_or_else(geometry::type_mismatch)?;
        if axis != RangeAxis::Cells {
            return Err(AutoError::not_modelled(
                "indexed Rows/Columns are not supported",
                OWNER,
            ));
        }
        if call.args.is_empty() || call.args.len() > 2 || call.arg(0).is_missing() {
            return Err(geometry::bad_count());
        }
        let info = Self::with_ctx(|ctx| ctx.metadata(range))?;
        let to_u32 = |v: &AutoValue| -> Result<u32, AutoError> {
            let n = geometry::integer(v)?;
            u32::try_from(n)
                .ok()
                .filter(|&n| n > 0)
                .ok_or_else(|| AutoError::excel(1004, "Range index is outside this host"))
        };
        let (row, column) = if call.args.len() == 1 || call.arg(1).is_missing() {
            let n = to_u32(call.arg(0))? - 1;
            (n / info.columns + 1, n % info.columns + 1)
        } else {
            (to_u32(call.arg(0))?, to_u32(call.arg(1))?)
        };
        let selected = Self::with_ctx(|ctx| ctx.cell(range, row, column))?;
        Ok(AutoValue::Object(Self::range_object(
            selected,
            RangeAxis::Cells,
        )))
    }

    fn areas_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let ObjectKey::Host(tag) = target.key else {
            return Err(geometry::type_mismatch());
        };
        let range = decode_areas(tag).ok_or_else(geometry::type_mismatch)?;
        let info = Self::with_ctx(|ctx| ctx.metadata(range))?;
        Ok(AutoValue::I4(info.areas as i32))
    }

    fn areas_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let ObjectKey::Host(tag) = target.key else {
            return Err(geometry::type_mismatch());
        };
        let range = decode_areas(tag).ok_or_else(geometry::type_mismatch)?;
        let index = geometry::integer(call.arg(0))?;
        let index = u32::try_from(index)
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| AutoError::excel(1004, "Areas index out of bounds"))?;
        let selected = Self::with_ctx(|ctx| ctx.area(range, index))?;
        Ok(AutoValue::Object(Self::range_object(
            selected,
            RangeAxis::Cells,
        )))
    }

    fn worksheet_func(&self, _: &ObjectRef, call: &Call<'_>) -> Reply {
        let through_application = worksheet_function::take_application_call();
        let name = call.member.name.as_str();
        let mut end = call.args.len();
        while end > 0 && call.arg(end - 1).is_missing() {
            end -= 1;
        }
        let mut args = Vec::with_capacity(end);
        for arg in &call.args[..end] {
            match arg {
                AutoValue::Object(obj) => {
                    if let Some((range, _)) = Self::context_range(obj) {
                        args.push(Value::Range(range));
                    } else {
                        return Err(geometry::type_mismatch());
                    }
                }
                other => args.push(auto_to_vba(other)?),
            }
        }
        let value = Self::with_ctx(|ctx| ctx.worksheet_function(name, &args))?;
        if matches!(value, Value::Error(_)) && !through_application {
            return Err(AutoError::excel(
                1004,
                format!("WorksheetFunction.{name} could not calculate"),
            ));
        }
        vba_to_auto(&value)
    }

    fn read_context_range(range: Range) -> Result<Value, AutoError> {
        let info = Self::with_ctx(|ctx| ctx.metadata(range))?;
        if info.cells > 1_048_576 {
            return Err(AutoError::not_modelled(
                "Range.Value2 exceeds the per-read cell limit",
                OWNER,
            ));
        }
        let value = Self::with_ctx(|ctx| ctx.read_range(range))?;
        let shape_ok = match &value {
            Value::Array { rows, columns, .. } => {
                *rows == info.rows
                    && *columns == info.columns
                    && (info.rows != 1 || info.columns != 1)
            }
            Value::Range(_) | Value::Missing => false,
            _ => info.rows == 1 && info.columns == 1,
        };
        if !shape_ok {
            return Err(AutoError::not_modelled(
                "host supplied an invalid Range.Value2 shape",
                OWNER,
            ));
        }
        Ok(value)
    }
}

impl ObjectModel for InvocationModel {
    fn application(&self) -> ObjectRef {
        self.fallback.application()
    }

    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if let Some(handler) = self.handlers.get(call.interface, call.member) {
            handler(self, target, call)
        } else {
            Err(AutoError::not_modelled(call.label(), OWNER))
        }
    }
}

const TAG_MASK: u64 = 0xF000_0000_0000_0000;
const CELLS_TAG: u64 = 0x1000_0000_0000_0000;
const ROWS_TAG: u64 = 0x2000_0000_0000_0000;
const COLS_TAG: u64 = 0x3000_0000_0000_0000;
const AREAS_TAG: u64 = 0x4000_0000_0000_0000;
const EVENT_RANGE_TAG: u64 = 0x5000_0000_0000_0000;

pub(crate) fn encode_range(range: Range, axis: RangeAxis) -> u64 {
    let tag = match axis {
        RangeAxis::Cells => CELLS_TAG,
        RangeAxis::Rows => ROWS_TAG,
        RangeAxis::Columns => COLS_TAG,
    };
    tag | (range.id & !TAG_MASK)
}

fn encode_areas(range: Range) -> u64 {
    AREAS_TAG | (range.id & !TAG_MASK)
}

pub(crate) fn decode_range(encoded: u64) -> Option<(Range, RangeAxis)> {
    let axis = match encoded & TAG_MASK {
        CELLS_TAG => RangeAxis::Cells,
        ROWS_TAG => RangeAxis::Rows,
        COLS_TAG => RangeAxis::Columns,
        _ => return None,
    };
    Some((Range { id: encoded & !TAG_MASK }, axis))
}

fn decode_areas(encoded: u64) -> Option<Range> {
    ((encoded & TAG_MASK) == AREAS_TAG).then_some(Range {
        id: encoded & !TAG_MASK,
    })
}

fn no_args(call: &Call<'_>) -> Result<(), AutoError> {
    if call.args.is_empty() {
        Ok(())
    } else {
        Err(geometry::bad_count())
    }
}

pub(crate) fn calc_to_auto(value: &CalcValue) -> Result<AutoValue, AutoError> {
    let vba = crate::to_vba(value).map_err(|why| AutoError::not_modelled(why, "dc-hsa5.12"))?;
    vba_to_auto(&vba)
}

pub(crate) fn auto_to_calc(value: &AutoValue) -> Result<CalcValue, AutoError> {
    let vba = auto_to_vba(value)?;
    crate::from_vba(vba).map_err(|why| AutoError::not_modelled(why, "dc-hsa5.12"))
}

pub(crate) fn vba_to_auto(value: &Value) -> Result<AutoValue, AutoError> {
    Ok(match value {
        Value::Empty => AutoValue::Empty,
        Value::Missing => AutoValue::Missing,
        Value::Number(n) => AutoValue::R8(*n),
        Value::Text(t) => AutoValue::text(t),
        Value::Boolean(b) => AutoValue::Bool(*b),
        Value::Error(code) => AutoValue::Error(*code),
        Value::Range(range) => AutoValue::Object(InvocationModel::range_object(
            *range,
            RangeAxis::Cells,
        )),
        Value::Array {
            rows,
            columns,
            cells,
        } => {
            let values = cells
                .iter()
                .map(vba_to_auto)
                .collect::<Result<Vec<_>, _>>()?;
            AutoValue::Array(Box::new(AutoArray {
                rows: *rows,
                columns: *columns,
                lower: (1, 1),
                values,
            }))
        }
    })
}

pub(crate) fn auto_to_vba(value: &AutoValue) -> Result<Value, AutoError> {
    Ok(match value {
        AutoValue::Empty => Value::Empty,
        AutoValue::Missing => Value::Missing,
        AutoValue::Bool(v) => Value::Boolean(*v),
        AutoValue::I2(v) => Value::Number(f64::from(*v)),
        AutoValue::I4(v) => Value::Number(f64::from(*v)),
        AutoValue::R4(v) if v.is_finite() => Value::Number(f64::from(*v)),
        AutoValue::R8(v) if v.is_finite() => Value::Number(*v),
        AutoValue::Text(t) => Value::Text(
            String::from_utf16(t.utf16_code_units())
                .map_err(|_| AutoError::not_modelled("invalid UTF-16 text", "dc-hsa5.12"))?,
        ),
        AutoValue::Error(code) => Value::Error(*code),
        AutoValue::Array(a) => {
            let cells = a
                .values
                .iter()
                .map(auto_to_vba)
                .collect::<Result<Vec<_>, _>>()?;
            Value::Array {
                rows: a.rows,
                columns: a.columns,
                cells,
            }
        }
        _ => return Err(geometry::type_mismatch()),
    })
}

/// Construct a portable `ObjectRef` for an `EventSource` document (`Worksheet` or `Workbook`).
pub(crate) fn standalone_document_object(interface: &str) -> ObjectRef {
    let d = Descriptor::get();
    let id = d.default_interface(d.id(interface).expect("document interface in descriptor"));
    let key = if interface.eq_ignore_ascii_case("Workbook") {
        ObjectKey::Workbook(0)
    } else {
        ObjectKey::Worksheet {
            workbook: 0,
            sheet: SheetId::from_parts(0, 0),
        }
    };
    ObjectRef::new(id, key)
}
