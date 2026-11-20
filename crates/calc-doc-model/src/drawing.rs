//! The drawing layer of a sheet (dc-zjsg.1): the objects of its drawing part that are not charts.
//! Shapes, text boxes, pictures, connectors and groups, each with its anchor, name, geometry,
//! fill, outline and text. An object the file states is a *source* object: its XML stays in the
//! package as stored, and what this model holds of it is what a reader may ask. A new object is
//! written by the saver as Excel writes one. Objects the model does not hold (SmartArt, OLE
//! frames, ink, non-chart graphic frames) are counted in [`SheetShapes::unread`] and stay in the
//! package untouched.

use crate::ChartAnchor;

/// The kind of drawing object, by the element the drawing part stores it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    /// `xdr:sp`: an auto shape (a preset or custom geometry, optional text).
    Shape,
    /// `xdr:sp` with `txBox="1"` on its non-visual properties: a text box.
    TextBox,
    /// `xdr:pic`.
    Picture,
    /// `xdr:cxnSp`: a connector (a line is a connector of the `line` geometry).
    Connector,
    /// `xdr:grpSp`.
    Group,
}

/// A colour as the drawing part states it: `RRGGBB` or a scheme colour's name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ShapeColour {
    Rgb(u32),
    Scheme(String),
    /// A colour with the modifiers its element carries (`a:lumMod`, `a:lumOff`, `a:tint`, `a:shade`,
    /// `a:alpha`, `a:satMod`, ...), each by local name with its `val` (100000ths of a percent; `hue`
    /// ones in 60000ths of a degree), in file order. The base is an `Rgb` or a `Scheme` (dc-zjsg.2).
    Modified(Box<ShapeColour>, Vec<(String, i64)>),
}

/// A fill: what `a:noFill` or `a:solidFill` of the shape's properties say, `None` meaning the
/// shape's style decides.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShapeFill {
    #[default]
    Style,
    None,
    Solid(ShapeColour),
    /// `a:gradFill` (dc-zjsg.2).
    Gradient(ShapeGradient),
    /// `a:pattFill` (dc-zjsg.2): a preset 8 by 8 pattern in its foreground and background colours.
    Pattern(ShapePattern),
    /// `a:blipFill` with an image (dc-zjsg.9): its `a:blip r:embed`; the image itself is the shape's `media`.
    Picture(String),
    /// A picture or group fill: kept in the source, not held here.
    Other,
}

/// `a:pattFill`: the preset (`pct50`, `ltHorz`, `smCheck`, ...) and its colours (`a:fgClr`, `a:bgClr`);
/// a colour the file leaves out is `None` (black foreground, white background).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShapePattern {
    pub preset: String,
    pub foreground: Option<ShapeColour>,
    pub background: Option<ShapeColour>,
}

/// One reference of a shape's style (`xdr:style` `a:lnRef`, `a:fillRef`, `a:effectRef`): the index into
/// the theme's style list (0 is none) and the colour that takes the list's placeholder colour.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShapeStyleRef {
    pub index: i64,
    pub colour: Option<ShapeColour>,
}

/// `xdr:style`: what a shape draws with where its own properties say nothing (dc-zjsg.2).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShapeStyle {
    pub line: ShapeStyleRef,
    pub fill: ShapeStyleRef,
    pub effect: ShapeStyleRef,
    /// `a:fontRef`'s colour: the text's colour where a run says none.
    pub font: Option<ShapeColour>,
}

/// A point of a custom geometry path: each coordinate a number or a guide's name, as the file states it.
pub type GeometryPoint = (String, String);

/// One command of a custom geometry path (`a:moveTo`, `a:lnTo`, `a:arcTo`, `a:quadBezTo`, `a:cubicBezTo`,
/// `a:close`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GeometryCommand {
    Move(GeometryPoint),
    Line(GeometryPoint),
    /// `wR`, `hR`, `stAng`, `swAng`.
    Arc([String; 4]),
    Quad([GeometryPoint; 2]),
    Cubic([GeometryPoint; 3]),
    Close,
}

/// One `a:path` of a custom geometry: its own coordinate space (`w`, `h`; the shape's when absent),
/// whether it is filled (`fill`: `norm`, `none`, `lighten`, `lightenLess`, `darken`, `darkenLess`) and
/// stroked, and its commands.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct GeometryPath {
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub fill: Option<String>,
    pub stroke: bool,
    pub commands: Vec<GeometryCommand>,
}

