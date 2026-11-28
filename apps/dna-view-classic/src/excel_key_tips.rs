//! Windows Excel 365 en-US sequences listed in the Classic conformance table.

use dna_kit::commands::{KeyTip, KeyTipAction};

const fn group(path: &'static str, label: &'static str) -> KeyTip {
    KeyTip {
        path,
        label,
        action: KeyTipAction::Group,
        hidden: false,
    }
}

const fn command(path: &'static str, label: &'static str, id: &'static str) -> KeyTip {
    KeyTip {
        path,
        label,
        action: KeyTipAction::Command(id),
        hidden: false,
    }
}

const fn pending(path: &'static str, label: &'static str) -> KeyTip {
    KeyTip {
        path,
        label,
        action: KeyTipAction::NotModelled { owner: "dc-s2xup" },
        hidden: false,
    }
}

pub const EXCEL_KEY_TIPS: &[KeyTip] = &[
    group("F", "File"),
    group("H", "Home"),
    group("N", "Insert"),
    group("P", "Page Layout"),
    group("M", "Formulas"),
    group("A", "Data"),
    group("R", "Review"),
    group("W", "View"),
    group("L", "Developer"),
    command("Q", "Command Search", "shell.search"),
    group("H V", "Paste"),
    command("H V V", "Paste Values", "classic.paste-values"),
    command("H V F", "Paste Formulas", "classic.paste-formulas"),
    command("H V T", "Paste Transpose", "classic.paste-transpose"),
    command("H V S", "Paste Special", dna_kit::paste_special::OPEN),
    group("H O", "Format"),
    pending("H O I", "AutoFit Column Width"),
    pending("H O A", "AutoFit Row Height"),
    group("H O U", "Hide and Unhide"),
    command("H O U C", "Hide Columns", "grid.hide-columns"),
    group("H B", "Borders"),
    pending("H L", "Conditional Formatting"),
    group("H F", "Editing"),
    group("H F I", "Fill"),
    command("H F I D", "Down", "grid.fill-down"),
    command("H F I R", "Right", "grid.fill-right"),
    command("H F I U", "Up", "grid.fill-up"),
    command("H F I L", "Left", "grid.fill-left"),
    pending("H F I A", "Across Worksheets"),
    pending("H F I S", "Series"),
    pending("H F I J", "Justify"),
    command("H F I F", "Flash Fill", "grid.flash-fill"),
    group("A V", "Data Tools"),
    command("A V V", "Data Validation", dna_kit::data_validation::OPEN),
    group("A S", "Sort"),
    command("A S S", "Sort", dna_kit::sort_dialog::OPEN),
    command("A T", "Filter", "grid.toggle-filter"),
    group("A W", "What-If Analysis"),
    pending("A W G", "Goal Seek"),
    command("A W S", "Scenario Manager", dna_kit::scenario_manager::OPEN),
    command("M N", "Name Manager", dna_kit::name_manager::OPEN),
    command("M P", "Trace Precedents", "grid.trace-precedents"),
    command("M D", "Trace Dependents", "grid.trace-dependents"),
    group("M A", "Remove Arrows"),
    command("M A A", "Remove All Arrows", "grid.clear-traces"),
    command("M V", "Evaluate Formula", "explorer.dock"),
    group("W F", "Freeze Panes"),
    command("W F F", "Freeze Panes", "grid.freeze-panes"),
    group("W V", "Show"),
    command("W V G", "Gridlines", "grid.toggle-gridlines"),
    pending("W L", "Normal View"),
    pending("W P", "Page Layout View"),
    command("W U", "Customisation Studio", "studio.open"),
    pending("W I", "Page Break Preview"),
    group("R P", "Protect"),
    pending("R P S", "Protect Sheet"),
    // Excel 2003 menu accelerators are accepted alongside the Deck's displayed paths.
    KeyTip {
        hidden: true,
        ..group("E", "Edit")
    },
    group("E S", "Paste Special"),
    group("E S V", "Values"),
    command("E S V ENTER", "Paste Values", "classic.paste-values"),
];
