//! Worksheet entries for private procedures in opened workbook projects.
//!
//! The embedding API binds public handles. Generate a forwarding entry in the
//! same module, after the original project has compiled with its real visibility.
//! Both procedures execute in the one owning session. Original source and VBA
//! visibility are retained; these entry names never enter the worksheet registry
//! or the saved project. Argument defaults are applied by the ordinary adapter.
use super::{Error, OxTy, sym, type_name};
use std::collections::BTreeMap;

pub(super) type Bridges = BTreeMap<(String, String), oxvba_embed::Procedure>;

pub(super) fn private_entries(
    manifest: &mut sym::SymbolProjectManifest,
    engine: &oxvba_embed::Module,
) -> Result<Bridges, Error> {
    let mut bridges = Bridges::new();
    let source = manifest
        .modules
        .iter()
        .map(|m| m.source.to_lowercase())
        .collect::<String>();
    let mut serial = 0u64;
    let procedures: Vec<_> = engine
        .procedures()
        .filter(|p| {
            p.visibility == oxvba_embed::Visibility::Private
                && p.kind == oxvba_embed::ProcedureKind::Function
                && manifest.modules.iter().any(|m| {
                    m.module_name == p.module && m.module_kind == sym::ModuleKind::Procedural
                })
        })
        .cloned()
        .collect();
    let mut generated = 0usize;
    for procedure in &procedures {
        let scalar = |ty: &OxTy| {
            matches!(
                ty,
                OxTy::Long | OxTy::Double | OxTy::Bool | OxTy::Str | OxTy::Variant
            )
        };
        if procedure
            .parameters
            .iter()
            .any(|p| p.param_array || p.array_element.is_some() || !scalar(&p.ty))
            || !procedure.return_type.as_ref().is_some_and(scalar)
        {
            return Err(Error::UnsupportedSignature(format!(
                "private worksheet entry {}.{} needs a signature outside the scalar file-project carriers (dc-snz.15)",
                procedure.module, procedure.name
            )));
        }
        let name = loop {
            let name = format!("DnaCalcWorksheetPrivate{serial}");
            serial += 1;
            // Check all source, including comments/strings, so a generated name
            // cannot shadow any existing identifier. Serial names are unique.
            if !source.contains(&name.to_lowercase()) {
                break name;
            }
        };
        let parameters = procedure
            .parameters
            .iter()
            .map(|p| {
                format!(
                    "{} {} As {}",
                    if p.by_ref { "ByRef" } else { "ByVal" },
                    p.name,
                    type_name(&p.ty)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let arguments = procedure
            .parameters
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let result = type_name(
            procedure
                .return_type
                .as_ref()
                .expect("checked function result"),
        );
        let entry = format!(
            "\nPublic Function {name}({parameters}) As {result}\n {name} = {}({arguments})\nEnd Function\n",
            procedure.name
        );
        generated += entry.len();
        if generated > 1_048_576 {
            return Err(Error::UnsupportedSignature(
                "private worksheet entries exceed the file-project source limit (dc-snz.15)".into(),
            ));
        }
        let module = manifest
            .modules
            .iter_mut()
            .find(|m| m.module_name == procedure.module)
            .ok_or_else(|| Error::Compile("private entry module disappeared".into()))?;
        module.source.push_str(&entry);
        bridges.insert((procedure.module.clone(), name), procedure.clone());
    }
    Ok(bridges)
}