/// `a:custGeom` (dc-zjsg.2): its adjust values and guides (name, formula) in file order, its text
/// rectangle (`l`, `t`, `r`, `b` as numbers or guide names) and its paths.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CustomGeometry {
    pub adjust: Vec<(String, String)>,
    pub guides: Vec<(String, String)>,
    pub text_rect: Option<[String; 4]>,
    pub paths: Vec<GeometryPath>,
    /// `a:cxnLst`: each connection site's position (`a:pos x y`, numbers or guide names), in order.
    pub sites: Vec<(String, String)>,
}

/// A gradient fill: `a:gsLst` stops with either a linear direction (`a:lin`) or a path (`a:path`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShapeGradient {
    /// Each `a:gs`: its `pos` in thousandths of a percent (0 to 100000) and its colour.
    pub stops: Vec<(i64, ShapeColour)>,
    /// `a:lin ang`, in 60,000ths of a degree clockwise from left-to-right; `None` without `a:lin`.
    pub angle: Option<i64>,
    /// `a:lin scaled`: the angle is scaled by the shape's aspect ratio.
    pub scaled: bool,
    /// `a:path path`: `circle`, `rect` or `shape`.
    pub path: Option<String>,
    /// `a:fillToRect` `l`, `t`, `r`, `b` in thousandths of a percent (the path gradient's focus).
    pub focus: Option<[i64; 4]>,
}

/// One end of a line (`a:headEnd`, `a:tailEnd`): the arrowhead's type and its `w` and `len` sizes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LineEnd {
    /// `none`, `triangle`, `stealth`, `diamond`, `oval` or `arrow`.
    pub kind: String,
    /// `sm`, `med` or `lg`; `None` is `med`.
    pub width: Option<String>,
    pub length: Option<String>,
}

/// An outline (`a:ln`): `None` for a field means the style decides.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShapeLine {
    /// `w`, in EMU.
    pub weight: Option<i64>,
    /// `a:noFill` under `a:ln`.
    pub hidden: bool,
    pub colour: Option<ShapeColour>,
    /// `a:prstDash val`.
    pub dash: Option<String>,
    /// `a:headEnd`: the arrowhead at the line's start.
    pub head: Option<LineEnd>,
    /// `a:tailEnd`: the arrowhead at the line's end.
    pub tail: Option<LineEnd>,
}

/// An outer shadow (`a:effectLst` `a:outerShdw`): drawn as a second pass beneath the shape (dc-zjsg.2).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShapeShadow {
    /// `blurRad`, in EMU.
    pub blur: i64,
    /// `dist`, in EMU.
    pub distance: i64,
    /// `dir`, in 60,000ths of a degree clockwise from the right.
    pub direction: i64,
    /// `sx`, `sy`: the shadow's scale in 100000ths of a percent (100000 when absent).
    pub scale: (i64, i64),
    /// `algn`: the point the scale is about (`b` when absent): `tl`, `t`, `tr`, `l`, `ctr`, `r`, `bl`, `b`, `br`.
    pub align: Option<String>,
    /// `rotWithShape` (true when absent): the shadow's direction turns with the shape.
    pub rotate_with_shape: bool,
    pub colour: ShapeColour,
}

/// A picture's media part: the bytes as stored (empty when the part is larger than the reader keeps;
/// `size` is its stored size either way).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PictureMedia {
    /// The part name in the package (`xl/media/image1.png`); empty for a picture made in the session.
    pub part: String,
    /// The part's content type (`image/png`).
    pub content_type: String,
    pub size: u64,
    pub bytes: std::sync::Arc<[u8]>,
}

/// A picture's image (`a:blip r:embed`, with the SVG of an `asvg:svgBlip` beside it), its crop and its
/// description, as the file states them (dc-zjsg.3).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShapePicture {
    /// The blip's own image: PNG, JPEG, GIF, BMP, EMF, WMF, ...
    pub image: Option<PictureMedia>,
    /// The SVG Excel writes beside a PNG fallback.
    pub svg: Option<PictureMedia>,
    /// `a:srcRect` `l`, `t`, `r`, `b` in thousandths of a percent (negative extends the image).
    pub crop: [i64; 4],
    /// `cNvPr descr`.
    pub description: Option<String>,
}

/// A text body's properties as the drawing part states them (`a:bodyPr`, the paragraphs' `a:pPr algn`
/// and the first run's `a:rPr`): what a text box draws with (dc-zjsg.2). `None` is the style's value.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShapeBody {
    /// `lIns`, `tIns`, `rIns`, `bIns`, in EMU.
    pub left_inset: Option<i64>,
    pub top_inset: Option<i64>,
    pub right_inset: Option<i64>,
    pub bottom_inset: Option<i64>,
    /// `anchor`: `t`, `ctr`, `b` (also `just`, `dist`).
    pub anchor: Option<String>,
    /// `wrap`: `square` or `none`.
    pub wrap: Option<String>,
    /// Each paragraph's `algn` (`l`, `ctr`, `r`, `just`), in paragraph order.
    pub alignment: Vec<Option<String>>,
    /// The first run's `sz`, in hundredths of a point.
    pub size: Option<i64>,
    /// The first run's explicit Latin typeface.
    pub face: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub colour: Option<ShapeColour>,
    /// Explicit, individually styled runs in paragraph order; empty when not represented.
    pub runs: Vec<Vec<ShapeTextRun>>,
    /// The first run's `a:noFill`: the text is there but not drawn (an equation's fallback picture carries it).
    pub text_hidden: bool,
}

