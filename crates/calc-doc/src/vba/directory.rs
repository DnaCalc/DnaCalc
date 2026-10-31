//! Project directory parser ported from ovba 0.7.1, src/lib.rs and src/parser.rs.
//! Copyright (c) 2020 Tim Weis; MIT, retained in deploy/notices/ported.txt.
//! Changes: fallible strict text decoding, no panics, full-input consumption; decompression
//! and compound storage are supplied by the parent reader. No VBA source is parsed here.

#![forbid(unsafe_code)]
#![allow(dead_code)]

use codepage::to_encoding;
use nom::{
    Err::Error,
    IResult,
    bytes::complete::{tag, take},
    combinator::opt,
    error::{ErrorKind, ParseError},
    multi::length_data,
    number::complete::{le_u16, le_u32},
    sequence::{preceded, tuple},
};

/// Specifies the platform for which the VBA project is created.
#[derive(Debug)]
pub enum SysKind {
    /// For 16-bit Windows Platforms.
    Win16,
    /// For 32-bit Windows Platforms.
    Win32,
    /// For Macintosh Platforms.
    MacOs,
    /// For 64-bit Windows Platforms.
    Win64,
}

// TODO: Remove exemption once the implementation is complete.
#[allow(dead_code)]
/// Specifies a reference to a twiddled type library and its extended type library.
#[derive(Debug)]
pub struct ReferenceControl {
    /// (Optional) Name entry
    name: Option<String>,
    libid_original: Option<String>,
    libid_twiddled: String,
    name_extended: Option<String>,
    libid_extended: String,
    guid: Vec<u8>, // Should be an `[u8; 16]`, though I'm not sure how to convert &[u8] returned by the parser into an array.
    /// MUST be Unique for each `ReferenceControl` in the VBA projectwith the same
    /// libid_original.
    cookie: u32,
}

// TODO: Remove exemption once the implementation is complete.
#[allow(dead_code)]
/// Specifies the identifier of the Automation type library the containing
/// [`ReferenceControl`]'s twiddled type library was generated from.
#[derive(Debug)]
pub struct ReferenceOriginal {
    /// (Optional) Name entry
    name: Option<String>,
    libid_original: String,
}

// TODO: Remove exemption once the implementation is complete.
#[allow(dead_code)]
/// Specifies a reference to an Automation type library.
#[derive(Debug)]
pub struct ReferenceRegistered {
    name: Option<String>,
    libid: String,
}

// TODO: Remove exemption once the implementation is complete.
#[allow(dead_code)]
/// Specifies a reference to an external VBA project.
#[derive(Debug)]
pub struct ReferenceProject {
    name: Option<String>,
    libid_absolute: String,
    libid_relative: String,
    major_version: u32,
    minor_version: u16,
}

/// Specifies a reference to an Automation type library or VBA project.
#[derive(Debug)]
pub enum Reference {
    /// The `Reference` is a [`ReferenceControl`].
    Control(ReferenceControl),
    /// The `Reference` is a [`ReferenceOriginal`].
    Original(ReferenceOriginal),
    /// The `Reference` is a [`ReferenceRegistered`].
    Registered(ReferenceRegistered),
    /// The `Reference` is a [`ReferenceProject`].
    Project(ReferenceProject),
}

// TODO: Remove exemption once the implementation is complete.
#[allow(dead_code)]
/// Specifies version-independent information for the VBA project.
#[derive(Debug)]
pub struct Information {
    /// Specifies the platform for which the VBA project is created.
    pub sys_kind: SysKind,
    compat: Option<u32>,
    lcid: u32,
    lcid_invoke: u32,
    /// Specifies the code page for the VBA project.
    ///
    pub code_page: u16,
    name: String,
    doc_string: String,
    help_file_1: String,
    help_context: u32,
    lib_flags: u32,
    version_major: u32,
    version_minor: u16,
    constants: Option<String>,
}

