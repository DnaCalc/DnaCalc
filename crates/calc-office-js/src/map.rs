//! Office.js camelCase members to descriptor members (docs/architecture/OFFICE_JS_MAP.md).
//! The client can invoke only the members mapped here.

use calc_object_model::{AutoValue, InvokeKind};

/// A COM member an Office.js name denotes, with the arguments Office.js semantics imply and the
/// shift from COM's 1-based numbers to Office.js's 0-based ones.
pub struct Mapped {
    pub com: &'static str,
    pub preset: fn() -> Vec<AutoValue>,
    pub offset: i32,
}

fn none() -> Vec<AutoValue> {
    Vec::new()
}
/// `Range.address`: sheet-qualified, relative, A1 style.
fn address() -> Vec<AutoValue> {
    vec![
        AutoValue::Bool(false),
        AutoValue::Bool(false),
        AutoValue::I4(1),
    ]
}

const fn m(com: &'static str) -> Mapped {
    Mapped {
        com,
        preset: none,
        offset: 0,
    }
}

pub fn lookup(office: &str) -> Option<Mapped> {
    Some(match office {
        "worksheets" => m("Worksheets"),
        "activeWorksheet" => m("ActiveSheet"),
        "getItem" => m("Item"),
        "getRange" => m("Range"),
        "getUsedRange" | "usedRange" => m("UsedRange"),
        "values" => m("Value2"),
        "formulas" => m("Formula"),
        "numberFormat" => m("NumberFormat"),
        "text" => m("Text"),
        "address" => Mapped {
            com: "Address",
            preset: address,
            offset: 0,
        },
        "rowIndex" => Mapped {
            com: "Row",
            preset: none,
            offset: -1,
        },
        "columnIndex" => Mapped {
            com: "Column",
            preset: none,
            offset: -1,
        },
        "calculate" => m("Calculate"),
        "clear" => m("Clear"),
        "activate" => m("Activate"),
        "delete" => m("Delete"),
        "add" => m("Add"),
        "name" => m("Name"),
        "count" => m("Count"),
        _ => return None,
    })
}

/// The kinds to try, in order, for an operation on a COM member (COM collections expose `Item` as
/// a parameterised property, so a method call may be a Get).
pub fn kinds(operation: Operation, name: &str) -> &'static [InvokeKind] {
    match operation {
        Operation::Get => &[InvokeKind::Get],
        Operation::Call if matches!(name, "Item" | "Range" | "UsedRange") => &[InvokeKind::Get],
        Operation::Call => &[InvokeKind::Method],
        Operation::Set => &[InvokeKind::Put],
    }
}

#[derive(Clone, Copy)]
pub enum Operation {
    Get,
    Call,
    Set,
}
