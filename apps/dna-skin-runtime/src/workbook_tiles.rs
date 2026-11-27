//! Join binary fragments and control updates before publishing a workbook frame.

use calc_skin::grid::Area;
use calc_skin::tiles::{MAX_PUBLICATION_BYTES, MAX_TILE_CELLS, TileBatch, TileStamp, decode_tiles};
use calc_skin::{CellWindow, Frame, Refusal, Update, WatchId, WorkbookCell};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct TileFrames {
    publications: BTreeMap<WatchId, Received>,
    retained: usize,
}

struct Received {
    stamp: TileStamp,
    next: u32,
    sealed: bool,
    window: CellWindow,
}

impl TileFrames {
    pub fn fragment(&mut self, stamp: TileStamp, bytes: &[u8]) -> Result<(), Refusal> {
        let batch: TileBatch<WorkbookCell, Area> =
            decode_tiles(bytes).map_err(Refusal::Transport)?;
        if stamp != batch.stamp || !stamp.reset || !batch.removed.is_empty() {
            return Err(Refusal::Transport("Inconsistent tile window".into()));
        }
        self.retained = self
            .retained
            .saturating_add(batch.retained_bytes())
            .saturating_add(bytes.len());
        if self.retained > MAX_PUBLICATION_BYTES {
            self.clear();
            return Err(Refusal::Transport(
                "Tile frame exceeds its byte limit".into(),
            ));
        }
        let stream = self
            .publications
            .entry(WatchId(stamp.id))
            .or_insert_with(|| Received {
                stamp,
                next: 0,
                sealed: false,
                window: CellWindow {
                    areas: Vec::new(),
                    cells: Vec::new(),
                },
            });
        if stream.stamp != stamp
            || stream.sealed
            || stream.next != batch.fragment
            || (stream.next != 0 && !batch.geometry.is_empty())
        {
            self.clear();
            return Err(Refusal::Transport(
                "Tile fragments are out of sequence".into(),
            ));
        }
        if stream.window.cells.len().saturating_add(batch.cells.len()) > MAX_TILE_CELLS {
            self.clear();
            return Err(Refusal::Transport(
                "Tile frame exceeds its cell limit".into(),
            ));
        }
        stream.next += 1;
        stream.sealed = batch.final_fragment;
        stream
            .window
            .areas
            .extend(batch.geometry.into_iter().map(|(_, area)| area));
        stream.window.cells.extend(batch.cells);
        Ok(())
    }

    fn clear(&mut self) {
        self.publications.clear();
        self.retained = 0;
    }

    pub fn finish(&mut self, mut frame: Frame) -> Result<(Frame, Option<u64>), Refusal> {
        let mut publications = std::mem::take(&mut self.publications);
        self.retained = 0;
        let mut transaction = None;
        for update in &mut frame.updates {
            if let Update::Tiles {
                watch,
                coverage,
                barrier,
            } = update
            {
                let received = publications
                    .remove(watch)
                    .filter(|received| received.sealed && received.stamp.barrier() == *barrier)
                    .filter(|received| {
                        received.stamp.workbook == frame.stamp.workbook.0
                            && received.stamp.revision == frame.stamp.model.0
                    })
                    .ok_or_else(|| Refusal::Transport("Worksheet window is incomplete".into()))?;
                if transaction
                    .replace(barrier.transaction)
                    .is_some_and(|value| value != barrier.transaction)
                {
                    return Err(Refusal::Transport(
                        "Tile windows came from different frames".into(),
                    ));
                }
                *update = Update::Ready {
                    watch: *watch,
                    coverage: coverage.clone(),
                    body: serde_json::to_value(received.window)
                        .map_err(|error| Refusal::Transport(error.to_string()))?,
                };
            }
        }
        if !publications.is_empty() {
            return Err(Refusal::Transport("Tiles arrived without a frame".into()));
        }
        Ok((frame, transaction))
    }
}
