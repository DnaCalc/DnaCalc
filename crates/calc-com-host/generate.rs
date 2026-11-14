//! Generate complete x64 interface layouts; ABI facts come only from the shared descriptor.
use crate::descriptor::{Descriptor, InterfaceId, TypeRef};
use std::{collections::BTreeSet, fmt::Write, path::PathBuf};

fn class(ty: &TypeRef) -> &'static str {
    match ty.underlying() {
        TypeRef::Base(base) if base == "R4" => "f32",
        TypeRef::Base(base) if base == "R8" || base == "DATE" => "f64",
        _ => "usize",
    }
}

/// Exact native types for foreign interfaces and generated verification callers. Unlike the
/// thunk register classes, these retain integer widths and every pointer level.
fn native_type(ty: &TypeRef) -> String {
    match ty.underlying() {
        TypeRef::Base(base) => match base.as_str() {
            "HRESULT" => "HRESULT",
            "I4" | "INT" | "ERROR" => "i32",
            "UI4" | "UINT" => "u32",
            "I2" | "BOOL" => "i16",
            "R4" => "f32",
            "R8" | "DATE" => "f64",
            "CY" => "i64",
            "BSTR" => "*const u16",
            "VARIANT" => "VARIANT",
            other => panic!("unsupported native ABI type {other}"), // ubs:ignore - This build-script error must stop generation rather than emit an incorrect COM signature.
        }
        .into(),
        TypeRef::Named(_) => "c_void".into(),
        TypeRef::Pointer(inner) => format!("*mut {}", native_type(&inner)),
        TypeRef::SafeArray(_) => "*mut SAFEARRAY".into(),
        other => panic!("unsupported native ABI type {other:?}"), // ubs:ignore - Build-script-only rejection of a descriptor type without a proven native ABI mapping.
    }
}

fn verification_callers(descriptor: &Descriptor, code: &mut String) {
    let application = descriptor.id("_Application").unwrap();
    for (name, caller) in [
        ("Version", "call_version"),
        ("Hwnd", "call_hwnd"),
        ("ThisCell", "call_this_cell"),
    ] {
        let member = descriptor
            .member(application, name, crate::descriptor::InvokeKind::Get)
            .unwrap();
        let types: Vec<_> = member
            .params
            .iter()
            .map(|p| native_type(&p.type_ref()))
            .collect();
        let parameters = types
            .iter()
            .enumerate()
            .map(|(i, ty)| format!("a{i}: {ty}"))
            .collect::<Vec<_>>()
            .join(",");
        let arguments = (0..types.len())
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        if name == "ThisCell" {
            code.push_str("#[cfg(test)]\n");
        }
        writeln!(code, "pub(crate) unsafe fn {caller}(this: *mut c_void, {parameters}) -> HRESULT {{\n\
            let table = unsafe {{ *(this as *const *const *const c_void) }};\n\
            let function: unsafe extern \"system\" fn(*mut c_void, {signature}) -> HRESULT = unsafe {{ std::mem::transmute(*table.add({slot})) }};\n\
            unsafe {{ function(this, {arguments}) }}\n}}", signature=types.join(","), slot=member.slot.unwrap()).unwrap();
    }
}

fn foreign_checks(descriptor: &Descriptor, out_dir: &std::path::Path) {
    let mut checks = String::from("fn check_foreign_parameter_types() {\n");
    let mut coverage = Vec::new();
    for (name, rust) in [
        ("_IDTExtensibility2", "IDTExtensibility2"),
        ("IRibbonExtensibility", "IRibbonExtensibility"),
        ("IRtdServer", "IRtdServer"),
        ("IRTDUpdateEvent", "IRTDUpdateEvent"),
    ] {
        let id = descriptor.id(name).unwrap();
        writeln!(checks, "let _ = |table: &{rust}_Vtbl| {{").unwrap();
        for member in descriptor.members(id) {
            let field = match member.invoke {
                crate::descriptor::InvokeKind::Put | crate::descriptor::InvokeKind::Putref => {
                    format!("Set{}", member.name)
                }
                _ => member.name.clone(),
            };
            let parameters = member
                .params
                .iter()
                .map(|p| native_type(&p.type_ref()))
                .collect::<Vec<_>>()
                .join(",");
            writeln!(checks, "let _: unsafe extern \"system\" fn(*mut c_void{comma}{parameters}) -> {ret} = table.{field};",
                comma=if parameters.is_empty() { "" } else { "," }, ret=native_type(&TypeRef::parse(&member.ret))).unwrap();
            coverage.push(format!(
                "({name:?}, {}, {}),",
                member.slot.unwrap(),
                member.params.len()
            ));
        }
        checks.push_str("};\n");
    }
    checks.push_str("}\n");
    std::fs::write(out_dir.join("foreign_checks.rs"), checks).unwrap();
    std::fs::write(
        out_dir.join("foreign_coverage.rs"),
        format!(
            "const FOREIGN_PARAMETERS: &[(&str, u16, usize)] = &[{}];",
            coverage.join("")
        ),
    )
    .unwrap();
}

