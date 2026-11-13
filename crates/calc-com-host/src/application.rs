//! XLL application services for the shared values-only profile.
use crate::values;
use calc_object_model::{AutoError, AutoValue, profile::ApplicationHost};
use calc_xll_host::XllHost;
use std::{cell::Cell, rc::Rc};

pub(crate) struct ApplicationServices {
    pub hwnd: usize,
    pub host: Rc<XllHost>,
    pub events: Cell<bool>,
}
impl ApplicationHost for ApplicationServices {
    fn hwnd(&self) -> isize {
        self.hwnd as isize
    }
    fn run(&self, name: &str, args: &[AutoValue]) -> Result<AutoValue, AutoError> {
        let mut end = args.len();
        while end > 0 && args[end - 1].is_missing() {
            end -= 1;
        }
        let args = args[..end]
            .iter()
            .map(values::to_calc)
            .collect::<Result<Vec<_>, _>>()?;
        let (value, integer) = self
            .host
            .run_variant(name, &args)
            .map_err(|why| AutoError::excel(1004, why))?;
        values::from_calc(&value, integer)
    }
    fn events_enabled(&self) -> bool {
        self.events.get()
    }
    fn set_events_enabled(&self, enabled: bool) {
        self.events.set(enabled);
    }
    fn record_refusal(&self, error: &AutoError) {
        let owner = match error {
            AutoError::NotModelled { owner, .. } => *owner,
            AutoError::Excel {
                number: 1004,
                description,
            } if description == "needs a workbook: open in DNA Calc" => "dc-svqh",
            AutoError::Excel { number: 1004, .. } => "dc-1nbp.5",
            _ => return,
        };
        let _ = self.host.with_workbook_context(|context| {
            Ok(context.refuse_by("COM object-model call refused", owner))
        });
    }
    fn path(&self) -> Result<String, AutoError> {
        std::env::current_exe()
            .map_err(|why| AutoError::excel(1004, why.to_string()))?
            .parent()
            .map(|path| path.to_string_lossy().into_owned())
            .ok_or_else(|| AutoError::excel(1004, "application executable has no directory"))
    }
}
