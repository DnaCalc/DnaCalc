use calc_value::{CalcValue, CellKey, CoreRef, ExcelText, GridArea, ReferenceLike, SheetId};
use calc_xll_host::XllHost;
use std::{cell::RefCell, ffi::c_void, rc::Rc};

#[repr(C)]
pub(crate) struct Value {
    pub kind: u32,
    pub number: f64,
    pub text: *const u16,
    pub length: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Area {
    row: u32,
    column: u32,
    end_row: u32,
    end_column: u32,
}
type Bridge = unsafe extern "system" fn(*const c_void, u32, *mut Value) -> i32;
struct WorkbookBinding {
    bridge: RefCell<Option<Rc<dyn crate::WorkbookBridge>>>,
    host: Rc<XllHost>,
    areas: RefCell<Vec<Area>>,
}
#[repr(C)]
struct RangeSpec {
    handle: *mut c_void,
    sheet: u64,
    name: *mut u16,
    areas: *const Area,
    count: usize,
}
type RangeApi = unsafe extern "system" fn(
    *const c_void,
    u32,
    *mut c_void,
    *const u16,
    usize,
    *mut RangeSpec,
) -> i32;
type RangeRead = unsafe extern "system" fn(*const c_void, *mut c_void, bool, *mut c_void) -> i32;
type RangeFree = unsafe extern "system" fn(*mut c_void);

unsafe extern "system" fn workbook_bridge(
    context: *const c_void,
    operation: u32,
    output: *mut Value,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let binding = unsafe { &*context.cast::<WorkbookBinding>() };
        let bridge = binding.bridge.borrow().clone();
        let Some(bridge) = bridge else {
            return 0x80004001u32 as i32;
        };
        let output = unsafe { &mut *output };
        if operation == 2 {
            output.number = f64::from(u8::from(bridge.events_enabled()));
            return 0;
        }
        if operation == 3 {
            bridge.set_events_enabled(output.number != 0.);
            return 0;
        }
        let text = match operation {
            0 => bridge.full_name(),
            1 => bridge.active_sheet().unwrap_or_default(),
            _ => return 0x80004001u32 as i32,
        };
        let units: Vec<_> = text.encode_utf16().collect();
        output.text = unsafe { SysAllocStringLen(units.as_ptr(), units.len() as u32) };
        if output.text.is_null() {
            return 0x8007000eu32 as i32;
        }
        output.length = units.len();
        0
    }))
    .unwrap_or(0x80004005u32 as i32)
}
unsafe extern "system" fn range_free(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<calc_func::StoredReference>()) });
    }
}
fn range_failure(context: *const c_void, why: &str) -> i32 {
    if why.contains("not modelled") || why.contains("NotModelledEdit") {
        // A swallowed COM exception must still leave a typed refusal on its formula.
        let binding = unsafe { &*context.cast::<WorkbookBinding>() };
        let _ = binding.host.with_workbook_context(|context| {
            Ok(context.refuse_by("COM workbook range operation is not modelled", "dc-0f1k.24"))
        });
    }
    eprintln!("COM Range: {why}");
    0x800a03ecu32 as i32
}
unsafe extern "system" fn range_api(
    context: *const c_void,
    operation: u32,
    handle: *mut c_void,
    address: *const u16,
    length: usize,
    output: *mut RangeSpec,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let binding = unsafe { &*context.cast::<WorkbookBinding>() };
        let bridge = binding
            .bridge
            .borrow()
            .clone()
            .ok_or("dc-0f1k.24: workbook closed")?;
        let output = unsafe { &mut *output };
        let reference = match operation {
            0 => {
                let address =
                    String::from_utf16(unsafe { std::slice::from_raw_parts(address, length) })
                        .map_err(|_| "dc-0f1k.24: invalid range text")?;
                bridge.range(&address)?
            }
            1 => {
                let caller = match binding
                    .host
                    .with_workbook_context(|context| Ok(context.caller()))
                {
                    Ok(Some(caller)) => caller,
                    Ok(None) => return Ok(false),
                    Err(why) if why.ends_with("no admitted worksheet invocation") => {
                        return Ok(false);
                    }
                    Err(why) => return Err(why),
                };
                bridge.retain(&ReferenceLike::cell(caller))?
            }
            2 if !handle.is_null() => {
                unsafe { &*handle.cast::<calc_func::StoredReference>() }.clone()
            }
            3 => {
                let sheet = SheetId::from_parts(output.sheet as u32, (output.sheet >> 32) as u32);
                let areas = unsafe { std::slice::from_raw_parts(output.areas, output.count) }
                    .iter()
                    .map(|area| {
                        let first = CellKey::new(
                            sheet,
                            area.row.checked_add(1).ok_or("invalid row")?,
                            area.column.checked_add(1).ok_or("invalid column")?,
                        )
                        .map_err(|_| "invalid range start")?;
                        let last = CellKey::new(
                            sheet,
                            area.end_row.checked_add(1).ok_or("invalid row")?,
                            area.end_column.checked_add(1).ok_or("invalid column")?,
                        )
                        .map_err(|_| "invalid range end")?;
                        GridArea::new(first, last).map_err(|_| "invalid range")
                    })
                    .collect::<Result<Vec<_>, &str>>()?;
                bridge.retain(
                    &ReferenceLike::from_areas(areas)
                        .map_err(|_| "dc-0f1k.24: Union crosses worksheets")?,
                )?
            }
            _ => return Err("dc-0f1k.24: invalid range operation".into()),
        };
        let resolved = bridge.resolve(&reference)?;
        let sheet = resolved.areas()[0].sheet();
        let units: Vec<_> = bridge.sheet_name(sheet)?.encode_utf16().collect();
        let name = unsafe { SysAllocStringLen(units.as_ptr(), units.len() as u32) };
        if name.is_null() {
            return Err("COM range sheet allocation failed".into());
        }
        let mut areas = binding.areas.borrow_mut();
        *areas = resolved
            .areas()
            .iter()
            .map(|a| Area {
                row: a.first().row() - 1,
                column: a.first().column() - 1,
                end_row: a.last().row() - 1,
                end_column: a.last().column() - 1,
            })
            .collect();
        output.handle = if operation == 2 {
            handle
        } else {
            Box::into_raw(Box::new(reference)).cast()
        };
        output.sheet = u64::from(sheet.slot()) | (u64::from(sheet.generation()) << 32);
        output.name = name;
        output.areas = areas.as_ptr();
        output.count = areas.len();
        Ok::<_, String>(true)
    }))
    .map_or(0x80004005u32 as i32, |result| match result {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(why) => range_failure(context, &why),
    })
}
unsafe extern "system" fn range_read(
    context: *const c_void,
    handle: *mut c_void,
    value2: bool,
    output: *mut c_void,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if handle.is_null() {
            return Err("dc-0f1k.24: no workbook range".into());
        }
        let binding = unsafe { &*context.cast::<WorkbookBinding>() };
        let bridge = binding
            .bridge
            .borrow()
            .clone()
            .ok_or("dc-0f1k.24: workbook closed")?;
        let reference = bridge.resolve(unsafe { &*handle.cast::<calc_func::StoredReference>() })?;
        // Excel Value reads the first area. Other areas remain part of Address/Count/Union.
        let area = reference.areas()[0];
        let reference = ReferenceLike::area(area);
        let value = binding
            .host
            .with_workbook_context(|context| context.values_of(&reference))?;
        let (rows, columns, cells) = match value.as_array() {
            Some(array) => (
                array.shape().rows as u32,
                array.shape().cols as u32,
                array.iter_row_major().collect::<Vec<_>>(),
            ),
            None => (0, 0, vec![&value]),
        };
        let mut texts: Vec<Vec<u16>> = Vec::new();
        let mut values = Vec::with_capacity(cells.len());
        for (index, cell) in cells.into_iter().enumerate() {
            if !value2 && matches!(cell.core_ref(), CoreRef::Number(_)) {
                let at = CellKey::new(
                    area.sheet(),
                    area.first().row() + (index / area.column_count() as usize) as u32,
                    area.first().column() + (index % area.column_count() as usize) as u32,
                )
                .map_err(|_| "invalid Value coordinate")?;
                if let Err(why) = bridge.value_format_supported(at) {
                    let _ = binding.host.with_workbook_context(|context| {
                        Ok(context.refuse_by(
                            "Range.Value format conversion is not modelled",
                            "dc-0f1k.24",
                        ))
                    });
                    return Err(why);
                }
            }
            let mut v = Value {
                kind: 0,
                number: 0.,
                text: std::ptr::null(),
                length: 0,
            };
            match cell.core_ref() {
                CoreRef::Empty | CoreRef::Missing => {}
                CoreRef::Number(number) => {
                    v.kind = 1;
                    v.number = number;
                }
                CoreRef::Logical(flag) => {
                    v.kind = 3;
                    v.number = f64::from(u8::from(flag));
                }
                CoreRef::Text(text) => {
                    let units = text.utf16_code_units().to_vec();
                    v.kind = 2;
                    v.text = units.as_ptr();
                    v.length = units.len();
                    texts.push(units);
                }
                CoreRef::Error(error) => {
                    v.kind = 5;
                    v.number = f64::from(
                        2000 + calc_xll_host::error_number(error)
                            .ok_or("dc-0f1k.24: COM error code not modelled")?,
                    );
                }
                _ => return Err("dc-0f1k.24: COM range cell kind not modelled".into()),
            }
            values.push(v);
        }
        let status = unsafe { dna_com_values(output, values.as_ptr(), rows, columns) };
        if status < 0 {
            return Err(format!("COM Value conversion: {status:#x}"));
        }
        Ok::<_, String>(())
    }))
    .map_or(0x80004005u32 as i32, |result| match result {
        Ok(()) => 0,
        Err(why) => range_failure(context, &why),
    })
}
type Run = unsafe extern "system" fn(
    *const c_void,
    *const u16,
    usize,
    *const Value,
    usize,
    *mut Value,
) -> i32;
type Qualifier =
    unsafe extern "system" fn(*const u16, usize, *const u16, usize, *mut *mut u16) -> i32;
