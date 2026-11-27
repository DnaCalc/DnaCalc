pub use calc_skin::address::*;
use calc_value::SheetId;

impl SheetNames for crate::projection::Summary {
    fn sheet_name_of(&self, sheet: SheetId) -> Option<String> {
        self.sheets
            .iter()
            .find(|(s, _)| s.id() == sheet)
            .map(|(_, name)| name.clone())
            .or_else(|| {
                self.hidden_sheets
                    .iter()
                    .find(|(s, _, _)| s.id() == sheet)
                    .map(|(_, name, _)| name.clone())
            })
    }
}

impl<T: SheetNames> SheetNames for crate::Snapshot<T> {
    fn sheet_name_of(&self, sheet: SheetId) -> Option<String> {
        (**self).sheet_name_of(sheet)
    }
}
