//! Excel C API `xlc*` commands as translations to object-model
//! members. A command is implemented once, here: it decodes its operands the way the XLM command
//! documents them, then calls the same described member Excel's object model has for the
//! operation (`Range.FormulaR1C1`, `Application.Calculate`, `Names.Add`, ...) on whatever
//! [`ObjectModel`] the host passes in. The command therefore does exactly what the member does:
//! a member the profile has not implemented answers its own typed refusal, and the command passes
//! it through unchanged, so a command starts working when its member lands.
//!
//! The caller is the in-process `Excel12` entry (dc-17spk.16) and `ExecuteExcel4Macro`
//! (dc-17spk.19); neither is in this crate. Operands are [`AutoValue`]s: an omitted operand is
//! [`AutoValue::Missing`], a reference operand is an [`AutoValue::Object`] range or the XLM text
//! form (`R2C3`, `R2C3:R4C5`, always R1C1).
//!
//! Sources: the Excel 4.0 Macro Functions Reference (arguments and codes), MS-XLSB Cetab (operand
//! counts), and docs/architecture/C_API_COMMANDS.md (ordinals). The portable dispatcher currently
//! maps completed member calls to TRUE. The Windows C2 capture records command-specific return
//! values in Excel. Their replay and the remaining operand translations belong to dc-oxh8w.9.

use calc_object_model::{
    AutoError, AutoValue, Call, Descriptor, InvokeKind, ObjectModel, ObjectRef, geometry,
};
use calc_fml::reference::{AxisRef, column_letters};
use calc_value::{MAX_COLUMNS, MAX_ROWS};

/// Owner of every refusal this module raises for an unmeasured form.
const OWNER: &str = "dc-oxh8w.9";

/// One command: its ordinal (the callback number without the `xlCommand` bit), the SDK
/// symbol, the XLM name, the most operands it takes and the object-model member it translates to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XlcCommand {
    /// Ordinal in `XLCALL.H`; the callback number is this ORed with `0x8000`.
    pub ordinal: u16,
    /// SDK symbol, for example `xlcFormula`.
    pub symbol: &'static str,
    /// XLM name, for example `FORMULA`.
    pub xlm: &'static str,
    /// Maximum number of operands in the documented XLM signature.
    pub max_operands: usize,
    /// The member (or members) the command calls, as `Interface.Member`.
    pub member: &'static str,
}

const fn c(
    ordinal: u16,
    symbol: &'static str,
    xlm: &'static str,
    max_operands: usize,
    member: &'static str,
) -> XlcCommand {
    XlcCommand {
        ordinal,
        symbol,
        xlm,
        max_operands,
        member,
    }
}

/// The portable dispatcher commands, by ordinal.
pub const COMMANDS: &[XlcCommand] = &[
    c(4, "xlcSave", "SAVE", 0, "_Workbook.Save"),
    c(17, "xlcRun", "RUN", 2, "_Application.Run"),
    c(22, "xlcSetPrintArea", "SET.PRINT.AREA", 1, "PageSetup.PrintArea"),
    c(31, "xlcCalculateNow", "CALCULATE.NOW", 0, "_Application.Calculate"),
    c(42, "xlcFormatNumber", "FORMAT.NUMBER", 1, "Range.NumberFormat"),
    c(47, "xlcColumnWidth", "COLUMN.WIDTH", 5, "Range.ColumnWidth"),
    c(49, "xlcCut", "CUT", 2, "Range.Cut"),
    c(50, "xlcCopy", "COPY", 2, "Range.Copy"),
    c(51, "xlcPaste", "PASTE", 1, "_Worksheet.Paste"),
    c(52, "xlcClear", "CLEAR", 1, "Range.Clear"),
    c(53, "xlcPasteSpecial", "PASTE.SPECIAL", 7, "Range.PasteSpecial"),
    c(54, "xlcEditDelete", "EDIT.DELETE", 1, "Range.Delete"),
    c(55, "xlcInsert", "INSERT", 2, "Range.Insert"),
    c(61, "xlcDefineName", "DEFINE.NAME", 7, "Names.Add"),
    c(96, "xlcFormula", "FORMULA", 2, "Range.FormulaR1C1"),
    c(97, "xlcFormulaFill", "FORMULA.FILL", 2, "Range.FormulaR1C1"),
    c(109, "xlcSelect", "SELECT", 2, "Range.Select"),
    c(110, "xlcDeleteName", "DELETE.NAME", 1, "Name.Delete"),
    c(122, "xlcMessage", "MESSAGE", 2, "_Application.StatusBar"),
    c(127, "xlcRowHeight", "ROW.HEIGHT", 4, "Range.RowHeight"),
    c(141, "xlcEcho", "ECHO", 1, "_Application.ScreenUpdating"),
    c(167, "xlcCalculateDocument", "CALCULATE.DOCUMENT", 0, "_Worksheet.Calculate"),
    c(354, "xlcWorkbookInsert", "WORKBOOK.INSERT", 1, "Sheets.Add"),
    c(386, "xlcWorkbookName", "WORKBOOK.NAME", 2, "_Worksheet.Name"),
];

