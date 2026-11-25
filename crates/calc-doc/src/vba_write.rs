//! Author an Excel project from VBA source, using the MS-OVBA stream layout.

use crate::model::{SheetName, value::ExcelText};
use crate::xls::cfb_write::{self, Node};
use std::collections::BTreeSet;
use std::fmt::{self, Write};

const PROJECT_NAME: &str = "VBAProject";
const PROJECT_ID: &str = "{CB4A6FD3-4DB2-4698-915C-D69261AB7550}";
const WORKBOOK_CODE_NAME: &str = "ThisWorkbook";
const SHEET_CODE_NAME: &str = "Sheet1";
const MAX_MODULES: usize = 1024;
const MAX_MODULE_BYTES: usize = 4 * 1024 * 1024;
const MAX_PROJECT_BYTES: usize = 8 * 1024 * 1024;

/// The kind of module to author.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceModuleKind {
    /// A procedural module.
    Standard,
    /// An instantiable class module.
    Class,
    /// The document module bound to the workbook's code name.
    Workbook,
    /// A document module bound to a worksheet's code name.
    Worksheet,
}

/// A module's authored source.
#[derive(Clone, Copy, Debug)]
pub struct SourceModule<'a> {
    /// ASCII VBA identifier, at most 31 characters to fit the compound-file stream name.
    pub name: &'a str,
    /// The kind the VBA host gives this module.
    pub kind: SourceModuleKind,
    /// Code as shown in the VBE editor. The writer supplies project module attributes.
    pub source: &'a str,
}

/// A source project could not be authored.
#[derive(Debug, PartialEq, Eq)]
pub enum VbaWriteError {
    /// A name is invalid, reserved, duplicated, or cannot bind to the requested document.
    ModuleName(String),
    /// This module's source contains attributes, a NUL or text outside Windows-1252.
    Source(String),
    /// Invalid or duplicate tab name.
    SheetName(String),
    /// A workbook must have at least one sheet.
    NoSheets,
    /// The source project exceeds a size limit.
    Limit(&'static str),
}

impl fmt::Display for VbaWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModuleName(name) => {
                write!(f, "invalid, reserved or duplicate VBA module name {name:?}")
            }
            Self::Source(name) => write!(
                f,
                "invalid source in VBA module {name:?}: expected Windows-1252 code without module attributes or NUL"
            ),
            Self::SheetName(name) => write!(f, "invalid or duplicate worksheet name {name:?}"),
            Self::NoSheets => f.write_str("a macro-enabled workbook needs at least one sheet"),
            Self::Limit(limit) => write!(f, "VBA project exceeds {limit} limit"),
        }
    }
}

impl std::error::Error for VbaWriteError {}

