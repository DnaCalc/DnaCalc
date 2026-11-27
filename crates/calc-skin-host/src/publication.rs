//! Bounded binary window publications over the skin carrier.

use calc_skin::grid::{Area, Sheet};
use calc_skin::tiles::{MAX_TILE_BYTES, MAX_TILE_RECORDS, TileBatch, TileStamp, encode_tiles};
use calc_skin::{CellWindow, Envelope, Message, Packet, Stamp, WatchId, WorkbookCell};

pub(crate) fn window(
    stamp: Stamp,
    watch: WatchId,
    sequence: u64,
    window: CellWindow,
    remaining: usize,
) -> Result<(Vec<Packet>, usize), String> {
    let tile = TileStamp {
        id: watch.0,
        workbook: stamp.workbook.0,
        transaction: sequence,
        base: sequence - 1,
        next: sequence,
        revision: stamp.model.0,
        geometry_version: sequence,
        style_version: sequence,
        dictionary: sequence,
        reset: true,
        calculation_through: None,
    };
    let mut fragment = TileBatch {
        stamp: tile,
        fragment: 0,
        final_fragment: false,
        geometry: window
            .areas
            .into_iter()
            .map(|area| (Sheet(area.0.0, area.0.1), area))
            .collect(),
        cells: Vec::new(),
        removed: Vec::new(),
    };
    let mut packets = Vec::new();
    let mut total = 0usize;
    let mut push = |part: &TileBatch<WorkbookCell, Area>| -> Result<(), String> {
        let bytes = encode_tiles(part)?;
        total = total
            .saturating_add(bytes.len())
            .saturating_add(part.retained_bytes());
        if total > remaining {
            return Err("The worksheet window exceeds its delivery budget".into());
        }
        packets.push(Packet {
            envelope: Envelope::new(0, Message::Tile(tile)),
            bytes: Some(bytes),
        });
        Ok(())
    };
    for cell in window.cells {
        fragment.cells.push(cell);
        if fragment.cells.len() > MAX_TILE_RECORDS || fragment.retained_bytes() > MAX_TILE_BYTES {
            let last = fragment
                .cells
                .pop()
                .expect("this fragment just received a cell");
            push(&fragment)?;
            fragment.fragment += 1;
            fragment.geometry.clear();
            fragment.cells.clear();
            fragment.cells.push(last);
        }
    }
    fragment.final_fragment = true;
    push(&fragment)?;
    Ok((packets, total))
}
