//! The image of a drawing picture as the page can show it (dc-zjsg.3): which of the file's bytes to send
//! (the SVG Excel writes beside its PNG fallback, else the blip's own image), what they are by their
//! signature (the content type a package states is not trusted), the PNG a metafile may carry inside it,
//! and, for an image no browser decodes, the typed label that stands in for it with its size.

use calc_host::{PictureMedia, ShapePicture};

/// A picture's image kind by its signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Svg,
    Emf,
    Wmf,
    Tiff,
}

impl Kind {
    /// The content type the page decodes this kind as, `None` for a kind browsers do not show.
    pub(crate) fn content_type(self) -> Option<&'static str> {
        Some(match self {
            Kind::Png => "image/png",
            Kind::Jpeg => "image/jpeg",
            Kind::Gif => "image/gif",
            Kind::Bmp => "image/bmp",
            Kind::Svg => "image/svg+xml",
            Kind::Emf | Kind::Wmf | Kind::Tiff => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Png => "PNG",
            Kind::Jpeg => "JPEG",
            Kind::Gif => "GIF",
            Kind::Bmp => "BMP",
            Kind::Svg => "SVG",
            Kind::Emf => "EMF",
            Kind::Wmf => "WMF",
            Kind::Tiff => "TIFF",
        }
    }
}

const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// The kind of `bytes` by what they start with.
pub(crate) fn sniff(bytes: &[u8]) -> Option<Kind> {
    if bytes.starts_with(&PNG) {
        Some(Kind::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Kind::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(Kind::Gif)
    } else if bytes.starts_with(b"BM") && bytes.len() > 14 {
        Some(Kind::Bmp)
    } else if bytes.len() > 44 && bytes[0..4] == [1, 0, 0, 0] && &bytes[40..44] == b" EMF" {
        Some(Kind::Emf)
    } else if bytes.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A])
        || (bytes.len() > 18 && matches!(bytes[0..4], [1, 0, 9, 0] | [2, 0, 9, 0]))
    {
        Some(Kind::Wmf)
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        Some(Kind::Tiff)
    } else {
        let text = bytes.get(..bytes.len().min(2048)).unwrap_or(bytes);
        let text = String::from_utf8_lossy(text);
        let text = text.trim_start_matches('\u{feff}').trim_start();
        (text.starts_with('<') && text.contains("<svg")).then_some(Kind::Svg)
    }
}

/// The first PNG inside `bytes` (a metafile carrying its bitmap as PNG), up to its `IEND` chunk.
pub(crate) fn embedded_png(bytes: &[u8]) -> Option<&[u8]> {
    let mut from = 0;
    while let Some(at) = bytes[from..].windows(8).position(|w| w == PNG) {
        let start = from + at;
        let mut at = start + 8;
        while at + 12 <= bytes.len() {
            let length = u32::from_be_bytes(bytes[at..at + 4].try_into().ok()?) as usize;
            let kind = &bytes[at + 4..at + 8];
            let end = at.checked_add(12)?.checked_add(length)?;
            if end > bytes.len() {
                break;
            }
            if kind == b"IEND" {
                return Some(&bytes[start..end]);
            }
            at = end;
        }
        from = start + 8;
    }
    None
}