type OwnsModule = unsafe extern "system" fn(*const c_void, *const u16, usize) -> bool;
#[link(name = "oleaut32")]
unsafe extern "system" {
    fn SysAllocStringLen(text: *const u16, length: u32) -> *mut u16;
}
unsafe extern "C" {
    #[cfg(test)]
    fn dna_com_test_events() -> i32;
    #[cfg(test)]
    fn dna_com_test_ranges() -> i32;
    #[cfg(test)]
    fn dna_com_test_workbook_bridge() -> i32;
    #[cfg(test)]
    fn dna_com_test_office() -> i32;
    #[cfg(test)]
    fn dna_com_test_refusals() -> i32;
    #[cfg(test)]
    fn dna_com_test_ribbon_reentry() -> i32;
    fn dna_com_create(
        hwnd: usize,
        context: *const c_void,
        run: Run,
        qualifier: Qualifier,
        owns_module: OwnsModule,
    ) -> *mut c_void;
    fn dna_com_ranges(
        projection: *mut c_void,
        api: RangeApi,
        read: RangeRead,
        free: RangeFree,
    ) -> i32;
    fn dna_com_values(output: *mut c_void, values: *const Value, rows: u32, columns: u32) -> i32;
    fn dna_com_bridge(projection: *mut c_void, context: *const c_void, bridge: Bridge) -> i32;
    fn dna_com_getobject(projection: *mut c_void, wparam: usize, object_id: isize) -> isize;
    fn dna_com_destroy(projection: *mut c_void);
    fn dna_com_close_office(projection: *mut c_void) -> i32;
    fn dna_com_ribbons(
        projection: *mut c_void,
        visit: unsafe extern "system" fn(*mut c_void, *const u16, usize, *const u16, usize),
        context: *mut c_void,
    ) -> i32;
    fn dna_com_verify(projection: *mut c_void) -> i32;
    fn dna_com_workbook(projection: *mut c_void, name: *const u16, length: usize) -> i32;
    fn dna_com_sheet(projection: *mut c_void, name: *const u16, length: usize) -> i32;
    fn dna_com_enable_events(projection: *mut c_void, enabled: bool) -> i32;
    fn dna_com_sheet_change(projection: *mut c_void, row: u32, column: u32) -> i32;
    fn dna_com_sheet_change_areas(
        projection: *mut c_void,
        sheet: u64,
        name: *const u16,
        length: usize,
        areas: *const Area,
        count: usize,
    ) -> i32;
    fn dna_com_after_calculate(projection: *mut c_void) -> i32;
    fn dna_com_run_early(
        projection: *mut c_void,
        name: *const u16,
        length: usize,
        output: *mut Value,
    ) -> i32;
}

