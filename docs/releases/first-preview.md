## Changes in the first Windows installer preview

- DNA Calc and DNA OneCalc each have a per-user Windows installer with a Start menu entry
  and an entry under Installed apps. DNA Calc adds **Open with** entries for Excel
  workbooks. OneCalc adds entries for XLL and XLAM add-ins. (dc-fk66.2)
- The grid fits rows with stacked text and draws imported outline, shadow and accounting
  underline. (dc-971p.63)

## Known gaps in this draft

- Clean-machine install, upgrade and uninstall are tracked in dc-fk66.3. The owner will
  review those results and the signed files before approving downloads.
- Windows hosts need the Microsoft Edge WebView2 Runtime. DNA Calc for the browser is
  available separately. Native XLL add-ins use the Windows desktop hosts.
- Excel pixel measurements for the new grid font effects remain open in dc-971p.62.
