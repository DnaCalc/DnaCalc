//! Owner-side async return authority, shared by live and pinned calculations.
use crate::{EngineState, RecalcWork, contract::*};
use calc_doc_model::ModelRoot;
use calc_value::CalcValue;
use std::collections::BTreeMap;

#[derive(Debug)]
struct AsyncRecord {
    token: AsyncToken,
    returned: Option<CalcValue>,
    revoked: bool,
}

/// Retains identity and duplicate-return tombstones, never executable provider code.
#[derive(Debug, Default)]
pub struct AsyncReturns {
    records: BTreeMap<u64, AsyncRecord>,
}
impl AsyncReturns {
    pub fn absorb(&mut self, other: Self) -> Result<(), AsyncRejection> {
        for (id, incoming) in &other.records {
            if let Some(existing) = self.records.get(id) {
                if existing.token != incoming.token {
                    return Err(AsyncRejection::IdentityMismatch);
                }
                if let (Some(a), Some(b)) = (&existing.returned, &incoming.returned)
                    && a != b
                {
                    return Err(AsyncRejection::ConflictingDuplicate);
                }
            }
        }
        for (id, incoming) in other.records {
            self.records
                .entry(id)
                .and_modify(|existing| {
                    existing.revoked |= incoming.revoked;
                    if existing.returned.is_none() {
                        existing.returned = incoming.returned.clone();
                    }
                })
                .or_insert(incoming);
        }
        Ok(())
    }
    pub fn observe(&mut self, events: &[CalcEvent]) {
        for event in events {
            match event {
                CalcEvent::AwaitingAsync(token) => {
                    self.records.entry(token.id).or_insert_with(|| AsyncRecord {
                        token: token.clone(),
                        returned: None,
                        revoked: false,
                    });
                }
                CalcEvent::Superseded(receipt) | CalcEvent::Completed(receipt)
                    if receipt.outcome != CompletionOutcome::Complete =>
                {
                    for record in self.records.values_mut() {
                        if record.token.invocation.pass.command == receipt.command {
                            record.revoked = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    pub fn revoke(&mut self) {
        for record in self.records.values_mut() {
            record.revoked = true;
        }
    }
    /// Authentication and delivery only. An accepted return resumes on a later owner step.
    pub fn deliver(
        &mut self,
        state: &EngineState,
        inputs: &ModelRoot,
        work: Option<&mut RecalcWork>,
        token: &AsyncToken,
        value: CalcValue,
    ) -> AsyncDisposition {
        use AsyncDisposition as D;
        use AsyncRejection as R;
        match self.records.get_mut(&token.id) {
            None => D::Rejected(R::UnknownToken),
            Some(record) if record.token != *token => D::Rejected(R::IdentityMismatch),
            Some(record) if record.returned.is_some() => {
                if record.returned.as_ref() == Some(&value) {
                    D::Duplicate
                } else {
                    D::Rejected(R::ConflictingDuplicate)
                }
            }
            Some(record) if record.revoked => D::Rejected(R::Revoked),
            Some(record) => {
                if let Some(work) =
                    work.filter(|work| work.admission().command == token.invocation.pass.command)
                {
                    match work.accept_async(state, inputs, token, value.clone()) {
                        Ok(()) => {
                            record.returned = Some(value);
                            D::Accepted
                        }
                        Err(reason) => {
                            record.revoked = true;
                            D::Rejected(reason)
                        }
                    }
                } else {
                    record.revoked = true;
                    D::Rejected(R::Revoked)
                }
            }
        }
    }
}
