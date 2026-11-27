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
    group("H O", "Format"),
    pending("H O I", "AutoFit Column Width"),
    pending("H O A", "AutoFit Row Height"),
    group("H O U", "Hide and Unhide"),
    pending("H O U C", "Hide Columns"),
    group("H B", "Borders"),
    pending("H L", "Conditional Formatting"),
    group("H F", "Editing"),
    pending("H F I", "Fill"),
    group("A V", "Data Tools"),
    pending("A V V", "Data Validation"),
    group("A S", "Sort"),
    pending("A S S", "Sort"),
    pending("A T", "Filter"),
    group("A W", "What-If Analysis"),
    pending("A W G", "Goal Seek"),
    pending("A W S", "Scenario Manager"),
    pending("M N", "Name Manager"),
    pending("M P", "Trace Precedents"),
    pending("M D", "Trace Dependents"),
    group("M A", "Remove Arrows"),
    pending("M A A", "Remove All Arrows"),
    command("M V", "Evaluate Formula", "explorer.dock"),
    group("W F", "Freeze Panes"),
    pending("W F F", "Freeze Panes"),
    group("W V", "Show"),
    pending("W V G", "Gridlines"),
    pending("W L", "Normal View"),
    pending("W P", "Page Layout View"),
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