/// The command with this ordinal, when the dispatcher serves it.
pub fn command(ordinal: u16) -> Option<&'static XlcCommand> {
    COMMANDS.iter().find(|command| command.ordinal == ordinal)
}

/// The command a callback number (`xlCommand | ordinal`) names; `None` when the command bit is
/// absent or the dispatcher does not serve the ordinal.
pub fn command_for_callback(callback: u32) -> Option<&'static XlcCommand> {
    if callback & 0xC000 != 0x8000 {
        return None;
    }
    command((callback & 0x3FFF) as u16)
}

/// Why a command did not complete.
#[derive(Clone, Debug, PartialEq)]
pub enum XlcError {
    /// The dispatcher does not serve this ordinal (the caller's refusal names the owning bead).
    NotServed(u16),
    /// More operands than the command takes, or an operand of a type the command cannot read
    /// (Excel answers `xlretInvCount` / `xlretInvXloper`).
    Operand(&'static str),
    /// A form whose Excel behaviour is not measured yet; owned by the named bead.
    NotModelled {
        /// The unmeasured form.
        what: &'static str,
        /// Tracker ID owning it.
        owner: &'static str,
    },
    /// The member the command translates to failed: its own typed refusal or Excel error.
    Model(AutoError),
}

impl From<AutoError> for XlcError {
    fn from(error: AutoError) -> Self {
        XlcError::Model(error)
    }
}

type Done = Result<AutoValue, XlcError>;

/// Run the command `ordinal` with `operands` against `model`. Success is TRUE.
pub fn execute(model: &dyn ObjectModel, ordinal: u16, operands: &[AutoValue]) -> Done {
    let spec = command(ordinal).ok_or(XlcError::NotServed(ordinal))?;
    // Trailing omitted operands are as if absent; an omitted one before a given one stays.
    let mut operands = operands;
    while matches!(operands.last(), Some(AutoValue::Missing)) {
        operands = &operands[..operands.len() - 1];
    }
    if operands.len() > spec.max_operands {
        return Err(XlcError::Operand("too many operands"));
    }
    let run = Run { model, operands };
    let result = match ordinal {
        4 => run.save(),
        17 => run.run_macro(),
        22 => run.set_print_area(),
        31 => run.calculate_now(),
        42 => run.format_number(),
        47 => run.column_width(),
        49 => run.copy_cut("Cut"),
        50 => run.copy_cut("Copy"),
        51 => run.paste(),
        52 => run.clear(),
        53 => run.paste_special(),
        54 => run.shift("Delete", [-4159, -4162]),
        55 => run.shift("Insert", [-4161, -4121]),
        61 => run.define_name(),
        96 => run.formula(false),
        97 => run.formula(true),
        109 => run.select(),
        110 => run.delete_name(),
        122 => run.message(),
        127 => run.row_height(),
        141 => run.echo(),
        167 => run.calculate_document(),
        354 => run.workbook_insert(),
        386 => run.workbook_name(),
        _ => Err(XlcError::NotServed(ordinal)),
    };
    if let Err(XlcError::Model(error @ AutoError::NotModelled { .. })) = &result {
        model.record_refusal(error);
    }
    result.map(|_| AutoValue::Bool(true))
}

struct Run<'a> {
    model: &'a dyn ObjectModel,
    operands: &'a [AutoValue],
}

