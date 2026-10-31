//! One worksheet projection, split into bounded frozen and scrolling rectangles.
use super::{
    geometry::{self, CellRect, GridMetrics, HitTarget, Viewport},
    render_plan::{self, RenderPlan},
    selection::GridExtent,
    viewport,
};
use crate::cells::CellShown;
use calc_host::value::{CellKey, GridArea, SheetId};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Panes {
    pub rows: u32,
    pub columns: u32,
}

impl Panes {
    pub fn size(self, m: &GridMetrics) -> (f64, f64) {
        (m.col_start(self.columns + 1), m.row_start(self.rows + 1))
    }
    fn view(self, v: &Viewport, frozen_row: bool, frozen_col: bool) -> Viewport {
        Viewport {
            scroll_x: if frozen_col { 0.0 } else { v.scroll_x },
            scroll_y: if frozen_row { 0.0 } else { v.scroll_y },
            ..*v
        }
    }
    fn x_bounds(self, m: &GridMetrics, v: &Viewport, frozen: bool) -> (f64, f64) {
        let edge = (m.header_w + self.size(m).0).min(v.width);
        if frozen {
            (m.header_w, edge)
        } else {
            (edge, v.width)
        }
    }
    fn y_bounds(self, m: &GridMetrics, v: &Viewport, frozen: bool) -> (f64, f64) {
        let edge = (m.header_h + self.size(m).1).min(v.height);
        if frozen {
            (m.header_h, edge)
        } else {
            (edge, v.height)
        }
    }
    pub fn bounds(self, m: &GridMetrics, v: &Viewport, row: u32, col: u32) -> CellRect {
        let (x, right) = self.x_bounds(m, v, col <= self.columns);
        let (y, bottom) = self.y_bounds(m, v, row <= self.rows);
        CellRect {
            x,
            y,
            w: (right - x).max(0.0),
            h: (bottom - y).max(0.0),
        }
    }
    pub fn rect(self, m: &GridMetrics, v: &Viewport, row: u32, col: u32) -> CellRect {
        geometry::cell_rect(
            m,
            &self.view(v, row <= self.rows, col <= self.columns),
            row,
            col,
        )
    }
    pub fn hit(
        self,
        m: &GridMetrics,
        v: &Viewport,
        rows: u32,
        cols: u32,
        x: f64,
        y: f64,
    ) -> HitTarget {
        let (width, height) = self.size(m);
        geometry::hit_test(
            m,
            &self.view(v, y < m.header_h + height, x < m.header_w + width),
            rows,
            cols,
            x,
            y,
        )
    }
    pub fn reveal(self, m: &GridMetrics, v: &Viewport, row: u32, col: u32) -> (f64, f64) {
        let (w, h) = self.size(m);
        let x = if col <= self.columns {
            v.scroll_x
        } else {
            viewport::reveal_span(
                m.col_start(col) - w,
                m.col_start(col) + m.width(col) - w,
                m.header_w + w,
                v.width,
                v.scroll_x,
            )
        };
        let y = if row <= self.rows {
            v.scroll_y
        } else {
            viewport::reveal_span(
                m.row_start(row) - h,
                m.row_start(row) + m.height(row) - h,
                m.header_h + h,
                v.height,
                v.scroll_y,
            )
        };
        (x, y)
    }
    fn categories(self) -> impl Iterator<Item = (bool, bool)> {
        [(false, false), (false, true), (true, false), (true, true)]
            .into_iter()
            .filter(move |&(r, c)| (!r || self.rows > 0) && (!c || self.columns > 0))
    }
    fn domain(self, area: GridArea, r: bool, c: bool) -> Option<GridArea> {
        let top = if r {
            area.first().row()
        } else {
            area.first().row().max(self.rows + 1)
        };
        let bottom = if r {
            area.last().row().min(self.rows)
        } else {
            area.last().row()
        };
        let left = if c {
            area.first().column()
        } else {
            area.first().column().max(self.columns + 1)
        };
        let right = if c {
            area.last().column().min(self.columns)
        } else {
            area.last().column()
        };
        if top > bottom || left > right {
            return None;
        }
        GridArea::new(
            CellKey::new(area.sheet(), top, left).ok()?,
            CellKey::new(area.sheet(), bottom, right).ok()?,
        )
        .ok()
    }
    pub fn interests(
        self,
        sheet: SheetId,
        m: &GridMetrics,
        v: &Viewport,
        extent: GridExtent,
    ) -> Vec<GridArea> {
        self.categories()
            .filter_map(|(r, c)| {
                self.domain(
                    super::interest_area(sheet, m, &self.view(v, r, c), extent)?,
                    r,
                    c,
                )
            })
            .collect()
    }
    pub fn plan(
        self,
        cells: &[CellShown],
        m: &GridMetrics,
        v: &Viewport,
        rows: u32,
        cols: u32,
    ) -> RenderPlan {
        let mut out = RenderPlan {
            cells: Vec::new(),
            col_headers: Vec::new(),
            row_headers: Vec::new(),
            extent_rows: rows,
            extent_cols: cols,
        };
        let (mut col_headers, mut row_headers) = (BTreeMap::new(), BTreeMap::new());
        for (r, c) in self.categories() {
            let plan = render_plan::build_render_plan(cells, m, &self.view(v, r, c), rows, cols);
            out.cells.extend(plan.cells.into_iter().filter(|cell| {
                (cell.row <= self.rows) == r
                    && (cell.col <= self.columns) == c
                    && intersection(cell.rect, self.bounds(m, v, cell.row, cell.col)).is_some()
            }));
            for header in plan
                .col_headers
                .into_iter()
                .filter(|header| (header.col <= self.columns) == c)
            {
                let (x, right) = self.x_bounds(m, v, c);
                if header.rect.x + header.rect.w > x && header.rect.x < right {
                    col_headers.insert(header.col, header);
                }
            }
            for header in plan
                .row_headers
                .into_iter()
                .filter(|header| (header.row <= self.rows) == r)
            {
                let (y, bottom) = self.y_bounds(m, v, r);
                if header.rect.y + header.rect.h > y && header.rect.y < bottom {
                    row_headers.insert(header.row, header);
                }
            }
        }
        out.cells.sort_by_key(|cell| (cell.row, cell.col));
        out.col_headers = col_headers.into_values().collect();
        out.row_headers = row_headers.into_values().collect();
        out
    }
    pub fn area_rects(self, area: GridArea, m: &GridMetrics, v: &Viewport) -> Vec<CellRect> {
        self.categories()
            .filter_map(|(r, c)| {
                let area = self.domain(area, r, c)?;
                let first = self.rect(m, v, area.first().row(), area.first().column());
                let last = self.rect(m, v, area.last().row(), area.last().column());
                intersection(
                    CellRect {
                        x: first.x,
                        y: first.y,
                        w: last.x + last.w - first.x,
                        h: last.y + last.h - first.y,
                    },
                    self.bounds(m, v, area.first().row(), area.first().column()),
                )
            })
            .collect()
    }
    /// Clip overflowing text and header bands at their own pane boundary.
    pub fn clip_style(
        self,
        rect: CellRect,
        m: &GridMetrics,
        v: &Viewport,
        row: Option<u32>,
        col: Option<u32>,
    ) -> String {
        let (x, right) = col.map_or((0.0, m.header_w), |col| {
            self.x_bounds(m, v, col <= self.columns)
        });
        let (y, bottom) = row.map_or((0.0, m.header_h), |row| {
            self.y_bounds(m, v, row <= self.rows)
        });
        format!(
            ";clip-path:inset({}px {}px {}px {}px)",
            (y - rect.y).max(0.0),
            (rect.x + rect.w - right).max(0.0),
            (rect.y + rect.h - bottom).max(0.0),
            (x - rect.x).max(0.0)
        )
    }
}

