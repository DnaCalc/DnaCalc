//! Integration suites for this package.
//! xtask accept selects named cases under acceptance_cases.

#[path = "acceptance.rs"]
mod acceptance_cases;

#[path = "d1_arrays.rs"]
mod d1_arrays;

#[path = "d1_authoring.rs"]
mod d1_authoring;

#[path = "d1_file_form.rs"]
mod d1_file_form;

#[path = "d1_oxdoc.rs"]
mod d1_oxdoc;

#[path = "dc_4j0e_xls.rs"]
mod dc_4j0e_xls;

#[path = "dc_4j0e_xls_sweep.rs"]
mod dc_4j0e_xls_sweep;

#[path = "dc_5sr_xlsb.rs"]
mod dc_5sr_xlsb;

#[path = "dc_xuw_macro_sheet.rs"]
mod dc_xuw_macro_sheet;

#[path = "drawing_layer.rs"]
mod drawing_layer;

#[path = "drawing_schema_order.rs"]
mod drawing_schema_order;

#[path = "font_effects.rs"]
mod font_effects;

#[path = "preserved_sheets.rs"]
mod preserved_sheets;

#[path = "reader_tolerance.rs"]
mod reader_tolerance;

#[path = "session_file.rs"]
mod session_file;

#[path = "vba_udf_names.rs"]
mod vba_udf_names;

#[path = "vba_writer.rs"]
mod vba_writer;
