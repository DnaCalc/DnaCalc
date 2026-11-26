//! Sheet protection a macro sets (dc-oxh8w.68): `Worksheet.Protect`, `Unprotect` and the `Protect*` getters, and the
//! refusal a macro meets when it writes a locked cell of a protected sheet. The state belongs to the Automation
//! profile; the file's own `sheetProtection` (a sheet that loaded protected) counts as contents-protected too.
use super::*;
use crate::HostState;
use super::cells::{XL_SHIFT_DOWN, XL_SHIFT_UP};
use crate::value::{MAX_COLUMNS, MAX_ROWS};

const OWNER: &str = "dc-9b7zd";
/// Stands for a numeric Unprotect argument: a text no password check accepts (a NUL is not typed into a password box).
const NUMBER_NEVER_MATCHES: &str = "\u{0}number";
/// Allow* arguments, and UserInterfaceOnly on a protected sheet (open).
const ALLOW_OWNER: &str = "dc-ml3k0";

/// What `Worksheet.Protect` set.
#[derive(Clone, Debug)]
pub(super) struct SheetProtect {
    password: Option<String>,
    /// The password hash a file's `sheetProtection` holds (a sheet that loaded protected with a password).
    file_hash: Option<calc_doc::FileProtection>,
    drawing_objects: bool,
    contents: bool,
    scenarios: bool,
    user_interface_only: bool,
    /// The eleven Allow* arguments (Protect arguments 6 to 16), in argument order.
    allow: [bool; 11],
}

/// What a macro's `Protect` leaves to save: the flags, and the password the `sheetProtection` hash is made from.
#[derive(Clone, Debug)]
pub(crate) struct ProtectionSave {
    password: Option<String>,
    contents: bool,
    objects: bool,
    scenarios: bool,
    allow: [bool; 11],
}

impl ProtectionSave {
    /// The `sheetProtection` to write for the sheet named `sheet`: the SHA-512 hash, spun 100,000 times (what Excel
    /// writes), over a salt made from the sheet name and password (Excel's is random; any salt verifies).
    pub(crate) fn to_save(&self, sheet: &str) -> calc_doc::SheetProtectionSave {
        use sha2::{Digest, Sha512};
        let mut file = calc_doc::FileProtection { objects: self.objects, scenarios: self.scenarios, allow: self.allow, ..Default::default() };
        if let Some(password) = &self.password {
            let mut seed = Sha512::new();
            seed.update(b"dnacalc sheetProtection salt");
            seed.update(sheet.as_bytes());
            seed.update(password.as_bytes());
            let salt = seed.finalize()[..16].to_vec();
            file.algorithm = "SHA-512".to_owned();
            file.spin_count = 100_000;
            file.hash = base64_encode(&sha512_password_hash(&salt, password, file.spin_count));
            file.salt = base64_encode(&salt);
        }
        calc_doc::SheetProtectionSave { contents: self.contents, file }
    }
}

impl SheetProtect {
    fn to_saved(&self) -> ProtectionSave {
        ProtectionSave {
            password: self.password.clone(),
            contents: self.contents,
            objects: self.drawing_objects,
            scenarios: self.scenarios,
            allow: self.allow,
        }
    }

    /// Whether a password guards the protection.
    fn has_password(&self) -> bool {
        self.password.is_some() || self.file_hash.is_some()
    }

    /// Whether `given` is the password: compared as text for a macro's Protect, hashed as Excel does for a file's
    /// SHA-512 `sheetProtection` (salt, then the spun hash). The legacy 16-bit hash and other algorithms are
    /// refused typed: no Excel answer has been captured for them.
    fn password_matches(&self, given: &str) -> Result<bool, AutoError> {
        if let Some(password) = &self.password {
            return Ok(password == given);
        }
        let Some(file) = &self.file_hash else { return Ok(given.is_empty()) };
        if file.algorithm != "SHA-512" || file.hash.is_empty() {
            return Err(AutoError::not_modelled("Worksheet.Unprotect of a sheet with a legacy or unknown password hash", OWNER));
        }
        let salt = base64_decode(&file.salt).ok_or_else(|| AutoError::not_modelled("a sheetProtection saltValue that is not base64", OWNER))?;
        Ok(base64_encode(&sha512_password_hash(&salt, given, file.spin_count)) == file.hash)
    }
}

