//! The Excel C API `xlc*` commands of batch 1 (dc-oxh8w.5), as translations to object-model
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
//! Sources: Microsoft's Excel 4.0 Macro Functions Reference (the argument lists and the shift,
//! clear and format codes) and docs/architecture/C_API_COMMANDS.md (ordinals). The value a command
//! returns on success is TRUE. Where the documentation does not say what an omitted operand does,
//! the command refuses with a typed refusal owned by dc-oxh8w.9 until a Windows Excel capture
//! settles it (docs/architecture/C_API_COMMANDS.md, "Defaults ... require separate evidence").

use calc_object_model::{
    AutoError, AutoValue, Call, Descriptor, InvokeKind, ObjectModel, ObjectRef, geometry,
};

/// Owner of every refusal this module raises for an unmeasured form.
const OWNER: &str = "dc-oxh8w.9";

/// One batch-1 command: its ordinal (the callback number without the `xlCommand` bit), the SDK
/// symbol, the XLM name, the most operands it takes and the object-model member it translates to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XlcCommand {
    /// Ordinal in `XLCALL.H`; the callback number is this ORed with `0x8000`.
    pub ordinal: u16,
    /// SDK symbol, for example `xlcFormula`.
    pub symbol: &'static str,
    /// XLM name, for example `FORMULA`.
    pub xlm: &'static str,
    /// Most operands the command takes.
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

/// Every command batch 1 serves, by ordinal.
pub const COMMANDS: &[XlcCommand] = &[
    c(4, "xlcSave", "SAVE", 0, "_Workbook.Save"),
    c(17, "xlcRun", "RUN", 1, "_Application.Run"),
    c(31, "xlcCalculateNow", "CALCULATE.NOW", 0, "_Application.Calculate"),
    c(42, "xlcFormatNumber", "FORMAT.NUMBER", 1, "Range.NumberFormat"),
    c(47, "xlcColumnWidth", "COLUMN.WIDTH", 2, "Range.ColumnWidth"),
    c(49, "xlcCut", "CUT", 2, "Range.Cut"),
    c(50, "xlcCopy", "COPY", 2, "Range.Copy"),
    c(51, "xlcPaste", "PASTE", 1, "_Worksheet.Paste"),
    c(52, "xlcClear", "CLEAR", 1, "Range.Clear"),
    c(54, "xlcEditDelete", "EDIT.DELETE", 1, "Range.Delete"),
    c(55, "xlcInsert", "INSERT", 1, "Range.Insert"),
    c(61, "xlcDefineName", "DEFINE.NAME", 2, "Names.Add"),
    c(96, "xlcFormula", "FORMULA", 2, "Range.FormulaR1C1"),
    c(97, "xlcFormulaFill", "FORMULA.FILL", 2, "Range.FormulaR1C1"),
    c(109, "xlcSelect", "SELECT", 1, "Range.Select"),
    c(110, "xlcDeleteName", "DELETE.NAME", 1, "Name.Delete"),
    c(122, "xlcMessage", "MESSAGE", 2, "_Application.StatusBar"),
    c(127, "xlcRowHeight", "ROW.HEIGHT", 2, "Range.RowHeight"),
    c(141, "xlcEcho", "ECHO", 1, "_Application.ScreenUpdating"),
    c(167, "xlcCalculateDocument", "CALCULATE.DOCUMENT", 0, "_Worksheet.Calculate"),
];

/// The command with this ordinal, when batch 1 serves it.
pub fn command(ordinal: u16) -> Option<&'static XlcCommand> {
    COMMANDS.iter().find(|command| command.ordinal == ordinal)
}

/// The command a callback number (`xlCommand | ordinal`) names; `None` when the command bit is
/// absent or batch 1 does not serve the ordinal.
pub fn command_for_callback(callback: u32) -> Option<&'static XlcCommand> {
    if callback & 0xC000 != 0x8000 {
        return None;
    }
    command((callback & 0x3FFF) as u16)
}

