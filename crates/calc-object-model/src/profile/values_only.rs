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
        for (interface, names) in [
            (
                "Application",
                &[
                    "Application",
                    "Parent",
                    "Name",
                    "Version",
                    "Hwnd",
                    "EnableEvents",
                    "Path",
                    "Workbooks",
                    "ActiveWorkbook",
                    "ActiveSheet",
                    "Caller",
                    "ThisCell",
                    "Range",
                    "WorksheetFunction",
                ][..],
            ),
            (
                "Workbooks",
                &["Application", "Parent", "Count", "Item", "_Default"][..],
            ),
            (
                "Workbook",
                &[
                    "Application",
                    "Parent",
                    "Name",
                    "FullName",
                    "Path",
                    "ActiveSheet",
                ][..],
            ),
            ("Worksheet", &["Application", "Parent", "Name"][..]),
            ("Window", &["Application", "Parent", "Hwnd"][..]),
        ] {
            for name in names {
                handlers.on(interface, name, InvokeKind::Get, Self::get);
            }
        }
        // Event targets of a host without a workbook model (the command-line harness) are
        // inline references: their geometry answers here, their values need a workbook.
        for name in [
            "Address",
            "Row",
            "Column",
            "Count",
            "Worksheet",
            "Application",
            "Parent",
            "Value",
            "Value2",
        ] {
            handlers.on("Range", name, InvokeKind::Get, Self::range);
        }
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
        super::worksheet_function::register(&mut handlers, Self::worksheet_function);
        Self { host, handlers }
    }
    pub fn host(&self) -> &H {
        &self.host
    }
    pub fn handlers(&self) -> &Handlers<Self> {
        &self.handlers
    }
    fn worksheet_function(&self, _: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        if call.member.invoke == InvokeKind::Get {
            if !call.args.is_empty() {
                return Err(bad_count());
            }
            return Ok(match call.member.name.as_str() {
                "Application" | "Parent" => AutoValue::Object(self.application()),
                "Creator" => AutoValue::I4(0x5843_454C),
                _ => return Err(AutoError::not_modelled(call.label(), "dc-1nbp.5")),
            });
        }
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
    fn get(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        let name = call.member.name.as_str();
        if matches!(target.key, ObjectKey::Application)
            && matches!(name, "Caller" | "ThisCell" | "Range")
        {
            return Self::needs_workbook(self, target, call);
        }
        if matches!(target.key, ObjectKey::Workbooks) && matches!(name, "Item" | "_Default") {
            if call.args.len() != 1 {
                return Err(bad_count());
            }
            let found = match call.arg(0) {
                AutoValue::Text(_) => call.arg(0).as_text().unwrap().eq_ignore_ascii_case("Book1"),
                AutoValue::I2(n) => *n == 1,
                AutoValue::I4(n) => *n == 1,
                AutoValue::R8(n) => *n == 1.,
                AutoValue::R4(n) => *n == 1.,
                _ => false,
            };
            return if found {
                Ok(AutoValue::Object(object(
                    "Workbook",
                    ObjectKey::Workbook(0),
                )))
            } else {
                Err(AutoError::Status(0x8002_000Bu32 as i32))
            };
        }
        if !call.args.is_empty() {
            return Err(bad_count());
        }
        Ok(match name {
            "WorksheetFunction" => {
                AutoValue::Object(object("WorksheetFunction", ObjectKey::WorksheetFunction))
            }
            "Application" => AutoValue::Object(self.application()),
            "Parent" => AutoValue::Object(match target.key {
                ObjectKey::Worksheet { workbook, .. } => {
                    object("Workbook", ObjectKey::Workbook(workbook))
                }
                // A workbook window's Parent is its workbook, as Excel's is; xlOil's COM connect
                // reads Window.Parent as a _Workbook for its Application (Connect.cpp).
                ObjectKey::Window => object("Workbook", ObjectKey::Workbook(0)),
                _ => self.application(),
            }),
            "Name" => AutoValue::text(match target.key {
                ObjectKey::Application => "Microsoft Excel",
                ObjectKey::Workbook(_) => "Book1",
                ObjectKey::Worksheet { .. } => {
                    return Ok(AutoValue::text(
                        &self.host.shell_sheet().unwrap_or_default(),
                    ));
                }
                _ => return Err(AutoError::not_modelled(call.label(), "dc-1nbp.5")),
            }),
            "Version" => AutoValue::text("16.0"),
            "Hwnd" => AutoValue::I4(self.host.hwnd() as i32),
            "Path" if matches!(target.key, ObjectKey::Application) => {
                AutoValue::text(&self.host.path()?)
            }
            "Path" => AutoValue::text(""),
            "FullName" => AutoValue::text("Book1"),
            "EnableEvents" => AutoValue::Bool(self.host.events_enabled()),
            "Workbooks" => AutoValue::Object(object("Workbooks", ObjectKey::Workbooks)),
            "Count" => AutoValue::I4(1),
            "ActiveWorkbook" => AutoValue::Object(object("Workbook", ObjectKey::Workbook(0))),
            // The Book1 shell has a worksheet only when the host names one (the workbook harness).
            "ActiveSheet" => match self.host.shell_sheet() {
                Some(_) => AutoValue::Object(object(
                    "Worksheet",
                    ObjectKey::Worksheet {
                        workbook: 0,
                        sheet: calc_value::SheetId::from_parts(0, 0),
                    },
                )),
                None => return Err(AutoError::not_modelled(call.label(), "dc-svqh")),
            },
            _ => return Err(AutoError::not_modelled(call.label(), "dc-1nbp.5")),
        })
    }
    fn range(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue, AutoError> {
        let ObjectKey::Range(StoredReference::Inline(reference)) = &target.key else {
            return Self::needs_workbook(self, target, call);
        };
        let reference: &ReferenceLike = reference;
        let shell = || {
            object(
                "Worksheet",
                ObjectKey::Worksheet {
                    workbook: 0,
                    sheet: reference.areas()[0].sheet(),
                },
            )
        };
        match call.member.name.as_str() {
            "Address" => {
                let options = AddressOptions::from_call(call)?;
                if options.external || options.relative_to.is_some() {
                    return Err(AutoError::not_modelled(call.label(), "dc-1nbp.5"));
                }
                Ok(AutoValue::text(&geometry::address(
                    reference, options, None,
                )?))
            }
            "Row" => Ok(AutoValue::I4(geometry::row(reference) as i32)),
            "Column" => Ok(AutoValue::I4(geometry::column(reference) as i32)),
            "Count" => Ok(geometry::count_value(geometry::count(
                reference,
                RangeAxis::Cells,
            ))),
            "Worksheet" | "Parent" => Ok(AutoValue::Object(shell())),
            "Application" => Ok(AutoValue::Object(self.application())),
            _ => Self::needs_workbook(self, target, call),
        }
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
