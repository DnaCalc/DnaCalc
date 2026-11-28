//! Classic's dialog families, each a fill of the one dialog frame.

pub mod axis_size;
pub mod format_cells;
pub mod insert_delete;
pub mod number_format;

use crate::dialog::Family;
use crate::{Session, ViewCommand};
use calc_skin::ViewId;

/// Every family, in the order their commands register.
pub const FAMILIES: &[&Family] = &[
    &format_cells::FAMILY,
    &axis_size::ROWS,
    &axis_size::COLUMNS,
    &insert_delete::INSERT_FAMILY,
    &insert_delete::DELETE_FAMILY,
];

/// The commands of every family: each dialog's open command, OK and Cancel, and the commands
/// that reach a dialog another way (a named tab, or at once when no question is needed).
pub fn commands(view: &ViewId, session: Session) -> Vec<ViewCommand> {
    let mut commands = crate::dialog::commands(view, session, FAMILIES);
    commands.extend(format_cells::commands(view, session));
    commands.extend(insert_delete::commands(view, session));
    commands
}
