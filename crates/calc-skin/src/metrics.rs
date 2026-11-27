//! Shared coordinate and band sizes for the grid projection and host pagination.

/// The bands of one axis whose size differs from the default: a row's height or a column's
/// width in pixels, 0 for a hidden one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sizes {
    /// (1-based index, size), ascending by index, each different from the default.
    sized: Vec<(u32, f64)>,
    /// `before[i]`: the sum of (size - default) over `sized[..i]`.
    before: Vec<f64>,
}

impl Sizes {
    /// The sizes of the bands `bands` names (index, size), over `default`; a band whose size is
    /// the default, or not finite, or negative, is left at the default.
    pub fn new(default: f64, bands: impl IntoIterator<Item = (u32, f64)>) -> Sizes {
        let mut sized: Vec<(u32, f64)> = bands
            .into_iter()
            .filter(|&(index, size)| {
                index >= 1 && size.is_finite() && size >= 0.0 && size != default
            })
            .collect();
        sized.sort_by_key(|&(index, _)| index);
        sized.dedup_by_key(|&mut (index, _)| index);
        let mut sizes = Sizes {
            sized,
            before: Vec::new(),
        };
        sizes.rebuild(default);
        sizes
    }

    fn rebuild(&mut self, default: f64) {
        let mut total = 0.0;
        self.before = self
            .sized
            .iter()
            .map(|&(_, size)| {
                let here = total;
                total += size - default;
                here
            })
            .collect();
    }

    /// The sizes times `factor`, over a default times `factor`.
    fn scaled(&self, default: f64, factor: f64) -> Sizes {
        let mut sizes = Sizes {
            sized: self
                .sized
                .iter()
                .map(|&(index, size)| (index, size * factor))
                .collect(),
            before: Vec::new(),
        };
        sizes.rebuild(default * factor);
        sizes
    }

    /// The size of band `index`.
    pub fn size(&self, default: f64, index: u32) -> f64 {
        match self.sized.binary_search_by_key(&index, |&(at, _)| at) {
            Ok(at) => self.sized[at].1,
            Err(_) => default,
        }
    }

    fn visible(&self, default: f64, index: u32, forward: bool, limit: u32) -> u32 {
        let start = index.clamp(1, limit);
        if default <= 0.0 {
            let visible: Vec<_> = self
                .sized
                .iter()
                .filter(|&&(at, size)| at <= limit && size > 0.0)
                .map(|&(at, _)| at)
                .collect();
            return if forward {
                visible
                    .iter()
                    .copied()
                    .find(|&at| at >= start)
                    .or_else(|| visible.last().copied())
            } else {
                visible
                    .iter()
                    .rev()
                    .copied()
                    .find(|&at| at <= start)
                    .or_else(|| visible.first().copied())
            }
            .unwrap_or(start);
        }
        let search = |mut at: u32, forward: bool| {
            while self.size(default, at) == 0.0 {
                let next = if forward {
                    at.saturating_add(1).min(limit)
                } else {
                    at.saturating_sub(1).max(1)
                };
                if next == at {
                    return None;
                }
                at = next;
            }
            Some(at)
        };
        search(start, forward)
            .or_else(|| search(start, !forward))
            .unwrap_or(start)
    }

    /// The content offset of band `index`'s near edge (band 1 starts at 0).
    pub fn start(&self, default: f64, index: u32) -> f64 {
        let before = self.sized.partition_point(|&(at, _)| at < index);
        let delta = match before {
            0 => 0.0,
            n => self.before[n - 1] + (self.sized[n - 1].1 - default),
        };
        (f64::from(index) - 1.0) * default + delta
    }

