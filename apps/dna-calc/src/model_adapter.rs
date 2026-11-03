//! Reactive handles into portable model state. The model is the only writable source;
//! memos cache read projections for the lifetime of the mounted view.
use leptos::prelude::*;

struct Accessors<M, T> {
    get: Box<dyn for<'a> Fn(&'a M) -> &'a T + Send + Sync>,
    get_mut: Box<dyn for<'a> Fn(&'a mut M) -> &'a mut T + Send + Sync>,
}

pub struct Field<M: Send + Sync + 'static, T: Send + Sync + 'static> {
    source: RwSignal<M>,
    read: Memo<T>,
    access: StoredValue<Accessors<M, T>>,
}
impl<M: Send + Sync, T: Send + Sync> Copy for Field<M, T> {}
impl<M: Send + Sync, T: Send + Sync> Clone for Field<M, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<M: Send + Sync + 'static, T: Clone + PartialEq + Send + Sync + 'static> Field<M, T> {
    pub fn new(
        source: RwSignal<M>,
        get: impl for<'a> Fn(&'a M) -> &'a T + Send + Sync + 'static,
        get_mut: impl for<'a> Fn(&'a mut M) -> &'a mut T + Send + Sync + 'static,
    ) -> Self {
        let initial = source.with_untracked(|model| get(model).clone());
        // The getters have one owner and retire together. One arena handle keeps fields small
        // when grid event closures capture the controller on the wasm stack.
        let access = StoredValue::new(Accessors {
            get: Box::new(get),
            get_mut: Box::new(get_mut),
        });
        let read = Memo::new(move |previous: Option<&T>| {
            // A dependent can retain this memo briefly while its view is being torn down.
            // Its getter belongs to that view; do not read a disposed getter or model.
            access
                .try_with_value(|a| source.try_with(|model| (a.get)(model).clone()))
                .flatten()
                .or_else(|| previous.cloned())
                .unwrap_or_else(|| initial.clone())
        });
        Self {
            source,
            read,
            access,
        }
    }
    /// Retire accessors before removing the model entry they address. Retained reactive
    /// dependencies can finish with their previous snapshot while cleanup removes that entry.
    pub(crate) fn retire(&self) {
        self.access.dispose();
    }
    pub fn get_value(&self) -> T {
        self.get_untracked()
    }
    pub fn with_value<U>(&self, f: impl FnOnce(&T) -> U) -> U {
        self.with_untracked(f)
    }
    pub fn set_value(&self, value: T) {
        self.set(value);
    }
    pub fn update_value(&self, f: impl FnOnce(&mut T)) {
        self.update(f);
    }
    pub fn try_set_value(&self, value: T) -> Option<T> {
        self.try_set(value)
    }
    pub fn try_update_value<U>(&self, f: impl FnOnce(&mut T) -> U) -> Option<U> {
        self.try_update(f)
    }
}
impl<M: Send + Sync + 'static, T: Send + Sync + 'static> DefinedAt for Field<M, T> {
    fn defined_at(&self) -> Option<&'static std::panic::Location<'static>> {
        self.source.defined_at()
    }
}
impl<M: Send + Sync + 'static, T: Send + Sync + 'static> IsDisposed for Field<M, T> {
    fn is_disposed(&self) -> bool {
        self.source.is_disposed() || self.access.is_disposed()
    }
}
impl<M: Send + Sync + 'static, T: Send + Sync + 'static> Track for Field<M, T> {
    fn track(&self) {
        self.read.track();
    }
}
impl<M: Send + Sync + 'static, T: Send + Sync + 'static> With for Field<M, T> {
    type Value = T;
    fn try_with<U>(&self, f: impl FnOnce(&T) -> U) -> Option<U> {
        if self.is_disposed() {
            return None;
        }
        self.read.try_with(f)
    }
}
impl<M: Send + Sync + 'static, T: Send + Sync + 'static> WithUntracked for Field<M, T> {
    type Value = T;
    fn try_with_untracked<U>(&self, f: impl FnOnce(&T) -> U) -> Option<U> {
        if self.is_disposed() {
            return None;
        }
        self.access
            .try_with_value(|a| self.source.try_with_untracked(|model| f((a.get)(model))))
            .flatten()
    }
}
impl<M: Send + Sync + 'static, T: Send + Sync + 'static> Update for Field<M, T> {
    type Value = T;
    fn try_maybe_update<U>(&self, f: impl FnOnce(&mut T) -> (bool, U)) -> Option<U> {
        if self.is_disposed() {
            return None;
        }
        self.access
            .try_with_value(|a| self.source.try_maybe_update(|model| f((a.get_mut)(model))))
            .flatten()
    }
}
impl<M: Send + Sync + 'static> Field<M, String> {
    pub fn text_binding(self) -> dna_formula_ui::formula_field::TextBinding {
        dna_formula_ui::formula_field::TextBinding::new(self.read.into(), move |text| {
            self.set(text)
        })
    }
}