/// Native-only: tests the full workbook projection, Range TypeName and SAFEARRAY bounds.
#[test]
fn workbook_ranges_project_caller_union_metadata_and_values() {
    let status = unsafe { dna_com_test_ranges() };
    assert_eq!(status, 0, "COM workbook ranges: {status:#x}");
}
/// Native-only: exercises Excel COM interfaces, retained Range objects and reentrant event sinks.
#[test]
fn event_source_owns_cookies_and_disconnects_retained_ranges() {
    let status = unsafe { dna_com_test_events() };
    assert_eq!(status, 0, "COM event lifecycle: {status:#x}");
}
/// Native-only: exercises workbook metadata and the shared event switch across the C++ ABI.
#[test]
fn workbook_bridge_reads_current_identity_selection_and_event_switch() {
    let status = unsafe { dna_com_test_workbook_bridge() };
    assert_eq!(status, 0, "COM workbook bridge: {status:#x}");
}
/// Native-only: exercises the real COM dispatch tree and retained objects after teardown.
#[test]
fn office_controls_track_edits_and_disconnect_retained_objects() {
    let status = unsafe { dna_com_test_office() };
    assert_eq!(status, 0, "Office control lifecycle: {status:#x}");
}
/// Native-only: the values-only profile refuses Application.Caller and ThisCell (dc-svqh).
#[test]
fn caller_and_this_cell_need_a_workbook() {
    let status = unsafe { dna_com_test_refusals() };
    assert_eq!(status, 0, "Caller and ThisCell refusals: {status:#x}");
}
/// Native-only: actual COM providers mutate the collection and disconnect during UI callbacks.
#[test]
fn ribbon_callbacks_retain_objects_and_reject_stale_connections() {
    let status = unsafe { dna_com_test_ribbon_reentry() };
    assert_eq!(status, 0, "Ribbon reentry: {status:#x}");
}

