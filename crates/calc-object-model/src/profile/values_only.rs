//! OneCalc's application and Book1 shell. Workbook operations require DNA Calc (dc-svqh).

use super::ApplicationHost;
use crate::geometry::{bad_count, boolean, type_mismatch};
use crate::{
    AutoError, AutoValue, Call, Descriptor, Handlers, InvokeKind, ObjectKey, ObjectModel, ObjectRef,
};

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
        Self { host, handlers }
    }
    pub fn host(&self) -> &H {
        &self.host
    }
    pub fn handlers(&self) -> &Handlers<Self> {
        &self.handlers
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
            "Application" => AutoValue::Object(self.application()),
            "Parent" => AutoValue::Object(match target.key {
                ObjectKey::Worksheet { workbook, .. } => {
                    object("Workbook", ObjectKey::Workbook(workbook))
                }
                _ => self.application(),
            }),
            "Name" => AutoValue::text(match target.key {
                ObjectKey::Application => "Microsoft Excel",
                ObjectKey::Workbook(_) => "Book1",
                ObjectKey::Worksheet { .. } => "",
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
            // The values-only projection has no worksheet until a real workbook is bound.
            "ActiveSheet" => return Err(AutoError::not_modelled(call.label(), "dc-svqh")),
            _ => return Err(AutoError::not_modelled(call.label(), "dc-1nbp.5")),
        })
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
}
