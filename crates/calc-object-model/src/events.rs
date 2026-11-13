//! Events: the core raises them through the projection that owns the connections.

use crate::model::AutoError;
use crate::object::ObjectRef;
use crate::value::AutoValue;

/// Implemented by a projection (the COM connection points; CalcVBA's event route later). The
/// host raises an event once, from its one event source (calc-host's session events), and each
/// projection delivers it to its own subscribers with the descriptor's DISPIDs.
pub trait EventSource {
    /// Raise `event` of `interface` (for example `AppEvents`, `SheetChange`) on `source`'s
    /// subscribers, with arguments in the event's declared order.
    /// Object arguments each represent one acquisition. Materialize one wrapper per argument
    /// for the delivery (even with no subscribers), and retain that wrapper across subscribers.
    /// Its final release calls ObjectModel::released exactly once, including on failed delivery.
    fn raise(
        &self,
        source: &ObjectRef,
        interface: &str,
        event: &str,
        args: &[AutoValue],
    ) -> Result<(), AutoError>;
}