/// A text run whose size, weight, italic state and optional colour are specified.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShapeTextRun {
    pub text: String,
    /// Explicit Latin typeface, when represented.
    pub face: Option<String>,
    /// Hundredths of a point.
    pub size: i64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub colour: Option<ShapeColour>,
}

impl ShapeTextRun {
    pub fn same_style_as(&self, other: &Self) -> bool {
        self.face == other.face && self.size == other.size && self.bold == other.bold && self.italic == other.italic
            && self.underline == other.underline && self.colour == other.colour
    }
}

impl ShapeBody {
    /// Runs remain usable only while the paragraph and its first-run properties still match.
    pub fn rich_runs_for(&self, paragraphs: &[String]) -> Option<&[Vec<ShapeTextRun>]> {
        let first = self.runs.iter().flatten().next()?;
        if !self.runs.iter().flatten().any(|run| !run.same_style_as(first))
            || self.runs.iter().flatten().any(|run| !(100..=400_000).contains(&run.size))
            || self.runs.iter().flatten().any(|run| run.face.as_deref().is_some_and(|face|
                face.is_empty() || face.starts_with('+') || face.chars().any(|c| c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}'))))
            || paragraphs.len() != self.runs.len() || self.size != Some(first.size) || self.face != first.face
            || self.bold != first.bold || self.italic != first.italic || self.underline != first.underline || self.colour != first.colour
            || self.runs.iter().zip(paragraphs).any(|(runs, text)| runs.iter().map(|run| run.text.as_str()).collect::<String>() != *text)
        { return None; }
        Some(&self.runs)
    }
}

/// An element of a part the drawing reader keeps as a tree (an equation's OMML, dc-zjsg.9): its local name, its
/// attributes (a namespaced one as `m:val`, `r:id`), its text and its children.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct DrawingNode {
    pub local: String,
    pub attributes: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<DrawingNode>,
}

impl DrawingNode {
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
    pub fn child(&self, local: &str) -> Option<&DrawingNode> {
        self.children.iter().find(|c| c.local == local)
    }
}

/// One end of a connector: the shape it is glued to and the connection site on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShapeConnection {
    /// The glued shape's `cNvPr id`.
    pub shape: u32,
    pub site: u32,
}

/// One drawing object.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shape {
    /// `cNvPr id`, unique in the drawing part.
    pub id: u32,
    /// `cNvPr name`, e.g. `Rectangle 1`.
    pub name: String,
    pub kind: ShapeKind,
    /// The anchor of a top-level object. A group's children carry their group's anchor.
    pub anchor: ChartAnchor,
    /// The anchor's `editAs`: `twoCell`, `oneCell` or `absolute`; `None` when the file omits it.
    pub edit_as: Option<String>,
    /// `cNvPr hidden`.
    pub hidden: bool,
    /// The anchor's `xdr:clientData fPrintsWithSheet` (true when absent): printed with the sheet.
    pub prints: bool,
    /// `a:prstGeom prst`, e.g. `rect`, `line`, `straightConnector1`; `None` for custom geometry.
    pub geometry: Option<String>,
    /// `a:xfrm`: offset and extent in EMU (inside a group: in the group's child space).
    pub offset: (i64, i64),
    pub extent: (i64, i64),
    /// `a:xfrm rot`, in 60,000ths of a degree.
    pub rotation: i64,
    /// A group's child space (`a:chOff`, `a:chExt`): what its children's offsets are given in.
    pub child_offset: (i64, i64),
    pub child_extent: (i64, i64),
    /// `a:custGeom`: a geometry drawn from its own paths (`geometry` is then `None`).
    pub custom_geometry: Option<CustomGeometry>,
    /// `xdr:style`: the theme references for what the shape's own properties leave out.
    pub style: Option<ShapeStyle>,
    /// `a:prstGeom a:avLst a:gd`: the preset's adjust values by name (`adj`, `adj1`, ...), from `fmla="val N"`.
    pub adjust: Vec<(String, i64)>,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub fill: ShapeFill,
    pub line: ShapeLine,
    /// `a:effectLst` `a:outerShdw` of the shape's properties.
    pub shadow: Option<ShapeShadow>,
    /// The text body's paragraphs, each the concatenation of its runs; `None` without a body.
    pub text: Option<Vec<String>>,
    /// The text body's properties (empty without a body).
    pub body: ShapeBody,
    /// A picture's `a:blip r:embed`.
    pub picture: Option<String>,
    /// The `r:embed` of the picture's `asvg:svgBlip`.
    pub picture_svg: Option<String>,
    /// The picture's media, crop and description (read from a file, or made for a new picture).
    pub media: Option<ShapePicture>,
    /// A connector's ends (`a:stCxn`, `a:endCxn`).
    pub begin: Option<ShapeConnection>,
    pub end: Option<ShapeConnection>,
    /// A group's children, in drawing order.
    pub children: Vec<Shape>,
    /// An object DNA Calc keeps as stored and draws from what Excel stores beside it (dc-zjsg.9): `equation`
    /// (its fallback picture), `ink` (its fallback picture), `SmartArt` (the drawing Excel caches) or `OLE object`
    /// (its picture). `None` for an ordinary object.
    pub kept: Option<String>,
    /// An equation's OMML (`a14:m`'s `m:oMathPara` or `m:oMath`), for drawing it from its own markup.
    pub math: Option<DrawingNode>,
    /// The relationship id of the part a kept object is drawn from (a SmartArt frame's data part, `r:dm`).
    pub source_part: Option<String>,
    /// The language (`a:rPr lang`) of the text runs a shape this session made writes: Excel writes the
    /// machine's install/regional language on every run (dc-piw0.10 Q15); `None` writes `en-US`.
    pub lang: Option<String>,
}

