//! The workbook part of an Excel binary workbook (MS-XLSB 2.1.7.61): its sheets (BrtBundleSh),
//! properties (BrtWbProp), views (BrtBookView), calculation properties (BrtCalcProp), defined
//! names (BrtName) and supporting links (BrtSupSelf, BrtSupSame, BrtSupBookSrc, BrtSupAddin,
//! BrtExternSheet), read; and the SpreadsheetML workbook part written from them.

use super::formula::{self, Context, NameRef, Origin, Xti};
use super::parts::Shared;
use super::records::{Cursor, Read, records, text};
use super::{MAIN_NAMESPACE, RELATIONSHIPS_NAMESPACE, Xml, refusal};
use crate::OpenError;
use crate::opc::Package;

pub(crate) const BRT_NAME: u16 = 39;
const BRT_FILE_VERSION: u16 = 128;
const BRT_FILE_SHARING: u16 = 548;
const BRT_WB_PROP: u16 = 153;
const BRT_BUNDLE_SH: u16 = 156;
const BRT_CALC_PROP: u16 = 157;
const BRT_BOOK_VIEW: u16 = 158;
const BRT_SUP_BOOK_SRC: u16 = 355;
const BRT_SUP_SELF: u16 = 357;
const BRT_SUP_SAME: u16 = 358;
const BRT_EXTERN_SHEET: u16 = 362;
const BRT_SUP_ADDIN: u16 = 667;
const BRT_PLACEHOLDER_NAME: u16 = 361;

/// One sheet of the workbook.
pub(crate) struct Sheet {
    pub(crate) state: u32,
    pub(crate) tab_id: u32,
    pub(crate) relationship: String,
    pub(crate) name: String,
    /// The part the relationship names.
    pub(crate) part: Option<String>,
}

/// A defined name as read, its formula still parsed tokens.
pub(crate) struct Name {
    pub(crate) flags: u32,
    pub(crate) key: u8,
    pub(crate) scope: Option<u32>,
    pub(crate) name: String,
    pub(crate) rgce: Vec<u8>,
    pub(crate) rgcb: Vec<u8>,
    pub(crate) comment: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) help: Option<String>,
}

impl Name {
    fn hidden(&self) -> bool {
        self.flags & 1 != 0
    }

    /// The name as SpreadsheetML writes it: a built-in name (fBuiltin, bit 5) with `_xlnm.`
    /// before it (`_xlnm._FilterDatabase`, `_xlnm.Print_Area`).
    pub(crate) fn full_name(&self) -> String {
        if self.flags & (1 << 5) != 0 && !self.name.starts_with("_xlnm.") {
            format!("_xlnm.{}", self.name)
        } else {
            self.name.clone()
        }
    }

    /// A future function's name (`_xlfn.XLOOKUP`), which a SpreadsheetML part does not list.
    pub(crate) fn future_function(&self) -> bool {
        self.flags & (1 << 17) != 0
    }
}

/// A supporting link (2.2.7.2), as the workbook part lists it.
pub(crate) enum Link {
    This,
    Same,
    External { relationship: String },
    AddIn { names: Vec<String> },
}

/// The workbook part, read.
pub(crate) struct Book {
    pub(crate) part: String,
    pub(crate) sheets: Vec<Sheet>,
    pub(crate) names: Vec<Name>,
    pub(crate) links: Vec<Link>,
    pub(crate) xti: Vec<Xti>,
    /// BrtWbProp: its flags, theme version and code name.
    pub(crate) properties: Option<(u32, u32, Option<String>)>,
    pub(crate) views: Vec<[u32; 7]>,
    pub(crate) view_flags: Vec<u8>,
    /// BrtCalcProp: calculation id, mode, iteration count, delta, threads, flags.
    pub(crate) calculation: Option<(u32, u32, u32, f64, i32, u16)>,
    /// BrtFileVersion: the code name GUID, the application, last and lowest editing versions
    /// and build.
    pub(crate) version: Option<(Option<String>, [String; 4])>,
    /// BrtFileSharing: read-only recommended, the reservation password hash and the user name.
    pub(crate) sharing: Option<(bool, u16, String)>,
}

