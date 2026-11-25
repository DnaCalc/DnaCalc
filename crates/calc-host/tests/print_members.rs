//! dc-oxh8w.8: the PageSetup members a macro uses, through the one command route the Page Setup UI
//! edits (`WorkbookCommand::PageSetup`). Each test names the member it pins. Defaults are those of
//! an absent OOXML attribute; the Excel measurements still to take are PS1 in RUN-ON-WINDOWS.

#![allow(non_snake_case)]

use calc_host::print_members::{PrintError, PrintValue as V, read, write};
use calc_host::{
    HostOptions, HostSession, MemorySource, OpenPolicy, PageSetupChange, SkinIntent,
    WorkbookCommand,
};
use calc_value::SheetId;
use std::sync::Arc;

fn open() -> (HostSession, SheetId) {
    let source: Arc<dyn calc_host::PackageRead> =
        Arc::new(MemorySource::new(calc_host::blank_package()));
    let session = HostSession::open(source, &OpenPolicy::default(), HostOptions::default())
        .expect("a blank workbook opens");
    let sheet = session.host().sheets()[0].id();
    (session, sheet)
}

fn apply(session: &mut HostSession, sheet: SheetId, change: PageSetupChange) {
    let revision = session.host().revision().get();
    let receipt = session.dispatch(SkinIntent::Command(WorkbookCommand::PageSetup {
        sheet,
        revision,
        change,
    }));
    assert!(receipt.accepted(), "{:?}", receipt.error);
}

/// Set a member as the macro would, then read it back.
fn set(session: &mut HostSession, sheet: SheetId, member: &str, value: V) -> V {
    let change = write(session.host(), sheet, member, &value).expect(member);
    // A write that changes nothing is not an error in Excel; the command layer says so.
    let revision = session.host().revision().get();
    let receipt = session.dispatch(SkinIntent::Command(WorkbookCommand::PageSetup {
        sheet,
        revision,
        change,
    }));
    assert!(receipt.accepted() || format!("{:?}", receipt.error).contains("no changes"), "{member}: {:?}", receipt.error);
    read(session.host(), sheet, member).expect(member)
}

