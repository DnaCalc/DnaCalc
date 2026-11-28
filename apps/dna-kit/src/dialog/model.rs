//! The model of a Classic dialog: tabs of fields, their values and Excel's dialog keys.
//!
//! Nothing here touches the page. A dialog family states its tabs and fields once ([`Spec`]),
//! the frame draws them, and this model answers what a key does, which fields the person
//! changed and what the dialog holds, so a family's commit is a function of [`State`] alone.

use dna_skin_runtime::commands::KeyInput;
use std::collections::BTreeMap;

pub type FieldId = &'static str;

/// What a field holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A check box. `None` is the greyed box of a selection that differs.
    Flag(Option<bool>),
    /// An option group, drop-down, list box or toggle. `None` when nothing is selected.
    Choice(Option<String>),
    /// An edit box, spin box or combo box. Empty when the selection differs.
    Text(String),
}

/// One entry of an option group or list. `&` in a label marks its access key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

impl Choice {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }

    /// Entries whose value is their label.
    pub fn plain(labels: &[&str]) -> Vec<Self> {
        labels.iter().map(|label| Self::new(*label, *label)).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Check,
    /// Option buttons.
    Options(Vec<Choice>),
    DropDown(Vec<Choice>),
    /// A list box showing this many rows.
    List(Vec<Choice>, u8),
    Edit { max_length: u16 },
    /// An edit box with spin buttons over whole numbers.
    Spin { min: i32, max: i32 },
    /// An edit box over a list: the Font tab's Font, Font style and Size.
    Combo(Vec<Choice>),
    /// A colour box. Its value is `auto` or six hexadecimal digits.
    Color { automatic: &'static str },
    /// A push button that acts inside the dialog.
    Button,
    /// A button that stays pressed while its value is not `none`.
    Toggle,
    /// Drawn by the family: a preview or a diagram.
    Custom,
    /// Text the dialog shows.
    Note,
}

impl Kind {
    /// Whether the control takes typed characters, so a bare letter is not an access key there.
    fn takes_text(&self) -> bool {
        matches!(
            self,
            Kind::DropDown(_) | Kind::List(..) | Kind::Edit { .. } | Kind::Spin { .. } | Kind::Combo(_) | Kind::Color { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub id: FieldId,
    /// The caption, with `&` before its access key as Excel underlines it.
    pub label: &'static str,
    pub kind: Kind,
    /// The caption of the group box the field sits in, or empty.
    pub group: &'static str,
}

impl Field {
    pub fn new(id: FieldId, label: &'static str, kind: Kind) -> Self {
        Self {
            id,
            label,
            kind,
            group: "",
        }
    }

    pub fn in_group(mut self, group: &'static str) -> Self {
        self.group = group;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub id: &'static str,
    pub label: &'static str,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    /// The dialog's name in the page: `data-dialog="format-cells"`.
    pub id: &'static str,
    pub title: &'static str,
    /// One tab draws no tab strip.
    pub tabs: Vec<Tab>,
    /// The default button's caption.
    pub accept: &'static str,
}

/// The caption without its access-key mark, and the key it names in lower case.
pub fn caption(label: &str) -> (String, Option<char>) {
    let mut text = String::with_capacity(label.len());
    let mut key = None;
    let mut letters = label.chars().peekable();
    while let Some(letter) = letters.next() {
        if letter != '&' {
            text.push(letter);
            continue;
        }
        match letters.next() {
            Some('&') => text.push('&'),
            Some(next) => {
                key.get_or_insert(next.to_lowercase().next().unwrap_or(next));
                text.push(next);
            }
            None => {}
        }
    }
    (text, key)
}

/// Where the keyboard focus is when a key arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// On a field of the dialog.
    Field(FieldId),
    /// On the tab strip.
    Tabs,
    /// On the default button, Cancel, or the dialog itself.
    Frame,
}

/// What a key does in the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// The focused control keeps the key.
    Keep,
    Accept,
    Cancel,
    /// Show this tab. The focus moves with it only from the tab strip.
    Show(usize),
    /// Move to this field.
    Focus(FieldId),
    /// Move to this check box and switch it.
    Switch(FieldId),
    /// Select this option of the group.
    Choose(FieldId, String),
    /// Press this button.
    Press(FieldId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub spec: Spec,
    pub tab: usize,
    values: BTreeMap<FieldId, Value>,
    opened: BTreeMap<FieldId, Value>,
    /// What the dialog says about an entry it cannot accept.
    pub message: Option<String>,
}

impl State {
    /// A dialog showing `values`. A field without a value starts clear, empty or unselected.
    pub fn new(spec: Spec, values: impl IntoIterator<Item = (FieldId, Value)>) -> Self {
        let mut all: BTreeMap<FieldId, Value> = spec
            .tabs
            .iter()
            .flat_map(|tab| &tab.fields)
            .filter_map(|field| {
                Some((field.id, match field.kind {
                    Kind::Check => Value::Flag(Some(false)),
                    Kind::Options(_) | Kind::DropDown(_) | Kind::List(..) | Kind::Toggle | Kind::Color { .. } => {
                        Value::Choice(None)
                    }
                    Kind::Edit { .. } | Kind::Spin { .. } | Kind::Combo(_) => Value::Text(String::new()),
                    Kind::Button | Kind::Custom | Kind::Note => return None,
                }))
            })
            .collect();
        all.extend(values);
        Self {
            spec,
            tab: 0,
            opened: all.clone(),
            values: all,
            message: None,
        }
    }

    pub fn field(&self, id: &str) -> Option<&Field> {
        self.spec.tabs.iter().flat_map(|tab| &tab.fields).find(|field| field.id == id)
    }

    pub fn value(&self, id: &str) -> Option<&Value> {
        self.values.get(id)
    }

    pub fn flag(&self, id: &str) -> Option<bool> {
        match self.values.get(id) {
            Some(Value::Flag(flag)) => *flag,
            _ => None,
        }
    }

    pub fn choice(&self, id: &str) -> Option<&str> {
        match self.values.get(id) {
            Some(Value::Choice(choice)) => choice.as_deref(),
            _ => None,
        }
    }

    pub fn text(&self, id: &str) -> &str {
        match self.values.get(id) {
            Some(Value::Text(text)) => text,
            _ => "",
        }
    }

    /// The whole number in a spin or edit box.
    pub fn number(&self, id: &str) -> Option<i32> {
        self.text(id).trim().parse().ok()
    }

    pub fn set(&mut self, id: FieldId, value: Value) {
        self.values.insert(id, value);
        self.message = None;
    }

    pub fn set_text(&mut self, id: FieldId, text: impl Into<String>) {
        self.set(id, Value::Text(text.into()));
    }

    pub fn set_choice(&mut self, id: FieldId, choice: impl Into<String>) {
        self.set(id, Value::Choice(Some(choice.into())));
    }

    pub fn set_flag(&mut self, id: FieldId, flag: bool) {
        self.set(id, Value::Flag(Some(flag)));
    }

    /// Replaces the entries of a list, drop-down or combo field, as a category change does.
    pub fn set_choices(&mut self, id: &str, choices: Vec<Choice>) {
        for field in self.spec.tabs.iter_mut().flat_map(|tab| &mut tab.fields) {
            if field.id == id {
                match &mut field.kind {
                    Kind::Options(list) | Kind::DropDown(list) | Kind::List(list, _) | Kind::Combo(list) => {
                        *list = choices;
                    }
                    _ => {}
                }
                return;
            }
        }
    }

    /// Whether the field no longer holds what the dialog opened with.
    pub fn changed(&self, id: &str) -> bool {
        self.values.get(id) != self.opened.get(id)
    }

    /// Records the current values as the ones the dialog opened with.
    pub fn settle(&mut self) {
        self.opened = self.values.clone();
    }

    /// A click on a check box. A box that opened greyed cycles through its three states, as
    /// Excel's does; any other switches between set and clear.
    pub fn switch(&mut self, id: FieldId) {
        let mixed = matches!(self.opened.get(id), Some(Value::Flag(None)));
        let next = match self.flag_state(id) {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) if mixed => None,
            Some(false) => Some(true),
        };
        self.set(id, Value::Flag(next));
    }

    fn flag_state(&self, id: &str) -> Option<bool> {
        self.flag(id)
    }

    pub fn show(&mut self, tab: usize) {
        if tab < self.spec.tabs.len() {
            self.tab = tab;
        }
    }

    /// The tab holding a field.
    pub fn tab_of(&self, id: &str) -> Option<usize> {
        self.spec.tabs.iter().position(|tab| tab.fields.iter().any(|field| field.id == id))
    }

    fn access(&self, key: char, usable: &dyn Fn(FieldId) -> bool) -> Option<KeyAction> {
        let tab = self.spec.tabs.get(self.tab)?;
        for field in tab.fields.iter().filter(|field| usable(field.id)) {
            if let Kind::Options(choices) = &field.kind
                && let Some(choice) = choices.iter().find(|choice| caption(&choice.label).1 == Some(key))
            {
                return Some(KeyAction::Choose(field.id, choice.value.clone()));
            }
            if caption(field.label).1 != Some(key) {
                continue;
            }
            return Some(match field.kind {
                Kind::Check => KeyAction::Switch(field.id),
                Kind::Button | Kind::Toggle => KeyAction::Press(field.id),
                Kind::Custom | Kind::Note => continue,
                _ => KeyAction::Focus(field.id),
            });
        }
        None
    }

    /// What a key does, by the rules of an Excel dialog box:
    ///
    /// - Enter chooses the default button, unless a push button has the focus;
    /// - Escape cancels;
    /// - Ctrl+Tab and Ctrl+Page Down show the next tab, Ctrl+Shift+Tab and Ctrl+Page Up the
    ///   previous, and on the tab strip the arrow keys, Home and End move between tabs;
    /// - Alt with a letter is the access key of a field on the tab shown, and so is the letter
    ///   alone while the focus is on a control that takes no typing.
    ///
    /// `usable` says whether a field is shown and enabled now.
    pub fn key(&self, input: &KeyInput, focus: Focus, usable: &dyn Fn(FieldId) -> bool) -> KeyAction {
        if input.composing || input.meta {
            return KeyAction::Keep;
        }
        let tabs = self.spec.tabs.len();
        let step = |forward: bool| {
            KeyAction::Show(if forward { (self.tab + 1) % tabs } else { (self.tab + tabs - 1) % tabs })
        };
        let plain = !input.ctrl && !input.alt;
        match input.key.as_str() {
            "Escape" if plain => return KeyAction::Cancel,
            "Enter" if plain && !input.shift => {
                let on_button = matches!(focus, Focus::Field(id)
                    if self.field(id).is_some_and(|field| matches!(field.kind, Kind::Button | Kind::Toggle)));
                return if on_button { KeyAction::Keep } else { KeyAction::Accept };
            }
            "Tab" if input.ctrl && !input.alt && tabs > 1 => return step(!input.shift),
            "PageDown" if input.ctrl && !input.alt && tabs > 1 => return step(true),
            "PageUp" if input.ctrl && !input.alt && tabs > 1 => return step(false),
            "ArrowRight" | "ArrowDown" if plain && focus == Focus::Tabs && tabs > 1 => return step(true),
            "ArrowLeft" | "ArrowUp" if plain && focus == Focus::Tabs && tabs > 1 => return step(false),
            "Home" if plain && focus == Focus::Tabs => return KeyAction::Show(0),
            "End" if plain && focus == Focus::Tabs => return KeyAction::Show(tabs.saturating_sub(1)),
            _ => {}
        }
        let mut letters = input.key.chars();
        let (Some(letter), None) = (letters.next(), letters.next()) else {
            return KeyAction::Keep;
        };
        if input.ctrl || input.alt_graph || !letter.is_alphanumeric() {
            return KeyAction::Keep;
        }
        let typing = match focus {
            Focus::Field(id) => self.field(id).is_none_or(|field| field.kind.takes_text()),
            Focus::Tabs | Focus::Frame => false,
        };
        if !input.alt && typing {
            return KeyAction::Keep;
        }
        let key = letter.to_lowercase().next().unwrap_or(letter);
        self.access(key, usable).unwrap_or(KeyAction::Keep)
    }

    /// Applies what [`State::key`] decided to the values. The frame moves the focus.
    pub fn apply(&mut self, action: &KeyAction) {
        match action {
            KeyAction::Show(tab) => self.show(*tab),
            KeyAction::Switch(id) => self.switch(id),
            KeyAction::Choose(id, value) => self.set(id, Value::Choice(Some(value.clone()))),
            KeyAction::Keep | KeyAction::Accept | KeyAction::Cancel | KeyAction::Focus(_) | KeyAction::Press(_) => {}
        }
    }

    /// The dialog as the page's test probe reports it.
    pub fn probe(&self) -> serde_json::Value {
        let fields: serde_json::Map<String, serde_json::Value> = self
            .values
            .iter()
            .map(|(id, value)| {
                ((*id).to_owned(), match value {
                    Value::Flag(flag) => serde_json::json!(flag),
                    Value::Choice(choice) => serde_json::json!(choice),
                    Value::Text(text) => serde_json::json!(text),
                })
            })
            .collect();
        serde_json::json!({
            "id": self.spec.id,
            "title": self.spec.title,
            "tab": self.spec.tabs.get(self.tab).map(|tab| tab.id),
            "tabs": self.spec.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
            "fields": fields,
            "message": self.message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Spec {
        Spec {
            id: "sample",
            title: "Sample",
            accept: "OK",
            tabs: vec![
                Tab {
                    id: "first",
                    label: "First",
                    fields: vec![
                        Field::new("wrap", "&Wrap text", Kind::Check),
                        Field::new("shrink", "Shrin&k to fit", Kind::Check),
                        Field::new("indent", "&Indent:", Kind::Spin { min: 0, max: 250 }),
                        Field::new("paste", "Paste", Kind::Options(vec![
                            Choice::new("all", "&All"),
                            Choice::new("values", "&Values"),
                        ])),
                        Field::new("clear", "&None", Kind::Button),
                    ],
                },
                Tab {
                    id: "second",
                    label: "Second",
                    fields: vec![Field::new("locked", "&Locked", Kind::Check)],
                },
                Tab {
                    id: "third",
                    label: "Third",
                    fields: vec![Field::new("name", "&Name && value:", Kind::Edit { max_length: 31 })],
                },
            ],
        }
    }

    fn key(key: &str) -> KeyInput {
        KeyInput {
            key: key.into(),
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
            alt_graph: false,
            composing: false,
        }
    }

    fn all(_: FieldId) -> bool {
        true
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_caption_names_its_access_key_and_keeps_a_doubled_ampersand() {
        assert_eq!(caption("Shrin&k to fit"), ("Shrink to fit".into(), Some('k')));
        assert_eq!(caption("&Name && value:"), ("Name & value:".into(), Some('n')));
        assert_eq!(caption("Sample"), ("Sample".into(), None));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn enter_accepts_escape_cancels_and_a_focused_button_keeps_enter() {
        let state = State::new(spec(), []);
        assert_eq!(state.key(&key("Enter"), Focus::Field("indent"), &all), KeyAction::Accept);
        assert_eq!(state.key(&key("Enter"), Focus::Frame, &all), KeyAction::Accept);
        assert_eq!(state.key(&key("Enter"), Focus::Field("clear"), &all), KeyAction::Keep);
        assert_eq!(state.key(&key("Escape"), Focus::Field("indent"), &all), KeyAction::Cancel);
        let composing = KeyInput { composing: true, ..key("Enter") };
        assert_eq!(state.key(&composing, Focus::Field("indent"), &all), KeyAction::Keep);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn control_tab_and_control_page_keys_walk_the_tabs_and_wrap() {
        let mut state = State::new(spec(), []);
        let ctrl = |name: &str, shift| KeyInput { ctrl: true, shift, ..key(name) };
        assert_eq!(state.key(&ctrl("Tab", false), Focus::Field("wrap"), &all), KeyAction::Show(1));
        assert_eq!(state.key(&ctrl("Tab", true), Focus::Field("wrap"), &all), KeyAction::Show(2));
        assert_eq!(state.key(&ctrl("PageUp", false), Focus::Frame, &all), KeyAction::Show(2));
        state.show(2);
        assert_eq!(state.key(&ctrl("PageDown", false), Focus::Frame, &all), KeyAction::Show(0));
        // The arrow keys move between tabs only on the tab strip.
        assert_eq!(state.key(&key("ArrowLeft"), Focus::Tabs, &all), KeyAction::Show(1));
        assert_eq!(state.key(&key("ArrowRight"), Focus::Tabs, &all), KeyAction::Show(0));
        assert_eq!(state.key(&key("ArrowRight"), Focus::Field("name"), &all), KeyAction::Keep);
        assert_eq!(state.key(&key("Home"), Focus::Tabs, &all), KeyAction::Show(0));
        assert_eq!(state.key(&key("End"), Focus::Tabs, &all), KeyAction::Show(2));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_access_key_acts_on_the_shown_tab_and_a_bare_letter_only_off_typing_controls() {
        let mut state = State::new(spec(), []);
        let alt = |name: &str| KeyInput { alt: true, ..key(name) };
        assert_eq!(state.key(&alt("k"), Focus::Field("indent"), &all), KeyAction::Switch("shrink"));
        assert_eq!(state.key(&alt("I"), Focus::Field("wrap"), &all), KeyAction::Focus("indent"));
        assert_eq!(state.key(&alt("v"), Focus::Frame, &all), KeyAction::Choose("paste", "values".into()));
        assert_eq!(state.key(&alt("n"), Focus::Frame, &all), KeyAction::Press("clear"));
        // Locked is on another tab.
        assert_eq!(state.key(&alt("l"), Focus::Frame, &all), KeyAction::Keep);
        // A bare letter works from a check box and from the frame, and types in a spin box.
        assert_eq!(state.key(&key("w"), Focus::Field("shrink"), &all), KeyAction::Switch("wrap"));
        assert_eq!(state.key(&key("w"), Focus::Frame, &all), KeyAction::Switch("wrap"));
        assert_eq!(state.key(&key("w"), Focus::Field("indent"), &all), KeyAction::Keep);
        // A disabled field has no access key.
        let disabled = |id: FieldId| id != "shrink";
        assert_eq!(state.key(&alt("k"), Focus::Frame, &disabled), KeyAction::Keep);
        // AltGr types a character on European layouts and is never an access key.
        let alt_graph = KeyInput { alt: true, ctrl: true, alt_graph: true, ..key("w") };
        assert_eq!(state.key(&alt_graph, Focus::Frame, &all), KeyAction::Keep);
        state.show(1);
        assert_eq!(state.key(&alt("l"), Focus::Frame, &all), KeyAction::Switch("locked"));
        state.apply(&KeyAction::Switch("locked"));
        assert_eq!(state.flag("locked"), Some(true));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_greyed_box_cycles_through_three_states_and_changes_are_tracked_per_field() {
        let mut state = State::new(spec(), [("wrap", Value::Flag(None)), ("indent", Value::Text("2".into()))]);
        assert!(!state.changed("wrap"));
        state.switch("wrap");
        assert_eq!(state.flag("wrap"), Some(true));
        assert!(state.changed("wrap"));
        state.switch("wrap");
        assert_eq!(state.flag("wrap"), Some(false));
        state.switch("wrap");
        assert_eq!(state.value("wrap"), Some(&Value::Flag(None)));
        assert!(!state.changed("wrap"), "back to the greyed box leaves the property alone");
        // A box that opened set or clear has two states.
        state.switch("shrink");
        state.switch("shrink");
        assert_eq!(state.flag("shrink"), Some(false));
        assert!(!state.changed("shrink"));
        state.set_text("indent", "3");
        assert_eq!(state.number("indent"), Some(3));
        assert!(state.changed("indent"));
        assert_eq!(state.tab_of("locked"), Some(1));
        state.message = Some("Enter a whole number.".into());
        state.set_text("indent", "4");
        assert_eq!(state.message, None, "an edit clears the alert");
        assert_eq!(state.probe()["fields"]["indent"], "4");
        assert_eq!(state.probe()["tab"], "first");
    }
}