/// Why a command did not complete.
#[derive(Clone, Debug, PartialEq)]
pub enum XlcError {
    /// Batch 1 does not serve this ordinal (the caller's refusal names the owning bead).
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
        31 => run.calculate_now(),
        42 => run.format_number(),
        47 => run.column_width(),
        49 => run.copy_cut("Cut"),
        50 => run.copy_cut("Copy"),
        51 => run.paste(),
        52 => run.clear(),
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
            AutoValue::Text(_) => {
                let a1 = r1c1_text_to_a1(&self.operand(index).as_text().unwrap_or_default())?;
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

    // ECHO (logical): screen updating. Omitted is not documented as a toggle or as TRUE.
    fn echo(&self) -> Result<(), XlcError> {
        if self.operand(0).is_missing() {
            return Err(refuse("ECHO without an argument"));
        }
        let on = geometry::boolean(self.operand(0)).map_err(|_| XlcError::Operand("logical operand"))?;
        self.put(&self.application(), "ScreenUpdating", AutoValue::Bool(on))
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

    // FORMULA (formula_text, reference) enters the text, read in R1C1, in the reference (the
    // selection's cell when omitted). FORMULA.FILL enters the one relative formula in every cell.
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
        let range = self.range(1)?;
        if !fill {
            let count = self.call(&range, "Count", InvokeKind::Get, vec![])?;
            if !matches!(count, AutoValue::I4(1) | AutoValue::R8(1.0)) {
                return Err(refuse("FORMULA over more than one cell"));
            }
        }
        self.put(&range, "FormulaR1C1", text)
    }

    // CLEAR (type_num) on the selection: 1 all, 2 formats, 3 formulas, 4 notes.
    fn clear(&self) -> Result<(), XlcError> {
        if self.operand(0).is_missing() {
            return Err(refuse("CLEAR without type_num"));
        }
        let member = match self.number(0)? as i64 {
            1 => "Clear",
            2 => "ClearFormats",
            3 => "ClearContents",
            4 => "ClearComments",
            _ => return Err(XlcError::Operand("type_num")),
        };
        let selection = self.object(&self.application(), "Selection")?;
        self.method(&selection, member, vec![])
    }

    // SELECT (selection): the active cell argument is not served.
    fn select(&self) -> Result<(), XlcError> {
        if self.operands.is_empty() {
            return Err(refuse("SELECT without a selection"));
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
        let width = self.number(0)?;
        let range = self.range(1)?;
        self.put(&range, "ColumnWidth", AutoValue::R8(width))
    }

    // ROW.HEIGHT (row_height, reference): points, as `Range.RowHeight`.
    fn row_height(&self) -> Result<(), XlcError> {
        let height = self.number(0)?;
        let range = self.range(1)?;
        self.put(&range, "RowHeight", AutoValue::R8(height))
    }

    // INSERT / EDIT.DELETE (shift_num) on the selection: 1 shift cells right/left, 2 down/up,
    // 3 the entire rows, 4 the entire columns. `codes` are the XlInsertShiftDirection or
    // XlDeleteShiftDirection constants for 1 and 2.
    fn shift(&self, member: &str, codes: [i32; 2]) -> Result<(), XlcError> {
        if self.operand(0).is_missing() {
            return Err(refuse("INSERT or EDIT.DELETE without shift_num"));
        }
        let shift = self.number(0)? as i64;
        if !(1..=4).contains(&shift) {
            return Err(XlcError::Operand("shift_num"));
        }
        let selection = self.object(&self.application(), "Selection")?;
        match shift {
            1 => self.method(&selection, member, vec![AutoValue::I4(codes[0])]),
            2 => self.method(&selection, member, vec![AutoValue::I4(codes[1])]),
            3 => {
                let rows = self.object(&selection, "EntireRow")?;
                self.method(&rows, member, vec![])
            }
            4 => {
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

    // DEFINE.NAME (name_text, refers_to): refers_to is formula text in R1C1.
    fn define_name(&self) -> Result<(), XlcError> {
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

/// An XLM text reference (`R2C3`, `R2C3:R4C5`, with optional `$`... never) as the A1 text
/// `Application.Range` takes. Sheet-qualified and relative forms belong to dc-oxh8w.9.
fn r1c1_text_to_a1(text: &str) -> Result<String, XlcError> {
    fn cell(part: &str) -> Option<String> {
        let rest = part.strip_prefix(['R', 'r'])?;
        let (row, column) = rest.split_once(['C', 'c'])?;
        let (row, column): (u32, u32) = (row.parse().ok()?, column.parse().ok()?);
        if row == 0 || row > 1_048_576 || column == 0 || column > 16_384 {
            return None;
        }
        let mut letters = Vec::new();
        let mut n = column;
        while n > 0 {
            letters.push(b'A' + ((n - 1) % 26) as u8);
            n = (n - 1) / 26;
        }
        letters.reverse();
        Some(format!("{}{row}", String::from_utf8(letters).ok()?))
    }
    if text.contains(['!', '[']) {
        return Err(refuse("a sheet-qualified or relative text reference"));
    }
    let converted = match text.split_once(':') {
        Some((first, last)) => cell(first).zip(cell(last)).map(|(a, b)| format!("{a}:{b}")),
        None => cell(text),
    };
    converted.ok_or(XlcError::Operand("text reference"))
}