/// The size in pixels at 96 dots per inch a metafile states, `None` when its header does not say.
fn metafile_size(kind: Kind, bytes: &[u8]) -> Option<(f64, f64)> {
    let i32_at = |at: usize| bytes.get(at..at + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()) as f64);
    let i16_at = |at: usize| bytes.get(at..at + 2).map(|b| i16::from_le_bytes(b.try_into().unwrap()) as f64);
    match kind {
        // The frame in hundredths of a millimetre.
        Kind::Emf => {
            let (l, t, r, b) = (i32_at(24)?, i32_at(28)?, i32_at(32)?, i32_at(36)?);
            let px = |v: f64| (v / 100.0 / 25.4 * 96.0).round();
            Some((px(r - l), px(b - t))).filter(|(w, h)| *w > 0.0 && *h > 0.0)
        }
        // The placeable header: a box in units per inch.
        Kind::Wmf if bytes.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A]) => {
            let per_inch = bytes.get(14..16).map(|b| u16::from_le_bytes(b.try_into().unwrap()) as f64)?;
            let (l, t, r, b) = (i16_at(6)?, i16_at(8)?, i16_at(10)?, i16_at(12)?);
            let px = |v: f64| (v / per_inch * 96.0).round();
            Some((px(r - l), px(b - t))).filter(|(w, h)| per_inch > 0.0 && *w > 0.0 && *h > 0.0)
        }
        _ => None,
    }
}

/// What the page shows of a picture: the bytes with the content type to decode them as, or the label
/// of the image it cannot decode.
pub(crate) fn source(media: &ShapePicture) -> Result<(&'static str, Vec<u8>), String> {
    // The SVG first: Excel keeps it beside the PNG it falls back to.
    for item in [media.svg.as_ref(), media.image.as_ref()].into_iter().flatten() {
        if let Some(found) = decodable(item) {
            return Ok(found);
        }
    }
    let Some(image) = media.image.as_ref() else {
        return Err("Picture: its image is missing".to_owned());
    };
    if image.bytes.is_empty() {
        return Err(format!("Picture too large to keep ({} bytes)", image.size));
    }
    Err(match sniff(&image.bytes) {
        Some(kind) => match metafile_size(kind, &image.bytes) {
            Some((w, h)) => format!("{} picture {w} × {h}", kind.name()),
            None => format!("{} picture", kind.name()),
        },
        None => "Picture: an image format not recognised".to_owned(),
    })
}

