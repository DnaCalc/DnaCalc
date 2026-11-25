//! Pixels for a chart (dc-oxh8w.22): the SVG of [`crate::chart_svg`] rendered by resvg and tiny-skia,
//! text set in the bundled Carlito (metric-compatible with Calibri, SIL Open Font License; the licence
//! is `fonts/OFL-Carlito.txt`). No system font is read, so the same chart draws the same pixels on
//! every host. Excel's own pixels (its fonts and anti-aliasing) are not reproduced.

use crate::chart_svg;
use calc_host::chart::ExportFormat;
use calc_host::ChartView;
use resvg::{tiny_skia, usvg};
use std::sync::{Arc, OnceLock};

const FONTS: [&[u8]; 4] = [
    include_bytes!("../fonts/Carlito-Regular.ttf"),
    include_bytes!("../fonts/Carlito-Bold.ttf"),
    include_bytes!("../fonts/Carlito-Italic.ttf"),
    include_bytes!("../fonts/Carlito-BoldItalic.ttf"),
];

fn fonts() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        for font in FONTS {
            db.load_font_data(font.to_vec());
        }
        for set in [
            usvg::fontdb::Database::set_sans_serif_family,
            usvg::fontdb::Database::set_serif_family,
            usvg::fontdb::Database::set_cursive_family,
            usvg::fontdb::Database::set_fantasy_family,
            usvg::fontdb::Database::set_monospace_family,
        ] {
            set(&mut db, "Carlito");
        }
        Arc::new(db)
    })
    .clone()
}

/// The chart drawn `width` by `height` pixels on white.
pub fn raster(chart: &ChartView, width: u32, height: u32) -> Result<tiny_skia::Pixmap, String> {
    if width == 0 || height == 0 {
        return Err("an empty picture".into());
    }
    let svg = chart_svg(chart, f64::from(width), f64::from(height)).replace("Calibri,", "Carlito,");
    let options = usvg::Options { fontdb: fonts(), font_family: "Carlito".into(), ..Default::default() };
    let tree = usvg::Tree::from_str(&svg, &options).map_err(|e| e.to_string())?;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or("a picture too large to draw")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    Ok(pixmap)
}

/// The picture's file bytes in `format`.
pub fn export(chart: &ChartView, width: u32, height: u32, format: ExportFormat) -> Result<Vec<u8>, String> {
    let pixmap = raster(chart, width, height)?;
    match format {
        ExportFormat::Png => pixmap.encode_png().map_err(|e| e.to_string()),
        ExportFormat::Jpg => {
            let rgb: Vec<u8> = pixmap.pixels().iter().flat_map(|p| [p.red(), p.green(), p.blue()]).collect();
            let mut out = Vec::new();
            let (w, h) = (u16::try_from(width).map_err(|e| e.to_string())?, u16::try_from(height).map_err(|e| e.to_string())?);
            jpeg_encoder::Encoder::new(&mut out, 90)
                .encode(&rgb, w, h, jpeg_encoder::ColorType::Rgb)
                .map_err(|e| e.to_string())?;
            Ok(out)
        }
        ExportFormat::Gif => {
            let mut rgb: Vec<u8> = pixmap.pixels().iter().flat_map(|p| [p.red(), p.green(), p.blue()]).collect();
            let (w, h) = (u16::try_from(width).map_err(|e| e.to_string())?, u16::try_from(height).map_err(|e| e.to_string())?);
            let mut out = Vec::new();
            {
                let mut encoder = gif::Encoder::new(&mut out, w, h, &[]).map_err(|e| e.to_string())?;
                let frame = gif::Frame::from_rgb(w, h, &mut rgb);
                encoder.write_frame(&frame).map_err(|e| e.to_string())?;
            }
            Ok(out)
        }
    }
}

/// Install this rasteriser as the one `Chart.Export` calls.
pub fn install() {
    calc_host::chart::set_chart_exporter(export);
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent, WorkbookCommand};

    fn column_chart() -> ChartView {
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(calc_host::blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let sheet = book.host().sheets()[0].id();
        for (row, name, value) in [(1, "Region", "Sales"), (2, "North", "10"), (3, "South", "30"), (4, "East", "50")] {
            for (column, text) in [(1, name), (2, value)] {
                let at = calc_host::value::CellKey::new(sheet, row, column).unwrap();
                assert!(book.dispatch(SkinIntent::EnterCell { at, text: text.into() }).accepted());
            }
        }
        let area = calc_host::value::GridArea::new(
            calc_host::value::CellKey::new(sheet, 1, 1).unwrap(),
            calc_host::value::CellKey::new(sheet, 4, 2).unwrap(),
        )
        .unwrap();
        let command = WorkbookCommand::ChartCreate {
            revision: book.host().revision().get(),
            area,
            kind: "column".into(),
            rows: false,
            headers: true,
            categories: true,
        };
        assert!(book.dispatch(SkinIntent::Command(command)).accepted());
        let views = book.host().charts(sheet).0;
        assert_eq!(views.len(), 1);
        views.into_iter().next().unwrap()
    }

    /// dc-oxh8w.22: the exported PNG has the requested size, draws marks and text in more than white,
    /// and the same chart draws the same bytes; GIF and JPEG carry their signatures and the same size.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_oxh8w_22__a_chart_exports_as_png_gif_and_jpg_of_the_requested_size() {
        let chart = column_chart();
        let png = export(&chart, 400, 267, ExportFormat::Png).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!((u32::from_be_bytes(png[16..20].try_into().unwrap()), u32::from_be_bytes(png[20..24].try_into().unwrap())), (400, 267));
        assert_eq!(png, export(&chart, 400, 267, ExportFormat::Png).unwrap(), "deterministic");

        let pixmap = raster(&chart, 400, 267).unwrap();
        let ink = pixmap.pixels().iter().filter(|p| (p.red(), p.green(), p.blue()) != (255, 255, 255)).count();
        assert!(ink > 2_000, "marks, gridlines and text are drawn: {ink} pixels");
        let colours: std::collections::HashSet<_> = pixmap.pixels().iter().map(|p| (p.red(), p.green(), p.blue())).collect();
        assert!(colours.len() > 8, "anti-aliased text and a coloured series: {} colours", colours.len());
        // The plot's bars lie in the lower middle; the text rows at the very top are not all white (title area is empty
        // here, so look for the category labels near the bottom instead).
        let bottom_ink = (240..267).flat_map(|y| (0..400).map(move |x| (x, y)))
            .filter(|&(x, y)| { let p = pixmap.pixel(x, y).unwrap(); (p.red(), p.green(), p.blue()) != (255, 255, 255) })
            .count();
        assert!(bottom_ink > 50, "category labels are set in the bundled font: {bottom_ink}");

        let gif = export(&chart, 400, 267, ExportFormat::Gif).unwrap();
        assert_eq!(&gif[..6], b"GIF89a");
        assert_eq!((u16::from_le_bytes([gif[6], gif[7]]), u16::from_le_bytes([gif[8], gif[9]])), (400, 267));

        let jpg = export(&chart, 400, 267, ExportFormat::Jpg).unwrap();
        assert_eq!(&jpg[..2], &[0xFF, 0xD8]);
        let sof = jpg.windows(2).position(|w| w == [0xFF, 0xC0]).expect("a baseline frame header");
        let (h, w) = (u16::from_be_bytes([jpg[sof + 5], jpg[sof + 6]]), u16::from_be_bytes([jpg[sof + 7], jpg[sof + 8]]));
        assert_eq!((w, h), (400, 267));
        assert!(raster(&chart, 0, 10).is_err());
    }
}