pub fn write() {
    for file in [
        "excel.json",
        "office.json",
        "addin-designer.json",
        "accessibility.json",
    ] {
        println!("cargo:rerun-if-changed=../calc-object-model/descriptor/{file}");
    }
    println!("cargo:rerun-if-changed=../calc-object-model/src/descriptor.rs");
    println!("cargo:rerun-if-changed=generate.rs");
    let descriptor = Descriptor::get();
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let mut code = String::from("// Generated from calc-object-model/descriptor. Do not edit.\n");
    let mut shapes = BTreeSet::new();
    let mut layouts = Vec::new();
    for (id, ty) in descriptor.types() {
        let Some(slots) = ty.vtable_slots else {
            continue;
        };
        let members: Option<Vec<_>> = (7..slots)
            .map(|slot| descriptor.member_by_slot(id, slot))
            .collect();
        // Rule 4: a missing inherited layout is never replaced by an ABI-incorrect stub.
        let Some(members) = members else { continue };
        let mut entries = vec![
            "query".to_string(),
            "add_ref".into(),
            "release".into(),
            "type_count".into(),
            "type_info".into(),
            "names".into(),
            "invoke".into(),
        ];
        for member in members {
            // A hidden `void _DummyNN()` slot (Excel's _Chart has one at slot 35) takes no
            // arguments, so a thunk that returns an HRESULT is ABI-compatible: the caller never
            // reads the return register of a void function.
            assert!(
                member.ret == "HRESULT" || (member.ret == "VOID" && member.params.is_empty()),
                "unsupported ABI return {} in {}",
                member.ret,
                ty.name
            );
            let shape: Vec<_> = member
                .params
                .iter()
                .map(|param| class(&param.type_ref()))
                .collect();
            let key = shape.join("_");
            shapes.insert(shape);
            entries.push(format!("thunk_{key}::<{}, {}>", id.0, member.slot.unwrap()));
        }
        writeln!(
            code,
            "static VTABLE_{}: RawVtable<{}> = RawVtable([{}]);",
            id.0,
            slots,
            entries
                .iter()
                .map(|e| format!("{e} as *const () as *const c_void"))
                .collect::<Vec<_>>()
                .join(",")
        )
        .unwrap();
        layouts.push((id, slots));
    }
    for shape in shapes {
        let key = shape.join("_");
        let params = shape
            .iter()
            .enumerate()
            .map(|(n, ty)| format!("a{n}: {ty}"))
            .collect::<Vec<_>>()
            .join(",");
        let args = shape
            .iter()
            .enumerate()
            .map(|(n, ty)| {
                if *ty == "usize" {
                    format!("a{n}")
                } else {
                    format!("a{n}.to_bits() as usize")
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        writeln!(code, "unsafe extern \"system\" fn thunk_{key}<const I: u32, const S: u16>(this: *mut Wrapper{comma}{params}) -> HRESULT {{ abi(|| unsafe {{ decode_slot(this, InterfaceId(I), S, &[{args}]) }}) }}", comma=if params.is_empty(){""}else{","}).unwrap();
    }
    code.push_str(
        "fn generated_vtable(id: InterfaceId) -> Option<&'static [*const c_void]> { match id.0 {\n",
    );
    for (InterfaceId(id), _) in &layouts {
        writeln!(code, "{id} => Some(&VTABLE_{id}.0),").unwrap();
    }
    code.push_str("_ => None } }\n");
    verification_callers(descriptor, &mut code);
    foreign_checks(descriptor, &out_dir);
    // The public run_early control must drive the generated slot with the caller's real
    // by-value VARIANT ABI. Derive its argument count and slot from that same descriptor.
    let application = descriptor.id("_Application").unwrap();
    let run = descriptor
        .member(application, "Run", crate::descriptor::InvokeKind::Method)
        .unwrap();
    assert!(run.arguments().all(|p| p.ty == "VARIANT"));
    assert_eq!(run.retval().unwrap().ty, "VARIANT*");
    let count = run.arguments().count();
    let signature = std::iter::repeat_n("std::mem::ManuallyDrop<VARIANT>", count)
        .collect::<Vec<_>>()
        .join(",");
    let args = (0..count)
        .map(|i| {
            format!(
                "std::mem::ManuallyDrop::new(std::ptr::read({}))",
                if i == 0 { "macro_value" } else { "missing" }
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    writeln!(code, "pub(crate) unsafe fn call_run(this: *mut c_void, macro_value: &VARIANT, missing: &VARIANT, output: *mut VARIANT) -> HRESULT {{\nlet table = unsafe {{ *(this as *const *const *const c_void) }};\nlet function: unsafe extern \"system\" fn(*mut c_void, {signature}, *mut VARIANT) -> HRESULT = unsafe {{ std::mem::transmute(*table.add({slot})) }};\nunsafe {{ function(this, {args}, output) }}\n}}", slot=run.slot.unwrap()).unwrap();
    std::fs::write(
        PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("vtables.rs"),
        code,
    )
    .unwrap();
    let manifest = layouts
        .iter()
        .map(|(id, slots)| format!("({}, {}),", id.0, slots))
        .collect::<String>();
    std::fs::write(
        PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("layouts.rs"),
        format!("pub const GENERATED_LAYOUTS: &[(u32, u16)] = &[{manifest}];\n"),
    )
    .unwrap();
}