/// Specifies the containing module's type.
#[derive(Debug)]
pub enum ModuleType {
    /// Specifies a procedural module.
    ///
    /// A procedural module is a collection of subroutines and functions.
    Procedural,
    /// Specifies a document module, class module, or designer module.
    ///
    /// A document module is a type of VBA project item that specifies a module for
    /// embedded macros and programmatic access operations that are associated with a
    /// document.
    ///
    /// A class module is a module that contains the definition for a new object. Each
    /// instance of a class creates a new object, and procedures that are defined in the
    /// module become properties and methods of the object.
    ///
    /// A designer module is a VBA module that extends the methods and properties of an
    /// ActiveX control that has been registered with the project.
    ///
    /// The file format specification doesn't distinguish between these three module
    /// types and encodes them using a single umbrella type ID.
    DocClsDesigner,
}

/// Specifies data for a module.
#[derive(Debug)]
pub struct Module {
    /// Specifies a VBA identifier as the name of the containing `Module`.
    pub name: String,
    /// Specifies the stream name in the VBA storage corresponding to the containing
    /// `Module`.
    pub stream_name: String,
    /// Specifies the description for the containing `Module`.
    pub doc_string: String,
    /// Specifies the location of the source code within the stream that corresponds to
    /// the containing `Module`.
    pub text_offset: usize,
    /// Specifies the Help topic identifier for the containing `Module`.
    pub help_context: u32,
    /// Specifies whether the containing `Module` is a procedural module, document
    /// module, class module, or designer module.
    pub module_type: ModuleType,
    /// Specifies that the containing `Module` is read-only.
    pub read_only: bool,
    /// Specifies that the containing `Module` is only usable from within the current VBA
    /// project.
    pub private: bool,
}

#[derive(Debug)]
pub(crate) struct ProjectInformation {
    /// Specifies version-independent information for the VBA project.
    pub information: Information,
    /// Specifies the external references of the VBA project.
    pub references: Vec<Reference>,
    /// Specifies the modules in the project.
    pub modules: Vec<Module>,
}

// TODO: Make this error private by translating to a crate-level error type
//       at the public parser interface.
#[derive(Debug, PartialEq)]
pub(crate) enum FormatError<I> {
    UnexpectedValue,
    Nom(I, ErrorKind),
}

impl<I> ParseError<I> for FormatError<I> {
    fn from_error_kind(input: I, kind: ErrorKind) -> Self {
        FormatError::Nom(input, kind)
    }
    fn append(_: I, _: ErrorKind, other: Self) -> Self {
        other
    }
}

// Several size fields in the binary format have fixed values.
const U32_FIXED_SIZE_4: &[u8] = &[0x04, 0x00, 0x00, 0x00];
const U32_FIXED_SIZE_2: &[u8] = &[0x02, 0x00, 0x00, 0x00];

fn parse_syskind(i: &[u8]) -> IResult<&[u8], SysKind, FormatError<&[u8]>> {
    const SYS_KIND_SIGNATURE: &[u8] = &[0x01, 0x00];
    let (i, sys_kind) = preceded(
        tuple((tag(SYS_KIND_SIGNATURE), tag(U32_FIXED_SIZE_4))),
        le_u32,
    )(i)?;
    match sys_kind {
        0x0000_0000 => Ok((i, SysKind::Win16)),
        0x0000_0001 => Ok((i, SysKind::Win32)),
        0x0000_0002 => Ok((i, SysKind::MacOs)),
        0x0000_0003 => Ok((i, SysKind::Win64)),
        _ => Err(Error(FormatError::UnexpectedValue)),
    }
}

fn parse_compat(i: &[u8]) -> IResult<&[u8], Option<u32>, FormatError<&[u8]>> {
    const COMPAT_SIGNATURE: &[u8] = &[0x4A, 0x00];
    let (i, compat) = opt(preceded(
        tuple((tag(COMPAT_SIGNATURE), tag(U32_FIXED_SIZE_4))),
        le_u32,
    ))(i)?;
    Ok((i, compat))
}

fn parse_lcid(i: &[u8]) -> IResult<&[u8], u32, FormatError<&[u8]>> {
    const LCID_SIGNATURE: &[u8] = &[0x02, 0x00];
    let (i, lcid) = preceded(tuple((tag(LCID_SIGNATURE), tag(U32_FIXED_SIZE_4))), le_u32)(i)?;
    Ok((i, lcid))
}