/// Lives on the window's STA; disconnected COM references fail after it is dropped.
pub struct Projection {
    pointer: *mut c_void,
    _host: Rc<XllHost>,
    workbook: Box<WorkbookBinding>,
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
        let pointer =
            unsafe { dna_com_create(hwnd, Rc::as_ptr(&host).cast(), run, qualifier, owns_module) };
        if pointer.is_null() {
            return Err("cannot create local COM projection".into());
        }
        Ok(Self {
            pointer,
            _host: host.clone(),
            workbook: Box::new(WorkbookBinding {
                bridge: RefCell::new(None),
                host,
                areas: RefCell::default(),
            }),
        })
    }
    pub fn bind_workbook(&self, bridge: Rc<dyn crate::WorkbookBridge>) -> Result<(), String> {
        *self.workbook.bridge.borrow_mut() = Some(bridge);
        Self::checked(
            unsafe { dna_com_ranges(self.pointer, range_api, range_read, range_free) },
            "COM workbook ranges",
        )?;
        Self::checked(
            unsafe {
                dna_com_bridge(
                    self.pointer,
                    (&*self.workbook as *const WorkbookBinding).cast(),
                    workbook_bridge,
                )
            },
            "COM workbook bridge",
        )
    }
    /// All committed input areas, in their owning worksheet, after its VBA handler.
    pub fn sheet_change_areas(
        &self,
        sheet: SheetId,
        name: &str,
        areas: &[calc_value::GridArea],
    ) -> Result<(), String> {
        let name: Vec<_> = name.encode_utf16().collect();
        let areas: Vec<_> = areas
            .iter()
            .map(|a| Area {
                row: a.first().row() - 1,
                column: a.first().column() - 1,
                end_row: a.last().row() - 1,
                end_column: a.last().column() - 1,
            })
            .collect();
        Self::checked(
            unsafe {
                dna_com_sheet_change_areas(
                    self.pointer,
                    u64::from(sheet.slot()) | (u64::from(sheet.generation()) << 32),
                    name.as_ptr(),
                    name.len(),
                    areas.as_ptr(),
                    areas.len(),
                )
            },
            "COM SheetChange",
        )
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
    /// Bind the owning workbook identity on its STA before loading add-ins.
    pub fn set_workbook(&self, name: &str) -> Result<(), String> {
        let units = name.encode_utf16().collect::<Vec<_>>();
        Self::checked(
            unsafe { dna_com_workbook(self.pointer, units.as_ptr(), units.len()) },
            "COM workbook",
        )
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
        self._host.close()?;
        Self::checked(
            unsafe { dna_com_close_office(self.pointer) },
            "COM add-in teardown",
        )
    }
    /// Request the connected add-ins' actual IRibbonExtensibility callbacks.
    pub fn ribbons(&self) -> Result<Vec<(String, String)>, String> {
        unsafe extern "system" fn visit(
            context: *mut c_void,
            progid: *const u16,
            progid_length: usize,
            xml: *const u16,
            xml_length: usize,
        ) {
            let rows = unsafe { &mut *context.cast::<Vec<(String, String)>>() };
            rows.push((
                String::from_utf16_lossy(unsafe {
                    std::slice::from_raw_parts(progid, progid_length)
                }),
                String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(xml, xml_length) }),
            ));
        }
        let mut rows = Vec::new();
        Self::checked(
            unsafe {
                dna_com_ribbons(
                    self.pointer,
                    visit,
                    (&mut rows as *mut Vec<(String, String)>).cast(),
                )
            },
            "Ribbon GetCustomUI",
        )?;
        Ok(rows)
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

