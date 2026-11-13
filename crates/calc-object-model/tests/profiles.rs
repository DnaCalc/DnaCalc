use calc_object_model::geometry::{self, AddressOptions, RangeAxis, ReferenceStyle};
use calc_object_model::profile::{ApplicationHost, values_only::ValuesOnly};
use calc_object_model::{
    AutoError, AutoValue, Call, Descriptor, InvokeKind, ObjectModel, ObjectRef,
};
use calc_value::{CellKey, GridArea, IdAllocator, ReferenceLike, SheetId};
use std::cell::{Cell, RefCell};

fn references() -> (ReferenceLike, ReferenceLike) {
    let sheet = IdAllocator::<SheetId>::new().issue().unwrap();
    let cell = |r, c| CellKey::new(sheet, r, c).unwrap();
    (
        ReferenceLike::area(GridArea::new(cell(2, 2), cell(3, 3)).unwrap()),
        ReferenceLike::cell(cell(8, 6)),
    )
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn address_flags_multi_area_external_and_absolute_r1c1() {
    let (first, second) = references();
    let range = geometry::union(&[first, second]).unwrap();
    for (row_absolute, column_absolute, expected) in [
        (true, true, "$B$2:$C$3,$F$8"),
        (false, true, "$B2:$C3,$F8"),
        (true, false, "B$2:C$3,F$8"),
        (false, false, "B2:C3,F8"),
    ] {
        assert_eq!(
            geometry::address(
                &range,
                AddressOptions {
                    row_absolute,
                    column_absolute,
                    ..Default::default()
                },
                None
            )
            .unwrap(),
            expected
        );
    }
    assert_eq!(
        geometry::address(
            &range,
            AddressOptions {
                style: ReferenceStyle::R1C1,
                ..Default::default()
            },
            None
        )
        .unwrap(),
        "R2C2:R3C3,R8C6"
    );
    assert_eq!(
        geometry::address(
            &range,
            AddressOptions {
                external: true,
                ..Default::default()
            },
            Some("'[Named.xlsx]Q2 Data'")
        )
        .unwrap(),
        "'[Named.xlsx]Q2 Data'!$B$2:$C$3,$F$8"
    );
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn unmeasured_address_forms_refuse_with_probe_owner() {
    let (range, _) = references();
    for options in [
        AddressOptions {
            style: ReferenceStyle::R1C1,
            row_absolute: false,
            ..Default::default()
        },
        AddressOptions {
            relative_to: Some(range.areas()[0].first()),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            geometry::address(&range, options, None),
            Err(AutoError::NotModelled {
                owner: "dc-1nbp.6",
                ..
            })
        ));
    }
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn union_preserves_order_multiplicity_and_first_area_views() {
    let (first, second) = references();
    let range = geometry::union(&[second.clone(), first.clone(), first.clone()]).unwrap();
    assert_eq!(
        range.areas(),
        &[second.areas()[0], first.areas()[0], first.areas()[0]]
    );
    assert_eq!(geometry::count(&range, RangeAxis::Cells), 9);
    assert_eq!(geometry::count(&range, RangeAxis::Rows), 1);
    assert_eq!(geometry::count(&range, RangeAxis::Columns), 1);
    assert_eq!((geometry::row(&range), geometry::column(&range)), (8, 6));
    let reversed = geometry::union(&[first, second]).unwrap();
    assert_eq!(geometry::count(&reversed, RangeAxis::Rows), 2);
    assert_eq!(geometry::count(&reversed, RangeAxis::Columns), 2);
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn cross_sheet_union_is_excel_1004_and_large_count_is_double() {
    let mut ids = IdAllocator::<SheetId>::new();
    let references = (0..2)
        .map(|_| ReferenceLike::cell(CellKey::new(ids.issue().unwrap(), 1, 1).unwrap()))
        .collect::<Vec<_>>();
    assert!(matches!(
        geometry::union(&references),
        Err(AutoError::Excel { number: 1004, .. })
    ));
    let first = references[0].areas()[0].first();
    let whole = ReferenceLike::area(
        GridArea::new(
            first,
            CellKey::new(first.sheet(), 1_048_576, 16_384).unwrap(),
        )
        .unwrap(),
    );
    assert!(
        matches!(geometry::count_value(geometry::count(&whole, RangeAxis::Cells)), AutoValue::R8(n) if n == 17_179_869_184.)
    );
}

#[derive(Default)]
struct Host {
    enabled: Cell<bool>,
    calls: RefCell<Vec<(String, usize)>>,
}
impl ApplicationHost for Host {
    fn hwnd(&self) -> isize {
        123
    }
    fn run(&self, name: &str, args: &[AutoValue]) -> Result<AutoValue, AutoError> {
        self.calls.borrow_mut().push((name.into(), args.len()));
        Ok(AutoValue::I4(42))
    }
    fn events_enabled(&self) -> bool {
        self.enabled.get()
    }
    fn set_events_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }
    fn path(&self) -> Result<String, AutoError> {
        Ok(r"C:\DNA Calc".into())
    }
}
fn invoke(
    model: &impl ObjectModel,
    target: &ObjectRef,
    name: &str,
    kind: InvokeKind,
    args: Vec<AutoValue>,
) -> Result<AutoValue, AutoError> {
    let member = Descriptor::get()
        .member(target.interface, name, kind)
        .unwrap();
    model.invoke(
        target,
        &Call {
            interface: target.interface,
            member,
            args,
            lcid: 0,
        },
    )
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn values_only_application_uses_host_and_descriptor_handlers() {
    let model = ValuesOnly::new(Host::default());
    let app = model.application();
    for (name, expected) in [
        ("Name", "Microsoft Excel"),
        ("Version", "16.0"),
        ("Path", r"C:\DNA Calc"),
    ] {
        assert_eq!(
            invoke(&model, &app, name, InvokeKind::Get, vec![])
                .unwrap()
                .as_text()
                .as_deref(),
            Some(expected)
        );
    }
    assert!(matches!(
        invoke(&model, &app, "Hwnd", InvokeKind::Get, vec![]).unwrap(),
        AutoValue::I4(123)
    ));
    for name in ["Application", "Parent"] {
        assert!(
            matches!(invoke(&model, &app, name, InvokeKind::Get, vec![]).unwrap(), AutoValue::Object(o) if o == app)
        );
    }
    invoke(
        &model,
        &app,
        "EnableEvents",
        InvokeKind::Put,
        vec![AutoValue::Bool(true)],
    )
    .unwrap();
    assert!(model.host().enabled.get());
    assert!(matches!(
        invoke(&model, &app, "EnableEvents", InvokeKind::Get, vec![]).unwrap(),
        AutoValue::Bool(true)
    ));
    assert!(matches!(
        invoke(
            &model,
            &app,
            "Run",
            InvokeKind::Method,
            vec![
                AutoValue::text("Command"),
                AutoValue::I4(7),
                AutoValue::Missing
            ]
        )
        .unwrap(),
        AutoValue::I4(42)
    ));
    assert_eq!(&*model.host().calls.borrow(), &[("Command".into(), 1)]);
    assert!(
        model
            .handlers()
            .implemented()
            .iter()
            .any(|s| s == "_Application.Run (Method)")
    );
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn values_only_preserves_book1_shell_and_uniform_workbook_refusals() {
    let model = ValuesOnly::new(Host::default());
    let app = model.application();
    for (name, kind) in [
        ("Caller", InvokeKind::Get),
        ("ThisCell", InvokeKind::Get),
        ("Range", InvokeKind::Get),
        ("Union", InvokeKind::Method),
    ] {
        let error = invoke(&model, &app, name, kind, vec![]).unwrap_err();
        assert_eq!(error.number(), 1004);
        assert_eq!(error.description(), "needs a workbook: open in DNA Calc");
    }
    let AutoValue::Object(books) =
        invoke(&model, &app, "Workbooks", InvokeKind::Get, vec![]).unwrap()
    else {
        panic!("Workbooks");
    };
    assert!(matches!(
        invoke(&model, &books, "Count", InvokeKind::Get, vec![]).unwrap(),
        AutoValue::I4(1)
    ));
    for index in [AutoValue::I4(1), AutoValue::text("book1")] {
        let AutoValue::Object(book) =
            invoke(&model, &books, "Item", InvokeKind::Get, vec![index]).unwrap()
        else {
            panic!("Workbook");
        };
        for (name, expected) in [("Name", "Book1"), ("FullName", "Book1"), ("Path", "")] {
            assert_eq!(
                invoke(&model, &book, name, InvokeKind::Get, vec![])
                    .unwrap()
                    .as_text()
                    .as_deref(),
                Some(expected)
            );
        }
    }
    assert!(
        invoke(
            &model,
            &books,
            "Item",
            InvokeKind::Get,
            vec![AutoValue::I4(2)]
        )
        .is_err()
    );
    assert!(matches!(
        invoke(&model, &app, "ActiveSheet", InvokeKind::Get, vec![]),
        Err(AutoError::NotModelled {
            owner: "dc-svqh",
            ..
        })
    ));
    assert!(matches!(
        invoke(&model, &app, "Calculate", InvokeKind::Method, vec![]),
        Err(AutoError::NotModelled {
            owner: "dc-1nbp.5",
            ..
        })
    ));
}