fn parse_lcid_invoke(i: &[u8]) -> IResult<&[u8], u32, FormatError<&[u8]>> {
    const LCID_INVOKE_SIGNATURE: &[u8] = &[0x14, 0x00];
    let (i, lcid_invoke) = preceded(
        tuple((tag(LCID_INVOKE_SIGNATURE), tag(U32_FIXED_SIZE_4))),
        le_u32,
    )(i)?;
    Ok((i, lcid_invoke))
}

fn parse_code_page(i: &[u8]) -> IResult<&[u8], u16, FormatError<&[u8]>> {
    const CODE_PAGE_SIGNATURE: &[u8] = &[0x03, 0x00];
    let (i, code_page) = preceded(
        tuple((tag(CODE_PAGE_SIGNATURE), tag(U32_FIXED_SIZE_2))),
        le_u16,
    )(i)?;
    Ok((i, code_page))
}

fn parse_name(i: &[u8]) -> IResult<&[u8], Vec<u8>, FormatError<&[u8]>> {
    const NAME_SIGNATURE: &[u8] = &[0x04, 0x00];
    let (i, name) = preceded(tag(NAME_SIGNATURE), length_data(le_u32))(i)?;
    Ok((i, name.to_vec()))
}

fn parse_doc_string(i: &[u8]) -> IResult<&[u8], Vec<u8>, FormatError<&[u8]>> {
    const DOC_STRING_SIGNATURE: &[u8] = &[0x05, 0x00];
    let (i, doc_string) = preceded(tag(DOC_STRING_SIGNATURE), length_data(le_u32))(i)?;
    Ok((i, doc_string.to_vec()))
}

fn parse_doc_string_unicode(i: &[u8]) -> IResult<&[u8], Vec<u8>, FormatError<&[u8]>> {
    const DOC_STRING_UNICODE_SIGNATURE: &[u8] = &[0x40, 0x00];
    let (i, doc_string_unicode) =
        preceded(tag(DOC_STRING_UNICODE_SIGNATURE), length_data(le_u32))(i)?;
    // `doc_string_unicode` represents a sequence of UTF-16 code units. If its length is uneven,
    // the input is malformed.
    if (doc_string_unicode.len() & 1_usize) != 0 {
        Err(Error(FormatError::UnexpectedValue))
    } else {
        Ok((i, doc_string_unicode.to_vec()))
    }
}

fn parse_help_file_1(i: &[u8]) -> IResult<&[u8], Vec<u8>, FormatError<&[u8]>> {
    const HELP_FILE_1_SIGNATURE: &[u8] = &[0x06, 0x00];
    let (i, help_file_1) = preceded(tag(HELP_FILE_1_SIGNATURE), length_data(le_u32))(i)?;
    Ok((i, help_file_1.to_vec()))
}

fn parse_help_file_2(i: &[u8]) -> IResult<&[u8], Vec<u8>, FormatError<&[u8]>> {
    const HELP_FILE_2_SIGNATURE: &[u8] = &[0x3d, 0x00];
    let (i, help_file_2) = preceded(tag(HELP_FILE_2_SIGNATURE), length_data(le_u32))(i)?;
    Ok((i, help_file_2.to_vec()))
}

fn parse_help_context(i: &[u8]) -> IResult<&[u8], u32, FormatError<&[u8]>> {
    const HELP_CONTEXT_SIGNATURE: &[u8] = &[0x07, 0x00];
    let (i, help_context) = preceded(
        tuple((tag(HELP_CONTEXT_SIGNATURE), tag(U32_FIXED_SIZE_4))),
        le_u32,
    )(i)?;
    Ok((i, help_context))
}

fn parse_lib_flags(i: &[u8]) -> IResult<&[u8], u32, FormatError<&[u8]>> {
    const LIB_FLAGS_SIGNATURE: &[u8] = &[0x08, 0x00];
    let (i, lib_flags) = preceded(
        tuple((tag(LIB_FLAGS_SIGNATURE), tag(U32_FIXED_SIZE_4))),
        le_u32,
    )(i)?;
    Ok((i, lib_flags))
}