fn malformed(part: &str) -> impl Fn(super::Malformed) -> OpenError + '_ {
    move |error| refusal(part, error.0)
}

pub(crate) fn read(
    package: &Package,
    part: &str,
    read: &dyn Fn(&str) -> Result<Vec<u8>, OpenError>,
) -> Result<Book, OpenError> {
    let data = read(part)?;
    let records = records(&data).map_err(malformed(part))?;
    let relationships = package.relationships_of(Some(part));
    let target = |id: &str| -> Option<String> {
        relationships?
            .items()
            .iter()
            .find(|item| item.id == id)
            .and_then(|item| item.resolved.clone())
    };
    let mut book = Book {
        part: part.to_owned(),
        sheets: Vec::new(),
        names: Vec::new(),
        links: Vec::new(),
        xti: Vec::new(),
        properties: None,
        views: Vec::new(),
        view_flags: Vec::new(),
        calculation: None,
        version: None,
        sharing: None,
    };
    let result: Read<()> = (|| {
        for record in &records {
            let mut c = Cursor::new(record.body);
            match record.kind {
                BRT_BUNDLE_SH => {
                    let state = c.u32("BrtBundleSh")?;
                    let tab_id = c.u32("BrtBundleSh")?;
                    let relationship = text(&c.nullable_wide("BrtBundleSh")?.unwrap_or_default());
                    let name = text(&c.wide("BrtBundleSh")?);
                    let part = target(&relationship);
                    book.sheets.push(Sheet {
                        state,
                        tab_id,
                        relationship,
                        name,
                        part,
                    });
                }
                BRT_NAME => {
                    let flags = c.u32("BrtName")?;
                    let key = c.u8("BrtName")?;
                    let itab = c.u32("BrtName")?;
                    let name = text(&c.wide("BrtName")?);
                    let cce = c.u32("BrtName")? as usize;
                    let rgce = c.bytes(cce, "BrtName")?.to_vec();
                    let cb = c.u32("BrtName")? as usize;
                    let rgcb = c.bytes(cb, "BrtName")?.to_vec();
                    let comment = c.nullable_wide("BrtName")?.map(|u| text(&u));
                    let (mut description, mut help) = (None, None);
                    if flags & (1 << 3) != 0 && c.remaining() > 0 {
                        let _unused = c.nullable_wide("BrtName")?;
                        description = c.nullable_wide("BrtName")?.map(|u| text(&u));
                        help = c.nullable_wide("BrtName")?.map(|u| text(&u));
                    }
                    book.names.push(Name {
                        flags,
                        key,
                        scope: (itab != 0xFFFF_FFFF).then_some(itab),
                        name,
                        rgce,
                        rgcb,
                        comment,
                        description,
                        help,
                    });
                }
                BRT_WB_PROP => {
                    let flags = c.u32("BrtWbProp")?;
                    let theme = c.u32("BrtWbProp")?;
                    let code = if c.remaining() > 0 {
                        c.nullable_wide("BrtWbProp")?.map(|u| text(&u))
                    } else {
                        None
                    };
                    book.properties = Some((flags, theme, code));
                }
                BRT_BOOK_VIEW => {
                    let mut view = [0u32; 7];
                    for value in &mut view {
                        *value = c.u32("BrtBookView")?;
                    }
                    book.views.push(view);
                    book.view_flags.push(c.u8("BrtBookView")?);
                }
                BRT_CALC_PROP => {
                    book.calculation = Some((
                        c.u32("BrtCalcProp")?,
                        c.u32("BrtCalcProp")?,
                        c.u32("BrtCalcProp")?,
                        c.f64("BrtCalcProp")?,
                        c.i32("BrtCalcProp")?,
                        c.u16("BrtCalcProp")?,
                    ));
                }
                BRT_FILE_VERSION => {
                    let code = super::parts::guid(c.bytes(16, "BrtFileVersion")?)
                        .filter(|g| g != "{00000000-0000-0000-0000-000000000000}");
                    let mut version: [String; 4] = Default::default();
                    for field in &mut version {
                        *field = text(&c.wide("BrtFileVersion")?);
                    }
                    book.version = Some((code, version));
                }
                BRT_FILE_SHARING => {
                    let read_only = c.u16("BrtFileSharing")? != 0;
                    let password = c.u16("BrtFileSharing")?;
                    let user = c
                        .nullable_wide("BrtFileSharing")?
                        .map(|u| text(&u))
                        .unwrap_or_default();
                    book.sharing = Some((read_only, password, user));
                }
                BRT_SUP_SELF => book.links.push(Link::This),
                BRT_SUP_SAME => book.links.push(Link::Same),
                BRT_SUP_BOOK_SRC => {
                    let relationship = text(&c.wide("BrtSupBookSrc")?);
                    book.links.push(Link::External { relationship });
                }
                BRT_SUP_ADDIN => {
                    // An add-in's functions; Excel writes the record empty when it lists none.
                    let mut names = Vec::new();
                    if c.remaining() > 0 {
                        let count = c.u32("BrtSupAddin")?;
                        for _ in 0..count {
                            names.push(text(&c.wide("BrtSupAddin")?));
                        }
                    }
                    book.links.push(Link::AddIn { names });
                }
                // A function of the add-in whose BrtSupAddin came last (2.4.729).
                BRT_PLACEHOLDER_NAME => {
                    let name = text(&c.wide("BrtPlaceholderName")?);
                    match book.links.last_mut() {
                        Some(Link::AddIn { names }) => names.push(name),
                        _ => {
                            return Err(super::Malformed(
                                "a BrtPlaceholderName after no BrtSupAddin".to_owned(),
                            ));
                        }
                    }
                }
                BRT_EXTERN_SHEET => {
                    let count = c.u32("BrtExternSheet")?;
                    for _ in 0..count {
                        book.xti.push(Xti {
                            supbook: c.u32("BrtExternSheet")?,
                            first: c.i32("BrtExternSheet")?,
                            last: c.i32("BrtExternSheet")?,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(())
    })();
    result.map_err(malformed(part))?;
    Ok(book)
}

/// The names a formula of the workbook can name, by their 1-based index.
pub(crate) fn name_refs(book: &Book) -> Vec<NameRef> {
    book.names
        .iter()
        .map(|name| NameRef {
            name: name.full_name(),
            sheet: name.scope,
        })
        .collect()
}

/// The SpreadsheetML workbook part.
pub(crate) fn write(book: &Book, shared: &Shared) -> Result<Vec<u8>, OpenError> {
    let part = &book.part;
    let mut x = Xml::new();
    x.open(
        "workbook",
        &[
            ("xmlns", MAIN_NAMESPACE.to_owned()),
            ("xmlns:r", RELATIONSHIPS_NAMESPACE.to_owned()),
        ],
    );
    if let Some((code, version)) = &book.version {
        let mut attributes = Vec::new();
        for (name, value) in ["appName", "lastEdited", "lowestEdited", "rupBuild"]
            .into_iter()
            .zip(version)
        {
            if !value.is_empty() {
                attributes.push((name, value.clone()));
            }
        }
        if let Some(code) = code {
            attributes.push(("codeName", code.clone()));
        }
        x.empty("fileVersion", &attributes);
    }
    if let Some((read_only, password, user)) = &book.sharing {
        let mut attributes = Vec::new();
        if *read_only {
            attributes.push(("readOnlyRecommended", "1".to_owned()));
        }
        if !user.is_empty() {
            attributes.push(("userName", user.clone()));
        }
        if *password != 0 {
            attributes.push(("reservationPassword", format!("{password:04X}")));
        }
        x.empty("fileSharing", &attributes);
    }
    if let Some((flags, theme, code)) = &book.properties {
        let mut attributes = Vec::new();
        if let Some(code) = code.as_ref().filter(|code| !code.is_empty()) {
            attributes.push(("codeName", code.clone()));
        }
        let bit = |n: u32| flags & (1 << n) != 0;
        if bit(0) {
            attributes.push(("date1904", "1".to_owned()));
        }
        // BrtWbProp's flags (2.4.820), as workbookPr's attributes in the schema's order.
        match (flags >> 13) & 0x3 {
            1 => attributes.push(("showObjects", "placeholders".to_owned())),
            2 => attributes.push(("showObjects", "none".to_owned())),
            _ => {}
        }
        if bit(2) {
            attributes.push(("showBorderUnselectedTables", "0".to_owned()));
        }
        if bit(3) {
            attributes.push(("filterPrivacy", "1".to_owned()));
        }
        if bit(4) {
            attributes.push(("promptedSolutions", "1".to_owned()));
        }
        if !bit(5) {
            attributes.push(("showInkAnnotation", "0".to_owned()));
        }
        if bit(6) {
            attributes.push(("backupFile", "1".to_owned()));
        }
        if bit(7) {
            attributes.push(("saveExternalLinkValues", "0".to_owned()));
        }
        match (flags >> 8) & 0x3 {
            1 => attributes.push(("updateLinks", "never".to_owned())),
            2 => attributes.push(("updateLinks", "always".to_owned())),
            _ => {}
        }
        if bit(11) {
            attributes.push(("publishItems", "1".to_owned()));
        }
        if bit(12) {
            attributes.push(("checkCompatibility", "1".to_owned()));
        }
        if !bit(16) {
            attributes.push(("autoCompressPictures", "0".to_owned()));
        }
        if bit(18) {
            attributes.push(("refreshAllConnections", "1".to_owned()));
        }
        if *theme != 0 {
            attributes.push(("defaultThemeVersion", theme.to_string()));
        }
        x.empty("workbookPr", &attributes);
    }
    if !book.views.is_empty() {
        x.open("bookViews", &[]);
        for (view, flags) in book.views.iter().zip(&book.view_flags) {
            let mut attributes = vec![
                ("xWindow", (view[0] as i32).to_string()),
                ("yWindow", (view[1] as i32).to_string()),
                ("windowWidth", view[2].to_string()),
                ("windowHeight", view[3].to_string()),
            ];
            for (value, name, default) in [
                (view[4], "tabRatio", 600),
                (view[5], "firstSheet", 0),
                (view[6], "activeTab", 0),
            ] {
                if value != default {
                    attributes.push((name, value.to_string()));
                }
            }
            if flags & 0x02 != 0 {
                attributes.push(("visibility", "veryHidden".to_owned()));
            } else if flags & 0x01 != 0 {
                attributes.push(("visibility", "hidden".to_owned()));
            }
            // fIconic, fDspHScroll, fDspVScroll, fBotAdornment (BrtBookView, 2.4.313).
            if flags & 0x04 != 0 {
                attributes.push(("minimized", "1".to_owned()));
            }
            if flags & 0x08 == 0 {
                attributes.push(("showHorizontalScroll", "0".to_owned()));
            }
            if flags & 0x10 == 0 {
                attributes.push(("showVerticalScroll", "0".to_owned()));
            }
            if flags & 0x20 == 0 {
                attributes.push(("showSheetTabs", "0".to_owned()));
            }
            if flags & 0x40 == 0 {
                attributes.push(("autoFilterDateGrouping", "0".to_owned()));
            }
            x.empty("workbookView", &attributes);
        }
        x.close("bookViews");
    }
    x.open("sheets", &[]);
    for sheet in &book.sheets {
        let mut attributes = vec![
            ("name", sheet.name.clone()),
            ("sheetId", sheet.tab_id.to_string()),
        ];
        match sheet.state {
            1 => attributes.push(("state", "hidden".to_owned())),
            2 => attributes.push(("state", "veryHidden".to_owned())),
            _ => {}
        }
        attributes.push(("r:id", sheet.relationship.clone()));
        x.empty("sheet", &attributes);
    }
    x.close("sheets");
    let externals: Vec<&String> = book
        .links
        .iter()
        .filter_map(|link| match link {
            Link::External { relationship } => Some(relationship),
            _ => None,
        })
        .collect();
    if !externals.is_empty() {
        x.open("externalReferences", &[]);
        for relationship in externals {
            x.empty("externalReference", &[("r:id", relationship.clone())]);
        }
        x.close("externalReferences");
    }
    let listed: Vec<&Name> = book
        .names
        .iter()
        .filter(|name| {
            // A future function's name, a LAMBDA parameter's and a name with no formula (a
            // user-defined function a formula calls) are not listed in SpreadsheetML.
            !name.future_function() && !name.name.starts_with("_xlpm.") && !name.rgce.is_empty()
        })
        .collect();
    if !listed.is_empty() {
        x.open("definedNames", &[]);
        for name in listed {
            let context = Context {
                sheets: &shared.sheet_names,
                xti: &book.xti,
                supbooks: &shared.supbooks,
                names: &shared.names,
                tables: &shared.tables,
                sheet: name.scope,
            };
            let text = match formula::print(&name.rgce, &name.rgcb, &context, Origin::default())
                .map_err(|error| {
                    refusal(
                        part,
                        format!(
                            "the defined name {:?}: {} ({})",
                            name.name,
                            error.0,
                            name.rgce
                                .iter()
                                .map(|b| format!("{b:02x}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ),
                    )
                })? {
                Ok(text) => text,
                Err(_) => return Err(refusal(part, "a defined name holding a PtgExp")),
            };
            let mut attributes = vec![("name", name.full_name())];
            if let Some(comment) = &name.comment {
                attributes.push(("comment", comment.clone()));
            }
            if let Some(description) = &name.description {
                attributes.push(("description", description.clone()));
            }
            if let Some(help) = &name.help {
                attributes.push(("help", help.clone()));
            }
            if let Some(scope) = name.scope {
                attributes.push(("localSheetId", scope.to_string()));
            }
            if name.hidden() {
                attributes.push(("hidden", "1".to_owned()));
            }
            // fFunc (bit 1), fOB (bit 2, a VBA procedure) and fProc (bit 3, a macro).
            let (function, visual_basic, procedure) = (
                name.flags & (1 << 1) != 0,
                name.flags & (1 << 2) != 0,
                name.flags & (1 << 3) != 0,
            );
            if function {
                attributes.push(("function", "1".to_owned()));
            }
            if visual_basic {
                attributes.push(("vbProcedure", "1".to_owned()));
            }
            if procedure && !visual_basic {
                attributes.push(("xlm", "1".to_owned()));
            }
            let group = (name.flags >> 6) & 0x1FF;
            if procedure && group != 0 {
                attributes.push(("functionGroupId", group.to_string()));
            }
            if name.key != 0 {
                attributes.push(("shortcutKey", char::from(name.key).to_string()));
            }
            if name.flags & (1 << 15) != 0 {
                attributes.push(("publishToServer", "1".to_owned()));
            }
            if name.flags & (1 << 16) != 0 {
                attributes.push(("workbookParameter", "1".to_owned()));
            }
            x.element("definedName", &attributes, &text);
        }
        x.close("definedNames");
    }
    if let Some((id, mode, count, delta, threads, flags)) = book.calculation {
        let mut attributes = vec![("calcId", id.to_string())];
        match mode {
            0 => attributes.push(("calcMode", "manual".to_owned())),
            2 => attributes.push(("calcMode", "autoNoTable".to_owned())),
            _ => {}
        }
        if flags & 0x01 != 0 {
            attributes.push(("fullCalcOnLoad", "1".to_owned()));
        }
        if flags & 0x02 == 0 {
            attributes.push(("refMode", "R1C1".to_owned()));
        }
        if flags & 0x04 != 0 {
            attributes.push(("iterate", "1".to_owned()));
        }
        if count != 100 {
            attributes.push(("iterateCount", count.to_string()));
        }
        if delta != 0.001 {
            attributes.push(("iterateDelta", formula::number_text(delta)));
        }
        if flags & 0x08 == 0 {
            attributes.push(("fullPrecision", "0".to_owned()));
        }
        if flags & 0x20 == 0 {
            attributes.push(("calcOnSave", "0".to_owned()));
        }
        if flags & 0x40 == 0 {
            attributes.push(("concurrentCalc", "0".to_owned()));
        }
        if flags & 0x80 != 0 {
            attributes.push(("concurrentManualCount", threads.to_string()));
        }
        if flags & 0x100 != 0 {
            attributes.push(("forceFullCalc", "1".to_owned()));
        }
        x.empty("calcPr", &attributes);
    }
    x.close("workbook");
    Ok(x.out)
}
