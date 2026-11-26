//! Sheet protection a macro sets (dc-oxh8w.68): `Worksheet.Protect`, `Unprotect` and the `Protect*` getters, and the
//! refusal a macro meets when it writes a locked cell of a protected sheet. The state belongs to the Automation
//! profile; the file's own `sheetProtection` (a sheet that loaded protected) counts as contents-protected too.
use super::*;
use crate::HostState;

const OWNER: &str = "dc-9b7zd";
/// Allow* arguments, and UserInterfaceOnly on a protected sheet (open).
const ALLOW_OWNER: &str = "dc-ml3k0";

/// What `Worksheet.Protect` set.
#[derive(Clone, Debug)]
pub(super) struct SheetProtect {
    password: Option<String>,
    drawing_objects: bool,
    contents: bool,
    scenarios: bool,
    user_interface_only: bool,
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
        host.loaded().sheet(sheet).is_some_and(|s| s.protected()).then(|| SheetProtect {
            password: None,
            drawing_objects: false,
            contents: true,
            scenarios: false,
            user_interface_only: false,
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

    /// A write whose result on a sheet protected for contents Excel has not been asked about (`UserInterfaceOnly` set:
    /// allowed): refused typed, owned by dc-5oumk.
    pub(super) fn refuse_while_protected(&self, session: &HostSession, sheet: SheetId, what: &str) -> Result<(), AutoError> {
        match self.sheet_protection(session.host(), sheet) {
            Some(held) if held.contents && !held.user_interface_only => {
                Err(AutoError::not_modelled(format!("{what} on a sheet protected for contents"), "dc-5oumk"))
            }
            _ => Ok(()),
        }
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
            AutoValue::R4(_) | AutoValue::R8(_) => {
                return Err(AutoError::not_modelled("Worksheet.Protect with a floating-point Password", OWNER));
            }
            _ => return Err(geometry::type_mismatch()),
        };
        let flag = |index: usize, default: bool| match call.arg(index) {
            AutoValue::Missing | AutoValue::Empty => Ok(default),
            AutoValue::Bool(b) => Ok(*b),
            _ => Err(geometry::type_mismatch()),
        };
        let protect = SheetProtect {
            password,
            drawing_objects: flag(1, true)?,
            contents: flag(2, true)?,
            scenarios: flag(3, true)?,
            user_interface_only: flag(4, false)?,
        };
        // The Allow* arguments say what a protected sheet still admits: not modelled, so not dropped silently.
        for index in 5..16 {
            if flag(index, false)? {
                return Err(AutoError::not_modelled("Worksheet.Protect Allow* arguments", ALLOW_OWNER));
            }
        }
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            // Protecting a sheet that is already protected leaves its protection as it is.
            match self.sheet_protection(session.host(), sheet) {
                None => {
                    self.sheet_protect.borrow_mut().insert(sheet, Some(protect.clone()));
                }
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

    fn worksheet_unprotect(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let sheet = Self::worksheet_sheet(target)?;
        if call.args.len() > 1 {
            return Err(geometry::bad_count());
        }
        let given = match call.arg(0) {
            AutoValue::Missing | AutoValue::Empty => None,
            AutoValue::Text(t) => Some(t.to_string_lossy()),
            _ => return Err(geometry::type_mismatch()),
        };
        self.with_write(|session| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let Some(held) = self.sheet_protection(session.host(), sheet) else {
                return Ok(AutoValue::Empty);
            };
            if !self.sheet_protect.borrow().contains_key(&sheet) {
                return Err(AutoError::not_modelled("Worksheet.Unprotect of a sheet the file protects", OWNER));
            }
            // ww21 Unprotect.omitted: Excel opens the Unprotect Sheet dialog.
            if held.password.is_some() && call.arg(0).is_missing() {
                return Err(AutoError::not_modelled("Worksheet.Unprotect password prompt", OWNER));
            }
            if held.password.is_some() && held.password != given {
                return Err(wrong_password());
            }
            self.sheet_protect.borrow_mut().insert(sheet, None);
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
