use calc_value::{CalcValue, CoreRef, ExcelText};
use calc_xll_host::XllHost;
use std::{ffi::c_void, rc::Rc};

#[repr(C)]
struct Value {
    kind: u32,
    number: f64,
    text: *const u16,
    length: usize,
}
type Run = unsafe extern "system" fn(
    *const c_void,
    *const u16,
    usize,
    *const Value,
    usize,
    *mut Value,
) -> i32;
unsafe extern "C" {
    #[cfg(test)]
    fn dna_com_test_events() -> i32;
    fn dna_com_create(hwnd: usize, context: *const c_void, run: Run) -> *mut c_void;
    fn dna_com_getobject(projection: *mut c_void, wparam: usize, object_id: isize) -> isize;
    fn dna_com_destroy(projection: *mut c_void);
    fn dna_com_verify(projection: *mut c_void) -> i32;
    fn dna_com_sheet(projection: *mut c_void, name: *const u16, length: usize) -> i32;
    fn dna_com_enable_events(projection: *mut c_void, enabled: bool) -> i32;
    fn dna_com_sheet_change(projection: *mut c_void, row: u32, column: u32) -> i32;
    fn dna_com_after_calculate(projection: *mut c_void) -> i32;
    fn dna_com_run_early(
        projection: *mut c_void,
        name: *const u16,
        length: usize,
        output: *mut Value,
    ) -> i32;
}

/// Native-only: exercises Excel COM interfaces, retained Range objects and reentrant event sinks.
#[test]
fn event_source_owns_cookies_and_disconnects_retained_ranges() {
    let status = unsafe { dna_com_test_events() };
    assert_eq!(status, 0, "COM event lifecycle: {status:#x}");
}

/// Lives on the window's STA; disconnected COM references fail after it is dropped.
pub struct Projection {
    pointer: *mut c_void,
    _host: Rc<XllHost>,
}
impl Projection {
    fn checked(status: i32, operation: &str) -> Result<(), String> {
        if status < 0 {
            Err(format!("{operation}: {status:#x}"))
        } else {
            Ok(())
        }
    }
    pub fn new(hwnd: usize, host: Rc<XllHost>) -> Result<Self, String> {
        let pointer = unsafe { dna_com_create(hwnd, Rc::as_ptr(&host).cast(), run) };
        if pointer.is_null() {
            return Err("cannot create local COM projection".into());
        }
        Ok(Self {
            pointer,
            _host: host,
        })
    }
    /// Called only by the workbook window's WM_GETOBJECT handler.
    pub fn get_object(&self, wparam: usize, object_id: isize) -> isize {
        unsafe { dna_com_getobject(self.pointer, wparam, object_id) }
    }
    /// Checks Excel IIDs and the measured early-bound slots, without activating Excel.
    pub fn verify_layout(&self) -> Result<(), String> {
        let status = unsafe { dna_com_verify(self.pointer) };
        if status < 0 {
            Err(format!("COM layout check failed: {status:#x}"))
        } else {
            Ok(())
        }
    }
    /// Bind the real session's single worksheet name before loading add-ins.
    pub fn set_sheet(&self, name: &str) -> Result<(), String> {
        let units = name.encode_utf16().collect::<Vec<_>>();
        Self::checked(
            unsafe { dna_com_sheet(self.pointer, units.as_ptr(), units.len()) },
            "COM worksheet",
        )
    }
    pub fn enable_events(&self, enabled: bool) -> Result<(), String> {
        Self::checked(
            unsafe { dna_com_enable_events(self.pointer, enabled) },
            "COM EnableEvents",
        )
    }
    /// Notify after a committed session edit; coordinates use the model's one-based grid.
    pub fn sheet_change(&self, row: u32, column: u32) -> Result<(), String> {
        let row = row.checked_sub(1).ok_or("invalid event row")?;
        let column = column.checked_sub(1).ok_or("invalid event column")?;
        Self::checked(
            unsafe { dna_com_sheet_change(self.pointer, row, column) },
            "COM SheetChange",
        )
    }
    pub fn after_calculate(&self) -> Result<(), String> {
        Self::checked(
            unsafe { dna_com_after_calculate(self.pointer) },
            "COM AfterCalculate",
        )
    }
    /// Invoke Excel's early-bound Run slot with its thirty optional arguments omitted.
    pub fn run_early(&self, name: &str) -> Result<CalcValue, String> {
        let name = name.encode_utf16().collect::<Vec<_>>();
        let mut output = Value {
            kind: 0,
            number: 0.,
            text: std::ptr::null(),
            length: 0,
        };
        let status =
            unsafe { dna_com_run_early(self.pointer, name.as_ptr(), name.len(), &mut output) };
        if status < 0 {
            return Err(format!("early-bound Application.Run: {status:#x}"));
        }
        match output.kind {
            1 => Ok(CalcValue::number(output.number)),
            3 => Ok(CalcValue::logical(output.number != 0.)),
            _ => Err("unsupported COM Run return".into()),
        }
    }
    /// Close add-ins while the workbook window can still answer Application lookup.
    pub fn close_addins(&self) -> Result<(), String> {
        self._host.close()
    }
}
impl Drop for Projection {
    fn drop(&mut self) {
        // Give add-ins their final main-lane callbacks while Application is still connected.
        if let Err(why) = self.close_addins() {
            eprintln!("COM projection shutdown: {why}");
        }
        unsafe {
            dna_com_destroy(self.pointer);
        }
    }
}

unsafe extern "system" fn run(
    context: *const c_void,
    name: *const u16,
    length: usize,
    args: *const Value,
    count: usize,
    result: *mut Value,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let name = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(name, length) });
        let args = if count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(args, count) }
        };
        let values = args
            .iter()
            .map(|v| match v.kind {
                0 => Ok(CalcValue::missing()),
                1 => Ok(CalcValue::number(v.number)),
                2 => Ok(CalcValue::text(ExcelText::from_utf16_code_units(
                    unsafe { std::slice::from_raw_parts(v.text, v.length) }.to_vec(),
                ))),
                3 => Ok(CalcValue::logical(v.number != 0.)),
                _ => Err(()),
            })
            .collect::<Result<Vec<_>, _>>();
        let Ok(values) = values else {
            return 0x80020005u32 as i32;
        };
        match unsafe { &*context.cast::<XllHost>() }.run(&name, &values) {
            Ok(value) => {
                let out = unsafe { &mut *result };
                match value.core_ref() {
                    CoreRef::Number(n) => {
                        out.kind = 1;
                        out.number = n;
                    }
                    CoreRef::Logical(b) => {
                        out.kind = 3;
                        out.number = f64::from(u8::from(b));
                    }
                    _ => return 0x80004001u32 as i32,
                }
                0
            }
            Err(why) => {
                eprintln!("Application.Run: {why}");
                0x800a03ecu32 as i32
            }
        }
    }))
    .unwrap_or(0x80004005u32 as i32)
}