unsafe extern "system" fn owns_module(
    context: *const c_void,
    path: *const u16,
    length: usize,
) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        use std::os::windows::ffi::OsStringExt;
        let path =
            std::ffi::OsString::from_wide(unsafe { std::slice::from_raw_parts(path, length) });
        unsafe { &*context.cast::<XllHost>() }.contains_loaded_module(std::path::Path::new(&path))
    }))
    .unwrap_or(false)
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
        match unsafe { &*context.cast::<XllHost>() }.run_variant(&name, &values) {
            Ok((value, integer)) => {
                let out = unsafe { &mut *result };
                match value.core_ref() {
                    CoreRef::Number(n) => {
                        out.kind = if integer { 4 } else { 1 };
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

unsafe extern "system" fn qualifier(
    book: *const u16,
    book_len: usize,
    sheet: *const u16,
    sheet_len: usize,
    output: *mut *mut u16,
) -> i32 {
    if output.is_null() {
        return 0x80004003u32 as i32;
    }
    unsafe {
        *output = std::ptr::null_mut();
    }
    let book = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(book, book_len) });
    let sheet = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(sheet, sheet_len) });
    let text = calc_xll_host::format_workbook_sheet_qualifier(&book, &sheet)
        .encode_utf16()
        .collect::<Vec<_>>();
    let value = unsafe { SysAllocStringLen(text.as_ptr(), text.len() as u32) };
    if value.is_null() {
        return 0x8007000eu32 as i32;
    }
    unsafe {
        *output = value;
    }
    0
}