pub fn intersection(a: CellRect, b: CellRect) -> Option<CellRect> {
    let (x, y) = (a.x.max(b.x), a.y.max(b.y));
    let (right, bottom) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    (right > x && bottom > y).then_some(CellRect {
        x,
        y,
        w: right - x,
        h: bottom - y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn frozen_and_scrolling_cells_share_the_inverse_at_every_zoom() {
        for zoom in [0.5, 1.0, 1.75, 4.0] {
            let m = GridMetrics::default().scaled(zoom);
            let p = Panes {
                rows: 1,
                columns: 1,
            };
            let v = Viewport {
                scroll_x: 400.0 * zoom,
                scroll_y: 200.0 * zoom,
                width: 1000.0 * zoom,
                height: 600.0 * zoom,
            };
            for (row, col) in [(1, 1), (1, 10), (20, 1), (20, 10)] {
                let rect = p.rect(&m, &v, row, col);
                let visible = intersection(rect, p.bounds(&m, &v, row, col)).unwrap();
                assert_eq!(
                    p.hit(
                        &m,
                        &v,
                        100,
                        100,
                        visible.x + visible.w / 2.0,
                        visible.y + visible.h / 2.0
                    ),
                    HitTarget::Cell { row, col }
                );
            }
            assert_eq!(
                p.rect(&m, &v, 1, 1),
                p.rect(
                    &m,
                    &Viewport {
                        scroll_x: 0.0,
                        scroll_y: 0.0,
                        ..v
                    },
                    1,
                    1
                )
            );
            assert_eq!(p.reveal(&m, &v, 1, 1), (v.scroll_x, v.scroll_y));
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hidden_bands_do_not_change_the_frozen_cell_hit_target() {
        for zoom in [0.5, 1.0, 2.5] {
            let mut m = GridMetrics::default();
            m.rows = geometry::Sizes::new(m.row_height, [(1, 0.0), (3, 42.0)]);
            m.cols = geometry::Sizes::new(m.col_width, [(2, 0.0), (4, 120.0)]);
            let m = m.scaled(zoom);
            let p = Panes {
                rows: 3,
                columns: 4,
            };
            let v = Viewport {
                scroll_x: 500.0 * zoom,
                scroll_y: 300.0 * zoom,
                width: 1000.0 * zoom,
                height: 600.0 * zoom,
            };
            for (row, col) in [(2, 1), (3, 4), (20, 1), (2, 12), (20, 12)] {
                let rect = p.rect(&m, &v, row, col);
                let rect = intersection(rect, p.bounds(&m, &v, row, col)).unwrap();
                assert_eq!(
                    p.hit(
                        &m,
                        &v,
                        100,
                        100,
                        rect.x + rect.w / 2.0,
                        rect.y + rect.h / 2.0
                    ),
                    HitTarget::Cell { row, col }
                );
            }
            assert_eq!(p.size(&m), (280.0 * zoom, 64.0 * zoom));
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn frozen_interests_and_selection_stay_bounded_at_the_last_row() {
        let mut ids: calc_host::value::IdAllocator<SheetId> = calc_host::value::IdAllocator::new();
        let sheet = ids.issue().unwrap();
        let m = GridMetrics::default();
        let p = Panes {
            rows: 1,
            columns: 1,
        };
        let v = Viewport {
            scroll_x: 5000.0,
            scroll_y: m.row_start(1_048_550),
            width: 600.0,
            height: 400.0,
        };
        let areas = p.interests(sheet, &m, &v, GridExtent::new(1_048_576, 16_384));
        assert_eq!(areas.len(), 4);
        assert!(areas.iter().map(|a| a.cell_count()).sum::<u64>() < 5000);
        let area = GridArea::new(
            CellKey::new(sheet, 1, 1).unwrap(),
            CellKey::new(sheet, 1_048_576, 16_384).unwrap(),
        )
        .unwrap();
        let rects = p.area_rects(area, &m, &v);
        assert_eq!(rects.len(), 4);
        assert!(rects.iter().all(|r| r.x >= m.header_w
            && r.y >= m.header_h
            && r.x + r.w <= v.width
            && r.y + r.h <= v.height));
    }
}
