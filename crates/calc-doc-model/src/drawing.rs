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
    /// A pattern or picture fill: kept in the source, not held here.
    Other,
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
    pub bold: bool,
    pub italic: bool,
    pub colour: Option<ShapeColour>,
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
    /// `a:prstGeom a:avLst a:gd`: the preset's adjust values by name (`adj`, `adj1`, ...), from `fmla="val N"`.
    pub adjust: Vec<(String, i64)>,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub fill: ShapeFill,
    pub line: ShapeLine,
    /// The text body's paragraphs, each the concatenation of its runs; `None` without a body.
    pub text: Option<Vec<String>>,
    /// The text body's properties (empty without a body).
    pub body: ShapeBody,
    /// A picture's `a:blip r:embed`.
    pub picture: Option<String>,
    /// A connector's ends (`a:stCxn`, `a:endCxn`).
    pub begin: Option<ShapeConnection>,
    pub end: Option<ShapeConnection>,
    /// A group's children, in drawing order.
    pub children: Vec<Shape>,
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
            geometry: None,
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
            text: None,
            body: ShapeBody::default(),
            picture: None,
            begin: None,
            end: None,
            children: Vec::new(),
            lang: None,
        }
    }
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
}