/// The sheet-protection password hash of ECMA-376 (agile): SHA-512 of the salt and the UTF-16LE password, then `spin_count` rounds
/// of SHA-512 over the previous hash and the round number (little-endian u32). Verified against Excel's own
/// `sheetProtection` (ww22 protect/packages.json, password "abc").
fn sha512_password_hash(salt: &[u8], password: &str, spin_count: u32) -> Vec<u8> {
    use sha2::{Digest, Sha512};
    let mut hasher = Sha512::new();
    hasher.update(salt);
    for unit in password.encode_utf16() {
        hasher.update(unit.to_le_bytes());
    }
    let mut hash = hasher.finalize().to_vec();
    for round in 0..spin_count {
        let mut hasher = Sha512::new();
        hasher.update(&hash);
        hasher.update(round.to_le_bytes());
        hash = hasher.finalize().to_vec();
    }
    hash
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut n, mut bits) = (0u32, 0);
    for c in text.trim_end_matches('=').bytes() {
        n = (n << 6) | BASE64.iter().position(|&b| b == c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
            n &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Index into [`SheetProtect::allow`].
#[derive(Clone, Copy)]
pub(super) enum Allow {
    InsertingColumns = 3,
    InsertingRows = 4,
    DeletingColumns = 6,
    DeletingRows = 7,
    Filtering = 9,
}

/// Whether the cells of style `style` are locked: the format's own flag, else its cell style's, else locked.
pub(super) fn style_locked(host: &HostState, style: u32) -> bool {
    let styles = host.styles();
    let format = styles.cell_formats.get(style as usize);
    let own = format.and_then(|f| f.protection.as_ref()).and_then(|p| p.locked);
    let parent = format
        .and_then(|f| f.xf_id)
        .and_then(|id| styles.cell_style_formats.get(id as usize))
        .and_then(|f| f.protection.as_ref())
        .and_then(|p| p.locked);
    own.or(parent).unwrap_or(true)
}

fn protected_sheet() -> AutoError {
    AutoError::excel(
        1004,
        "The cell or chart you're trying to change is on a protected sheet. To make a change, unprotect the sheet. You might be requested to enter a password.",
    )
}

fn wrong_password() -> AutoError {
    AutoError::excel(
        1004,
        "The password you supplied is not correct. Verify that the CAPS LOCK key is off and be sure to use the correct capitalization.",
    )
}

impl WorkbookAutomation {
    pub(super) fn register_protection(handlers: &mut Handlers<Self>) {
        handlers.on("Worksheet", "Protect", InvokeKind::Method, Self::worksheet_protect);
        handlers.on("Worksheet", "Unprotect", InvokeKind::Method, Self::worksheet_unprotect);
        handlers.on("Worksheet", "ProtectContents", InvokeKind::Get, Self::protect_contents);
        handlers.on("Worksheet", "ProtectDrawingObjects", InvokeKind::Get, Self::protect_drawing_objects);
        handlers.on("Worksheet", "ProtectScenarios", InvokeKind::Get, Self::protect_scenarios);
        handlers.on("Worksheet", "ProtectionMode", InvokeKind::Get, Self::protection_mode);
    }

    /// The protection a sheet has now: what a macro set, else what the file holds (contents only).
    fn sheet_protection(&self, host: &HostState, sheet: SheetId) -> Option<SheetProtect> {
        if let Some(held) = self.sheet_protect.borrow().get(&sheet) {
            return held.clone();
        }
        let loaded = host.loaded().sheet(sheet).filter(|s| s.protected())?;
        let file = loaded.protection();
        Some(SheetProtect {
            password: None,
            file_hash: file.filter(|f| !f.hash.is_empty() || !f.legacy_password.is_empty()).cloned(),
            drawing_objects: file.is_some_and(|f| f.objects),
            contents: true,
            scenarios: file.is_some_and(|f| f.scenarios),
            user_interface_only: false,
            allow: file.map_or([false; 11], |f| f.allow),
        })
    }

    /// A macro's write to `reference`: refused with 1004 when a cell it covers is locked on a sheet protected for contents
    /// (unless `UserInterfaceOnly` was set).
    pub(super) fn guard_locked(&self, session: &HostSession, reference: &ReferenceLike) -> Result<(), AutoError> {
        let host = session.host();
        for area in reference.areas() {
            let Some(held) = self.sheet_protection(host, area.sheet()) else { continue };
            if !held.contents || held.user_interface_only {
                continue;
            }
            let samples = crate::format::format_samples(host, std::slice::from_ref(area))
                .map_err(|reason| AutoError::not_modelled(reason, OWNER))?;
            if samples.into_iter().any(|at| style_locked(host, host.cell_style(at))) {
                return Err(protected_sheet());
            }
        }
        Ok(())
    }

    /// A macro's write to one area: [`Self::guard_locked`] for a [`GridArea`].
    pub(super) fn guard_area(&self, session: &HostSession, area: GridArea) -> Result<(), AutoError> {
        let reference = ReferenceLike::from_areas(vec![area]).map_err(|_| geometry::type_mismatch())?;
        self.guard_locked(session, &reference)
    }

    /// The area a copy or paste of `source` writes when given the destination `to`: a one-cell destination grows to the
    /// source's size (its height and width swapped when transposed), so its neighbours meet the protection too.
    pub(super) fn written_area(source: GridArea, to: GridArea, transpose: bool) -> GridArea {
        if to.cell_count() != 1 {
            return to;
        }
        let (height, width) = if transpose {
            (source.column_count(), source.row_count())
        } else {
            (source.row_count(), source.column_count())
        };
        CellKey::new(to.sheet(), to.first().row() + height - 1, to.first().column() + width - 1)
            .ok()
            .and_then(|last| GridArea::new(to.first(), last).ok())
            .unwrap_or(to)
    }

    /// The protection of `sheet` when it is protected for contents.
    fn contents_protection(&self, session: &HostSession, sheet: SheetId) -> Option<SheetProtect> {
        self.sheet_protection(session.host(), sheet).filter(|held| held.contents)
    }

    /// Table features on a sheet protected for contents (ww22 protect/): `ListObjects.Add` is error 3000 and
    /// `ListObject.Delete` 1004, with `UserInterfaceOnly` too. `Unlist` is allowed.
    pub(super) fn guard_table_features(&self, session: &HostSession, sheet: SheetId, number: i32) -> Result<(), AutoError> {
        match self.contents_protection(session, sheet) {
            Some(_) => Err(AutoError::excel(number, "Table features aren't available because the sheet is protected.")),
            None => Ok(()),
        }
    }

    /// `Worksheet.AutoFilterMode = False` on a sheet protected for contents: 1004 unless AllowFiltering (`UserInterfaceOnly`
    /// does not admit it).
    pub(super) fn guard_filter_mode(&self, session: &HostSession, sheet: SheetId) -> Result<(), AutoError> {
        match self.contents_protection(session, sheet) {
            Some(held) if !held.allow[Allow::Filtering as usize] => {
                Err(AutoError::excel(1004, "Unable to set the AutoFilterMode property of the Worksheet class"))
            }
            _ => Ok(()),
        }
    }

    /// `Worksheet.ShowAllData` on a sheet protected for contents: 1004 unless `UserInterfaceOnly` (the Allow* flags do not
    /// admit it).
    pub(super) fn guard_show_all(&self, session: &HostSession, sheet: SheetId) -> Result<(), AutoError> {
        match self.contents_protection(session, sheet) {
            Some(held) if !held.user_interface_only => {
                Err(AutoError::excel(1004, "Unable to get the ShowAllData property of the Worksheet class"))
            }
            _ => Ok(()),
        }
    }

    /// `Range.Insert` / `Range.Delete` on a sheet protected for contents (`UserInterfaceOnly` allows it). A cell shift is
    /// 1004 when a cell it moves is locked (ww22: an unlocked D11 over a locked D12 refused). Whole rows or columns are
    /// allowed by the matching Allow* flag for Insert, and for Delete when their cells are unlocked; the other cases
    /// have no Excel answer yet (dc-ml3k0).
    pub(super) fn guard_shift(
        &self,
        session: &HostSession,
        area: GridArea,
        insert: bool,
        shift: Option<i64>,
    ) -> Result<(), AutoError> {
        let sheet = area.sheet();
        let Some(held) = self.contents_protection(session, sheet) else { return Ok(()) };
        if held.user_interface_only {
            return Ok(());
        }
        let (first, last) = (area.first(), area.last());
        let whole_columns = first.row() == 1 && last.row() == MAX_ROWS;
        let whole_rows = first.column() == 1 && last.column() == MAX_COLUMNS;
        let verb = if insert { "Insert" } else { "Delete" };
        if whole_columns || whole_rows {
            let flag = match (whole_columns, insert) {
                (true, true) => Allow::InsertingColumns,
                (false, true) => Allow::InsertingRows,
                (true, false) => Allow::DeletingColumns,
                (false, false) => Allow::DeletingRows,
            };
            if !held.allow[flag as usize] {
                return Err(AutoError::not_modelled(format!("Range.{verb} of whole rows or columns on a protected sheet"), ALLOW_OWNER));
            }
            return if insert { Ok(()) } else { self.guard_area(session, area) };
        }
        let down = match shift {
            Some(XL_SHIFT_DOWN | XL_SHIFT_UP) => true,
            Some(_) => false,
            None => area.column_count() >= area.row_count(),
        };
        let moved = if down {
            CellKey::new(sheet, MAX_ROWS, last.column()).ok().and_then(|end| GridArea::new(first, end).ok())
        } else {
            CellKey::new(sheet, last.row(), MAX_COLUMNS).ok().and_then(|end| GridArea::new(first, end).ok())
        };
        let Some(moved) = moved else { return Ok(()) };
        self.guard_area(session, moved)
            .map_err(|_| AutoError::excel(1004, format!("Unable to get the {verb} property of the Range class")))
    }

    /// A macro's change to something the whole sheet holds (a conditional-format rule): refused with 1004 while the
    /// sheet is protected for contents, unless `UserInterfaceOnly` was set.
    pub(super) fn guard_sheet_contents(&self, session: &HostSession, sheet: SheetId) -> Result<(), AutoError> {
        match self.sheet_protection(session.host(), sheet) {
            Some(held) if held.contents && !held.user_interface_only => Err(protected_sheet()),
            _ => Ok(()),
        }
    }

    fn worksheet_protect(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let sheet = Self::worksheet_sheet(target)?;
        if call.args.len() > 16 {
            return Err(geometry::bad_count());
        }
        let password = match call.arg(0) {
            AutoValue::Missing | AutoValue::Empty => None,
            AutoValue::Text(t) => Some(t.to_string_lossy()).filter(|p| !p.is_empty()),
            // ww21 Protect.numericPassword: an integer is accepted as its decimal text.
            AutoValue::I2(n) => Some(n.to_string()),
            AutoValue::I4(n) => Some(n.to_string()),
            // ww24 g/case5: a double is the text Excel shows for it (1.5 needs "1.5").
            AutoValue::R8(n) => Some(Self::number_password(*n)?),
            AutoValue::R4(n) => Some(Self::number_password(f64::from(*n))?),
            _ => return Err(geometry::type_mismatch()),
        };
        let flag = |index: usize, default: bool| match call.arg(index) {
            AutoValue::Missing | AutoValue::Empty => Ok(default),
            AutoValue::Bool(b) => Ok(*b),
            _ => Err(geometry::type_mismatch()),
        };
        let protect = SheetProtect {
            password,
            file_hash: None,
            drawing_objects: flag(1, true)?,
            contents: flag(2, true)?,
            scenarios: flag(3, true)?,
            user_interface_only: flag(4, false)?,
            allow: {
                let mut allow = [false; 11];
                for (offset, slot) in allow.iter_mut().enumerate() {
                    *slot = flag(5 + offset, false)?;
                }
                allow
            },
        };
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            match self.sheet_protection(session.host(), sheet) {
                None => {
                    self.sheet_protect.borrow_mut().insert(sheet, Some(protect.clone()));
                    session.host.sheet_protection_saves.insert(sheet, Some(protect.to_saved()));
                }
                // ww22 Protect.Again: the same Password protects again with the new arguments (UserInterfaceOnly
                // becomes True), a wrong one is 1004 and changes nothing, an omitted one opens the Unprotect Sheet
                // prompt.
                Some(held) if held.has_password() => {
                    if call.arg(0).is_missing() {
                        return Err(AutoError::not_modelled("Worksheet.Protect password prompt on a protected sheet", OWNER));
                    }
                    let given = protect.password.clone().unwrap_or_default();
                    if !held.password_matches(&given)? {
                        return Err(wrong_password());
                    }
                    self.sheet_protect.borrow_mut().insert(sheet, Some(protect.clone()));
                    session.host.sheet_protection_saves.insert(sheet, Some(protect.to_saved()));
                }
                // Protecting a sheet that is already protected without a password leaves its protection as it is.
                Some(held) if protect.user_interface_only && !held.user_interface_only => {
                    return Err(AutoError::not_modelled(
                        "Worksheet.Protect with UserInterfaceOnly on a sheet already protected",
                        ALLOW_OWNER,
                    ));
                }
                Some(_) => {}
            }
            Ok(AutoValue::Empty)
        })
    }

    /// The text a numeric `Password` stands for: plain decimal for the magnitudes Excel prints without an exponent.
    fn number_password(n: f64) -> Result<String, AutoError> {
        if n.is_finite() && (n == 0.0 || (1e-5..1e15).contains(&n.abs())) {
            Ok(format!("{n}"))
        } else {
            Err(AutoError::not_modelled("Worksheet.Protect with a Password number Excel prints in exponent form", OWNER))
        }
    }

    fn worksheet_unprotect(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let sheet = Self::worksheet_sheet(target)?;
        if call.args.len() > 1 {
            return Err(geometry::bad_count());
        }
        let given = match call.arg(0) {
            AutoValue::Missing | AutoValue::Empty => None,
            AutoValue::Text(t) => Some(t.to_string_lossy()),
            // ww24 g/case4-5: a number is never the password, even when its text is (1004).
            AutoValue::I2(_) | AutoValue::I4(_) | AutoValue::R4(_) | AutoValue::R8(_) => Some(NUMBER_NEVER_MATCHES.to_owned()),
            _ => return Err(geometry::type_mismatch()),
        };
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let Some(held) = self.sheet_protection(session.host(), sheet) else {
                return Ok(AutoValue::Empty);
            };
            // ww21 Unprotect.omitted: Excel opens the Unprotect Sheet dialog.
            if held.has_password() && call.arg(0).is_missing() {
                return Err(AutoError::not_modelled("Worksheet.Unprotect password prompt", OWNER));
            }
            if held.has_password() && !held.password_matches(&given.clone().unwrap_or_default())? {
                return Err(wrong_password());
            }
            self.sheet_protect.borrow_mut().insert(sheet, None);
            session.host.sheet_protection_saves.insert(sheet, None);
            Ok(AutoValue::Empty)
        })
    }

    fn protection_flag(&self, target: &ObjectRef, call: &Call<'_>, read: fn(&SheetProtect) -> bool) -> Reply {
        no_args(call)?;
        let sheet = Self::worksheet_sheet(target)?;
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            Ok(AutoValue::Bool(self.sheet_protection(session.host(), sheet).is_some_and(|held| read(&held))))
        })
    }
    fn protect_contents(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.protection_flag(target, call, |p| p.contents)
    }
    fn protect_drawing_objects(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.protection_flag(target, call, |p| p.drawing_objects)
    }
    fn protect_scenarios(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.protection_flag(target, call, |p| p.scenarios)
    }
    fn protection_mode(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.protection_flag(target, call, |p| p.user_interface_only)
    }
}
