//! The capabilities of the wire codec, declared once in the `capabilities!` invocation below. Each
//! declaration gives a capability's name, its version, and each operation with its argument names
//! and types in wire order. From that one set of declarations the macros generate:
//!
//! - per capability, a crate-private enum with one variant per operation, the capability's
//!   [`OpCapability`], a writer that emits the envelope and the arguments in declaration order,
//!   and a reader that judges the version before the envelope's other members and then admits
//!   exactly the declared operations and argument names;
//! - for all of them, the list [`super::op_capabilities`] returns, the wire-operation enum with
//!   one variant per capability, its writer, and the envelope reader that dispatches on the
//!   capability name.
//!
//! So a declared capability is listed, written and read, with no second list to keep in step. The
//! hand-written conversions below map between these wire operations and [`WorkbookOp`], resolving
//! sheets through a [`ModelRoot`]; their matches are exhaustive, so a capability or an operation
//! without a conversion does not compile.

use super::json::{self, Fields, Node};
use super::value::WireValue;
use super::{DecodeError, EncodeError, OpCapability, check_version};
use crate::input::CellInput;
use crate::layout::{BlockId, BlockKind, PlacementId, SkinLayoutEdit, Spot};
use crate::op::{Axis, AxisChange, AxisEdit, SheetEdit, WorkbookOp};
use crate::root::{ModelRoot, SheetEntry};
use crate::sheet::{SheetKey, SheetName, SheetVisibility};
use calc_value::{CellKey, GridArea, SheetId};
use std::num::NonZeroU32;

/// The members of every operation envelope, in wire order.
const ENVELOPE_FIELDS: [&str; 4] = ["capability", "version", "op", "args"];

/// Writes one operation envelope: the capability's name and version, the operation's name, and
/// its arguments in the order the writer receives them.
struct ArgsWriter<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> ArgsWriter<'a> {
    fn begin(out: &'a mut String, declaration: &OpCapability, operation: &str) -> Self {
        out.push_str("{\"capability\":\"");
        out.push_str(declaration.name);
        out.push_str("\",\"version\":");
        json::push_unsigned(out, u64::from(declaration.version));
        out.push_str(",\"op\":\"");
        out.push_str(operation);
        out.push_str("\",\"args\":{");
        ArgsWriter { out, first: true }
    }

    fn field<T: WireValue>(&mut self, name: &str, value: &T) {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        self.out.push('"');
        self.out.push_str(name);
        self.out.push_str("\":");
        value.write(self.out);
    }

    fn end(self) {
        self.out.push_str("}}");
    }
}

fn read_field<T: WireValue>(args: &Fields<'_>, name: &str) -> Result<T, DecodeError> {
    T::read(args.node(name)?, &args.path(name))
}

fn unknown_operation(
    envelope: &Fields<'_>,
    capability: &'static str,
    operation: &str,
) -> DecodeError {
    DecodeError::UnknownOperation {
        at: envelope.path("op"),
        capability,
        operation: operation.to_owned(),
    }
}

/// The refusal of the envelope at `at`, whose capability no declaration names.
fn unknown_capability(at: &str, capability: &str) -> DecodeError {
    DecodeError::UnknownCapability {
        at: json::path(at, "capability"),
        capability: capability.to_owned(),
    }
}