pub type ModelField<T> = Field<dna_calc_model::Model, T>;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_retained_dependency_never_reopens_a_retired_field_getter() {
        let root = Owner::new();
        root.with(|| {
            let model = RwSignal::new(dna_calc_model::Model::default());
            let view = Owner::new();
            let field = view
                .with(|| Field::new(model, |m| &m.selection.active, |m| &mut m.selection.active));
            let retained: ArcMemo<(u32, u32)> = field.read.into();
            assert_eq!(retained.get_untracked(), (1, 1));
            view.cleanup();
            model.update(|m| {
                m.selection.active = (8, 9);
                m.runtime.font_revision += 1;
            });
            assert_eq!(retained.get_untracked(), (1, 1));
            assert_eq!(field.try_get_untracked(), None);
            assert_eq!(field.try_set((7, 7)), Some((7, 7)));
            assert_eq!(model.with_untracked(|m| m.selection.active), (8, 9));
        });
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn lazy_retained_dependency_survives_entry_removal_during_cleanup() {
        let root = Owner::new();
        root.with(|| {
            let source = RwSignal::new(Some(String::from("original")));
            let view = Owner::new();
            let field =
                view.with(|| Field::new(source, |m| m.as_ref().unwrap(), |m| m.as_mut().unwrap()));
            let retained: ArcMemo<String> = field.read.into();
            // No initial read: ArcMemo is lazy, including during view teardown.
            field.retire();
            source.set(None);
            assert_eq!(retained.get_untracked(), "original");
            view.cleanup();
            assert_eq!(retained.get_untracked(), "original");
            assert_eq!(field.try_get_untracked(), None);
            assert_eq!(
                field.try_set(String::from("late")),
                Some(String::from("late"))
            );
            assert_eq!(source.get_untracked(), None);
        });
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn retired_view_accessors_refuse_callbacks_without_disposing_the_model() {
        let root = Owner::new();
        root.with(|| {
            let model = RwSignal::new(dna_calc_model::Model::default());
            let view = Owner::new();
            let field = view
                .with(|| Field::new(model, |m| &m.selection.active, |m| &mut m.selection.active));
            field.set((2, 3));
            assert_eq!(field.get_untracked(), (2, 3));
            view.cleanup();
            assert!(field.is_disposed());
            assert_eq!(field.try_get_untracked(), None);
            assert_eq!(field.try_get(), None);
            assert_eq!(field.try_set((9, 9)), Some((9, 9)));
            assert_eq!(field.try_update(|active| *active = (9, 9)), None);
            assert!(!model.is_disposed());
            assert_eq!(model.with_untracked(|m| m.selection.active), (2, 3));
        });
    }
}

impl<M: Send + Sync + 'static, T: Clone + Send + Sync + 'static> Field<M, T> {
    pub fn state_binding(self) -> dna_formula_ui::formula_field::StateBinding<T> {
        dna_formula_ui::formula_field::StateBinding::new_with_update(
            self.read.into(),
            move |v| self.set(v),
            move |f| {
                self.try_update(|value| f(value));
            },
        )
    }
}
impl dna_formula_ui::formula_field::HistoryStore for ModelField<dna_calc_model::TextHistory> {
    fn available(&self, redo: bool) -> bool {
        self.with_untracked(|h| h.available(redo))
    }
    fn start(&self, original: String, text: String, end: usize) {
        self.update(|h| h.start(original, text, end));
    }
    fn record(&self, text: String, caret: usize) {
        self.update(|h| h.record(text, caret));
    }
    fn step(&self, redo: bool) -> Option<(String, usize)> {
        self.try_update(|h| h.step(redo)).flatten()
    }
}
