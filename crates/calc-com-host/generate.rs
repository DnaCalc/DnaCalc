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

pub fn write() {
    for file in ["excel.json", "office.json", "addin-designer.json", "accessibility.json"] {
        println!("cargo:rerun-if-changed=../calc-object-model/descriptor/{file}");
    }
    println!("cargo:rerun-if-changed=../calc-object-model/src/descriptor.rs");
    println!("cargo:rerun-if-changed=generate.rs");
    let descriptor = Descriptor::get();
    let mut code = String::from("// Generated from calc-object-model/descriptor. Do not edit.\n");
    let mut shapes = BTreeSet::new();
    let mut layouts = Vec::new();
    for (id, ty) in descriptor.types() {
        let Some(slots) = ty.vtable_slots else { continue };
        let members: Option<Vec<_>> = (7..slots).map(|slot| descriptor.member_by_slot(id, slot)).collect();
        // Rule 4: a missing inherited layout is never replaced by an ABI-incorrect stub.
        let Some(members) = members else { continue };
        let mut entries = vec!["query".to_string(), "add_ref".into(), "release".into(),
            "type_count".into(), "type_info".into(), "names".into(), "invoke".into()];
        for member in members {
            assert_eq!(member.ret, "HRESULT", "unsupported ABI return in {}", ty.name);
            let shape: Vec<_> = member.params.iter().map(|param| class(&param.type_ref())).collect();
            let key = shape.join("_");
            shapes.insert(shape);
            entries.push(format!("thunk_{key}::<{}, {}>", id.0, member.slot.unwrap()));
        }
        writeln!(code, "static VTABLE_{}: RawVtable<{}> = RawVtable([{}]);", id.0, slots,
            entries.iter().map(|e| format!("{e} as *const () as *const c_void")).collect::<Vec<_>>().join(",")).unwrap();
        layouts.push((id, slots));
    }
    for shape in shapes {
        let key = shape.join("_");
        let params = shape.iter().enumerate().map(|(n, ty)| format!("a{n}: {ty}")).collect::<Vec<_>>().join(",");
        let args = shape.iter().enumerate().map(|(n, ty)| {
            if *ty == "usize" { format!("a{n}") } else { format!("a{n}.to_bits() as usize") }
        }).collect::<Vec<_>>().join(",");
        writeln!(code, "unsafe extern \"system\" fn thunk_{key}<const I: u32, const S: u16>(this: *mut Wrapper{comma}{params}) -> HRESULT {{ abi(|| unsafe {{ decode_slot(this, InterfaceId(I), S, &[{args}]) }}) }}", comma=if params.is_empty(){""}else{","}).unwrap();
    }
    code.push_str("fn generated_vtable(id: InterfaceId) -> Option<&'static [*const c_void]> { match id.0 {\n");
    for (InterfaceId(id), _) in &layouts {
        writeln!(code, "{id} => Some(&VTABLE_{id}.0),").unwrap();
    }
    code.push_str("_ => None } }\n");
    std::fs::write(PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("vtables.rs"), code).unwrap();
    let manifest = layouts.iter().map(|(id, slots)| format!("({}, {}),", id.0, slots)).collect::<String>();
    std::fs::write(PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("layouts.rs"),
        format!("pub const GENERATED_LAYOUTS: &[(u32, u16)] = &[{manifest}];\n")).unwrap();
}