/// One capability's codec, generated from its declaration. Only `capabilities!` invokes it.
macro_rules! capability {
    (
        $(#[$meta:meta])*
        $codec:ident = $name:literal, version $version:literal {
            $( $op:literal => $variant:ident { $( $field:ident: $ty:ty ),+ $(,)? } ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug)]
        pub(crate) enum $codec {
            $( $variant { $( $field: $ty ),+ }, )+
        }

        impl $codec {
            const NAME: &'static str = $name;

            const DECLARATION: OpCapability = OpCapability {
                name: $name,
                version: $version,
                operations: &[$($op),+],
            };

            fn write(&self, out: &mut String) {
                match self {
                    $(
                        $codec::$variant { $( $field ),+ } => {
                            let mut args = ArgsWriter::begin(out, &Self::DECLARATION, $op);
                            $( args.field(stringify!($field), $field); )+
                            args.end();
                        }
                    )+
                }
            }

            /// Reads the envelope at `at`, whose capability is this one: its version first, then
            /// its members, its operation and that operation's arguments.
            fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
                check_version(
                    &json::path(at, "version"),
                    $name,
                    json::lone_unsigned(node, at, "version")?,
                    $version,
                )?;
                let envelope = Fields::new(node, at, &ENVELOPE_FIELDS)?;
                let operation = envelope.text("op")?;
                let args_at = envelope.path("args");
                $(
                    if operation == $op {
                        let args = Fields::new(
                            envelope.node("args")?,
                            &args_at,
                            &[$( stringify!($field) ),+],
                        )?;
                        return Ok($codec::$variant {
                            $( $field: read_field(&args, stringify!($field))?, )+
                        });
                    }
                )+
                Err(unknown_operation(&envelope, $name, operation))
            }
        }
    };
}

