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
    /// A gradient, pattern or picture fill: kept in the source, not held here.
    Other,
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
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub fill: ShapeFill,
    pub line: ShapeLine,
    /// The text body's paragraphs, each the concatenation of its runs; `None` without a body.
    pub text: Option<Vec<String>>,
    /// A picture's `a:blip r:embed`.
    pub picture: Option<String>,
    /// A connector's ends (`a:stCxn`, `a:endCxn`).
    pub begin: Option<ShapeConnection>,
    pub end: Option<ShapeConnection>,
    /// A group's children, in drawing order.
    pub children: Vec<Shape>,
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
            flip_horizontal: false,
            flip_vertical: false,
            fill: ShapeFill::Style,
            line: ShapeLine::default(),
            text: None,
            picture: None,
            begin: None,
            end: None,
            children: Vec::new(),
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