fn parse_version(i: &[u8]) -> IResult<&[u8], (u32, u16), FormatError<&[u8]>> {
    const VERSION_SIGNATURE: &[u8] = &[0x09, 0x00];
    let (i, version) = preceded(
        tuple((tag(VERSION_SIGNATURE), tag(U32_FIXED_SIZE_4))),
        tuple((le_u32, le_u16)),
    )(i)?;
    Ok((i, version))
}

fn parse_constants(i: &[u8]) -> IResult<&[u8], Option<Vec<u8>>, FormatError<&[u8]>> {
    const CONSTANTS_SIGNATURE: &[u8] = &[0x0c, 0x00];
    let (i, constants) = opt(preceded(tag(CONSTANTS_SIGNATURE), length_data(le_u32)))(i)?;
    let constants = constants.map(Vec::from);
    Ok((i, constants))
}

fn parse_constants_unicode(i: &[u8]) -> IResult<&[u8], Option<Vec<u8>>, FormatError<&[u8]>> {
    const CONSTANTS_UNICODE_SIGNATURE: &[u8] = &[0x3c, 0x00];
    let (i, constants_unicode) = opt(preceded(
        tag(CONSTANTS_UNICODE_SIGNATURE),
        length_data(le_u32),
    ))(i)?;
    let constants_unicode = constants_unicode.map(|slice| slice.to_vec());
    Ok((i, constants_unicode))
}

// -------------------------------------------------------------------------
// -------------------------------------------------------------------------

