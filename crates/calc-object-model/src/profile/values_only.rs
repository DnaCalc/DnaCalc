//! OneCalc's application and Book1 shell. Workbook operations require DNA Calc (dc-svqh).

use super::ApplicationHost;
use crate::geometry::{self, AddressOptions, RangeAxis, bad_count, boolean, type_mismatch};
use crate::{
    AutoError, AutoValue, Call, Descriptor, Handlers, InvokeKind, ObjectKey, ObjectModel, ObjectRef,
};
use calc_func::StoredReference;
use calc_value::ReferenceLike;

pub struct ValuesOnly<H: ApplicationHost> {
    host: H,
    handlers: Handlers<Self>,
}

/// Construct an identity from a descriptor name; profiles do not carry interface numbers.
pub fn object(interface: &str, key: ObjectKey) -> ObjectRef {
    let d = Descriptor::get();
    ObjectRef::new(
        d.default_interface(d.id(interface).expect("profile interface in descriptor")),
        key,
    )
}

impl<H: ApplicationHost> ValuesOnly<H> {
    pub fn new(host: H) -> Self {
        let mut handlers = Handlers::new();
        handlers.on(
            "Application",
            "Application",
            InvokeKind::Get,
            Self::application_member,
        );
        handlers.on("Application", "Parent", InvokeKind::Get, Self::parent);
        handlers.on(
            "Workbooks",
            "Application",
            InvokeKind::Get,
            Self::application_member,
        );
        handlers.on("Workbooks", "Parent", InvokeKind::Get, Self::parent);
        handlers.on(
            "Workbook",
            "Application",
            InvokeKind::Get,
            Self::application_member,
        );
        handlers.on("Workbook", "Parent", InvokeKind::Get, Self::parent);
        handlers.on(
            "Worksheet",
            "Application",
            InvokeKind::Get,
            Self::application_member,
        );
        handlers.on("Worksheet", "Parent", InvokeKind::Get, Self::parent);
        handlers.on(
            "Window",
            "Application",
            InvokeKind::Get,
            Self::application_member,
        );
        handlers.on("Window", "Parent", InvokeKind::Get, Self::parent);
        handlers.on(
            "Application",
            "Name",
            InvokeKind::Get,
            Self::application_name,
        );
        handlers.on("Application", "Version", InvokeKind::Get, Self::version);
        handlers.on("Application", "Hwnd", InvokeKind::Get, Self::hwnd);
        handlers.on("Application", "EnableEvents", InvokeKind::Get, Self::events);
        handlers.on(
            "Application",
            "AutomationSecurity",
            InvokeKind::Get,
            Self::security,
        );
        handlers.on(
            "Application",
            "Path",
            InvokeKind::Get,
            Self::application_path,
        );
        handlers.on("Application", "Workbooks", InvokeKind::Get, Self::workbooks);
        handlers.on(
            "Application",
            "ActiveWorkbook",
            InvokeKind::Get,
            Self::active_workbook,
        );
        handlers.on(
            "Application",
            "ActiveSheet",
            InvokeKind::Get,
            Self::active_sheet,
        );
        handlers.on(
            "Application",
            "Caller",
            InvokeKind::Get,
            Self::needs_workbook,
        );
        handlers.on(
            "Application",
            "ThisCell",
            InvokeKind::Get,
            Self::needs_workbook,
        );
        handlers.on(
            "Application",
            "Range",
            InvokeKind::Get,
            Self::needs_workbook,
        );
        handlers.on(
            "Application",
            "WorksheetFunction",
            InvokeKind::Get,
            Self::functions,
        );
        handlers.on("Workbooks", "Count", InvokeKind::Get, Self::count);
        handlers.on("Workbooks", "Item", InvokeKind::Get, Self::item);
        handlers.on("Workbooks", "_Default", InvokeKind::Get, Self::item);
        handlers.on("Workbook", "Name", InvokeKind::Get, Self::book_name);
        handlers.on("Workbook", "FullName", InvokeKind::Get, Self::book_name);
        handlers.on("Workbook", "Path", InvokeKind::Get, Self::book_path);
        handlers.on(
            "Workbook",
            "ActiveSheet",
            InvokeKind::Get,
            Self::active_sheet,
        );
        handlers.on("Worksheet", "Name", InvokeKind::Get, Self::sheet_name);
        handlers.on("Window", "Hwnd", InvokeKind::Get, Self::hwnd);
        // Event targets of a host without a workbook model (the command-line harness) are
        // inline references: their geometry answers here, their values need a workbook.
        handlers.on("Range", "Address", InvokeKind::Get, Self::range_address);
        handlers.on("Range", "Row", InvokeKind::Get, Self::range_row);
        handlers.on("Range", "Column", InvokeKind::Get, Self::range_column);
        handlers.on("Range", "Count", InvokeKind::Get, Self::range_count);
        handlers.on("Range", "Worksheet", InvokeKind::Get, Self::range_sheet);
        handlers.on(
            "Range",
            "Application",
            InvokeKind::Get,
            Self::range_application,
        );
        handlers.on("Range", "Parent", InvokeKind::Get, Self::range_sheet);
        handlers.on("Range", "Value", InvokeKind::Get, Self::needs_workbook);
        handlers.on("Range", "Value2", InvokeKind::Get, Self::needs_workbook);
        handlers.on("Application", "Run", InvokeKind::Method, Self::run);
        handlers.on(
            "Application",
            "Union",
            InvokeKind::Method,
            Self::needs_workbook,
        );
        handlers.on(
            "Application",
            "EnableEvents",
            InvokeKind::Put,
            Self::put_events,
        );
        super::worksheet_function::register(
            &mut handlers,
            Self::worksheet_function,
            Self::application_member,
            Self::creator,
        );
        Self { host, handlers }
    }
    pub fn host(&self) -> &H {
        &self.host
    }
    pub fn handlers(&self) -> &Handlers<Self> {
        &self.handlers
    }
    fn worksheet_function(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        super::worksheet_function::dispatch(
            call,
            super::worksheet_function::to_calc,
            |name, args| {
                super::worksheet_function::catalogue_call(
                    name,
                    args,
                    &mut calc_func::EvalServices::new(&calc_value::NumericMode::default()),
                )
            },
        )
    }
    fn needs_workbook(_: &Self, _: &ObjectRef, _: &Call<'_>) -> Result<AutoValue, AutoError> {
        Err(AutoError::excel(1004, "needs a workbook: open in DNA Calc"))
    }
    fn no_args(call: &Call<'_>) -> Result<(), AutoError> {
        if call.args.is_empty() {
            Ok(())
        } else {
            Err(bad_count())
        }
    }
    pub fn application_member(
        &self,
        _: &ObjectRef,
        call: &Call<'_>,
    ) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::Object(self.application()))
    }
    fn parent(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::Object(match target.key {
            ObjectKey::Worksheet { workbook, .. } => {
                object("Workbook", ObjectKey::Workbook(workbook))
            }
            ObjectKey::Window => object("Workbook", ObjectKey::Workbook(0)),
            _ => self.application(),
        }))
    }
    /// Shared Workbooks.Item parsing; the workbook profile supplies its current name and identity.
    pub fn workbook_item(
        call: &Call<'_>,
        name: &str,
        workbook: u64,
    ) -> Result<AutoValue, AutoError> {
        if call.args.len() != 1 {
            return Err(bad_count());
        }
        let found = match call.arg(0) {
            AutoValue::Text(text) => text.to_string_lossy().eq_ignore_ascii_case(name),
            AutoValue::I2(n) => *n == 1,
            AutoValue::I4(n) => *n == 1,
            AutoValue::R8(n) => *n == 1.,
            AutoValue::R4(n) => *n == 1.,
            _ => false,
        };
        if found {
            Ok(AutoValue::Object(object(
                "Workbook",
                ObjectKey::Workbook(workbook),
            )))
        } else {
            Err(AutoError::Status(0x8002_000Bu32 as i32))
        }
    }
    fn item(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::workbook_item(call, "Book1", 0)
    }
    fn application_name(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::text("Microsoft Excel"))
    }
    fn version(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::text("16.0"))
    }
    fn hwnd(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::I4(self.host.hwnd() as i32))
    }
    fn events(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::Bool(self.host.events_enabled()))
    }
    fn security(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::I4(1))
    }
    fn application_path(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::text(&self.host.path()?))
    }
    fn workbooks(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::Object(object("Workbooks", ObjectKey::Workbooks)))
    }
    fn active_workbook(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::Object(object(
            "Workbook",
            ObjectKey::Workbook(0),
        )))
    }
    fn book_name(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::text("Book1"))
    }
    fn book_path(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::text(""))
    }
    fn sheet_name(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::text(
            &self.host.shell_sheet().unwrap_or_default(),
        ))
    }
    fn functions(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::Object(object(
            "WorksheetFunction",
            ObjectKey::WorksheetFunction,
        )))
    }
    fn creator(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::I4(0x5843_454C))
    }
    fn count(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        Ok(AutoValue::I4(1))
    }
    fn active_sheet(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        Self::no_args(call)?;
        match self.host.shell_sheet() {
            Some(_) => Ok(AutoValue::Object(object(
                "Worksheet",
                ObjectKey::Worksheet {
                    workbook: 0,
                    sheet: calc_value::SheetId::from_parts(0, 0),
                },
            ))),
            None => Err(AutoError::not_modelled(call.label(), "dc-svqh")),
        }
    }
    fn inline_range<'a>(
        &self,
        target: &'a ObjectRef,
        call: &Call<'_>,
    ) -> Result<&'a ReferenceLike, AutoError> {
        match &target.key {
            ObjectKey::Range(StoredReference::Inline(reference)) => Ok(reference),
            _ => Err(Self::needs_workbook(self, target, call).unwrap_err()),
        }
    }
    fn range_address(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        let reference = self.inline_range(target, call)?;
        let options = AddressOptions::from_call(call)?;
        if options.external || options.relative_to.is_some() {
            return Err(AutoError::not_modelled(call.label(), "dc-1nbp.5"));
        }
        Ok(AutoValue::text(&geometry::address(
            reference, options, None,
        )?))
    }
    fn range_row(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        let reference = self.inline_range(target, call)?;
        let _ = reference;
        Ok(AutoValue::I4(geometry::row(reference) as i32))
    }
    fn range_column(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        let reference = self.inline_range(target, call)?;
        let _ = reference;
        Ok(AutoValue::I4(geometry::column(reference) as i32))
    }
    fn range_count(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        let reference = self.inline_range(target, call)?;
        let _ = reference;
        Ok(geometry::count_value(geometry::count(
            reference,
            RangeAxis::Cells,
        )))
    }
    fn range_sheet(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        let reference = self.inline_range(target, call)?;
        let _ = reference;
        Ok(AutoValue::Object(object(
            "Worksheet",
            ObjectKey::Worksheet {
                workbook: 0,
                sheet: reference.sheet(),
            },
        )))
    }
    fn range_application(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
    ) -> Result<AutoValue, AutoError> {
        let reference = self.inline_range(target, call)?;
        let _ = reference;
        Ok(AutoValue::Object(self.application()))
    }
    fn put_events(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        if call.args.len() != 1 {
            return Err(bad_count());
        }
        self.host.set_events_enabled(boolean(call.arg(0))?);
        Ok(AutoValue::Empty)
    }
    fn run(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        if call.args.is_empty() || call.args.len() > 31 {
            return Err(bad_count());
        }
        let name = call.arg(0).as_text().ok_or_else(type_mismatch)?;
        let mut end = call.args.len();
        while end > 1
            && matches!(
                call.arg(end - 1),
                AutoValue::Missing | AutoValue::Empty | AutoValue::Null
            )
        {
            end -= 1;
        }
        self.host.run(&name, &call.args[1..end])
    }
}
impl<H: ApplicationHost> ObjectModel for ValuesOnly<H> {
    fn application(&self) -> ObjectRef {
        object("Application", ObjectKey::Application)
    }
    fn invoke(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        self.handlers.dispatch(self, target, call, "dc-1nbp.5")
    }
    fn record_refusal(&self, error: &AutoError) {
        self.host.record_refusal(error);
    }
}