fn refuse(what: &'static str) -> XlcError {
    XlcError::NotModelled { what, owner: OWNER }
}

impl Run<'_> {
    /// A documented optional form may still require a separate translation.
    fn require_supported_operands(&self, count: usize, what: &'static str) -> Result<(), XlcError> {
        if self.operands.len() > count {
            return Err(refuse(what));
        }
        Ok(())
    }

    fn operand(&self, index: usize) -> &AutoValue {
        self.operands.get(index).unwrap_or(&AutoValue::Missing)
    }

    /// Call a described member. `args` are the caller-supplied arguments in declared order; a put's
    /// assigned value is the last.
    fn call(
        &self,
        target: &ObjectRef,
        member: &str,
        kind: InvokeKind,
        args: Vec<AutoValue>,
    ) -> Done {
        let member = Descriptor::get()
            .member(target.interface, member, kind)
            .ok_or(XlcError::Operand("member is not described"))?;
        Ok(self.model.invoke(
            target,
            &Call {
                interface: target.interface,
                member,
                args,
                lcid: 0,
            },
        )?)
    }

    fn object(&self, target: &ObjectRef, member: &str) -> Result<ObjectRef, XlcError> {
        match self.call(target, member, InvokeKind::Get, vec![])? {
            AutoValue::Object(object) => Ok(object),
            _ => Err(XlcError::Operand("member did not answer an object")),
        }
    }

    fn application(&self) -> ObjectRef {
        self.model.application()
    }

    /// The reference operand at `index`: a range object, the XLM text form, or the selection when
    /// the operand is omitted.
    fn range(&self, index: usize) -> Result<ObjectRef, XlcError> {
        match self.operand(index) {
            AutoValue::Object(object) => Ok(object.clone()),
            AutoValue::Missing => self.object(&self.application(), "Selection"),
            AutoValue::Text(text) => {
                let a1 = r1c1_text_to_a1(&text.to_string_lossy(), || self.active_coordinates())?;
                match self.call(
                    &self.application(),
                    "Range",
                    InvokeKind::Get,
                    vec![AutoValue::text(&a1)],
                )? {
                    AutoValue::Object(object) => Ok(object),
                    _ => Err(XlcError::Operand("Range did not answer an object")),
                }
            }
            _ => Err(XlcError::Operand("reference operand")),
        }
    }

    /// An operand supplied as an object is borrowed; text and selection acquire a range.
    fn with_range_operand<T>(
        &self,
        index: usize,
        use_range: impl FnOnce(&ObjectRef) -> Result<T, XlcError>,
    ) -> Result<T, XlcError> {
        let range = self.range(index)?;
        let result = use_range(&range);
        if !matches!(self.operand(index), AutoValue::Object(_)) {
            self.model.released(&range);
        }
        result
    }

    fn active_coordinates(&self) -> Result<(u32, u32), XlcError> {
        let active = self.object(&self.application(), "ActiveCell")?;
        let coordinate = |member| match self.call(&active, member, InvokeKind::Get, vec![])? {
            AutoValue::I4(value) => u32::try_from(value).map_err(|_| INVALID_REFERENCE),
            _ => Err(INVALID_REFERENCE),
        };
        let position = (|| Ok((coordinate("Row")?, coordinate("Column")?)))();
        self.model.released(&active);
        position
    }

    fn put(&self, range: &ObjectRef, member: &str, value: AutoValue) -> Result<(), XlcError> {
        self.call(range, member, InvokeKind::Put, vec![value])
            .map(|_| ())
    }

    fn method(&self, target: &ObjectRef, member: &str, args: Vec<AutoValue>) -> Result<(), XlcError> {
        self.call(target, member, InvokeKind::Method, args)
            .map(|_| ())
    }

    fn number(&self, index: usize) -> Result<f64, XlcError> {
        match self.operand(index) {
            AutoValue::I2(v) => Ok(f64::from(*v)),
            AutoValue::I4(v) => Ok(f64::from(*v)),
            AutoValue::R4(v) => Ok(f64::from(*v)),
            AutoValue::R8(v) => Ok(*v),
            AutoValue::Bool(v) => Ok(f64::from(u8::from(*v))),
            _ => Err(XlcError::Operand("numeric operand")),
        }
    }

    fn text(&self, index: usize) -> Result<AutoValue, XlcError> {
        match self.operand(index) {
            value @ AutoValue::Text(_) => Ok(value.clone()),
            _ => Err(XlcError::Operand("text operand")),
        }
    }

    // CALCULATE.NOW (): the application's calculate.
    fn calculate_now(&self) -> Result<(), XlcError> {
        self.method(&self.application(), "Calculate", vec![])
    }

    // CALCULATE.DOCUMENT (): the active sheet.
    fn calculate_document(&self) -> Result<(), XlcError> {
        let sheet = self.object(&self.application(), "ActiveSheet")?;
        self.method(&sheet, "Calculate", vec![])
    }

    // SAVE (): the active workbook.
    fn save(&self) -> Result<(), XlcError> {
        let book = self.object(&self.application(), "ActiveWorkbook")?;
        self.method(&book, "Save", vec![])
    }

    // ECHO (logical): screen updating. C2 measured an omitted operand toggling the current value.
    fn echo(&self) -> Result<(), XlcError> {
        let app = self.application();
        let on = if self.operand(0).is_missing() {
            let AutoValue::Bool(current) = self.call(&app, "ScreenUpdating", InvokeKind::Get, vec![])? else {
                return Err(XlcError::Operand("ScreenUpdating did not answer a Boolean"));
            };
            !current
        } else {
            geometry::boolean(self.operand(0)).map_err(|_| XlcError::Operand("logical operand"))?
        };
        self.put(&app, "ScreenUpdating", AutoValue::Bool(on))
    }

    // MESSAGE (logical, text): a number or logical shows the text in the status bar when it is
    // nonzero or TRUE; zero, FALSE and every other type restore it. Showing without text restores.
    fn message(&self) -> Result<(), XlcError> {
        let show = match self.operand(0) {
            AutoValue::Bool(v) => *v,
            AutoValue::I2(v) => *v != 0,
            AutoValue::I4(v) => *v != 0,
            AutoValue::R4(v) => *v != 0.,
            AutoValue::R8(v) => *v != 0.,
            _ => false,
        };
        let value = match (show, self.operand(1)) {
            (true, text @ AutoValue::Text(_)) => text.clone(),
            _ => AutoValue::Bool(false),
        };
        self.put(&self.application(), "StatusBar", value)
    }

    // RUN (macro_text): the named procedure. A reference to a macro-sheet cell and the step
    // argument are XLM macro-sheet control flow, not a procedure call.
    fn run_macro(&self) -> Result<(), XlcError> {
        self.require_supported_operands(1, "RUN with a second operand")?;
        match self.operand(0) {
            AutoValue::Text(_) => {
                let name = self.text(0)?;
                self.call(&self.application(), "Run", InvokeKind::Method, vec![name])
                    .map(|_| ())
            }
            AutoValue::Object(_) => Err(refuse("RUN of a macro-sheet reference")),
            _ => Err(XlcError::Operand("macro operand")),
        }
    }

    // FORMULA enters text in the active cell or the first cell of an explicit range (C2).
    // FORMULA.FILL enters the relative formula in every cell of the range or selection.
    fn formula(&self, fill: bool) -> Result<(), XlcError> {
        let text = match self.operand(0) {
            AutoValue::Missing => return Err(XlcError::Operand("formula text")),
            value @ (AutoValue::Text(_)
            | AutoValue::R8(_)
            | AutoValue::I4(_)
            | AutoValue::I2(_)
            | AutoValue::Bool(_)) => value.clone(),
            _ => return Err(XlcError::Operand("formula text")),
        };
        if !fill && self.operand(1).is_missing() {
            let active = self.object(&self.application(), "ActiveCell")?;
            let result = self.put(&active, "FormulaR1C1", text);
            self.model.released(&active);
            return result;
        }
        self.with_range_operand(1, |range| {
            if fill {
                return self.put(range, "FormulaR1C1", text);
            }
            let count = self.call(range, "Count", InvokeKind::Get, vec![])?;
            if matches!(count, AutoValue::I4(1) | AutoValue::R8(1.0)) {
                return self.put(range, "FormulaR1C1", text);
            }
            let first = self.call(range, "Item", InvokeKind::Get, vec![AutoValue::I4(1), AutoValue::I4(1)])?;
            let AutoValue::Object(first) = first else {
                return Err(XlcError::Operand("Range.Item did not answer a range"));
            };
            let result = self.put(&first, "FormulaR1C1", text);
            self.model.released(&first);
            result
        })
    }

    // CLEAR on a worksheet defaults to clearing its contents (type 3).
    fn clear(&self) -> Result<(), XlcError> {
        let member = if self.operand(0).is_missing() {
            "ClearContents"
        } else {
            match self.number(0)? {
                1.0 => "Clear",
                2.0 => "ClearFormats",
                3.0 => "ClearContents",
                4.0 => "ClearComments",
                _ => return Err(XlcError::Operand("type_num")),
            }
        };
        let selection = self.object(&self.application(), "Selection")?;
        let result = self.method(&selection, member, vec![]);
        self.model.released(&selection);
        result
    }

    // SELECT (selection): the active cell argument is not served.
    fn select(&self) -> Result<(), XlcError> {
        self.require_supported_operands(1, "SELECT with an active-cell operand")?;
        if self.operands.is_empty() {
            return Err(refuse("SELECT without a selection"));
        }
        // The C2 capture's cross-sheet SELECT failed without changing the current sheet.
        if self.operand(0).as_text().is_some_and(|text| text.contains('!')) {
            return Err(refuse("SELECT with a sheet-qualified text reference"));
        }
        let range = self.range(0)?;
        self.method(&range, "Select", vec![])
    }

    // FORMAT.NUMBER (format_text) on the selection.
    fn format_number(&self) -> Result<(), XlcError> {
        let format = self.text(0)?;
        let selection = self.object(&self.application(), "Selection")?;
        self.put(&selection, "NumberFormat", format)
    }

    // COLUMN.WIDTH (width_num, reference): characters, as `Range.ColumnWidth`.
    fn column_width(&self) -> Result<(), XlcError> {
        self.require_supported_operands(2, "COLUMN.WIDTH with standard or type operands")?;
        let width = self.number(0)?;
        let range = self.range(1)?;
        self.put(&range, "ColumnWidth", AutoValue::R8(width))
    }

    // ROW.HEIGHT (row_height, reference): points, as `Range.RowHeight`.
    fn row_height(&self) -> Result<(), XlcError> {
        self.require_supported_operands(2, "ROW.HEIGHT with standard or type operands")?;
        let height = self.number(0)?;
        let range = self.range(1)?;
        self.put(&range, "RowHeight", AutoValue::R8(height))
    }

    // INSERT / EDIT.DELETE (shift_num) on the selection: 1 shift cells right/left, 2 down/up,
    // 3 the entire rows, 4 the entire columns. `codes` are the XlInsertShiftDirection or
    // XlDeleteShiftDirection constants for 1 and 2.
    fn shift(&self, member: &str, codes: [i32; 2]) -> Result<(), XlcError> {
        self.require_supported_operands(1, "INSERT with a second operand")?;
        if self.operand(0).is_missing() {
            // INSERT can paste an active cut or copy. That command form remains unmodelled.
            if member == "Insert" {
                return Err(refuse("INSERT without shift_num"));
            }
            let selection = self.object(&self.application(), "Selection")?;
            let result = self.method(&selection, member, vec![]);
            self.model.released(&selection);
            return result;
        }
        let shift = self.number(0)?;
        if !matches!(shift, 1.0 | 2.0 | 3.0 | 4.0) {
            return Err(XlcError::Operand("shift_num"));
        }
        let selection = self.object(&self.application(), "Selection")?;
        match shift {
            1.0 => self.method(&selection, member, vec![AutoValue::I4(codes[0])]),
            2.0 => self.method(&selection, member, vec![AutoValue::I4(codes[1])]),
            3.0 => {
                let rows = self.object(&selection, "EntireRow")?;
                self.method(&rows, member, vec![])
            }
            4.0 => {
                let columns = self.object(&selection, "EntireColumn")?;
                self.method(&columns, member, vec![])
            }
            _ => unreachable!("shift_num was checked"),
        }
    }

    // COPY (from_reference, to_reference) and CUT: to the clipboard, or at once to the target.
    fn copy_cut(&self, member: &str) -> Result<(), XlcError> {
        let from = self.range(0)?;
        let args = if self.operand(1).is_missing() {
            vec![]
        } else {
            vec![AutoValue::Object(self.range(1)?)]
        };
        self.method(&from, member, args)
    }

    // PASTE (to_reference): the clipboard into the reference, or at the selection, of the active sheet.
    fn paste(&self) -> Result<(), XlcError> {
        let sheet = self.object(&self.application(), "ActiveSheet")?;
        let args = if self.operand(0).is_missing() {
            vec![]
        } else {
            vec![AutoValue::Object(self.range(0)?)]
        };
        self.method(&sheet, "Paste", args)
    }

    // Worksheet PASTE.SPECIAL translates XLM dialog ordinals to VBA enumeration values.
    fn paste_special(&self) -> Result<(), XlcError> {
        self.require_supported_operands(4, "PASTE.SPECIAL chart or external clipboard form")?;
        if matches!(self.operand(0), AutoValue::Missing | AutoValue::Text(_)) {
            return Err(refuse("PASTE.SPECIAL without a worksheet paste code"));
        }
        let paste = match self.number(0)? {
            1.0 => -4104, // xlPasteAll
            2.0 => -4123, // xlPasteFormulas
            3.0 => -4163, // xlPasteValues
            4.0 => -4122, // xlPasteFormats
            5.0 | 6.0 => return Err(refuse("PASTE.SPECIAL comments or all except borders")),
            _ => return Err(XlcError::Operand("paste_num")),
        };
        let operation = if self.operand(1).is_missing() {
            AutoValue::Missing
        } else {
            AutoValue::I4(match self.number(1)? {
                1.0 => -4142, // xlPasteSpecialOperationNone
                2.0 | 3.0 | 4.0 | 5.0 => return Err(refuse("PASTE.SPECIAL arithmetic operation")),
                _ => return Err(XlcError::Operand("operation_num")),
            })
        };
        let flag = |index| match self.operand(index) {
            AutoValue::Missing => Ok(AutoValue::Missing),
            value => geometry::boolean(value)
                .map(AutoValue::Bool)
                .map_err(|_| XlcError::Operand("logical operand")),
        };
        let args = vec![AutoValue::I4(paste), operation, flag(2)?, flag(3)?];
        let selection = self.object(&self.application(), "Selection")?;
        let result = self.method(&selection, "PasteSpecial", args);
        self.model.released(&selection);
        result
    }

    // WORKBOOK.INSERT inserts a worksheet before the active worksheet.
    fn workbook_insert(&self) -> Result<(), XlcError> {
        match self.operand(0) {
            AutoValue::Missing => {}
            AutoValue::Text(_) => return Err(refuse("WORKBOOK.INSERT using a template")),
            _ => match self.number(0)? {
                1.0 => {}
                2.0 | 3.0 | 4.0 | 6.0 | 7.0 => {
                    return Err(refuse("WORKBOOK.INSERT of a non-worksheet sheet"));
                }
                _ => return Err(AutoError::excel(1004, "Invalid sheet type").into()),
            },
        }
        let sheets = self.object(&self.application(), "Sheets")?;
        let result = self.call(&sheets, "Add", InvokeKind::Method, vec![]);
        self.model.released(&sheets);
        if let AutoValue::Object(sheet) = result? {
            self.model.released(&sheet);
        }
        Ok(())
    }

    // SET.PRINT.AREA sets the active worksheet's area, or clears it with empty text.
    fn set_print_area(&self) -> Result<(), XlcError> {
        if self.operand(0).is_missing() {
            return Err(AutoError::excel(1004, "SET.PRINT.AREA requires a range or empty text").into());
        }
        let sheet = self.object(&self.application(), "ActiveSheet")?;
        let result = (|| {
            let address = if matches!(self.operand(0), AutoValue::Text(text) if text.utf16_code_units().is_empty()) {
                AutoValue::text("")
            } else {
                self.with_range_operand(0, |range| {
                    let owner = self.object(range, "Worksheet")?;
                    let same_sheet = owner.key == sheet.key;
                    self.model.released(&owner);
                    if !same_sheet {
                        return Err(refuse("SET.PRINT.AREA with a reference on another sheet"));
                    }
                    self.call(range, "Address", InvokeKind::Get, vec![])
                })?
            };
            let page = self.object(&sheet, "PageSetup")?;
            let result = self.put(&page, "PrintArea", address);
            self.model.released(&page);
            result
        })();
        self.model.released(&sheet);
        result
    }

    // WORKBOOK.NAME (oldname_text, newname_text) renames a sheet in the active workbook.
    fn workbook_name(&self) -> Result<(), XlcError> {
        let old = self.text(0)?;
        let new = self.text(1)?;
        let sheets = self.object(&self.application(), "Sheets")?;
        let AutoValue::Object(sheet) = self.call(&sheets, "Item", InvokeKind::Get, vec![old])? else {
            return Err(XlcError::Operand("Sheets.Item did not answer an object"));
        };
        self.put(&sheet, "Name", new)
    }

    // DEFINE.NAME (name_text, refers_to): refers_to is formula text in R1C1.
    fn define_name(&self) -> Result<(), XlcError> {
        self.require_supported_operands(2, "DEFINE.NAME with additional name options")?;
        let name = self.text(0)?;
        let refers_to = match self.operand(1) {
            AutoValue::Missing => return Err(refuse("DEFINE.NAME without refers_to")),
            AutoValue::Text(_) => self.text(1)?,
            _ => return Err(refuse("DEFINE.NAME with a reference object")),
        };
        let names = self.object(&self.application(), "Names")?;
        let add = Descriptor::get()
            .member(names.interface, "Add", InvokeKind::Method)
            .ok_or(XlcError::Operand("member is not described"))?;
        let position = |wanted: &str| {
            add.arguments()
                .position(|param| param.name.as_deref() == Some(wanted))
                .ok_or(XlcError::Operand("member is not described"))
        };
        let (name_at, refers_at) = (position("Name")?, position("RefersToR1C1")?);
        let mut args = vec![AutoValue::Missing; refers_at.max(name_at) + 1];
        args[name_at] = name;
        args[refers_at] = refers_to;
        self.method(&names, "Add", args)
    }

    // DELETE.NAME (name_text).
    fn delete_name(&self) -> Result<(), XlcError> {
        let name = self.text(0)?;
        let names = self.object(&self.application(), "Names")?;
        let item = match self.call(&names, "Item", InvokeKind::Method, vec![name])? {
            AutoValue::Object(item) => item,
            _ => return Err(XlcError::Operand("Names.Item did not answer an object")),
        };
        self.method(&item, "Delete", vec![])
    }
}