    /// The band in `[1, extent]` that holds content offset `offset` (`[start, start + size)`), or
    /// `None` before the first band or past the last.
    pub fn band_at(&self, default: f64, offset: f64, extent: u32) -> Option<u32> {
        if extent == 0 || !offset.is_finite() || offset < 0.0 || default <= 0.0 {
            return None;
        }
        // The last band whose start is at or before the offset.
        let (mut low, mut high) = (1u32, extent);
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            if self.start(default, middle) <= offset {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        (offset < self.start(default, low) + self.size(default, low)).then_some(low)
    }
}

/// Grid metrics in CSS pixels: the default row height and column width, the bands the workbook
/// sizes otherwise, and the two header strips.
///
/// The [`Default`] is Excel-like at 100%: a 22 px row, an 80 px column, a 22 px
/// column-header strip along the top, and a 48 px row-header strip down the
/// left, with no band sized otherwise. The grid origin (top-left of cell `R1C1`)
/// sits at `(header_w, header_h)` in the unscrolled viewport.
#[derive(Debug, Clone, PartialEq)]
pub struct GridMetrics {
    /// Display zoom, independent of the size of outline gutters.
    pub scale: f64,
    /// Height of a data row the workbook does not size, in pixels.
    pub row_height: f64,
    /// Width of a data column the workbook does not size, in pixels.
    pub col_width: f64,
    /// Height of the top column-header strip, in pixels.
    pub header_h: f64,
    /// Width of the left row-header strip, in pixels.
    pub header_w: f64,
    /// The rows the workbook sizes otherwise.
    pub rows: Sizes,
    /// The columns the workbook sizes otherwise.
    pub cols: Sizes,
}

impl Default for GridMetrics {
    fn default() -> Self {
        Self {
            scale: 1.0,
            row_height: 22.0,
            col_width: 80.0,
            header_h: 22.0,
            header_w: 48.0,
            rows: Sizes::default(),
            cols: Sizes::default(),
        }
    }
}

impl GridMetrics {
    /// The metrics scaled proportionally by a zoom `factor` (the grid skin's zoom).
    ///
    /// A pure, total multiply of every extent, so `scaled(1.0)` is exactly the input. A
    /// non-finite or non-positive `factor` returns the metrics unchanged, so a bad zoom value
    /// never yields `NaN` or zero pixels that would break hit testing or cell rectangles. The
    /// caller clamps `factor` to a legible range .
    #[must_use]
    pub fn scaled(&self, factor: f64) -> Self {
        if !factor.is_finite() || factor <= 0.0 {
            return self.clone();
        }
        Self {
            scale: self.scale * factor,
            row_height: self.row_height * factor,
            col_width: self.col_width * factor,
            header_h: self.header_h * factor,
            header_w: self.header_w * factor,
            rows: self.rows.scaled(self.row_height, factor),
            cols: self.cols.scaled(self.col_width, factor),
        }
    }

    /// Display zoom resolves each band to device pixels before adding its position. Excel's
    /// 30-pixel row occupies 23 pixels at 75%, so multiplying the cumulative offset drifts.
    /// Both painters, selection and hit testing consume these same resolved metrics.
    #[must_use]
    pub fn scaled_for_display(&self, factor: f64, device_scale: f64) -> Self {
        if !factor.is_finite() || factor <= 0.0 {
            return self.clone();
        }
        let mut out = self.scaled(factor);
        if !device_scale.is_finite() || device_scale <= 0.0 {
            return out;
        }
        let snap = |size: f64| {
            if size == 0.0 {
                0.0
            } else {
                (size * device_scale).round().max(1.0) / device_scale
            }
        };
        let row_height = snap(out.row_height);
        let col_width = snap(out.col_width);
        out.rows = Sizes::new(
            row_height,
            out.rows.sized.iter().map(|&(i, size)| (i, snap(size))),
        );
        out.cols = Sizes::new(
            col_width,
            out.cols.sized.iter().map(|&(i, size)| (i, snap(size))),
        );
        out.row_height = row_height;
        out.col_width = col_width;
        out.header_h = snap(out.header_h);
        out.header_w = snap(out.header_w);
        out
    }

    /// Row `row`'s height (0 when hidden).
    pub fn height(&self, row: u32) -> f64 {
        self.rows.size(self.row_height, row)
    }

    /// Column `col`'s width (0 when hidden).
    pub fn width(&self, col: u32) -> f64 {
        self.cols.size(self.col_width, col)
    }

    /// The content offset of row `row`'s top edge (row 1 at 0).
    pub fn row_start(&self, row: u32) -> f64 {
        self.rows.start(self.row_height, row)
    }

