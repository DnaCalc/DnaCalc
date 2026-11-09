//! Regressions against the Excel-authored dc-snz.7 file witnesses.

mod cdoc_m1_1_support;

use calc_doc::model::value::CellKey;
use calc_doc::model::{CellInput, OpBatch, WorkbookOp};
use cdoc_m1_1_support::d1::{load, part, part_text, save_model, save_untouched};

fn xml_fragment<'a>(xml: &'a str, tag: &str) -> &'a str {
    let start = xml
        .find(&format!("<{tag}"))
        .unwrap_or_else(|| panic!("missing {tag}"));
    let opening_end = start + xml[start..].find('>').unwrap() + 1;
    if xml[start..opening_end].ends_with("/>") {
        return &xml[start..opening_end];
    }
    let close = format!("</{tag}>");
    let end = opening_end + xml[opening_end..].find(&close).unwrap() + close.len();
    &xml[start..end]
}

/// Windows-1252 text of a committed source file, as the project's code page stores it.
fn code_page_1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{2c6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}',
        '\u{8f}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}',
        '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{9d}', '\u{17e}',
        '\u{178}',
    ];
    bytes
        .iter()
        .map(|&b| {
            if (0x80..0xa0).contains(&b) {
                HIGH[usize::from(b - 0x80)]
            } else {
                char::from(b)
            }
        })
        .collect()
}