const INVALID_REFERENCE: XlcError = XlcError::Operand("text reference");

/// Each endpoint has a row, a column, or both. An empty coordinate after R or C is relative zero.
fn r1c1_endpoint(text: &str) -> Option<(Option<AxisRef>, Option<AxisRef>)> {
    fn axis(text: &str, limit: u32) -> Option<AxisRef> {
        if text.is_empty() {
            return Some(AxisRef::Relative(0));
        }
        if let Some(offset) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            let offset = offset.parse::<i32>().ok()?;
            return (offset.unsigned_abs() < limit).then_some(AxisRef::Relative(offset));
        }
        if !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let index = text.parse().ok()?;
        (1..=limit).contains(&index).then_some(AxisRef::Absolute(index))
    }

    if let Some(row) = text.strip_prefix(['R', 'r']) {
        if let Some((row, column)) = row.split_once(['C', 'c']) {
            Some((Some(axis(row, MAX_ROWS)?), Some(axis(column, MAX_COLUMNS)?)))
        } else {
            Some((Some(axis(row, MAX_ROWS)?), None))
        }
    } else {
        Some((None, Some(axis(text.strip_prefix(['C', 'c'])?, MAX_COLUMNS)?)))
    }
}

/// Translate a single sheet's R1C1 area to Application.Range's A1 spelling.
fn r1c1_text_to_a1(
    text: &str,
    active: impl FnOnce() -> Result<(u32, u32), XlcError>,
) -> Result<String, XlcError> {
    let (qualifier, body) = match text.rsplit_once('!') {
        None => ("", text),
        Some((qualifier, body)) => {
            if qualifier.contains(['[', ']', ':']) {
                return Err(refuse("a workbook-qualified or 3-D text reference"));
            }
            let quoted = qualifier.strip_prefix('\'').and_then(|q| q.strip_suffix('\''));
            let valid = match quoted {
                Some(sheet) => !sheet.is_empty() && !sheet.replace("''", "").contains('\''),
                None => !qualifier.is_empty()
                    && qualifier.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.')),
            };
            if !valid {
                return Err(INVALID_REFERENCE);
            }
            (&text[..text.len() - body.len()], body)
        }
    };
    let (first, last) = body.split_once(':').unwrap_or((body, body));
    let first = r1c1_endpoint(first).ok_or(INVALID_REFERENCE)?;
    let last = r1c1_endpoint(last).ok_or(INVALID_REFERENCE)?;
    if first.0.is_some() != last.0.is_some() || first.1.is_some() != last.1.is_some() {
        return Err(INVALID_REFERENCE);
    }
    let (row, column) = if [first.0, first.1, last.0, last.1]
        .into_iter()
        .any(|axis| matches!(axis, Some(AxisRef::Relative(_))))
    {
        active()?
    } else {
        (1, 1)
    };
    let place = |axis: AxisRef, origin, limit| {
        u32::try_from(axis.at(origin))
            .ok()
            .filter(|n| (1..=limit).contains(n))
            .ok_or(INVALID_REFERENCE)
    };
    let cell = |(r, c): (Option<AxisRef>, Option<AxisRef>)| {
        let row = r.map(|axis| place(axis, row, MAX_ROWS)).transpose()?;
        let column = c.map(|axis| place(axis, column, MAX_COLUMNS)).transpose()?;
        Ok::<_, XlcError>(format!(
            "{}{}",
            column.map(column_letters).unwrap_or_default(),
            row.map(|row| row.to_string()).unwrap_or_default(),
        ))
    };
    let a1 = cell(first)?;
    if first == last && first.0.is_some() && first.1.is_some() {
        Ok(format!("{qualifier}{a1}"))
    } else {
        Ok(format!("{qualifier}{a1}:{}", cell(last)?))
    }
}