fn decodable(item: &PictureMedia) -> Option<(&'static str, Vec<u8>)> {
    let kind = sniff(&item.bytes)?;
    if let Some(content_type) = kind.content_type() {
        return Some((content_type, item.bytes.to_vec()));
    }
    // A metafile with a PNG inside it draws that PNG.
    matches!(kind, Kind::Emf | Kind::Wmf)
        .then(|| embedded_png(&item.bytes))
        .flatten()
        .map(|png| ("image/png", png.to_vec()))
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    fn png(extra: usize) -> Vec<u8> {
        let mut bytes = PNG.to_vec();
        // IHDR (13 bytes) and IEND.
        bytes.extend([0, 0, 0, 13]);
        bytes.extend(b"IHDR");
        bytes.extend([0; 13]);
        bytes.extend([0; 4]);
        bytes.extend([0, 0, 0, 0]);
        bytes.extend(b"IEND");
        bytes.extend([0xAE, 0x42, 0x60, 0x82]);
        bytes.extend(vec![7; extra]);
        bytes
    }

    fn media(bytes: Vec<u8>) -> PictureMedia {
        PictureMedia { part: "xl/media/image1.x".into(), content_type: "x".into(), size: bytes.len() as u64, bytes: bytes.into() }
    }

    fn emf(w_mm100: i32, h_mm100: i32, tail: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; 88];
        bytes[0] = 1;
        bytes[40..44].copy_from_slice(b" EMF");
        bytes[32..36].copy_from_slice(&w_mm100.to_le_bytes());
        bytes[36..40].copy_from_slice(&h_mm100.to_le_bytes());
        bytes.extend(tail);
        bytes
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_3__images_are_known_by_their_signatures_not_their_stated_type() {
        assert_eq!(sniff(&png(0)), Some(Kind::Png));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0]), Some(Kind::Jpeg));
        assert_eq!(sniff(b"GIF89a\x01\x00"), Some(Kind::Gif));
        assert_eq!(sniff(b"BM\x36\x00\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00\x28\x00"), Some(Kind::Bmp));
        assert_eq!(sniff("\u{feff} <?xml version='1.0'?><svg xmlns='http://www.w3.org/2000/svg'/>".as_bytes()), Some(Kind::Svg));
        assert_eq!(sniff(&emf(100, 100, &[])), Some(Kind::Emf));
        assert_eq!(sniff(&[0xD7, 0xCD, 0xC6, 0x9A, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), Some(Kind::Wmf));
        assert_eq!(sniff(b"II*\0\x08\0\0\0"), Some(Kind::Tiff));
        assert_eq!(sniff(b"plain text"), None);
        assert_eq!(sniff(&[]), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_3__a_picture_sends_its_svg_before_its_png_fallback() {
        let svg = media(b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec());
        let picture = ShapePicture { image: Some(media(png(0))), svg: Some(svg), ..Default::default() };
        let (content_type, bytes) = source(&picture).unwrap();
        assert_eq!(content_type, "image/svg+xml");
        assert!(bytes.starts_with(b"<svg"));
        // An SVG the page cannot read falls back to the PNG.
        let broken = ShapePicture { image: Some(media(png(0))), svg: Some(media(b"nonsense".to_vec())), ..Default::default() };
        assert_eq!(source(&broken).unwrap().0, "image/png");
        let jpeg = ShapePicture { image: Some(media(vec![0xFF, 0xD8, 0xFF, 1, 2, 3])), ..Default::default() };
        assert_eq!(source(&jpeg).unwrap().0, "image/jpeg");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_3__a_metafile_draws_the_png_inside_it_else_a_labelled_placeholder() {
        let inner = png(0);
        let with_png = emf(2540, 2540, &[&[9u8; 5][..], &inner, &[1, 2, 3][..]].concat());
        let picture = ShapePicture { image: Some(media(with_png)), ..Default::default() };
        let (content_type, bytes) = source(&picture).unwrap();
        assert_eq!((content_type, bytes), ("image/png", inner));
        // No PNG inside: the typed placeholder with the size the header gives (25.4 mm is 96 px at 96 dpi).
        let bare = ShapePicture { image: Some(media(emf(2540, 1270, &[0; 20]))), ..Default::default() };
        assert_eq!(source(&bare).unwrap_err(), "EMF picture 96 × 48");
        let mut wmf = vec![0xD7, 0xCD, 0xC6, 0x9A, 0, 0];
        for v in [0i16, 0, 1440, 720] {
            wmf.extend(v.to_le_bytes());
        }
        wmf.extend(1440u16.to_le_bytes());
        wmf.extend([0; 8]);
        let wmf = ShapePicture { image: Some(media(wmf)), ..Default::default() };
        assert_eq!(source(&wmf).unwrap_err(), "WMF picture 96 × 48");
        let tiff = ShapePicture { image: Some(media(b"II*\0\x08\0\0\0".to_vec())), ..Default::default() };
        assert_eq!(source(&tiff).unwrap_err(), "TIFF picture");
        let unknown = ShapePicture { image: Some(media(b"????".to_vec())), ..Default::default() };
        assert!(source(&unknown).unwrap_err().contains("not recognised"));
        // A part larger than the reader keeps is said to be, with its size.
        let big = PictureMedia { part: "p".into(), content_type: "image/png".into(), size: 40_000_000, bytes: Vec::new().into() };
        let big = ShapePicture { image: Some(big), ..Default::default() };
        assert_eq!(source(&big).unwrap_err(), "Picture too large to keep (40000000 bytes)");
        assert_eq!(source(&ShapePicture::default()).unwrap_err(), "Picture: its image is missing");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_3__an_embedded_png_is_cut_at_its_end_chunk() {
        let mut bytes = vec![1, 2, 3];
        bytes.extend(png(4));
        let found = embedded_png(&bytes).unwrap();
        assert_eq!(found, &png(0)[..]);
        // A signature with no complete chunk list is not an image.
        assert!(embedded_png(&PNG).is_none());
        assert!(embedded_png(&[]).is_none());
    }
}
