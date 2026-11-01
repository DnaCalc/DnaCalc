# UI action record (operator record from this Codex session)

Run: P07-ui3-safe. Owned Excel PID 38892, start/identity in the supervisor ledger. Target window ID 33292948,
title `P07-ui-owned.xlsm - Microsoft Excel - Safe Mode`. Computer Use (Sky) selected this returned window;
each input was followed by `get_window_state`. These are the actions sent in the session, recorded by the operator
from the tool calls. This file is not an independently generated keyboard-event log. Arm and collect acknowledgements,
native entry/return/cancellation events, values and package XML are independently retained alongside it.

| Cases | Arm/collect sequence numbers | UI inputs between arm and collect |
|---|---|---|
| ui-normal-01..10 | 1/2, 3/4, ..., 19/20 | F9 |
| ui-cancel-01..10 | 21/22, 23/24, ..., 39/40 | F9, Escape |
| ui-undo-01..10 | 41/42, 43/44, ..., 59/60 | click C1, F9, type_text("999"), Control_L+z |

The first Undo trial's returned screenshot showed C1=999 after type_text, then the original
`=XAfter(AsyncUI(XBefore(A1),B1))` formula after Ctrl+Z. The remaining trials repeated that observed input sequence
with a window-state refresh after each action. The final state returned the original formula text in every trial.
Screenshots were shown in the tool transcript; they were not saved as PNG files in this fixture. The numeric 999
is therefore a transient UI observation in this operator record, not a saved cache or a row of the VBA journal.

The earlier P07-ui2-safe session is a failed harness run: a non-atomic command-file replacement briefly removed
the file while the worker read it. It reached eight complete normal trials and one cancel trial, but these do not
replace the thirty completed P07-ui3-safe conditions. The successful retry uses atomic File.Move overwrite.

The separate P07-save-ui-safe run used owned PID 35468, window ID 48761758 with the same workbook title. Cases
ui-save-01..10 used arm/collect sequences 1/2 through 19/20: F9, refreshed window state, Ctrl+S, refreshed state,
then a filesystem copy of that run's P07-ui-owned.xlsm to ui-save-NN-on-disk.xlsm before Collect. The first trial
also clicked A1 to focus the worksheet after the VBE setup. Native logs independently show cancellation before
the late return; the package copies and VBA events distinguish this route from the delayed COM save attempts.