fn source_part(package: &[u8], name: &str) -> Vec<u8> {
    let archive = cdoc_m1_1_support::parse_zip(package);
    let entry = archive.entry(name);
    match entry.method {
        0 => entry.payload(package).to_vec(),
        8 => miniz_oxide::inflate::decompress_to_vec(entry.payload(package)).unwrap(),
        _ => panic!("unsupported fixture compression"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_14__explicit_binary_save_preserves_all_source_records() {
    for binary in [
        include_bytes!("fixtures/vba_udf_names/local.xlsb").as_slice(),
        include_bytes!("fixtures/vba_udf_names/collision.xlsb").as_slice(),
        include_bytes!("fixtures/vba_udf_names/consumer.xlsb").as_slice(),
        include_bytes!("fixtures/vba_udf_names/metadata-custom-vba.xlsb").as_slice(),
    ] {
        let loaded = load(binary);
        let plan = calc_doc::prepare_save(
            &loaded.snapshot(loaded.model()),
            &calc_doc::SavePolicy {
                format: calc_doc::SaveFormat::Xlsb,
                ..Default::default()
            },
        )
        .unwrap();
        let mut saved = Vec::new();
        let receipt = calc_doc::write(plan, &mut saved).unwrap();
        let archive = cdoc_m1_1_support::parse_zip(&saved);
        assert!(
            archive
                .entries
                .iter()
                .any(|entry| entry.name == "xl/workbook.bin")
        );
        assert!(
            !archive
                .entries
                .iter()
                .any(|entry| entry.name == "xl/workbook.xml")
        );
        let source = cdoc_m1_1_support::parse_zip(binary);
        assert_eq!(source.entries.len(), archive.entries.len());
        for entry in source.entries.iter() {
            assert_eq!(
                source_part(&saved, &entry.name),
                source_part(binary, &entry.name),
                "{}",
                entry.name
            );
        }
        assert_eq!(receipt.parts_written, 0);
        assert_eq!(receipt.values_written, 0);
        assert_eq!(receipt.caches_dropped, 0);
        let reopened = load(&saved);
        assert_eq!(reopened.model().as_ref(), loaded.model().as_ref());
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_14__binary_save_refuses_unwritten_changes_and_foreign_models() {
    let binary = include_bytes!("fixtures/vba_udf_names/collision.xlsb");
    let loaded = load(binary);
    let policy = calc_doc::SavePolicy {
        format: calc_doc::SaveFormat::Xlsb,
        ..Default::default()
    };
    let edited = cdoc_m1_1_support::d1::edited(
        &loaded,
        vec![WorkbookOp::SetCell {
            at: CellKey::new(loaded.model().sheets()[0].id(), 50, 4).unwrap(),
            input: cdoc_m1_1_support::d1::number(7.0),
        }],
    );
    assert!(matches!(
        calc_doc::prepare_save(&loaded.snapshot(&edited), &policy),
        Err(calc_doc::SaveRefusal::BinaryChangesNotModelled { owner: "dc-snz.14" })
    ));
    let other = load(binary);
    assert!(matches!(
        calc_doc::prepare_save(&loaded.snapshot(other.model()), &policy),
        Err(calc_doc::SaveRefusal::ForeignModel)
    ));
    let xml = load(include_bytes!("fixtures/vba_udf_names/collision.xlsm"));
    assert!(matches!(
        calc_doc::prepare_save(&xml.snapshot(xml.model()), &policy),
        Err(calc_doc::SaveRefusal::BinarySourceRequired)
    ));
    assert!(matches!(
        calc_doc::prepare_save(
            &loaded.snapshot(loaded.model()),
            &calc_doc::SavePolicy {
                max_package_bytes: 1,
                ..policy
            }
        ),
        Err(calc_doc::SaveRefusal::PackageTooLarge { .. })
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_14__binary_save_checks_the_original_source_before_writing() {
    use std::sync::Arc;
    let bytes = include_bytes!("fixtures/vba_udf_names/collision.xlsb");
    let changing = Arc::new(cdoc_m1_1_support::ChangingSource::new(bytes.to_vec()));
    let source: Arc<dyn calc_doc::PackageRead> = changing.clone();
    let document = calc_doc::open(source, &calc_doc::OpenPolicy::default()).unwrap();
    let plan = calc_doc::prepare_save(
        &document.snapshot(),
        &calc_doc::SavePolicy {
            format: calc_doc::SaveFormat::Xlsb,
            ..Default::default()
        },
    )
    .unwrap();
    let archive = cdoc_m1_1_support::parse_zip(bytes);
    let at = archive.entry("xl/workbook.bin").payload_start;
    changing.change(|data| data[at] ^= 1);
    let mut saved = Vec::new();
    assert!(matches!(
        calc_doc::write(plan, &mut saved),
        Err(calc_doc::WriteError::SourceChanged { .. })
    ));
    assert!(saved.is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_13__binary_conversion_keeps_categories_and_argument_help() {
    for (binary, twin) in [
        (
            include_bytes!("fixtures/vba_udf_names/metadata-unqualified.xlsb").as_slice(),
            include_bytes!("fixtures/vba_udf_names/metadata-unqualified.xlsm").as_slice(),
        ),
        (
            include_bytes!("fixtures/vba_udf_names/metadata-custom-vba.xlsb").as_slice(),
            include_bytes!("fixtures/vba_udf_names/metadata-custom-vba.xlsm").as_slice(),
        ),
    ] {
        let loaded = load(binary);
        assert!(loaded.model().definitions().macro_enabled());
        // Help metadata is separate from normal formula definitions and procedure discovery.
        assert!(
            !loaded
                .model()
                .definitions()
                .names()
                .iter()
                .any(|name| name.name == "MetaFunc")
        );
        let expected = part_text(twin, "xl/workbook.xml");
        let vba = source_part(binary, "xl/vbaProject.bin");
        let edited = cdoc_m1_1_support::d1::edited(
            &loaded,
            vec![WorkbookOp::SetCell {
                at: CellKey::new(loaded.model().sheets()[0].id(), 50, 4).unwrap(),
                input: cdoc_m1_1_support::d1::number(7.0),
            }],
        );
        for saved in [save_untouched(binary), save_model(&loaded, &edited).0] {
            let written = part_text(&saved, "xl/workbook.xml");
            for tag in ["functionGroups", "x14:definedNames"] {
                assert_eq!(
                    xml_fragment(&written, tag),
                    xml_fragment(&expected, tag),
                    "{tag}"
                );
            }
            assert_eq!(
                part(&saved, "xl/vbaProject.bin").as_deref(),
                Some(vba.as_slice())
            );
        }
    }
}

struct FunctionCatalog;
impl calc_fml::ReferenceCatalog for FunctionCatalog {
    fn table(&self, name: &str) -> calc_fml::reference::TableLookup {
        let _ = name;
        calc_fml::reference::TableLookup::NotModelled
    }

    fn table_containing(
        &self,
        cell: calc_func::value::CellKey,
    ) -> calc_fml::reference::TableLookup {
        let _ = cell;
        calc_fml::reference::TableLookup::NotModelled
    }

    fn sheet_span(
        &self,
        first: calc_func::value::SheetId,
        last: calc_func::value::SheetId,
    ) -> Option<Vec<calc_func::value::SheetId>> {
        let _ = (first, last);
        None
    }

    fn unknown_calls_are_names(&self) -> bool {
        false
    }

    fn external_sheet(
        &self,
        book: &calc_fml::reference::ExternalBookRef,
        sheet: &str,
    ) -> Option<calc_func::value::SheetId> {
        let _ = (book, sheet);
        None
    }

    fn external_function_target(
        &self,
        book: &calc_fml::reference::ExternalBookRef,
    ) -> Option<String> {
        let _ = book;
        None
    }

    fn workbook_name(&self) -> Option<&str> {
        Some("local.xlsm")
    }
    fn sheet_named(&self, _: &str) -> Option<calc_doc::model::value::SheetId> {
        None
    }
    fn defined_name(
        &self,
        _: &str,
        _: calc_fml::reference::NameScope,
    ) -> calc_fml::reference::NameDefinition {
        calc_fml::reference::NameDefinition::Unknown
    }
}

struct LinkedCatalog<'a>(&'a calc_doc::model::Definitions);
impl calc_fml::ReferenceCatalog for LinkedCatalog<'_> {
    fn table(&self, name: &str) -> calc_fml::reference::TableLookup {
        let _ = name;
        calc_fml::reference::TableLookup::NotModelled
    }

    fn table_containing(
        &self,
        cell: calc_func::value::CellKey,
    ) -> calc_fml::reference::TableLookup {
        let _ = cell;
        calc_fml::reference::TableLookup::NotModelled
    }

    fn sheet_span(
        &self,
        first: calc_func::value::SheetId,
        last: calc_func::value::SheetId,
    ) -> Option<Vec<calc_func::value::SheetId>> {
        let _ = (first, last);
        None
    }

    fn unknown_calls_are_names(&self) -> bool {
        false
    }

    fn workbook_name(&self) -> Option<&str> {
        None
    }

    fn external_sheet(
        &self,
        book: &calc_fml::reference::ExternalBookRef,
        sheet: &str,
    ) -> Option<calc_func::value::SheetId> {
        let _ = (book, sheet);
        None
    }

    fn sheet_named(&self, _: &str) -> Option<calc_doc::model::value::SheetId> {
        None
    }
    fn defined_name(
        &self,
        _: &str,
        _: calc_fml::reference::NameScope,
    ) -> calc_fml::reference::NameDefinition {
        calc_fml::reference::NameDefinition::Unknown
    }
    fn external_function_target(
        &self,
        book: &calc_fml::reference::ExternalBookRef,
    ) -> Option<String> {
        let calc_fml::reference::ExternalBookRef::Number(index) = book else {
            return None;
        };
        let held = self.0.external_book(*index)?;
        held.absolute_target.clone().or_else(|| held.target.clone())
    }
}

struct NoReads;
impl calc_fml::reference::ReferenceReader for NoReads {
    fn read_denial(&self) -> Option<calc_fml::ReadDenial> {
        None
    }

    fn read_fact(&mut self, _fact: calc_fml::FactRead) {}

    fn spill_extent(&mut self, _: calc_func::value::CellKey) -> calc_fml::reference::SpillExtent {
        calc_fml::reference::SpillExtent::NotModelled
    }

    fn subtotal_exclusions(
        &mut self,
        _: calc_func::value::GridArea,
    ) -> Option<calc_func::SubtotalExclusions> {
        None
    }

    fn read_area(
        &mut self,
        _request: calc_fml::ReadRequest,
        _: calc_func::value::GridArea,
        _: &mut dyn FnMut(calc_fml::reference::CellRun<'_>) -> std::ops::ControlFlow<()>,
    ) {
        panic!("the measured call arguments are literals, never file caches");
    }
}

fn evaluate(
    plan: &std::sync::Arc<calc_fml::CompiledPlan>,
    at: CellKey,
    functions: &calc_fml::CallableTable,
) -> calc_func::value::CalcValue {
    evaluate_with_reader(plan, at, functions, &mut NoReads)
}

fn evaluate_with_reader(
    plan: &std::sync::Arc<calc_fml::CompiledPlan>,
    at: CellKey,
    functions: &calc_fml::CallableTable,
    reader: &mut dyn calc_fml::reference::ReferenceReader,
) -> calc_func::value::CalcValue {
    let instance = calc_fml::instantiate(plan, calc_fml::reference::FormulaAnchor::new(at));
    let mut scratch = calc_fml::EvalScratch::new();
    calc_fml::evaluate(
        calc_fml::EvalFrame::new(&instance, &mut scratch),
        &mut calc_fml::FormulaEvaluationContext::new(
            &calc_func::value::NumericMode::Fast,
            reader,
            None,
            None,
            calc_fml::ReadRequest::DEPENDENCY,
        )
        .with_functions(functions),
    )
    .unwrap()
}

struct WorkbookLiteral<'a>(&'a calc_doc::model::ModelRoot);
impl calc_fml::reference::ReferenceReader for WorkbookLiteral<'_> {
    fn read_denial(&self) -> Option<calc_fml::ReadDenial> {
        None
    }

    fn read_fact(&mut self, _fact: calc_fml::FactRead) {}

    fn spill_extent(&mut self, _: calc_func::value::CellKey) -> calc_fml::reference::SpillExtent {
        calc_fml::reference::SpillExtent::NotModelled
    }

    fn subtotal_exclusions(
        &mut self,
        _: calc_func::value::GridArea,
    ) -> Option<calc_func::SubtotalExclusions> {
        None
    }

    fn read_area(
        &mut self,
        _request: calc_fml::ReadRequest,
        area: calc_func::value::GridArea,
        sink: &mut dyn FnMut(calc_fml::reference::CellRun<'_>) -> std::ops::ControlFlow<()>,
    ) {
        assert_eq!(area.first(), area.last());
        let Some(CellInput::Number(value)) = self.0.cell(area.first()) else {
            panic!("an authored numeric input")
        };
        let values = [calc_func::value::CalcValue::number(value.get())];
        let _ = sink(calc_fml::reference::CellRun::Values(&values));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_9__qualified_arguments_read_the_consumer_workbook() {
    use calc_fml::ReferenceCatalog;
    let loaded = load(include_bytes!(
        "fixtures/vba_udf_names/binding-arguments.xlsx"
    ));
    let catalog = LinkedCatalog(loaded.model().definitions());
    let book = calc_fml::reference::ExternalBookRef::Number(1);
    let provider = catalog.external_function_target(&book).unwrap();
    let mut functions = calc_fml::CallableTable::new();
    for name in ["OtherFunc", "Module1.OtherFunc"] {
        functions
            .register_for(Some(&provider), name, std::rc::Rc::new(Identity))
            .unwrap();
    }
    for row in 1..=2 {
        let at = CellKey::new(loaded.model().sheets()[0].id(), row, 2).unwrap();
        let CellInput::Formula(formula) = loaded.model().cell(at).unwrap() else {
            panic!("formula")
        };
        let tree = calc_fml::parse_formula(&format!("={}", formula.text().to_string_lossy()));
        let plan = calc_fml::compile_loaded_with_functions(
            &tree,
            calc_fml::reference::ReferenceNotation::A1,
            calc_fml::reference::FormulaAnchor::new(at),
            &catalog,
            &mut calc_fml::PlanCache::new(),
            calc_fml::FormulaForm::Dynamic,
            &functions,
        )
        .unwrap();
        assert_eq!(
            evaluate_with_reader(&plan, at, &functions, &mut WorkbookLiteral(loaded.model()))
                .as_number(),
            Some(2.0)
        );
    }
}

#[derive(Debug)]
struct Identity;
impl calc_func::UserFunction for Identity {
    fn call(
        &self,
        arguments: &[calc_func::value::CalcValue],
    ) -> Result<calc_func::value::CalcValue, String> {
        Ok(arguments
            .first()
            .cloned()
            .unwrap_or_else(calc_func::value::CalcValue::empty))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_11__registered_unicode_names_match_measured_entry_and_file_cases() {
    let mut functions = calc_fml::CallableTable::new();
    for declared in ["Café", "Ångström"] {
        functions
            .register(declared, std::rc::Rc::new(Identity))
            .unwrap();
    }
    for bytes in [
        include_bytes!("fixtures/vba_udf_names/local.xlsm").as_slice(),
        include_bytes!("fixtures/vba_udf_names/local.xlsb").as_slice(),
        include_bytes!("fixtures/vba_udf_names/unicode-case.xlsm").as_slice(),
    ] {
        let loaded = load(bytes);
        let sheet = loaded.model().sheets()[0].id();
        let anchor = CellKey::new(sheet, 1, 4).unwrap();
        let mut cache = calc_fml::PlanCache::new();
        let mut plans = Vec::new();
        for spelling in ["café", "CAFÉ", "ångström", "ÅNGSTRÖM"] {
            let tree = calc_fml::parse_formula(&format!("={spelling}(2)"));
            let plan = calc_fml::compile_with_functions(
                &tree,
                calc_fml::reference::ReferenceNotation::A1,
                calc_fml::reference::FormulaAnchor::new(anchor),
                &FunctionCatalog,
                &mut cache,
                calc_fml::FormulaForm::Dynamic,
                &functions,
            )
            .unwrap();
            assert_eq!(plan.user_calls().count(), 1, "typed {spelling}");
            assert_eq!(evaluate(&plan, anchor, &functions).as_number(), Some(2.0));
            plans.push(plan);
        }
        assert!(std::sync::Arc::ptr_eq(&plans[0], &plans[1]));
        assert!(std::sync::Arc::ptr_eq(&plans[2], &plans[3]));
        let mut checked = 0;
        for (at, input) in loaded.model().sheet_cells(sheet).into_iter().flatten() {
            let CellInput::Formula(formula) = input else {
                continue;
            };
            let text = formula.text().to_string_lossy();
            let Some((spelling, _)) = text.split_once('(') else {
                continue;
            };
            if !["Café", "Ångström"]
                .iter()
                .any(|declared| calc_func::value::workbook_names_equal(declared, spelling))
            {
                continue;
            }
            let tree = calc_fml::parse_formula(&format!("={text}"));
            let plan = calc_fml::compile_loaded_with_functions(
                &tree,
                calc_fml::reference::ReferenceNotation::A1,
                calc_fml::reference::FormulaAnchor::new(at),
                &FunctionCatalog,
                &mut calc_fml::PlanCache::new(),
                calc_fml::FormulaForm::Dynamic,
                &functions,
            )
            .unwrap();
            assert_eq!(plan.user_calls().count(), 1, "file {text}");
            assert_eq!(evaluate(&plan, at, &functions).as_number(), Some(2.0));
            checked += 1;
        }
        assert!(checked >= 2);
    }
    for spelling in ["CAFÉ", "ångström"] {
        assert!(
            functions
                .register(spelling, std::rc::Rc::new(Identity))
                .is_err()
        );
        assert_eq!(
            functions
                .call(spelling, &[calc_func::value::CalcValue::number(2.0)])
                .unwrap()
                .as_number(),
            Some(2.0)
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_9__own_workbook_calls_bind_the_registered_procedure() {
    for bytes in [
        include_bytes!("fixtures/vba_udf_names/local.xlsm").as_slice(),
        include_bytes!("fixtures/vba_udf_names/local.xlsb").as_slice(),
    ] {
        let loaded = load(bytes);
        let sheet = loaded.model().sheets()[0].id();
        let mut functions = calc_fml::CallableTable::new();
        functions
            .register("MyFunc", std::rc::Rc::new(Identity))
            .unwrap();
        functions
            .register("Module1.MyFunc", std::rc::Rc::new(Identity))
            .unwrap();
        for row in [3, 4] {
            let at = CellKey::new(sheet, row, 2).unwrap();
            let CellInput::Formula(formula) = loaded.model().cell(at).unwrap() else {
                panic!("formula")
            };
            let tree = calc_fml::parse_formula(&format!("={}", formula.text().to_string_lossy()));
            let mut cache = calc_fml::PlanCache::new();
            let plan = calc_fml::compile_with_functions(
                &tree,
                calc_fml::reference::ReferenceNotation::A1,
                calc_fml::reference::FormulaAnchor::new(at),
                &FunctionCatalog,
                &mut cache,
                calc_fml::FormulaForm::Dynamic,
                &functions,
            )
            .unwrap();
            assert_eq!(plan.user_calls().count(), 1);
            let (_, source) = plan.user_call_sources().next().unwrap();
            assert_eq!(
                source.unwrap().book,
                calc_fml::reference::ExternalBookRef::Number(0)
            );
            // An identity adapter proves binding and the shared evaluator route, not VBA execution.
            assert_eq!(evaluate(&plan, at, &functions).as_number(), Some(2.0));
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_9__external_calls_keep_link_and_provider_identity() {
    use calc_fml::ReferenceCatalog;
    for bytes in [
        include_bytes!("fixtures/vba_udf_names/consumer.xlsx").as_slice(),
        include_bytes!("fixtures/vba_udf_names/consumer.xlsb").as_slice(),
    ] {
        let loaded = load(bytes);
        let sheet = loaded.model().sheets()[0].id();
        let catalog = LinkedCatalog(loaded.model().definitions());
        for (row, index, name) in [
            (1, 1, "OtherFunc"),
            (2, 1, "Module1.OtherFunc"),
            (4, 1, "HiddenOther"),
            (5, 1, "PrivateOther"),
            (6, 2, "AddFunc"),
            (7, 2, "AddFunc"),
            (8, 2, "Module1.AddFunc"),
        ] {
            let book = calc_fml::reference::ExternalBookRef::Number(index);
            let provider = catalog.external_function_target(&book).unwrap();
            assert!(
                provider.starts_with("file:///C:"),
                "the measured absolute target: {provider}"
            );
            let mut functions = calc_fml::CallableTable::new();
            functions
                .register_for(Some(&provider), name, std::rc::Rc::new(Identity))
                .unwrap();
            // The same procedure spelling in another provider never satisfies this call.
            let wrong = format!(
                "file:///C:/wrong-folder/{}",
                loaded
                    .model()
                    .definitions()
                    .external_book(index)
                    .unwrap()
                    .file_name
                    .as_ref()
                    .unwrap()
            );
            let mut wrong_functions = calc_fml::CallableTable::new();
            wrong_functions
                .register(name, std::rc::Rc::new(Identity))
                .unwrap();
            wrong_functions
                .register_for(Some(&wrong), name, std::rc::Rc::new(Identity))
                .unwrap();
            let at = CellKey::new(sheet, row, 2).unwrap();
            let CellInput::Formula(formula) = loaded.model().cell(at).unwrap() else {
                panic!("formula")
            };
            let tree = calc_fml::parse_formula(&format!("={}", formula.text().to_string_lossy()));
            let plan = calc_fml::compile_loaded_with_functions(
                &tree,
                calc_fml::reference::ReferenceNotation::A1,
                calc_fml::reference::FormulaAnchor::new(at),
                &catalog,
                &mut calc_fml::PlanCache::new(),
                calc_fml::FormulaForm::Dynamic,
                &functions,
            )
            .unwrap();
            let (spelling, source) = plan.user_call_sources().next().unwrap();
            assert_eq!(spelling, calc_func::value::fold_workbook_name(name));
            assert_eq!(source.unwrap().book, book);
            assert_eq!(source.unwrap().target.as_deref(), Some(provider.as_str()));
            assert_eq!(evaluate(&plan, at, &functions).as_number(), Some(2.0));
            assert_eq!(
                evaluate(&plan, at, &wrong_functions),
                calc_func::value::CalcValue::error(calc_func::value::WorksheetErrorCode::Name)
            );
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_8__reentered_forced_udf_keeps_ordinary_arguments() {
    for bytes in [
        include_bytes!("fixtures/vba_udf_names/collision.xlsm").as_slice(),
        include_bytes!("fixtures/vba_udf_names/collision.xlsb").as_slice(),
    ] {
        let loaded = load(bytes);
        let root = loaded.model();
        let sheet = root.sheets()[0].id();
        let original = CellKey::new(sheet, 1, 2).unwrap();
        let input = root.cell(original).unwrap().clone();
        let CellInput::Formula(formula) = &input else {
            panic!("a formula")
        };
        assert_eq!(formula.text().to_string_lossy(), "_xludf.LAMBDA(Rate,3)");
        let target = CellKey::new(sheet, 1, 4).unwrap();
        let edit = calc_doc::prepare_edit(
            root,
            &OpBatch {
                base: root.revision(),
                operations: vec![WorkbookOp::SetCell { at: target, input }],
            },
        )
        .unwrap();
        let (saved, _) = save_model(&loaded, edit.model());
        let reopened = load(&saved);
        assert_eq!(reopened.model().cell(target), root.cell(original));
        // Ordinary language LAMBDA still receives parameter prefixes, as in local.xlsm.
        let local = load(include_bytes!("fixtures/vba_udf_names/local.xlsm"));
        let named = local
            .model()
            .definitions()
            .names()
            .iter()
            .find(|name| name.name == "NamedLambda")
            .unwrap();
        let CellInput::Formula(text) = cdoc_m1_1_support::d1::formula("LAMBDA(x,x+1400)") else {
            unreachable!()
        };
        assert_eq!(
            String::from_utf16(
                &calc_doc::file_form(&text, root.definitions().names())
                    .unwrap()
                    .text
            )
            .unwrap(),
            named.formula
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_12__binary_links_keep_the_measured_absolute_targets() {
    let xml = include_bytes!("fixtures/vba_udf_names/consumer.xlsx");
    let saved = save_untouched(include_bytes!("fixtures/vba_udf_names/consumer.xlsb"));
    // The saved source folder is also needed: Excel rebases relative links from it.
    let workbook = part_text(xml, "xl/workbook.xml");
    let location = workbook
        .split("<x15ac:absPath")
        .nth(1)
        .unwrap()
        .split("url=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let converted = part_text(&saved, "xl/workbook.xml");
    assert!(converted.contains("<x15ac:absPath"));
    assert!(converted.contains(&format!("url=\"{location}\"")));
    for index in 1..=2 {
        let link = format!("xl/externalLinks/externalLink{index}.xml");
        let expected = part_text(xml, &link);
        let written = part_text(&saved, &link);
        // Both the relative main link and its absolute alternate relationship survive.
        for fragment in [
            "r:id=\"rId1\"",
            "<xxl21:alternateUrls>",
            "<xxl21:absoluteUrl r:id=\"rId2\"/>",
            "</xxl21:alternateUrls>",
        ] {
            assert!(
                expected.contains(fragment),
                "Excel witness contains {fragment}"
            );
            assert!(
                written.contains(fragment),
                "converted {link} contains {fragment}: {written}"
            );
        }
        let relationships = format!("xl/externalLinks/_rels/externalLink{index}.xml.rels");
        assert_eq!(part(&saved, &relationships), part(xml, &relationships));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_15__read_measured_project_sources_without_executing_macros() {
    for source in [
        include_bytes!("fixtures/vba_udf_names/local.xlsm").as_slice(),
        include_bytes!("fixtures/vba_udf_names/local.xlsb").as_slice(),
        include_bytes!("fixtures/vba_udf_names/disabled-resaved.xlsm").as_slice(),
    ] {
        let loaded = load(source);
        // calc-doc hands over the part as it is; CalcVBA reads what is inside it.
        let held = loaded.vba_project(&Default::default()).unwrap().unwrap();
        assert_eq!(held.part, "xl/vbaProject.bin");
        assert_eq!(held.bytes, source_part(source, "xl/vbaProject.bin"));
        let project = calc_vba::read_office_project(&held.bytes).unwrap();
        assert_eq!(project.code_page(), 1252);
        assert_eq!(project.modules().len(), 4);
        let module = project
            .modules()
            .iter()
            .find(|m| m.name == "Module1")
            .unwrap();
        assert_eq!(module.stream_name, "Module1");
        assert_eq!(module.kind, calc_vba::OfficeModuleKind::Standard);
        assert!(!module.private);
        assert_eq!(
            module.source,
            code_page_1252(include_bytes!("fixtures/vba_udf_names/Module1.bas"))
        );
        assert!(module.source.contains("Public Function Caf\u{e9}("));
        assert!(
            module
                .source
                .contains("Public Function \u{c5}ngstr\u{f6}m(")
        );
        assert!(module.source.contains("Private Function SecretFunc("));
        let private = project
            .modules()
            .iter()
            .find(|m| m.name == "PrivateModule")
            .unwrap();
        assert!(private.private);
        assert_eq!(
            private.source,
            code_page_1252(include_bytes!("fixtures/vba_udf_names/PrivateModule.bas"))
        );
        assert_eq!(
            project
                .modules()
                .iter()
                .filter(|m| m.kind == calc_vba::OfficeModuleKind::Document)
                .count(),
            2
        );
        // Merely reading source has neither changed calls nor assigned calculated provenance.
        let (saved, _) = save_model(&loaded, loaded.model());
        assert_eq!(
            part(&saved, "xl/vbaProject.bin"),
            Some(source_part(source, "xl/vbaProject.bin"))
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_15__project_attributes_and_read_limits() {
    let loaded = load(include_bytes!(
        "fixtures/vba_udf_names/metadata-custom-vba.xlsb"
    ));
    let part = loaded.vba_project(&Default::default()).unwrap().unwrap();
    let project = calc_vba::read_office_project(&part.bytes).unwrap();
    let module = project
        .modules()
        .iter()
        .find(|m| m.name == "Module1")
        .unwrap();
    assert_eq!(
        module.source,
        code_page_1252(include_bytes!(
            "fixtures/vba_udf_names/Metadata-custom-vba.bas"
        ))
    );
    assert!(module.source.contains("Attribute MetaFunc.VB_Description"));
    // The part's size is calc-doc's limit; what is inside it is CalcVBA's.
    assert!(matches!(
        loaded.vba_project(&calc_doc::VbaReadPolicy {
            max_project_bytes: 1
        }),
        Err(calc_doc::VbaReadError::Limit(_))
    ));
    let defaults = calc_vba::OfficeReadLimits::default;
    for limits in [
        calc_vba::OfficeReadLimits {
            project_bytes: 1,
            ..defaults()
        },
        calc_vba::OfficeReadLimits {
            directory_bytes: 1,
            ..defaults()
        },
        calc_vba::OfficeReadLimits {
            modules: 1,
            ..defaults()
        },
        calc_vba::OfficeReadLimits {
            module_bytes: 1,
            ..defaults()
        },
        calc_vba::OfficeReadLimits {
            total_source_bytes: 1,
            ..defaults()
        },
    ] {
        let refused = calc_vba::read_office_project_with(&part.bytes, &limits).unwrap_err();
        assert!(refused.contains("limit"), "{refused}");
    }
    assert!(
        load(include_bytes!("fixtures/vba_udf_names/stripped.xlsx"))
            .vba_project(&Default::default())
            .unwrap()
            .is_none()
    );
}
