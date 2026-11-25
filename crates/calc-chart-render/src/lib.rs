//! Authored charts drawn from calc-host's `ChartView`: [`chart_svg`] (the SVG the worker's pane shows)
//! and [`raster`] (pixels for `Chart.Export` and printing, text in the bundled open fonts only).

mod hierarchy;
mod raster;
mod svg;
pub use raster::{export, install, raster};
pub use svg::{chart_svg, escape};