/// Every capability's codec, plus the list, the wire-operation enum and the envelope dispatch,
/// all generated from the same declarations.
macro_rules! capabilities {
    (
        $(
            $(#[$meta:meta])*
            $codec:ident = $name:literal, version $version:literal { $( $body:tt )* }
        )+
    ) => {
        $(
            capability! {
                $(#[$meta])*
                $codec = $name, version $version { $( $body )* }
            }
        )+

        /// Every declared capability, in declaration order.
        pub(super) const CAPABILITIES: &[OpCapability] = &[$( $codec::DECLARATION ),+];

        /// One operation in its capability's wire form.
        pub(super) enum WireOp {
            $( $codec($codec), )+
        }

        impl WireOp {
            pub(super) fn write(&self, out: &mut String) {
                match self {
                    $( WireOp::$codec(operation) => operation.write(out), )+
                }
            }
        }

        /// The operation an envelope holds, read by the codec of the capability it names. The
        /// capability is judged before the envelope's other members.
        pub(super) fn read_envelope(node: &Node, at: &str) -> Result<WireOp, DecodeError> {
            let capability = json::lone_text(node, at, "capability")?;
            $(
                if capability == $codec::NAME {
                    return $codec::read(node, at).map(WireOp::$codec);
                }
            )+
            Err(unknown_capability(at, capability))
        }
    };
}

capabilities! {
    /// Cell contents: set one cell's authored input; clear the authored contents of an area.
    Cells = "cells", version 1 {
        "set_cell" => SetCell { sheet: SheetKey, row: u32, column: u32, input: CellInput },
        "clear" => Clear { sheet: SheetKey, top: u32, left: u32, bottom: u32, right: u32 },
    }

    /// Rows and columns: insert or delete positions of one sheet.
    Axes = "axes", version 1 {
        "insert_rows" => InsertRows { sheet: SheetKey, first: u32, count: u32 },
        "delete_rows" => DeleteRows { sheet: SheetKey, first: u32, count: u32 },
        "insert_columns" => InsertColumns { sheet: SheetKey, first: u32, count: u32 },
        "delete_columns" => DeleteColumns { sheet: SheetKey, first: u32, count: u32 },
    }

    /// Sheets: insert, rename, move, remove, and set visibility.
    Sheets = "sheets", version 1 {
        "insert" => Insert { key: SheetKey, name: SheetName, position: NonZeroU32 },
        "rename" => Rename { sheet: SheetKey, name: SheetName },
        "move" => Move { sheet: SheetKey, position: NonZeroU32 },
        "remove" => Remove { sheet: SheetKey },
        "set_visibility" => SetVisibility { sheet: SheetKey, visibility: SheetVisibility },
    }

    /// Skin layout: create or remove a block over a sheet's cells, with its locator; add, move or
    /// remove a block's placement in the notebook, tree, canvas or grid view.
    Layouts = "skin_layout", version 1 {
        "create_block" => CreateBlock {
            block: BlockId,
            kind: BlockKind,
            sheet: SheetKey,
            top: u32,
            left: u32,
            bottom: u32,
            right: u32,
        },
        "remove_block" => RemoveBlock { block: BlockId },
        "add_placement" => AddPlacement { placement: PlacementId, block: BlockId, spot: Spot },
        "move_placement" => MovePlacement { placement: PlacementId, spot: Spot },
        "remove_placement" => RemovePlacement { placement: PlacementId },
    }
}

/// The key `root` gives the sheet `id`.
fn key_of(root: &ModelRoot, id: SheetId) -> Result<SheetKey, EncodeError> {
    root.sheet_by_id(id)
        .map(SheetEntry::key)
        .ok_or(EncodeError::UnknownSheet { sheet: id })
}

/// Records an inserted sheet's key; refused when `root` or an earlier insert holds it.
fn claim_for_encode(
    root: &ModelRoot,
    claimed: &mut Vec<SheetKey>,
    key: SheetKey,
) -> Result<(), EncodeError> {
    if root.sheet_by_key(key).is_some() || claimed.contains(&key) {
        return Err(EncodeError::SheetKeyInUse { key });
    }
    claimed.push(key);
    Ok(())
}

/// The sheet `root` holds under `key`. Only `root` is consulted, not the batch's earlier
/// operations: a key an earlier insert of the batch claimed is unknown here, and a key whose sheet
/// an earlier remove of the batch names still resolves. The applier checks each operation against
/// the state the earlier ones leave.
fn resolve(root: &ModelRoot, key: SheetKey, at: String) -> Result<SheetId, DecodeError> {
    root.sheet_by_key(key)
        .map(SheetEntry::id)
        .ok_or(DecodeError::UnknownSheetKey { at, key })
}

/// Records an inserted sheet's key; refused when `root` or an earlier insert holds it. A key that
/// a removed sheet held is not known here, because `root` keeps no record of it; the applier
/// checks that a key is never reused.
fn claim_for_decode(
    root: &ModelRoot,
    claimed: &mut Vec<SheetKey>,
    key: SheetKey,
    at: String,
) -> Result<(), DecodeError> {
    if root.sheet_by_key(key).is_some() || claimed.contains(&key) {
        return Err(DecodeError::SheetKeyInUse { at, key });
    }
    claimed.push(key);
    Ok(())
}

fn cell_key(sheet: SheetId, row: u32, column: u32, at: &str) -> Result<CellKey, DecodeError> {
    CellKey::new(sheet, row, column).map_err(|error| DecodeError::InvalidCell {
        at: at.to_owned(),
        error,
    })
}

/// The area from (`top`, `left`) to (`bottom`, `right`); refused when the corners are inverted,
/// so that each area has one wire form.
fn area(
    sheet: SheetId,
    (top, left, bottom, right): (u32, u32, u32, u32),
    at: &str,
) -> Result<GridArea, DecodeError> {
    if top > bottom || left > right {
        return Err(DecodeError::InvertedArea { at: at.to_owned() });
    }
    let first = cell_key(sheet, top, left, at)?;
    let last = cell_key(sheet, bottom, right, at)?;
    GridArea::new(first, last).map_err(|error| DecodeError::InvalidCell {
        at: at.to_owned(),
        error,
    })
}

fn axis_edit(
    sheet: SheetId,
    (axis, change): (Axis, AxisChange),
    first: u32,
    count: u32,
    at: &str,
) -> Result<AxisEdit, DecodeError> {
    AxisEdit::new(sheet, axis, change, first, count).map_err(|error| DecodeError::InvalidAxisEdit {
        at: at.to_owned(),
        error,
    })
}

impl Axes {
    fn from_edit(edit: AxisEdit, sheet: SheetKey) -> Self {
        let (first, count) = (edit.first(), edit.count());
        match (edit.axis(), edit.change()) {
            (Axis::Rows, AxisChange::Insert) => Axes::InsertRows {
                sheet,
                first,
                count,
            },
            (Axis::Rows, AxisChange::Delete) => Axes::DeleteRows {
                sheet,
                first,
                count,
            },
            (Axis::Columns, AxisChange::Insert) => Axes::InsertColumns {
                sheet,
                first,
                count,
            },
            (Axis::Columns, AxisChange::Delete) => Axes::DeleteColumns {
                sheet,
                first,
                count,
            },
        }
    }

    fn into_edit(self, root: &ModelRoot, at: &str) -> Result<AxisEdit, DecodeError> {
        let (sheet, kind, first, count) = match self {
            Axes::InsertRows {
                sheet,
                first,
                count,
            } => (sheet, (Axis::Rows, AxisChange::Insert), first, count),
            Axes::DeleteRows {
                sheet,
                first,
                count,
            } => (sheet, (Axis::Rows, AxisChange::Delete), first, count),
            Axes::InsertColumns {
                sheet,
                first,
                count,
            } => (sheet, (Axis::Columns, AxisChange::Insert), first, count),
            Axes::DeleteColumns {
                sheet,
                first,
                count,
            } => (sheet, (Axis::Columns, AxisChange::Delete), first, count),
        };
        let id = resolve(root, sheet, format!("{at}.sheet"))?;
        axis_edit(id, kind, first, count, at)
    }
}

impl Sheets {
    fn from_edit(
        edit: &SheetEdit,
        root: &ModelRoot,
        claimed: &mut Vec<SheetKey>,
    ) -> Result<Self, EncodeError> {
        Ok(match edit {
            SheetEdit::Insert {
                key,
                name,
                position,
            } => {
                claim_for_encode(root, claimed, *key)?;
                Sheets::Insert {
                    key: *key,
                    name: name.clone(),
                    position: *position,
                }
            }
            SheetEdit::Rename { sheet, name } => Sheets::Rename {
                sheet: key_of(root, *sheet)?,
                name: name.clone(),
            },
            SheetEdit::Move { sheet, position } => Sheets::Move {
                sheet: key_of(root, *sheet)?,
                position: *position,
            },
            SheetEdit::Remove { sheet } => Sheets::Remove {
                sheet: key_of(root, *sheet)?,
            },
            SheetEdit::SetVisibility { sheet, visibility } => Sheets::SetVisibility {
                sheet: key_of(root, *sheet)?,
                visibility: *visibility,
            },
        })
    }

    fn into_edit(
        self,
        root: &ModelRoot,
        claimed: &mut Vec<SheetKey>,
        at: &str,
    ) -> Result<SheetEdit, DecodeError> {
        let sheet_at = || format!("{at}.sheet");
        Ok(match self {
            Sheets::Insert {
                key,
                name,
                position,
            } => {
                claim_for_decode(root, claimed, key, format!("{at}.key"))?;
                SheetEdit::Insert {
                    key,
                    name,
                    position,
                }
            }
            Sheets::Rename { sheet, name } => SheetEdit::Rename {
                sheet: resolve(root, sheet, sheet_at())?,
                name,
            },
            Sheets::Move { sheet, position } => SheetEdit::Move {
                sheet: resolve(root, sheet, sheet_at())?,
                position,
            },
            Sheets::Remove { sheet } => SheetEdit::Remove {
                sheet: resolve(root, sheet, sheet_at())?,
            },
            Sheets::SetVisibility { sheet, visibility } => SheetEdit::SetVisibility {
                sheet: resolve(root, sheet, sheet_at())?,
                visibility,
            },
        })
    }
}

impl Layouts {
    fn from_edit(edit: &SkinLayoutEdit, root: &ModelRoot) -> Result<Self, EncodeError> {
        Ok(match *edit {
            SkinLayoutEdit::CreateBlock {
                block,
                kind,
                anchor,
            } => {
                let (first, last) = (anchor.first(), anchor.last());
                Layouts::CreateBlock {
                    block,
                    kind,
                    sheet: key_of(root, anchor.sheet())?,
                    top: first.row(),
                    left: first.column(),
                    bottom: last.row(),
                    right: last.column(),
                }
            }
            SkinLayoutEdit::RemoveBlock { block } => Layouts::RemoveBlock { block },
            SkinLayoutEdit::AddPlacement {
                placement,
                block,
                spot,
            } => Layouts::AddPlacement {
                placement,
                block,
                spot,
            },
            SkinLayoutEdit::MovePlacement { placement, spot } => {
                Layouts::MovePlacement { placement, spot }
            }
            SkinLayoutEdit::RemovePlacement { placement } => Layouts::RemovePlacement { placement },
        })
    }

    fn into_edit(self, root: &ModelRoot, at: &str) -> Result<SkinLayoutEdit, DecodeError> {
        Ok(match self {
            Layouts::CreateBlock {
                block,
                kind,
                sheet,
                top,
                left,
                bottom,
                right,
            } => {
                let sheet = resolve(root, sheet, format!("{at}.sheet"))?;
                SkinLayoutEdit::CreateBlock {
                    block,
                    kind,
                    anchor: area(sheet, (top, left, bottom, right), at)?,
                }
            }
            Layouts::RemoveBlock { block } => SkinLayoutEdit::RemoveBlock { block },
            Layouts::AddPlacement {
                placement,
                block,
                spot,
            } => SkinLayoutEdit::AddPlacement {
                placement,
                block,
                spot,
            },
            Layouts::MovePlacement { placement, spot } => {
                SkinLayoutEdit::MovePlacement { placement, spot }
            }
            Layouts::RemovePlacement { placement } => SkinLayoutEdit::RemovePlacement { placement },
        })
    }
}

impl WireOp {
    /// The wire form of `operation`, with each sheet written as its key in `root`. `claimed`
    /// holds the keys the batch's earlier inserts took.
    pub(super) fn from_op(
        operation: &WorkbookOp,
        root: &ModelRoot,
        claimed: &mut Vec<SheetKey>,
    ) -> Result<Self, EncodeError> {
        Ok(match operation {
            WorkbookOp::SetCell { at, input } => WireOp::Cells(Cells::SetCell {
                sheet: key_of(root, at.sheet())?,
                row: at.row(),
                column: at.column(),
                input: input.clone(),
            }),
            WorkbookOp::Clear { area } => {
                let (first, last) = (area.first(), area.last());
                WireOp::Cells(Cells::Clear {
                    sheet: key_of(root, area.sheet())?,
                    top: first.row(),
                    left: first.column(),
                    bottom: last.row(),
                    right: last.column(),
                })
            }
            WorkbookOp::EditAxis(edit) => {
                WireOp::Axes(Axes::from_edit(*edit, key_of(root, edit.sheet())?))
            }
            WorkbookOp::EditSheet(edit) => WireOp::Sheets(Sheets::from_edit(edit, root, claimed)?),
            WorkbookOp::EditSkinLayout(edit) => WireOp::Layouts(Layouts::from_edit(edit, root)?),
            WorkbookOp::SetArray { .. } => {
                unreachable!("encode_batch refuses an array formula before it reaches here")
            }
        })
    }

    /// The operation this wire form holds, with each sheet key resolved through `root`. `at` is
    /// the envelope's path; `claimed` holds the keys the batch's earlier inserts took.
    pub(super) fn into_op(
        self,
        root: &ModelRoot,
        claimed: &mut Vec<SheetKey>,
        at: &str,
    ) -> Result<WorkbookOp, DecodeError> {
        let args = format!("{at}.args");
        match self {
            WireOp::Cells(Cells::SetCell {
                sheet,
                row,
                column,
                input,
            }) => {
                let sheet = resolve(root, sheet, format!("{args}.sheet"))?;
                Ok(WorkbookOp::SetCell {
                    at: cell_key(sheet, row, column, &args)?,
                    input,
                })
            }
            WireOp::Cells(Cells::Clear {
                sheet,
                top,
                left,
                bottom,
                right,
            }) => {
                let sheet = resolve(root, sheet, format!("{args}.sheet"))?;
                Ok(WorkbookOp::Clear {
                    area: area(sheet, (top, left, bottom, right), &args)?,
                })
            }
            WireOp::Axes(operation) => operation.into_edit(root, &args).map(WorkbookOp::EditAxis),
            WireOp::Sheets(operation) => operation
                .into_edit(root, claimed, &args)
                .map(WorkbookOp::EditSheet),
            WireOp::Layouts(operation) => operation
                .into_edit(root, &args)
                .map(WorkbookOp::EditSkinLayout),
        }
    }
}