impl Shape {
    /// A new object with everything left to the style.
    pub fn new(id: u32, name: String, kind: ShapeKind, anchor: ChartAnchor) -> Self {
        Shape {
            id,
            name,
            kind,
            anchor,
            edit_as: None,
            hidden: false,
            prints: true,
            geometry: None,
            custom_geometry: None,
            style: None,
            offset: (0, 0),
            extent: (0, 0),
            rotation: 0,
            child_offset: (0, 0),
            child_extent: (0, 0),
            adjust: Vec::new(),
            flip_horizontal: false,
            flip_vertical: false,
            fill: ShapeFill::Style,
            line: ShapeLine::default(),
            shadow: None,
            text: None,
            body: ShapeBody::default(),
            picture: None,
            picture_svg: None,
            media: None,
            begin: None,
            end: None,
            children: Vec::new(),
            kept: None,
            math: None,
            source_part: None,
            lang: None,
        }
    }
}

/// A legacy form control (dc-zjsg.8): the worksheet's `control` with its `controlPr` anchor and its ctrlProp part's
/// `formControlPr`, as the file states them; the caption is the text of the hidden drawing shape Excel writes beside
/// it. Kept in the package as stored.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FormControl {
    /// `shapeId`.
    pub id: u32,
    pub name: String,
    pub anchor: ChartAnchor,
    /// `objectType`: `Button`, `CheckBox`, `Radio`, `List`, `Drop`, `Scroll`, `Spin`, `GBox`, `Label`, `EditBox`, `Dialog`.
    pub kind: String,
    pub caption: Option<String>,
    /// `checked`: `Checked`, `Mixed` or `Unchecked` (absent).
    pub checked: Option<String>,
    /// `fmlaLink`: the linked cell, as the file writes it (`$B$2`).
    pub link: Option<String>,
    /// `fmlaRange`: the input range of a list or combo box.
    pub range: Option<String>,
    pub value: Option<i64>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub increment: Option<i64>,
    pub page: Option<i64>,
    /// `sel`: the selected item of a list or combo box, from 1.
    pub selected: Option<i64>,
    pub drop_lines: Option<i64>,
    /// `horiz`: a horizontal scroll bar.
    pub horizontal: bool,
    /// `noThreeD`: drawn flat.
    pub flat: bool,
    /// The `controlPr macro` a button runs.
    pub macro_name: Option<String>,
}

/// A sheet's drawing objects and how many the reading left alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SheetShapes {
    pub shapes: Vec<Shape>,
    /// Anchors holding objects this model does not hold (kept in the package as stored).
    pub unread: usize,
    /// The same anchors by what they hold: the object's element (`graphicFrame` with the kind of
    /// its data, `AlternateContent`, `contentPart`, a `sp` or `pic` too malformed to read, ...).
    pub unread_by: std::collections::BTreeMap<String, usize>,
    /// The sheet's legacy form controls (dc-zjsg.8).
    pub controls: Vec<FormControl>,
}
