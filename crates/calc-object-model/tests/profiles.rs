use calc_object_model::geometry::{self, AddressOptions, RangeAxis, ReferenceStyle};
use calc_object_model::profile::{ApplicationHost, values_only::ValuesOnly};
use calc_object_model::{
    AutoError, AutoValue, Call, Descriptor, InvokeKind, ObjectModel, ObjectRef,
};
use calc_value::{CellKey, GridArea, IdAllocator, ReferenceLike, SheetId};
use std::cell::{Cell, RefCell};

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn values_only_forwards_each_projected_refusal_once_to_its_host() {
    struct Host(Cell<u32>);
    impl ApplicationHost for Host {
        fn hwnd(&self) -> isize {
            1
        }
        fn run(&self, _: &str, _: &[AutoValue]) -> Result<AutoValue, AutoError> {
            Ok(AutoValue::Empty)
        }
        fn events_enabled(&self) -> bool {
            true
        }
        fn set_events_enabled(&self, _: bool) {}
        fn record_refusal(&self, error: &AutoError) {
            assert_eq!(error.number(), 1004);
            self.0.set(self.0.get() + 1);
        }
    }
    let host = std::rc::Rc::new(Host(Cell::new(0)));
    let model = ValuesOnly::new(host.clone());
    let error = AutoError::not_modelled("_Application.Iteration", "dc-1nbp.5");
    model.record_refusal(&error);
    assert_eq!(host.0.get(), 1);
}

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
fn measured_relative_r1c1_uses_a1_origin_or_relative_to_and_omits_zero_offsets() {
    let (range, _) = references();
    let relative = AddressOptions {
        style: ReferenceStyle::R1C1,
        row_absolute: false,
        column_absolute: false,
        ..Default::default()
    };
    assert_eq!(
        geometry::address(&range, relative, None).unwrap(),
        "R[1]C[1]:R[2]C[2]"
    );
    let base = CellKey::new(range.sheet(), 3, 3).unwrap();
    assert_eq!(
        geometry::address(
            &range,
            AddressOptions {
                relative_to: Some(base),
                ..relative
            },
            None
        )
        .unwrap(),
        "R[-1]C[-1]:RC"
    );
    assert_eq!(
        geometry::address(
            &range,
            AddressOptions {
                relative_to: Some(base),
                column_absolute: true,
                ..relative
            },
            None
        )
        .unwrap(),
        "R[-1]C2:RC3"
    );
    assert_eq!(
        geometry::address(
            &range,
            AddressOptions {
                relative_to: Some(base),
                ..Default::default()
            },
            None
        )
        .unwrap(),
        "$B$2:$C$3"
    );
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn union_preserves_order_multiplicity_and_first_area_views() {
    let (first, second) = references();
    let overlap = ReferenceLike::area(
        GridArea::new(
            CellKey::new(first.sheet(), 3, 3).unwrap(),
            CellKey::new(first.sheet(), 4, 4).unwrap(),
        )
        .unwrap(),
    );
    let range = geometry::union(&[second.clone(), first.clone(), overlap.clone()]).unwrap();
    assert_eq!(
        range.areas(),
        &[second.areas()[0], first.areas()[0], overlap.areas()[0]]
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
fn measured_union_drops_an_identical_area_and_merges_adjacent_same_width() {
    let (range, _) = references();
    let twice = geometry::union(&[range.clone(), range.clone()]).unwrap();
    assert_eq!(twice, range);
    assert_eq!(geometry::count(&twice, RangeAxis::Cells), 4);
    let adjacent = ReferenceLike::area(
        GridArea::new(
            CellKey::new(range.sheet(), 4, 2).unwrap(),
            CellKey::new(range.sheet(), 5, 3).unwrap(),
        )
        .unwrap(),
    );
    let joined = geometry::union(&[range, adjacent]).unwrap();
    assert_eq!(
        geometry::address(&joined, Default::default(), None).unwrap(),
        "$B$2:$C$5"
    );
    assert_eq!(geometry::count(&joined, RangeAxis::Rows), 4);
    assert_eq!(geometry::count(&joined, RangeAxis::Cells), 8);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn relative_addresses_match_the_retained_excel_e11_trace() {
    let bytes =
        include_bytes!("../../calc-xll-reference/traces/2026-11-14-om-probes/trial-1/om-e11.tsv");
    let trace = std::str::from_utf8(bytes).unwrap();
    let (first, _) = references();
    let origin = CellKey::new(first.sheet(), 3, 3).unwrap();
    let relative = AddressOptions {
        style: ReferenceStyle::R1C1,
        row_absolute: false,
        column_absolute: false,
        ..Default::default()
    };
    for (case, options) in [
        (
            "Address xlR1C1 RowAbsolute=False ColumnAbsolute=False (no RelativeTo)",
            relative,
        ),
        (
            "Address xlR1C1 RowAbsolute=False ColumnAbsolute=False RelativeTo=C3",
            AddressOptions {
                relative_to: Some(origin),
                ..relative
            },
        ),
        (
            "Address xlR1C1 RowAbsolute=False RelativeTo=C3",
            AddressOptions {
                column_absolute: true,
                relative_to: Some(origin),
                ..relative
            },
        ),
        (
            "Address xlR1C1 absolute RelativeTo=C3",
            AddressOptions {
                relative_to: Some(origin),
                style: ReferenceStyle::R1C1,
                ..Default::default()
            },
        ),
        (
            "Address xlA1 RowAbsolute=False ColumnAbsolute=False RelativeTo=C3",
            AddressOptions {
                relative_to: Some(origin),
                style: ReferenceStyle::A1,
                ..relative
            },
        ),
    ] {
        let row = trace
            .lines()
            .find(|line| line.starts_with(&format!("{case}\tpia\tsingle B2:C3\t")))
            .unwrap();
        let expected = row
            .split_once("String:\"")
            .unwrap()
            .1
            .split_once('"')
            .unwrap()
            .0;
        assert_eq!(
            geometry::address(&first, options, None).unwrap(),
            expected,
            "{case}"
        );
    }
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
fn automation_security_is_a_values_only_descriptor_member() {
    let model = ValuesOnly::new(Host::default());
    let app = model.application();
    assert!(matches!(
        invoke(&model, &app, "AutomationSecurity", InvokeKind::Get, vec![]).unwrap(),
        AutoValue::I4(1)
    ));
    assert!(
        model
            .handlers()
            .implemented()
            .iter()
            .any(|member| member == "_Application.AutomationSecurity (Get)")
    );
    assert!(matches!(
        invoke(
            &model,
            &app,
            "AutomationSecurity",
            InvokeKind::Put,
            vec![AutoValue::I4(3)]
        ),
        Err(AutoError::NotModelled { .. })
    ));
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn values_only_window_parent_is_the_book1_workbook() {
    // xlOil's COM connect reads EXCEL7's Window.Parent as a _Workbook (Connect.cpp); Excel's
    // Window.Parent is the workbook, never the Application.
    struct Host;
    impl ApplicationHost for Host {
        fn hwnd(&self) -> isize {
            1
        }
        fn run(&self, _: &str, _: &[AutoValue]) -> Result<AutoValue, AutoError> {
            Ok(AutoValue::Empty)
        }
        fn events_enabled(&self) -> bool {
            true
        }
        fn set_events_enabled(&self, _: bool) {}
    }
    let model = ValuesOnly::new(std::rc::Rc::new(Host));
    let window = calc_object_model::profile::values_only::object(
        "Window",
        calc_object_model::ObjectKey::Window,
    );
    let get = |target: &ObjectRef, name: &str| {
        model
            .invoke(
                target,
                &Call {
                    interface: target.interface,
                    member: Descriptor::get()
                        .member(target.interface, name, InvokeKind::Get)
                        .unwrap(),
                    args: vec![],
                    lcid: 0,
                },
            )
            .unwrap()
    };
    let AutoValue::Object(parent) = get(&window, "Parent") else {
        panic!("Window.Parent is an object");
    };
    let AutoValue::Object(active) = get(&model.application(), "ActiveWorkbook") else {
        panic!("ActiveWorkbook is an object");
    };
    assert_eq!(parent, active);
    assert_eq!(get(&parent, "Name").as_text().as_deref(), Some("Book1"));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn worksheet_functions_share_the_catalogue_for_scalars_arrays_and_dotted_names() {
    let model = ValuesOnly::new(Host::default());
    let AutoValue::Object(functions) = invoke(
        &model,
        &model.application(),
        "WorksheetFunction",
        InvokeKind::Get,
        vec![],
    )
    .unwrap() else {
        panic!("WorksheetFunction");
    };
    let array = || {
        AutoValue::Array(Box::new(calc_object_model::AutoArray {
            rows: 2,
            columns: 2,
            lower: (1, 1),
            values: vec![
                AutoValue::R8(1.),
                AutoValue::R8(2.),
                AutoValue::Bool(true),
                AutoValue::text("ignored"),
            ],
        }))
    };
    assert!(matches!(
        invoke(
            &model,
            &functions,
            "Sum",
            InvokeKind::Method,
            vec![array(), AutoValue::I4(4), AutoValue::Missing]
        )
        .unwrap(),
        AutoValue::R8(7.)
    ));
    assert!(matches!(
        invoke(
            &model,
            &functions,
            "Ceiling_Math",
            InvokeKind::Method,
            vec![AutoValue::R8(2.2)]
        )
        .unwrap(),
        AutoValue::R8(3.)
    ));
    let AutoValue::Array(transposed) = invoke(
        &model,
        &functions,
        "Transpose",
        InvokeKind::Method,
        vec![array()],
    )
    .unwrap() else {
        panic!("array");
    };
    assert_eq!(
        (transposed.rows, transposed.columns, transposed.lower),
        (2, 2, (1, 1))
    );
    assert!(matches!(transposed.values[1], AutoValue::Bool(true)));
    assert!(matches!(transposed.values[2], AutoValue::R8(2.)));
    let error = invoke(
        &model,
        &functions,
        "_WSFunction",
        InvokeKind::Method,
        vec![],
    )
    .unwrap_err();
    assert!(matches!(error, AutoError::NotModelled { .. }));
    assert!(
        error
            .description()
            .contains("WorksheetFunction._WSFunction")
    );
    for member in Descriptor::get().members(functions.interface) {
        assert!(
            model.handlers().implements(functions.interface, member),
            "{}",
            member.name
        );
    }
    assert_eq!(Descriptor::get().members(functions.interface).len(), 420);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn worksheet_function_errors_are_1004_with_explicitly_unmeasured_e7_description() {
    let model = ValuesOnly::new(Host::default());
    let AutoValue::Object(functions) = invoke(
        &model,
        &model.application(),
        "WorksheetFunction",
        InvokeKind::Get,
        vec![],
    )
    .unwrap() else {
        panic!("WorksheetFunction");
    };
    let error = invoke(
        &model,
        &functions,
        "Match",
        InvokeKind::Method,
        vec![
            AutoValue::I4(9),
            AutoValue::Array(Box::new(calc_object_model::AutoArray {
                rows: 1,
                columns: 2,
                lower: (1, 1),
                values: vec![AutoValue::I4(1), AutoValue::I4(2)],
            })),
            AutoValue::I4(0),
        ],
    )
    .unwrap_err();
    assert_eq!(error.number(), 1004);
    // Owner-provided provisional description; E7 (dc-1nbp.6) has not measured its exact text.
    assert_eq!(
        error.description(),
        "Unable to get the Match property of the WorksheetFunction class"
    );
    assert!(matches!(
        invoke(
            &model,
            &functions,
            "Sum",
            InvokeKind::Method,
            vec![AutoValue::R4(f32::NAN)]
        ),
        Err(AutoError::Status(_))
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn values_only_descriptor_coverage_lists_every_registered_member() {
    let model = ValuesOnly::new(Host::default());
    let report = model.handlers().coverage("values-only", &[]);
    assert_eq!(
        report
            .lines()
            .filter(|line| line.contains(": MEMBER "))
            .count(),
        model.handlers().implemented().len()
    );
    println!("{report}");
    let d = Descriptor::get();
    let functions = d.members(d.id("WorksheetFunction").unwrap());
    let methods = functions
        .iter()
        .filter(|m| m.invoke == InvokeKind::Method)
        .collect::<Vec<_>>();
    let resolved = methods
        .iter()
        .filter(|m| {
            calc_func::catalogue()
                .resolve_surface(
                    calc_func::Notation::Call,
                    &calc_object_model::profile::worksheet_function::catalogue_name(&m.name),
                )
                .is_ok()
        })
        .count();
    println!(
        "WorksheetFunction: catalogue names {resolved}/{}; descriptor members {}",
        methods.len(),
        functions.len()
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn values_only_inline_ranges_have_no_geometry_path_without_a_workbook() {
    let model = ValuesOnly::new(Host::default());
    let (reference, _) = references();
    let range = calc_object_model::profile::values_only::object(
        "Range",
        calc_object_model::ObjectKey::Range(calc_func::StoredReference::Inline(reference)),
    );
    for member in [
        "Address",
        "Row",
        "Column",
        "Count",
        "Worksheet",
        "Application",
        "Parent",
        "Value",
        "Value2",
    ] {
        assert_eq!(
            invoke(&model, &range, member, InvokeKind::Get, vec![]).unwrap_err(),
            AutoError::excel(1004, "needs a workbook: open in DNA Calc")
        );
    }
}