    /// The content offset of column `col`'s left edge (column 1 at 0).
    pub fn col_start(&self, col: u32) -> f64 {
        self.cols.start(self.col_width, col)
    }

    /// A visible band at or beyond `index`, inspecting only stored exceptions.
    pub fn visible_row(&self, index: u32, forward: bool, limit: u32) -> u32 {
        self.rows.visible(self.row_height, index, forward, limit)
    }

    pub fn visible_col(&self, index: u32, forward: bool, limit: u32) -> u32 {
        self.cols.visible(self.col_width, index, forward, limit)
    }

    pub fn col_at(&self, offset: f64, limit: u32) -> u32 {
        self.cols
            .band_at(self.col_width, offset.max(0.0), limit)
            .unwrap_or(limit)
    }

    pub fn row_at(&self, offset: f64, limit: u32) -> u32 {
        self.rows
            .band_at(self.row_height, offset.max(0.0), limit)
            .unwrap_or(limit)
    }
}

pub type RowBand = (u32, Option<f64>, bool);
pub type ColumnBand = (u32, u32, Option<f64>, bool);

#[derive(Clone, Copy, Debug, Default)]
pub struct SheetBands<'a> {
    pub default_digit_px: Option<f64>,
    pub default_row_height: Option<f64>,
    pub default_column_width: Option<f64>,
    pub base_column_width: Option<f64>,
    pub column_outline_levels: u8,
    pub row_outline_levels: u8,
    pub columns: &'a [ColumnBand],
    pub rows: &'a [RowBand],
}

pub fn sheet_metrics(geometry: Option<SheetBands<'_>>) -> GridMetrics {
    // Stored OOXML widths already include the cell margins. The workbook's default font sets the
    // digit width (Aptos Narrow 11 and Calibri 11: 7 px; the worker carries it, absent is 7);
    // use the standard's truncation, including half-pixel cases.
    // https://learn.microsoft.com/dotnet/api/documentformat.openxml.spreadsheet.column
    let digit = geometry
        .and_then(|g| g.default_digit_px)
        .filter(|d| *d >= 1.)
        .unwrap_or(7.0);
    let column_px = |chars: f64| (((256. * chars + (128. / digit).floor()) / 256.) * digit).floor();
    // Excel's captured 23/32/20-point rows occupy 30/42/26 pixels at 96 DPI.
    let row_px = |points: f64| (points * 96.0 / 72.0).floor();
    // With no default width the sheet's base width counts in digits plus the 5 px padding, rounded
    // up to a multiple of 8 px (8 characters: 64 px at 7 px digits, 72 px at 8).
    let col_width = match geometry {
        Some(g) => g.default_column_width.map_or_else(
            || ((g.base_column_width.unwrap_or(8.) * digit + 5.) / 8.).ceil() * 8.,
            column_px,
        ),
        None => 64.0,
    };
    let row_height = geometry
        .and_then(|g| g.default_row_height)
        .map_or(20.0, row_px);
    let (columns, rows) = match geometry {
        Some(geometry) => (
            geometry
                .columns
                .iter()
                .flat_map(|&(first, last, width, hidden)| {
                    let size = if hidden {
                        0.0
                    } else {
                        width.map_or(col_width, column_px)
                    };
                    (first..=last.min(16_384)).map(move |column| (column, size))
                })
                .collect::<Vec<_>>(),
            geometry
                .rows
                .iter()
                .map(|&(row, height, hidden)| {
                    let size = if hidden {
                        0.0
                    } else {
                        height.map_or(row_height, row_px)
                    };
                    (row, size)
                })
                .collect::<Vec<_>>(),
        ),
        None => (Vec::new(), Vec::new()),
    };
    GridMetrics {
        scale: 1.0,
        row_height,
        col_width,
        header_h: 22.0 + geometry.map_or(0., |g| f64::from(g.column_outline_levels) * 16.),
        header_w: 64.0 + geometry.map_or(0., |g| f64::from(g.row_outline_levels) * 16.),
        rows: Sizes::new(row_height, rows),
        cols: Sizes::new(col_width, columns),
    }
}
