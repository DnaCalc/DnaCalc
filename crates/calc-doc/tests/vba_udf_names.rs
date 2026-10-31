//! Regressions against the Excel-authored dc-snz.7 file witnesses.

mod cdoc_m1_1_support;

use calc_doc::model::value::CellKey;
use calc_doc::model::{CellInput, OpBatch, WorkbookOp};
use cdoc_m1_1_support::d1::{load, part, part_text, save_model, save_untouched};

struct FunctionCatalog;
impl calc_fml::ReferenceCatalog for FunctionCatalog {
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
    fn read_area(
        &mut self,
        _: calc_func::value::GridArea,
        _: &mut dyn FnMut(calc_fml::reference::CellRun<'_>) -> std::ops::ControlFlow<()>,
    ) {
        panic!("the measured call arguments are literals, never file caches");
    }
}

fn evaluate(
    plan: &std::sync::Arc<calc_fml::CompiledPlan>,
    at: CellKey,
    functions: &calc_func::UserFunctions,
) -> calc_func::value::CalcValue {
    evaluate_with_reader(plan, at, functions, &mut NoReads)
}

fn evaluate_with_reader(
    plan: &std::sync::Arc<calc_fml::CompiledPlan>,
    at: CellKey,
    functions: &calc_func::UserFunctions,
    reader: &mut dyn calc_fml::reference::ReferenceReader,
) -> calc_func::value::CalcValue {
    let instance = calc_fml::instantiate(plan, calc_fml::reference::FormulaAnchor::new(at));
    let mut scratch = calc_fml::EvalScratch::new();
    calc_fml::evaluate(
        calc_fml::EvalFrame::new(&instance, &mut scratch),
        &mut calc_fml::EvaluationServices::new(&calc_func::value::NumericMode::Fast, reader)
            .with_functions(functions),
    )
    .unwrap()
}

struct WorkbookLiteral<'a>(&'a calc_doc::model::ModelRoot);
impl calc_fml::reference::ReferenceReader for WorkbookLiteral<'_> {
    fn read_area(
        &mut self,
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
    let mut functions = calc_func::UserFunctions::new();
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
    let mut functions = calc_func::UserFunctions::new();
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
        let mut functions = calc_func::UserFunctions::new();
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
            let mut functions = calc_func::UserFunctions::new();
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
            let mut wrong_functions = calc_func::UserFunctions::new();
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
