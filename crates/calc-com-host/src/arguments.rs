//! Bind DISPPARAMS positions to descriptor arguments before converting any client values.
use calc_object_model::{InvokeKind, Member, ParamFlag};

const PARAM_NOT_FOUND: i32 = 0x8002_0004u32 as i32;
const BAD_PARAM_COUNT: i32 = 0x8002_000Eu32 as i32;

/// Raw argument index of a binding error, for IDispatch::Invoke's puArgErr.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BindingError {
    pub code: i32,
    pub argument: usize,
}

pub(crate) fn bind(
    member: &Member,
    count: usize,
    named: &[i32],
) -> Result<Vec<usize>, BindingError> {
    let declared: Vec<_> = member.arguments().collect();
    let error = |code, argument| BindingError { code, argument };
    if count > declared.len() || named.len() > count {
        return Err(error(BAD_PARAM_COUNT, 0));
    }
    let mut slots = vec![0; count];
    let mut supplied = vec![false; declared.len()];
    for (raw, &name) in named.iter().enumerate() {
        let slot = if name == -3 && matches!(member.invoke, InvokeKind::Put | InvokeKind::Putref) {
            declared.len().checked_sub(1)
        } else {
            usize::try_from(name).ok().filter(|&n| n < declared.len())
        }
        .ok_or_else(|| error(PARAM_NOT_FOUND, raw))?;
        if supplied[slot] {
            return Err(error(PARAM_NOT_FOUND, raw));
        }
        slots[raw] = slot;
        supplied[slot] = true;
    }
    for (slot, raw) in (named.len()..count).rev().enumerate() {
        if supplied[slot] {
            return Err(error(PARAM_NOT_FOUND, raw));
        }
        slots[raw] = slot;
        supplied[slot] = true;
    }
    if declared.iter().zip(supplied).any(|(param, present)| {
        !present && !param.has(ParamFlag::Optional) && !param.has(ParamFlag::Hasdefault)
    }) {
        return Err(error(BAD_PARAM_COUNT, 0));
    }
    Ok(slots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_object_model::Descriptor;
    fn member(interface: &str, name: &str, kind: InvokeKind) -> &'static Member {
        let d = Descriptor::get();
        d.member(d.default_interface(d.id(interface).unwrap()), name, kind)
            .unwrap()
    }

    /// Native-only: the native Automation binder is tested without calling Windows APIs.
    #[test]
    fn positional_arguments_keep_their_declared_slots_with_named_arguments() {
        let run = member("Application", "Run", InvokeKind::Method);
        assert_eq!(bind(run, 3, &[2]).unwrap(), [2, 1, 0]);
        assert_eq!(bind(run, 3, &[]).unwrap(), [2, 1, 0]);
        // A named Macro plus one positional Macro is a duplicate, not an Arg1 value.
        assert_eq!(
            bind(run, 2, &[0]),
            Err(BindingError {
                code: PARAM_NOT_FOUND,
                argument: 1
            })
        );
        assert_eq!(
            bind(run, 3, &[2, 0]),
            Err(BindingError {
                code: PARAM_NOT_FOUND,
                argument: 2
            })
        );
    }

    /// Native-only: required arguments, named DISPIDs and property assignment use the native descriptor.
    #[test]
    fn binding_rejects_duplicates_unknown_names_and_missing_required_arguments() {
        let range = member("Application", "Range", InvokeKind::Get);
        for names in [&[0, 0][..], &[99][..], &[-3][..]] {
            assert_eq!(
                bind(range, names.len(), names).unwrap_err().code,
                PARAM_NOT_FOUND
            );
        }
        assert_eq!(bind(range, 1, &[1]).unwrap_err().code, BAD_PARAM_COUNT);
        assert_eq!(bind(range, 3, &[]).unwrap_err().code, BAD_PARAM_COUNT);
        assert_eq!(bind(range, 0, &[0]).unwrap_err().code, BAD_PARAM_COUNT);
        let put = member("Range", "Value2", InvokeKind::Put);
        assert_eq!(bind(put, 1, &[-3]).unwrap(), [0]);
        let value = member("Range", "Value", InvokeKind::Put);
        assert_eq!(bind(value, 2, &[-3]).unwrap(), [1, 0]);
    }
}
