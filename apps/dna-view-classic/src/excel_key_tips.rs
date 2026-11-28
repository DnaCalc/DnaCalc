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

const fn quick_access(path: &'static str) -> KeyTip {
    KeyTip {
        path,
        label: "Quick Access",
        action: KeyTipAction::QuickAccess,
        hidden: false,
    }
}

const fn command_group(path: &'static str, label: &'static str, id: &'static str) -> KeyTip {
    KeyTip {
        path,
        label,
        action: KeyTipAction::CommandGroup(id),
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
    quick_access("1"),
    quick_access("2"),
    quick_access("3"),
    quick_access("4"),
    quick_access("5"),
    quick_access("6"),
    quick_access("7"),
    quick_access("8"),
    quick_access("9"),
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
    command("H O U R", "Hide Rows", "grid.hide-rows"),
    command("H O U O", "Unhide Rows", "grid.unhide-rows"),
    command("H O U L", "Unhide Columns", "grid.unhide-columns"),
    group("H B", "Borders"),
    command("H B A", "All Borders", "grid.all-borders"),
    command("H B S", "Outside Borders", "grid.outline-border"),
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
    command("W F R", "Freeze Top Row", "grid.freeze-top-row"),
    command("W F C", "Freeze First Column", "grid.freeze-first-column"),
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
    command_group("E S", "Paste Special", dna_kit::paste_special::OPEN),
    command_group("E S V", "Values", dna_kit::paste_special::SELECT_VALUES),
    command("E S V ENTER", "Paste Values", "classic.paste-values"),
];
