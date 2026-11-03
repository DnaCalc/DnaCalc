//! DNA runtime composition. Procedure metadata projection follows OxVba 325257ea
//! crates/oxvba-embed/src/provider.rs; runtime code and session checks stay there.
use super::*;
use calcvba_host_api as api;
use oxvba_com::ComInterfaceIid;
pub(super) fn handle(
    handle: &api::BoundProcedure,
) -> Result<&oxvba_embed::BoundProcedure, api::Error> {
    let bound = handle
        .provider_payload::<oxvba_embed::BoundProcedure>()
        .ok_or(api::Error::StaleHandle)?;
    if bound.generation() != handle.generation() {
        return Err(api::Error::StaleHandle);
    }
    Ok(bound)
}
fn error(value: oxvba_embed::Error) -> api::Error {
    match value {
        oxvba_embed::Error::Compile(s) => api::Error::Compile(s),
        oxvba_embed::Error::StaleHandle => api::Error::StaleHandle,
        oxvba_embed::Error::ArgumentMismatch => api::Error::ArgumentMismatch,
        oxvba_embed::Error::BudgetExceeded => api::Error::BudgetExceeded,
        oxvba_embed::Error::Reentry => api::Error::Reentry,
        oxvba_embed::Error::UnsupportedSignature(s) => api::Error::UnsupportedSignature(s),
        oxvba_embed::Error::UnsupportedBackend(s) => api::Error::UnsupportedBackend(s),
        oxvba_embed::Error::BackendUnavailable => api::Error::BackendUnavailable,
        oxvba_embed::Error::Runtime {
            number,
            description,
            vba_source,
        } => api::Error::Runtime {
            number,
            description,
            vba_source,
        },
        other => api::Error::Backend(other.to_string()),
    }
}
pub(super) fn adapter_error(value: api::Error) -> Error {
    match value {
        api::Error::BudgetExceeded => Error::BudgetExceeded,
        api::Error::Runtime {
            number,
            description,
            vba_source,
        } => Error::Runtime {
            number,
            description,
            vba_source,
        },
        other => Error::Backend(other.to_string()),
    }
}
impl api::Session for OwnedSession {
    fn attach_document(&self, module: &str, source: &api::HostObject) -> Result<(), api::Error> {
        self.inner.attach_document(module, source).map_err(error)
    }
    fn procedures(&self) -> &[api::Procedure] {
        &self.catalogue
    }
    fn bind(
        &self,
        module: &str,
        name: &str,
        kind: api::ProcedureKind,
    ) -> Result<api::BoundProcedure, api::Error> {
        let kind = match kind {
            api::ProcedureKind::Sub => oxvba_embed::ProcedureKind::Sub,
            api::ProcedureKind::Function => oxvba_embed::ProcedureKind::Function,
            api::ProcedureKind::PropertyGet => oxvba_embed::ProcedureKind::PropertyGet,
            api::ProcedureKind::PropertyLet => oxvba_embed::ProcedureKind::PropertyLet,
            api::ProcedureKind::PropertySet => oxvba_embed::ProcedureKind::PropertySet,
        };
        let bound = self.inner.bind(module, name, kind).map_err(error)?;
        Ok(api::BoundProcedure::from_provider(
            bound.clone(),
            bound.generation(),
        ))
    }
    fn call_bound(
        &self,
        bound: &api::BoundProcedure,
        args: &[api::Variant],
        max_steps: u64,
    ) -> Result<api::Variant, api::Error> {
        self.inner
            .call_bound(handle(bound)?, args, max_steps)
            .map_err(error)
    }
    fn bind_slots(
        &self,
        module: &str,
        name: &str,
        arguments: &[api::ScalarType],
        result: api::ScalarType,
    ) -> Result<api::BoundProcedure, api::Error> {
        let bound = api::Session::bind(self, module, name, api::ProcedureKind::Function)?;
        self.type_bound_slots(&bound, arguments, result)
    }
    fn type_bound_slots(
        &self,
        bound: &api::BoundProcedure,
        arguments: &[api::ScalarType],
        result: api::ScalarType,
    ) -> Result<api::BoundProcedure, api::Error> {
        self.inner
            .bind_slots(handle(bound)?, arguments, result)
            .map_err(error)?;
        Ok(bound.clone())
    }
    fn call_slots(
        &self,
        bound: &api::BoundProcedure,
        args: &[api::Slot<'_>],
        context: &mut api::TypedContext,
    ) -> Result<api::ScalarBits, api::Error> {
        self.inner
            .call_slots(handle(bound)?, args, context)
            .map_err(error)
    }
    fn begin_invocation(
        &self,
        bound: &api::BoundProcedure,
        args: &[api::Slot<'_>],
        frame: api::InvocationFrame,
        max_blocks: u64,
    ) -> Result<Box<dyn api::Invocation>, api::Error> {
        self.inner
            .begin_invocation(handle(bound)?, args, frame, max_blocks)
            .map_err(error)
    }
    fn reset(&self) -> Result<(), api::Error> {
        self.inner.reset().map_err(error)
    }
    fn reload(&mut self, project: &api::Project) -> Result<(), api::Error> {
        let [module] = project.modules.as_slice() else {
            return Err(api::Error::UnsupportedSignature(
                "DNA module registration expects one source module".into(),
            ));
        };
        if !project.references.is_empty() || !project.libraries.is_empty() {
            return Err(api::Error::UnsupportedSignature(
                "external library replacement requires a prepared DNA project".into(),
            ));
        }
        let module = Module::load_spreadsheet(&module.name, &module.source)
            .map_err(|e| api::Error::Compile(e.to_string()))?;
        let (image, functions, spreadsheet, event_project) = split_module(module);
        self.inner.reload(image).map_err(error)?;
        self.catalogue = catalogue(&self.inner);
        self.bound = bind_functions(&functions, |p| self.inner.bind(&p.module, &p.name, p.kind))
            .map_err(|e| api::Error::Backend(e.to_string()))?
            .into_iter()
            .map(|bound| api::BoundProcedure::from_provider(bound.clone(), bound.generation()))
            .collect();
        self.typed = super::typed::entries(&self.inner, &functions, &self.bound);
        self.functions = functions;
        self.spreadsheet = spreadsheet;
        self.event_project = event_project;
        Ok(())
    }
}
pub(super) fn catalogue(session: &oxvba_embed::OwnedSession) -> Vec<api::Procedure> {
    session
        .procedures()
        .map(|p| api::Procedure {
            project: p.project.clone(),
            module: p.module.clone(),
            name: p.name.clone(),
            kind: match p.kind {
                oxvba_embed::ProcedureKind::Sub => api::ProcedureKind::Sub,
                oxvba_embed::ProcedureKind::Function => api::ProcedureKind::Function,
                oxvba_embed::ProcedureKind::PropertyGet => api::ProcedureKind::PropertyGet,
                oxvba_embed::ProcedureKind::PropertyLet => api::ProcedureKind::PropertyLet,
                oxvba_embed::ProcedureKind::PropertySet => api::ProcedureKind::PropertySet,
            },
            visibility: match p.visibility {
                oxvba_embed::Visibility::Public => api::Visibility::Public,
                oxvba_embed::Visibility::Private => api::Visibility::Private,
                oxvba_embed::Visibility::Friend => api::Visibility::Friend,
            },
            option_private_module: p.option_private_module,
            parameters: p
                .parameters
                .iter()
                .map(|p| api::Parameter {
                    name: p.name.clone(),
                    ty: ty(&p.ty, session),
                    by_ref: p.by_ref,
                    optional: p.optional,
                    param_array: p.param_array,
                    default: p.default.as_ref().map(default_value),
                    array_element: p.array_element.as_ref().map(array_element),
                })
                .collect(),
            return_type: p.return_type.as_ref().map(|t| ty(t, session)),
        })
        .collect()
}
fn ty(value: &oxvba_oxir::OxTy, session: &oxvba_embed::OwnedSession) -> api::Type {
    use {api::Type as T, oxvba_oxir::OxTy as O};
    match value {
        O::Variant => T::Variant,
        O::Bool => T::Boolean,
        O::Byte => T::Byte,
        O::Integer => T::Integer,
        O::Long => T::Long,
        O::LongLong => T::LongLong,
        O::Single => T::Single,
        O::Double => T::Double,
        O::Currency => T::Currency,
        O::Date => T::Date,
        O::Decimal => T::Decimal,
        O::Str => T::String,
        O::FixedStr(n) => T::FixedString(*n),
        O::ProcRef => T::Procedure,
        O::Object(oxvba_oxir::ObjClass::ComIface(i) | oxvba_oxir::ObjClass::Iface(i)) => {
            let interface = session.com_interfaces().get(i.0);
            T::Object {
                name: interface.map(|i| i.name().to_owned()),
                iid: interface.and_then(|i| match i {
                    oxvba_oxir::ComInterface::Com(meta) => meta.iid.map(interface_id),
                    _ => None,
                }),
            }
        }
        O::Object(_) => T::Object {
            name: None,
            iid: None,
        },
        O::Record(id) => T::Record(format!("record#{}", id.0)),
        O::Array(element, shape) => T::Array {
            element: Box::new(ty(element, session)),
            rank: match shape {
                oxvba_oxir::ArrayShape::Dynamic => None,
                oxvba_oxir::ArrayShape::Fixed { rank } => Some(*rank as usize),
            },
        },
    }
}
fn array_element(value: &oxvba_bundle::ArrayElementType) -> api::ArrayElement {
    use {
        api::{ArrayElement as A, Type as T},
        oxvba_bundle::ArrayElementType as E,
    };
    let scalar = match value {
        E::Variant => T::Variant,
        E::Integer => T::Integer,
        E::Long => T::Long,
        E::LongLong => T::LongLong,
        E::Byte => T::Byte,
        E::Single => T::Single,
        E::Double => T::Double,
        E::Currency => T::Currency,
        E::Date => T::Date,
        E::String => T::String,
        E::FixedString(n) => T::FixedString(*n as u32),
        E::Boolean => T::Boolean,
        E::Record(fields) => return A::Record(fields.iter().map(array_element).collect()),
        E::FixedArray { element, bounds } => {
            return A::FixedArray {
                element: Box::new(array_element(element)),
                bounds: bounds.iter().map(|b| (b.lower, b.len)).collect(),
            };
        }
    };
    A::Scalar(scalar)
}
fn default_value(value: &oxvba_bundle::coreir::CoreConst) -> api::Variant {
    use {api::Variant as V, oxvba_bundle::coreir::CoreConst as C};
    match value {
        C::Empty => V::empty(),
        C::Null => V::null(),
        C::Nothing => V::nothing(),
        C::Bool(b) => V::from_bool(*b),
        C::I16(n) => V::from_i16(*n),
        C::I32(n) => V::from_i32(*n),
        C::I64(n) => V::from_i64(*n),
        C::F64(n) => V::from_f64(f64::from_bits(*n)),
        C::F32(n) => V::from_f32(f32::from_bits(*n)),
        C::Currency(n) => V::from_currency_scaled_i64(*n),
        C::Date(n) => V::from_date_f64(f64::from_bits(*n)),
        C::Str(s) => V::from_string(s.clone()),
    }
}
fn interface_id(iid: ComInterfaceIid) -> api::InterfaceId {
    let mut bytes = [0; 16];
    bytes[..4].copy_from_slice(&iid.data1.to_le_bytes());
    bytes[4..6].copy_from_slice(&iid.data2.to_le_bytes());
    bytes[6..8].copy_from_slice(&iid.data3.to_le_bytes());
    bytes[8..].copy_from_slice(&iid.data4);
    bytes
}
