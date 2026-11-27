use crate::{Session, trial::invalid};
use calc_skin::{
    Capability, FormulaAnswer, FormulaOperation, ProviderCatalog, ProviderFunction,
    ProviderOperation, Refusal,
};

pub fn providers(session: &Session) -> ProviderCatalog {
    let descriptors = session.user_function_descriptors();
    let functions = session
        .user_function_names()
        .into_iter()
        .map(|name| {
            match descriptors
                .iter()
                .find(|item| item.name.eq_ignore_ascii_case(&name))
            {
                Some(item) => ProviderFunction {
                    name,
                    origin: item.origin.clone(),
                    signature: item.signature(),
                    description: item.description.clone(),
                },
                None => ProviderFunction {
                    signature: name.clone(),
                    name,
                    origin: String::new(),
                    description: String::new(),
                },
            }
        })
        .collect();
    ProviderCatalog {
        functions,
        vba_loaded: session.has_vba_module(),
        ..Default::default()
    }
}

pub fn operate(
    session: &mut Session,
    operation: FormulaOperation,
) -> Result<FormulaAnswer, Refusal> {
    match operation {
        FormulaOperation::Provider(ProviderOperation::LoadVba { name, source }) => {
            session
                .load_vba_module(&name, &source, calc_vba::DEFAULT_STEP_BUDGET)
                .map_err(invalid)?;
        }
        FormulaOperation::Provider(ProviderOperation::UnloadVba) => session.unload_vba_module(),
        FormulaOperation::Provider(_) => {
            return Err(Refusal::NotAvailableHere {
                capability: Capability::NativeAddins,
                route: Some("https://dnacalc.com/downloads/".into()),
            });
        }
    }
    Ok(FormulaAnswer::Providers(providers(session)))
}