#[allow(clippy::type_complexity)]
fn parse_reference_name(
    i: &[u8],
    code_page: u16,
) -> IResult<&[u8], Option<String>, FormatError<&[u8]>> {
    const NAME_SIGNATURE: &[u8] = &[0x16, 0x00];
    const NAME_UNICODE_SIGNATURE: &[u8] = &[0x3e, 0x00];
    let (i, name) = opt(tuple((
        preceded(tag(NAME_SIGNATURE), length_data(le_u32)),
        preceded(tag(NAME_UNICODE_SIGNATURE), length_data(le_u32)),
    )))(i)?;
    // name_unicode MUST contain the UTF-16 encoding of name. Can be dropped without
    // loss of information.
    if let Some((name, _name_unicode)) = name {
        let name =
            cp_to_string(name, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;
        Ok((i, Some(name)))
    } else {
        Ok((i, None))
    }
}

fn parse_reference_original(
    i: &[u8],
    code_page: u16,
) -> IResult<&[u8], String, FormatError<&[u8]>> {
    const ORIGINAL_SIGNATURE: &[u8] = &[0x33, 0x00];
    let (i, libid_original) = preceded(tag(ORIGINAL_SIGNATURE), length_data(le_u32))(i)?;
    let libid_original =
        cp_to_string(libid_original, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;
    Ok((i, libid_original))
}

fn parse_reference_control(
    i: &[u8],
    code_page: u16,
) -> IResult<&[u8], ReferenceControl, FormatError<&[u8]>> {
    // REFERENCEORIGINAL Record is optional here
    let (_, id) = le_u16(i)?;
    let (i, libid_original) = match id {
        0x0033_u16 => {
            let (i, libid_original) = parse_reference_original(i, code_page)?;
            (i, Some(libid_original))
        }
        _ => (i, None),
    };

    const CONTROL_SIGNATURE: &[u8] = &[0x2f, 0x00];
    let (i, libid_twiddled) =
        preceded(tuple((tag(CONTROL_SIGNATURE), le_u32)), length_data(le_u32))(i)?;
    let libid_twiddled =
        cp_to_string(libid_twiddled, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;

    const RESERVED_1: &[u8] = &[0x00, 0x00, 0x00, 0x00];
    const RESERVED_2: &[u8] = &[0x00, 0x00];
    let (i, _) = tuple((tag(RESERVED_1), tag(RESERVED_2)))(i)?;

    let (i, name_extended) = parse_reference_name(i, code_page)?;

    const RESERVED_3: &[u8] = &[0x30, 0x00];
    let (i, libid_extended) = preceded(tuple((tag(RESERVED_3), le_u32)), length_data(le_u32))(i)?;
    let libid_extended =
        cp_to_string(libid_extended, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;

    const RESERVED_4: &[u8] = &[0x00, 0x00, 0x00, 0x00];
    const RESERVED_5: &[u8] = &[0x00, 0x00];
    let (i, _) = tuple((tag(RESERVED_4), tag(RESERVED_5)))(i)?;

    let (i, guid) = take(16_usize)(i)?;
    let guid = guid.to_vec();

    let (i, cookie) = le_u32(i)?;

    Ok((
        i,
        ReferenceControl {
            name: None,
            libid_original,
            libid_twiddled,
            name_extended,
            libid_extended,
            guid,
            cookie,
        },
    ))
}

fn parse_reference_registered(
    i: &[u8],
    code_page: u16,
) -> IResult<&[u8], ReferenceRegistered, FormatError<&[u8]>> {
    const REGISTERED_SIGNATURE: &[u8] = &[0x0d, 0x00];
    let (i, libid) = preceded(
        tuple((tag(REGISTERED_SIGNATURE), le_u32)),
        length_data(le_u32),
    )(i)?;
    let libid = cp_to_string(libid, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;

    const RESERVED_1: &[u8] = &[0x00, 0x00, 0x00, 0x00];
    const RESERVED_2: &[u8] = &[0x00, 0x00];
    let (i, _) = tuple((tag(RESERVED_1), tag(RESERVED_2)))(i)?;

    Ok((i, ReferenceRegistered { name: None, libid }))
}

fn parse_reference_project(
    i: &[u8],
    code_page: u16,
) -> IResult<&[u8], ReferenceProject, FormatError<&[u8]>> {
    let (i, (libid_absolute, libid_relative, major_version, minor_version)) = tuple((
        preceded(tuple((tag(&[0x0e, 0x00]), le_u32)), length_data(le_u32)),
        length_data(le_u32),
        le_u32,
        le_u16,
    ))(i)?;
    let libid_absolute =
        cp_to_string(libid_absolute, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;
    let libid_relative =
        cp_to_string(libid_relative, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;

    Ok((
        i,
        ReferenceProject {
            name: None,
            libid_absolute,
            libid_relative,
            major_version,
            minor_version,
        },
    ))
}

/// Parses a single REFERENCE Record.
///
/// There are several tricky bits to this:
/// * The first entry (NameRecord) is optional.
/// * The REFERENCE Record can be one of 4 variants.
/// * The length is implied through a terminator (0x000F) that starts a PROJECTMODULES Record.
///
/// Returns `Some(reference)` if a variant was found, `None` if the end of the array was
/// reached, or an error.
fn parse_reference(
    i: &[u8],
    code_page: u16,
) -> IResult<&[u8], Option<Reference>, FormatError<&[u8]>> {
    let (i, name) = parse_reference_name(i, code_page)?;
    // Determine REFERENCE Record variant (or end of array)
    let (_, id) = le_u16(i)?;
    match id {
        0x002f_u16 => {
            let (i, mut value) = parse_reference_control(i, code_page)?;
            value.name = name;
            Ok((i, Some(Reference::Control(value))))
        }
        0x0033_u16 => {
            let (i, libid_original) = parse_reference_original(i, code_page)?;
            let original = ReferenceOriginal {
                name,
                libid_original,
            };
            Ok((i, Some(Reference::Original(original))))
        }
        0x000d_u16 => {
            let (i, mut value) = parse_reference_registered(i, code_page)?;
            value.name = name;
            Ok((i, Some(Reference::Registered(value))))
        }
        0x000e_u16 => {
            let (i, mut value) = parse_reference_project(i, code_page)?;
            value.name = name;
            Ok((i, Some(Reference::Project(value))))
        }
        0x000f_u16 => Ok((i, None)),
        _ => Err(Error(FormatError::UnexpectedValue)),
    }
}

fn parse_references(
    i: &[u8],
    code_page: u16,
) -> IResult<&[u8], Vec<Reference>, FormatError<&[u8]>> {
    let mut result = Vec::new();
    let mut i = i;
    loop {
        let (remainder, value) = parse_reference(i, code_page)?;
        i = remainder;
        if let Some(reference) = value {
            result.push(reference);
        } else {
            return Ok((i, result));
        }
    }
}

// -------------------------------------------------------------------------
// -------------------------------------------------------------------------

fn parse_module(i: &[u8], code_page: u16) -> IResult<&[u8], Module, FormatError<&[u8]>> {
    // MODULENAME Record
    let (i, name) = preceded(tag(&[0x19, 0x00]), length_data(le_u32))(i)?;
    let name = cp_to_string(name, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;

    // (Optional) MODULENAMEUNICODE Record
    // If present it MUST be the UTF-16 encoding of MODULENAME. It can safely be dropped.
    let (i, _name_unicode) = opt(preceded(tag(&[0x47, 0x00]), length_data(le_u32)))(i)?;

    // MODULESTREAMNAME Record
    // stream_name_unicode MUST be the UTF-16 encoding of stream_name. It can safely be dropped.
    let (i, (stream_name, _stream_name_unicode)) = tuple((
        preceded(tag(&[0x1a, 0x00]), length_data(le_u32)),
        preceded(tag(&[0x32, 0x00]), length_data(le_u32)),
    ))(i)?;
    let stream_name =
        cp_to_string(stream_name, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;

    // MODULEDOCSTRING Record
    // doc_string_unicode MUST be the UTF-16 encoding of doc_string. It can safely be dropped.
    let (i, (doc_string, _doc_string_unicode)) = tuple((
        preceded(tag(&[0x1c, 0x00]), length_data(le_u32)),
        preceded(tag(&[0x48, 0x00]), length_data(le_u32)),
    ))(i)?;
    let doc_string =
        cp_to_string(doc_string, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;

    // MODULEOFFSET Record
    let (i, text_offset) = preceded(tuple((tag(&[0x31, 0x00]), tag(U32_FIXED_SIZE_4))), le_u32)(i)?;
    let text_offset = text_offset as _;

    // MODULEHELPCONTEXT Record
    let (i, help_context) =
        preceded(tuple((tag(&[0x1e, 0x00]), tag(U32_FIXED_SIZE_4))), le_u32)(i)?;

    // MODULECOOKIE Record
    // Cookie MUST be ignored on read.
    let (i, _cookie) = preceded(tuple((tag(&[0x2c, 0x00]), tag(U32_FIXED_SIZE_2))), le_u16)(i)?;

    // MODULETYPE Record
    let (i, id) = le_u16(i)?;
    let module_type = match id {
        0x0021_u16 => ModuleType::Procedural,
        0x0022_u16 => ModuleType::DocClsDesigner,
        _ => return Err(Error(FormatError::UnexpectedValue)),
    };
    let (i, _) = tag(&[0x00, 0x00, 0x00, 0x00])(i)?;

    // MODULEREADONLY Record
    let (i, read_only) = opt(tag(&[0x25, 0x00, 0x00, 0x00, 0x00, 0x00]))(i)?;
    let read_only = read_only.is_some();

    // MODULEPRIVATE Record
    let (i, private) = opt(tag(&[0x28, 0x00, 0x00, 0x00, 0x00, 0x00]))(i)?;
    let private = private.is_some();

    // Terminator
    let (i, _) = tag(&[0x2b, 0x00])(i)?;

    // Reserved
    let (i, _) = tag(&[0x00, 0x00, 0x00, 0x00])(i)?;

    Ok((
        i,
        Module {
            name,
            stream_name,
            doc_string,
            text_offset,
            help_context,
            module_type,
            read_only,
            private,
        },
    ))
}

fn parse_modules(i: &[u8], code_page: u16) -> IResult<&[u8], Vec<Module>, FormatError<&[u8]>> {
    let (i, count) = preceded(tuple((tag(&[0x0f, 0x00]), tag(U32_FIXED_SIZE_2))), le_u16)(i)?;
    // Cookie MUST be ignored on read.
    let (i, _cookie) = preceded(tuple((tag(&[0x13, 0x00]), tag(U32_FIXED_SIZE_2))), le_u16)(i)?;

    let mut modules = Vec::new();
    let mut i = i;
    for _ in 0..count {
        let (remainder, module) = parse_module(i, code_page)?;
        i = remainder;
        modules.push(module);
    }

    Ok((i, modules))
}

// -------------------------------------------------------------------------
// -------------------------------------------------------------------------

/// *dir* stream parser.
pub(crate) fn parse_project_information(
    i: &[u8],
) -> IResult<&[u8], ProjectInformation, FormatError<&[u8]>> {
    let (i, sys_kind) = parse_syskind(i)?;
    let (i, compat) = parse_compat(i)?;
    let (i, lcid) = parse_lcid(i)?;
    let (i, lcid_invoke) = parse_lcid_invoke(i)?;
    let (i, code_page) = parse_code_page(i)?;

    let (i, name) = parse_name(i)?;
    let name = cp_to_string(&name, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;

    let (i, doc_string) = parse_doc_string(i)?;
    let doc_string =
        cp_to_string(&doc_string, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;

    // doc_string_unicode MUST contain the UTF-16 encoding of doc_string. Can safely be dropped.
    let (i, _doc_string_unicode) = parse_doc_string_unicode(i)?;

    let (i, help_file_1) = parse_help_file_1(i)?;
    let help_file_1 =
        cp_to_string(&help_file_1, code_page).map_err(|_| Error(FormatError::UnexpectedValue))?;

    // help_file_2 MUST contain the same bytes as help_file_1. Can safely be dropped.
    let (i, _help_file_2) = parse_help_file_2(i)?;

    let (i, help_context) = parse_help_context(i)?;
    let (i, lib_flags) = parse_lib_flags(i)?;
    let (i, (version_major, version_minor)) = parse_version(i)?;

    // The `PROJECTCONSTANTS` record is optional (as a whole); make sure to only parse the
    // Unicode portion if `parse_constants` returned `Some`.
    //
    // TODO: Consider consolidating CP and Unicode parsing into a single function. This
    // would avoid having to subsequently deal with the outcome of this function.
    let (i, constants) = parse_constants(i)?;
    let constants = constants
        .map(|constants| cp_to_string(&constants, code_page))
        .transpose()
        .map_err(|_| Error(FormatError::UnexpectedValue))?;

    let i = if constants.is_some() {
        // constants_unicode MUST contain the UTF-16 encoding of constants. Can safely be
        // dropped.
        let (i, _constants_unicode) = parse_constants_unicode(i)?;
        i
    } else {
        i
    };

    let (i, references) = parse_references(i, code_page)?;

    let (i, modules) = parse_modules(i, code_page)?;

    // Terminator
    let (i, _) = tag(&[0x10, 0x00])(i)?;

    // Reserved
    let (i, _) = tag(&[0x00, 0x00, 0x00, 0x00])(i)?;

    Ok((
        i,
        ProjectInformation {
            information: Information {
                sys_kind,
                compat,
                lcid,
                lcid_invoke,
                code_page,
                name,
                doc_string,
                help_file_1,
                help_context,
                lib_flags,
                version_major,
                version_minor,
                constants,
            },
            references,
            modules,
        },
    ))
}

// -------------------------------------------------------------------------
// -------------------------------------------------------------------------

fn cp_to_string(data: &[u8], code_page: u16) -> Result<String, ()> {
    to_encoding(code_page)
        .ok_or(())?
        .decode_without_bom_handling_and_without_replacement(data)
        .map(|text| text.into_owned())
        .ok_or(())
}

pub(super) fn project_code_page(input: &[u8]) -> Result<u16, ()> {
    fn prefix(i: &[u8]) -> IResult<&[u8], u16, FormatError<&[u8]>> {
        let (i, _) = parse_syskind(i)?;
        let (i, _) = parse_compat(i)?;
        let (i, _) = parse_lcid(i)?;
        let (i, _) = parse_lcid_invoke(i)?;
        parse_code_page(i)
    }
    prefix(input).map(|(_, cp)| cp).map_err(|_| ())
}

pub(super) fn read(input: &[u8]) -> Result<ProjectInformation, ()> {
    nom::combinator::all_consuming(parse_project_information)(input)
        .map(|(_, project)| project)
        .map_err(|_| ())
}