fn get(session: &HostSession, sheet: SheetId, member: &str) -> V {
    read(session.host(), sheet, member).expect(member)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__defaults_of_an_untouched_sheet() {
    let (s, sheet) = open();
    for (member, want) in [
        ("Orientation", V::Int(1)),
        ("Zoom", V::Int(100)),
        ("FitToPagesWide", V::Int(1)),
        ("FitToPagesTall", V::Int(1)),
        ("LeftMargin", V::Real(0.7 * 72.0)),
        ("RightMargin", V::Real(0.7 * 72.0)),
        ("TopMargin", V::Real(0.75 * 72.0)),
        ("BottomMargin", V::Real(0.75 * 72.0)),
        ("HeaderMargin", V::Real(0.3 * 72.0)),
        ("FooterMargin", V::Real(0.3 * 72.0)),
        ("CenterHorizontally", V::Bool(false)),
        ("CenterVertically", V::Bool(false)),
        ("PrintGridlines", V::Bool(false)),
        ("PrintHeadings", V::Bool(false)),
        ("BlackAndWhite", V::Bool(false)),
        ("Draft", V::Bool(false)),
        ("Order", V::Int(1)),
        ("PaperSize", V::Int(1)),
        ("FirstPageNumber", V::Int(-4105)),
        ("PrintComments", V::Int(-4142)),
        ("PrintErrors", V::Int(0)),
        ("LeftHeader", V::Text(String::new())),
        ("CenterFooter", V::Text(String::new())),
        ("AlignMarginsHeaderFooter", V::Bool(true)),
        ("DifferentFirstPageHeaderFooter", V::Bool(false)),
        ("OddAndEvenPagesHeaderFooter", V::Bool(false)),
        ("ScaleWithDocHeaderFooter", V::Bool(true)),
        ("PrintArea", V::Text(String::new())),
        ("PrintTitleRows", V::Text(String::new())),
        ("PrintTitleColumns", V::Text(String::new())),
    ] {
        assert_eq!(get(&s, sheet, member), want, "{member}");
    }
    assert_eq!(calc_host::print_members::MEMBERS.len(), 34);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__orientation_zoom_and_fit_to_pages() {
    let (mut s, sheet) = open();
    assert_eq!(set(&mut s, sheet, "Orientation", V::Int(2)), V::Int(2));
    assert_eq!(set(&mut s, sheet, "Orientation", V::Int(1)), V::Int(1));
    assert_eq!(write(s.host(), sheet, "Orientation", &V::Int(3)), Err(PrintError::Refused));
    assert_eq!(write(s.host(), sheet, "Orientation", &V::Text("x".into())), Err(PrintError::Mismatch));
    assert_eq!(set(&mut s, sheet, "Zoom", V::Int(80)), V::Int(80));
    assert_eq!(write(s.host(), sheet, "Zoom", &V::Int(9)), Err(PrintError::Refused));
    assert_eq!(write(s.host(), sheet, "Zoom", &V::Int(401)), Err(PrintError::Refused));
    assert_eq!(write(s.host(), sheet, "Zoom", &V::Bool(true)), Err(PrintError::Refused));
    // Zoom = False: fit-to-pages is in force, then FitToPages* are read as set.
    assert_eq!(set(&mut s, sheet, "Zoom", V::Bool(false)), V::Bool(false));
    assert_eq!(set(&mut s, sheet, "FitToPagesWide", V::Int(2)), V::Int(2));
    assert_eq!(set(&mut s, sheet, "FitToPagesTall", V::Bool(false)), V::Bool(false));
    assert_eq!(get(&s, sheet, "FitToPagesWide"), V::Int(2));
    // A number brings the scale back and leaves the fit counts alone.
    assert_eq!(set(&mut s, sheet, "Zoom", V::Int(125)), V::Int(125));
    assert_eq!(get(&s, sheet, "FitToPagesWide"), V::Int(2));
    assert_eq!(write(s.host(), sheet, "FitToPagesWide", &V::Int(-1)), Err(PrintError::Refused));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__margins_are_points() {
    let (mut s, sheet) = open();
    for (member, points) in [
        ("LeftMargin", 36.0),
        ("RightMargin", 54.0),
        ("TopMargin", 72.0),
        ("BottomMargin", 18.0),
        ("HeaderMargin", 21.6),
        ("FooterMargin", 0.0),
    ] {
        assert_eq!(set(&mut s, sheet, member, V::Real(points)), V::Real(points), "{member}");
    }
    // Another margin was not disturbed by the six writes.
    assert_eq!(get(&s, sheet, "LeftMargin"), V::Real(36.0));
    assert_eq!(write(s.host(), sheet, "LeftMargin", &V::Real(-1.0)), Err(PrintError::Refused));
    assert_eq!(write(s.host(), sheet, "LeftMargin", &V::Bool(true)), Err(PrintError::Mismatch));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__print_options_and_flags() {
    let (mut s, sheet) = open();
    for member in [
        "CenterHorizontally",
        "CenterVertically",
        "PrintGridlines",
        "PrintHeadings",
        "BlackAndWhite",
        "Draft",
        "DifferentFirstPageHeaderFooter",
        "OddAndEvenPagesHeaderFooter",
    ] {
        assert_eq!(set(&mut s, sheet, member, V::Bool(true)), V::Bool(true), "{member}");
    }
    for member in ["AlignMarginsHeaderFooter", "ScaleWithDocHeaderFooter"] {
        assert_eq!(set(&mut s, sheet, member, V::Bool(false)), V::Bool(false), "{member}");
    }
    assert_eq!(write(s.host(), sheet, "Draft", &V::Int(1)), Err(PrintError::Mismatch));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__order_paper_first_page_comments_errors() {
    let (mut s, sheet) = open();
    assert_eq!(set(&mut s, sheet, "Order", V::Int(2)), V::Int(2));
    assert_eq!(write(s.host(), sheet, "Order", &V::Int(3)), Err(PrintError::Refused));
    assert_eq!(set(&mut s, sheet, "PaperSize", V::Int(9)), V::Int(9));
    assert_eq!(write(s.host(), sheet, "PaperSize", &V::Int(0)), Err(PrintError::Refused));
    assert_eq!(set(&mut s, sheet, "FirstPageNumber", V::Int(5)), V::Int(5));
    assert_eq!(set(&mut s, sheet, "FirstPageNumber", V::Int(-4105)), V::Int(-4105));
    assert_eq!(set(&mut s, sheet, "PrintComments", V::Int(1)), V::Int(1));
    assert_eq!(set(&mut s, sheet, "PrintComments", V::Int(16)), V::Int(16));
    assert_eq!(set(&mut s, sheet, "PrintComments", V::Int(-4142)), V::Int(-4142));
    assert_eq!(write(s.host(), sheet, "PrintComments", &V::Int(2)), Err(PrintError::Refused));
    for code in 0..=3 {
        assert_eq!(set(&mut s, sheet, "PrintErrors", V::Int(code)), V::Int(code));
    }
    assert_eq!(write(s.host(), sheet, "PrintErrors", &V::Int(4)), Err(PrintError::Refused));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__headers_and_footers_keep_their_neighbour_sections() {
    let (mut s, sheet) = open();
    assert_eq!(set(&mut s, sheet, "CenterHeader", V::Text("Report &P".into())), V::Text("Report &P".into()));
    assert_eq!(set(&mut s, sheet, "LeftHeader", V::Text("&D".into())), V::Text("&D".into()));
    assert_eq!(set(&mut s, sheet, "RightHeader", V::Text("Bob".into())), V::Text("Bob".into()));
    assert_eq!(get(&s, sheet, "CenterHeader"), V::Text("Report &P".into()));
    let stored = s.host().model().definitions().page_setup(sheet);
    let header = stored.headers.get("oddHeader").unwrap();
    assert_eq!(String::from_utf16_lossy(header.utf16_code_units()), "&L&D&CReport &P&RBob");
    // Footer is its own string; clearing the last section removes the attribute.
    assert_eq!(set(&mut s, sheet, "RightFooter", V::Text("x".into())), V::Text("x".into()));
    assert_eq!(set(&mut s, sheet, "RightFooter", V::Text(String::new())), V::Text(String::new()));
    assert!(s.host().model().definitions().page_setup(sheet).headers.get("oddFooter").is_none());
    assert_eq!(write(s.host(), sheet, "LeftHeader", &V::Int(1)), Err(PrintError::Mismatch));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__print_area_and_titles() {
    let (mut s, sheet) = open();
    assert_eq!(set(&mut s, sheet, "PrintArea", V::Text("A1:C10".into())), V::Text("$A$1:$C$10".into()));
    assert_eq!(set(&mut s, sheet, "PrintArea", V::Text("$A$1:$B$2,$D$1:$E$2".into())), V::Text("$A$1:$B$2,$D$1:$E$2".into()));
    assert_eq!(set(&mut s, sheet, "PrintArea", V::Text("C3".into())), V::Text("$C$3".into()));
    assert_eq!(set(&mut s, sheet, "PrintArea", V::Bool(false)), V::Text(String::new()));
    assert_eq!(write(s.host(), sheet, "PrintArea", &V::Text("nonsense!!".into())), Err(PrintError::Refused));
    // Titles: rows and columns share one name and keep each other.
    assert_eq!(set(&mut s, sheet, "PrintTitleRows", V::Text("$1:$2".into())), V::Text("$1:$2".into()));
    assert_eq!(set(&mut s, sheet, "PrintTitleColumns", V::Text("$A:$B".into())), V::Text("$A:$B".into()));
    assert_eq!(get(&s, sheet, "PrintTitleRows"), V::Text("$1:$2".into()));
    assert_eq!(set(&mut s, sheet, "PrintTitleRows", V::Text(String::new())), V::Text(String::new()));
    assert_eq!(get(&s, sheet, "PrintTitleColumns"), V::Text("$A:$B".into()));
    assert_eq!(write(s.host(), sheet, "PrintTitleRows", &V::Text("A1:B2".into())), Err(PrintError::Refused));
    assert_eq!(write(s.host(), sheet, "PrintTitleColumns", &V::Text("$1:$2".into())), Err(PrintError::Refused));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__settings_survive_save_and_reopen() {
    let (mut s, sheet) = open();
    set(&mut s, sheet, "Orientation", V::Int(2));
    set(&mut s, sheet, "LeftMargin", V::Real(36.0));
    set(&mut s, sheet, "CenterHeader", V::Text("Title".into()));
    set(&mut s, sheet, "PrintArea", V::Text("A1:B5".into()));
    set(&mut s, sheet, "PrintTitleRows", V::Text("$1:$1".into()));
    let mut bytes = Vec::new();
    s.save(&calc_host::SavePolicy::default(), &mut bytes).unwrap();
    let source: Arc<dyn calc_host::PackageRead> = Arc::new(MemorySource::new(bytes));
    let again = HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).unwrap();
    let sheet = again.host().sheets()[0].id();
    assert_eq!(get(&again, sheet, "Orientation"), V::Int(2));
    assert_eq!(get(&again, sheet, "LeftMargin"), V::Real(36.0));
    assert_eq!(get(&again, sheet, "CenterHeader"), V::Text("Title".into()));
    assert_eq!(get(&again, sheet, "PrintArea"), V::Text("$A$1:$B$5".into()));
    assert_eq!(get(&again, sheet, "PrintTitleRows"), V::Text("$1:$1".into()));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_8__manual_breaks_follow_the_stored_breaks() {
    use calc_host::print_members::{manual_breaks, with_break, without_break};
    let (mut s, sheet) = open();
    let setup = s.host().model().definitions().page_setup(sheet);
    let rows = with_break(&setup, true, 21).unwrap();
    let columns = with_break(&setup, false, 5).unwrap();
    apply(&mut s, sheet, PageSetupChange { row_breaks: Some(rows), column_breaks: Some(columns), ..Default::default() });
    let setup = s.host().model().definitions().page_setup(sheet);
    assert_eq!(manual_breaks(&setup, true).iter().map(|b| b.id).collect::<Vec<_>>(), [20]);
    assert_eq!(manual_breaks(&setup, false).iter().map(|b| b.id).collect::<Vec<_>>(), [4]);
    apply(&mut s, sheet, PageSetupChange { row_breaks: Some(without_break(&setup, true, 21)), ..Default::default() });
    let setup = s.host().model().definitions().page_setup(sheet);
    assert!(manual_breaks(&setup, true).is_empty());
    assert_eq!(manual_breaks(&setup, false).len(), 1);
}

/// dc-oxh8w.11: the page count and the breaks come from the plan print preview draws. Letter portrait A1:J120 is
/// Excel PB1 c1 (6 pages, H A48 and A95, V J1); landscape and 150 per cent change the count as PB1 c3/c6 do for their
/// layouts. The descriptor has no `Pages` type yet (regenerate on Windows), so this reads the host route directly.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_11_page_count_follows_the_setup() {
    use calc_host::print_plan::{page_count, print_geometry, print_plan};
    let (mut s, sheet) = open();
    assert_eq!(page_count(s.host(), sheet), Ok(0));
    let key = calc_host::value::CellKey::new(sheet, 120, 10).unwrap();
    assert!(s.dispatch(SkinIntent::EnterCell { at: key, text: "1".into() }).accepted());
    assert_eq!(page_count(s.host(), sheet), Ok(6));
    let plan = print_plan(s.host(), sheet, &print_geometry(s.host(), sheet), (0, 0)).unwrap();
    let breaks = plan.breaks();
    assert_eq!(breaks.rows, [(48, false), (95, false)]);
    assert_eq!(breaks.columns, [(10, false)]);
    // PB1 c6: scale 150 on A1:J120 is 8 pages.
    set(&mut s, sheet, "Zoom", V::Int(150));
    assert_eq!(page_count(s.host(), sheet), Ok(8));
}