/// Write a deterministic `vbaProject.bin` with Windows-1252 source and no compiled cache.
/// This encodes module source and metadata. It does not compile or execute VBA.
pub fn write_vba_project(modules: &[SourceModule<'_>]) -> Result<Vec<u8>, VbaWriteError> {
    if modules.len() > MAX_MODULES {
        return Err(VbaWriteError::Limit("module count"));
    }
    let mut names = BTreeSet::from(["dir".to_owned(), "_vba_project".to_owned()]);
    let mut streams = vec![(
        "_VBA_PROJECT".to_owned(),
        Node::Stream(vec![0xcc, 0x61, 0xff, 0xff, 0, 1, 0]),
    )];
    let mut bytes = 0;
    for module in modules {
        let mut chars = module.name.bytes();
        if module.name.len() > 31
            || !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || !names.insert(module.name.to_ascii_lowercase())
        {
            return Err(VbaWriteError::ModuleName(module.name.to_owned()));
        }
        let source = encoded_source(module)?;
        bytes += source.len();
        if bytes > MAX_PROJECT_BYTES {
            return Err(VbaWriteError::Limit("project bytes"));
        }
        streams.push((
            module.name.to_owned(),
            Node::Stream(compress_vba_stream(&source)),
        ));
    }
    streams.push((
        "dir".to_owned(),
        Node::Stream(compress_vba_stream(&directory(modules))),
    ));
    cfb_write::write(vec![
        (
            "PROJECT".to_owned(),
            Node::Stream(project_stream(modules).into_bytes()),
        ),
        ("PROJECTwm".to_owned(), Node::Stream(name_map(modules))),
        ("VBA".to_owned(), Node::Storage(streams)),
    ])
    .filter(|project| project.len() <= MAX_PROJECT_BYTES)
    .ok_or(VbaWriteError::Limit("project bytes"))
}

/// Write a blank .xlsm whose single worksheet is `Sheet1`, with the supplied VBA modules.
/// Empty document modules fill in `ThisWorkbook` and `Sheet1` when omitted.
pub fn source_vba_package(modules: &[SourceModule<'_>]) -> Result<Vec<u8>, VbaWriteError> {
    source_vba_package_for_sheets(&[SHEET_CODE_NAME], modules)
}

/// Write a blank .xlsm with the requested worksheet tabs in order. The VBA document code
/// names are `ThisWorkbook` and `Sheet1`, `Sheet2`, and so on, independent of the tab labels.
pub fn source_vba_package_for_sheets(
    sheet_names: &[&str],
    modules: &[SourceModule<'_>],
) -> Result<Vec<u8>, VbaWriteError> {
    if sheet_names.is_empty() {
        return Err(VbaWriteError::NoSheets);
    }
    if sheet_names.len() >= MAX_MODULES || modules.len() > MAX_MODULES {
        return Err(VbaWriteError::Limit("module count"));
    }
    let mut tabs = BTreeSet::new();
    for name in sheet_names {
        let text = ExcelText::from_utf16_code_units(name.encode_utf16().collect());
        if SheetName::new(text).is_err()
            || name.chars().any(char::is_control)
            || !tabs.insert(name.to_lowercase())
        {
            return Err(VbaWriteError::SheetName((*name).to_owned()));
        }
    }
    let code_names: Vec<_> = (1..=sheet_names.len())
        .map(|index| format!("Sheet{index}"))
        .collect();
    let documents: Vec<_> = std::iter::once((SourceModuleKind::Workbook, WORKBOOK_CODE_NAME))
        .chain(
            code_names
                .iter()
                .map(|name| (SourceModuleKind::Worksheet, name.as_str())),
        )
        .collect();

    let mut project = modules.to_vec();
    for module in modules.iter().filter(|module| {
        matches!(
            module.kind,
            SourceModuleKind::Workbook | SourceModuleKind::Worksheet
        )
    }) {
        if !documents.contains(&(module.kind, module.name)) {
            return Err(VbaWriteError::ModuleName(module.name.to_owned()));
        }
    }
    for (kind, name) in documents {
        if !modules
            .iter()
            .any(|module| module.kind == kind && module.name == name)
        {
            project.push(SourceModule {
                name,
                kind,
                source: "",
            });
        }
    }

    let sheets: Vec<_> = sheet_names
        .iter()
        .zip(&code_names)
        .map(|(tab, code)| (*tab, Some(code.as_str())))
        .collect();
    let part = write_vba_project(&project)?;
    Ok(crate::blank::blank_package_with_sheets(
        &sheets,
        false,
        Some(crate::blank::Project {
            part: &part,
            workbook_code_name: Some(WORKBOOK_CODE_NAME),
            sheet_code_name: None,
        }),
    ))
}

fn encoded_source(module: &SourceModule<'_>) -> Result<Vec<u8>, VbaWriteError> {
    if module.source.len() > MAX_MODULE_BYTES {
        return Err(VbaWriteError::Limit("module bytes"));
    }
    let normalized = module.source.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.contains('\0')
        || normalized.lines().any(|line| {
            line.trim_start()
                .split_ascii_whitespace()
                .next()
                .is_some_and(|word| word.eq_ignore_ascii_case("Attribute"))
        })
    {
        return Err(VbaWriteError::Source(module.name.to_owned()));
    }

    let mut source = format!("Attribute VB_Name = \"{}\"\r\n", module.name);
    let base = match module.kind {
        SourceModuleKind::Standard => None,
        SourceModuleKind::Class => Some("FCFB3D2A-A0FA-1068-A738-08002B3371B5"),
        SourceModuleKind::Workbook => Some("00020819-0000-0000-C000-000000000046"),
        SourceModuleKind::Worksheet => Some("00020820-0000-0000-C000-000000000046"),
    };
    if let Some(base) = base {
        let document = if module.kind == SourceModuleKind::Class {
            "False"
        } else {
            "True"
        };
        source.push_str(&format!(
            "Attribute VB_Base = \"0{{{base}}}\"\r\n\
             Attribute VB_GlobalNameSpace = False\r\n\
             Attribute VB_Creatable = False\r\n\
             Attribute VB_PredeclaredId = {document}\r\n\
             Attribute VB_Exposed = {document}\r\n\
             Attribute VB_TemplateDerived = False\r\n\
             Attribute VB_Customizable = {document}\r\n"
        ));
    }
    source.push_str(&normalized.replace('\n', "\r\n"));
    let (encoded, _, unencodable) = encoding_rs::WINDOWS_1252.encode(&source);
    if unencodable {
        return Err(VbaWriteError::Source(module.name.to_owned()));
    }
    if encoded.len() > MAX_MODULE_BYTES {
        return Err(VbaWriteError::Limit("module bytes"));
    }
    Ok(encoded.into_owned())
}

fn directory(modules: &[SourceModule<'_>]) -> Vec<u8> {
    let mut dir = Vec::new();
    // MS-OVBA 2.3.4.2.1, PROJECTINFORMATION, in record order.
    record(&mut dir, 0x01, &3u32.to_le_bytes()); // Win64
    record(&mut dir, 0x4a, &6u32.to_le_bytes()); // VBA7 compatibility
    record(&mut dir, 0x02, &0x0409u32.to_le_bytes()); // LCID
    record(&mut dir, 0x14, &0x0409u32.to_le_bytes()); // invoke LCID
    record(&mut dir, 0x03, &1252u16.to_le_bytes()); // code page
    record(&mut dir, 0x04, PROJECT_NAME.as_bytes());
    for id in [0x05, 0x40, 0x06, 0x3d] {
        // description and HelpFile, both representations
        record(&mut dir, id, &[]);
    }
    record(&mut dir, 0x07, &[0; 4]); // help context
    record(&mut dir, 0x08, &[0; 4]); // library flags
    record(&mut dir, 0x09, &1u32.to_le_bytes()); // version major, followed by version minor
    dir.extend_from_slice(&0u16.to_le_bytes());
    record(&mut dir, 0x0c, &[]); // constants
    record(&mut dir, 0x3c, &[]);

    // Excel and VBA are intrinsic to the host. These are the regular additional
    // registered libraries of a new Excel VBA project (MS-OVBA 2.3.4.2.2).
    for (name, libid) in [
        (
            "stdole",
            r"*\G{00020430-0000-0000-C000-000000000046}#2.0#0#C:\Windows\System32\stdole2.tlb#OLE Automation",
        ),
        (
            "Office",
            r"*\G{2DF8D04C-5BFA-101B-BDE5-00AA0044DE52}#2.0#0#C:\Program Files\Common Files\Microsoft Shared\OFFICE16\MSO.DLL#Microsoft Office 16.0 Object Library",
        ),
    ] {
        record(&mut dir, 0x16, name.as_bytes());
        record(&mut dir, 0x3e, &utf16(name));
        let mut registered = (libid.len() as u32).to_le_bytes().to_vec();
        registered.extend_from_slice(libid.as_bytes());
        registered.extend_from_slice(&[0; 6]); // reserved
        record(&mut dir, 0x0d, &registered);
    }

    // MS-OVBA 2.3.4.2.3, PROJECTMODULES and one MODULE record per input module.
    record(&mut dir, 0x0f, &(modules.len() as u16).to_le_bytes());
    record(&mut dir, 0x13, &[0; 2]); // project cookie
    for module in modules {
        let name = module.name.as_bytes();
        let wide = utf16(module.name);
        record(&mut dir, 0x19, name);
        record(&mut dir, 0x47, &wide);
        record(&mut dir, 0x1a, name); // stream name
        record(&mut dir, 0x32, &wide);
        record(&mut dir, 0x1c, &[]); // description
        record(&mut dir, 0x48, &[]);
        record(&mut dir, 0x31, &[0; 4]); // source offset
        record(&mut dir, 0x1e, &[0; 4]); // help context
        record(&mut dir, 0x2c, &[0; 2]); // module cookie
        let id = if module.kind == SourceModuleKind::Standard {
            0x21
        } else {
            0x22
        };
        record(&mut dir, id, &[]);
        record(&mut dir, 0x2b, &[]); // module terminator
    }
    record(&mut dir, 0x10, &[]); // directory terminator
    dir
}

fn record(dir: &mut Vec<u8>, id: u16, bytes: &[u8]) {
    dir.extend_from_slice(&id.to_le_bytes());
    dir.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    dir.extend_from_slice(bytes);
}

fn utf16(name: &str) -> Vec<u8> {
    name.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn name_map(modules: &[SourceModule<'_>]) -> Vec<u8> {
    let mut map = Vec::new();
    for module in modules {
        map.extend_from_slice(module.name.as_bytes());
        map.push(0);
        map.extend(utf16(module.name));
        map.extend_from_slice(&[0; 2]);
    }
    map.extend_from_slice(&[0; 2]);
    map
}

fn project_stream(modules: &[SourceModule<'_>]) -> String {
    let mut project = format!("ID=\"{PROJECT_ID}\"\r\n");
    for module in modules {
        let (property, suffix) = match module.kind {
            SourceModuleKind::Standard => ("Module", ""),
            SourceModuleKind::Class => ("Class", ""),
            SourceModuleKind::Workbook | SourceModuleKind::Worksheet => ("Document", "/&H00000000"),
        };
        let _ = write!(project, "{property}={}{suffix}\r\n", module.name);
    }
    let _ = write!(
        project,
        "Name=\"{PROJECT_NAME}\"\r\nHelpContextID=\"0\"\r\nVersionCompatible32=\"393222000\"\r\n"
    );
    for (property, data) in [
        ("CMG", &[0; 4][..]), // unrestricted
        ("DPB", &[0][..]),    // no project password
        ("GC", &[0xff][..]),  // project visible
    ] {
        let _ = write!(project, "{property}=\"{}\"\r\n", project_property(data));
    }
    project.push_str("\r\n[Host Extender Info]\r\n&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000\r\n");
    project
}

/// MS-OVBA 2.4.3.2 uses this reversible encoding for PROJECT properties.
/// Seed zero gives a deterministic encoding with no ignored bytes.
fn project_property(data: &[u8]) -> String {
    let seed = 0u8;
    let version = 2u8;
    let key = PROJECT_ID.bytes().fold(0u8, u8::wrapping_add);
    let mut previous = key ^ seed;
    let mut earlier = version ^ seed;
    let mut unencoded = key;
    let mut result = format!("{seed:02X}{earlier:02X}{previous:02X}");
    for byte in (data.len() as u32)
        .to_le_bytes()
        .into_iter()
        .chain(data.iter().copied())
    {
        let encoded = byte ^ earlier.wrapping_add(unencoded);
        let _ = write!(result, "{encoded:02X}");
        earlier = previous;
        previous = encoded;
        unencoded = byte;
    }
    result
}

/// Encode an MS-OVBA compressed container with literal tokens and raw 4096-byte chunks.
/// A final raw chunk is zero-padded as specified by MS-OVBA 2.4.1.3.10.
pub fn compress_vba_stream(input: &[u8]) -> Vec<u8> {
    let mut output = vec![1];
    for chunk in input.chunks(4096) {
        let encoded_size = chunk.len() + chunk.len().div_ceil(8);
        if encoded_size > 4096 {
            output.extend_from_slice(&0x3fffu16.to_le_bytes());
            output.extend_from_slice(chunk);
            output.resize(output.len() + 4096 - chunk.len(), 0);
        } else {
            let header = 0xb000u16 | (encoded_size as u16 - 1);
            output.extend_from_slice(&header.to_le_bytes());
            for literals in chunk.chunks(8) {
                output.push(0);
                output.extend_from_slice(literals);
            }
        }
    }
    output
}
