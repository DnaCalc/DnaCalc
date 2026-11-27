//! The command table and the keymap.
//!
//! Everything a person, a skin, a macro or an agent can ask the page to do is a command with an
//! identity. A key chord, a Deck button, the command search, a skin's named sequence and a
//! `DnaCalc.View` request all end in [`CommandTable::run`]; none of them calls a view or the shell
//! directly. The keymap turns a key press into a command identity and nothing else.
//!
//! Resolution order (SKIN_ARCHITECTURE.md 8.3): the safety keys, the focused editor, the focused
//! component, the active skin, the active view's profile, the shell. A skin can bind any chord
//! except a safety key, and every binding that changes what the view's profile does is counted.

use calc_skin::{KeyBinding, ViewId};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(pub Cow<'static, str>);

impl CommandId {
    pub const fn new(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CommandId {
    fn from(id: &str) -> Self {
        Self(Cow::Owned(id.to_owned()))
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Excel's input modes, plus the two that other views add.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputMode {
    #[default]
    Ready,
    Enter,
    Edit,
    Point,
    Object,
    KeyTips,
    NotebookCommand,
}

impl InputMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Enter => "enter",
            Self::Edit => "edit",
            Self::Point => "point",
            Self::Object => "object",
            Self::KeyTips => "key-tips",
            Self::NotebookCommand => "notebook-command",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Enter => "Enter",
            Self::Edit => "Edit",
            Self::Point => "Point",
            Self::Object => "Object",
            Self::KeyTips => "Key tips",
            Self::NotebookCommand => "Command",
        }
    }

    /// Text is being typed into a draft.
    pub fn editing(self) -> bool {
        matches!(self, Self::Enter | Self::Edit | Self::Point)
    }

    fn parse(key: &str) -> Option<Self> {
        [
            Self::Ready,
            Self::Enter,
            Self::Edit,
            Self::Point,
            Self::Object,
            Self::KeyTips,
            Self::NotebookCommand,
        ]
        .into_iter()
        .find(|mode| mode.key() == key)
    }
}

/// Where the page stands when a key arrives or a command is asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub view: ViewId,
    pub mode: InputMode,
    pub read_only: bool,
}

/// A condition on the context: a conjunction of `view:<id>`, `mode:<mode>`, `editing`, `writable`
/// and their negations with `!`. The skin manifest writes it as `view:classic && mode:ready`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct When(Vec<(bool, Atom)>);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Atom {
    View(ViewId),
    Mode(InputMode),
    Editing,
    Writable,
}

impl When {
    pub const ALWAYS: Self = Self(Vec::new());

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut atoms = Vec::new();
        for part in text.split("&&").map(str::trim).filter(|p| !p.is_empty()) {
            let (wanted, part) = match part.strip_prefix('!') {
                Some(rest) => (false, rest.trim()),
                None => (true, part),
            };
            let atom = match part.split_once(':') {
                Some(("view", id)) if !id.is_empty() => Atom::View(ViewId(id.to_owned())),
                Some(("mode", mode)) => Atom::Mode(
                    InputMode::parse(mode).ok_or_else(|| format!("unknown input mode {mode}"))?,
                ),
                None if part == "editing" => Atom::Editing,
                None if part == "writable" => Atom::Writable,
                _ => return Err(format!("unknown condition {part}")),
            };
            atoms.push((wanted, atom));
        }
        Ok(Self(atoms))
    }

    pub fn view(view: &str) -> Self {
        Self(vec![(true, Atom::View(ViewId(view.to_owned())))])
    }

    pub fn and(mut self, other: Self) -> Self {
        self.0.extend(other.0);
        self
    }

    pub fn holds(&self, context: &Context) -> bool {
        self.0.iter().all(|(wanted, atom)| {
            *wanted
                == match atom {
                    Atom::View(view) => *view == context.view,
                    Atom::Mode(mode) => *mode == context.mode,
                    Atom::Editing => context.mode.editing(),
                    Atom::Writable => !context.read_only,
                }
        })
    }
}

/// A key with its modifiers, in one spelling: `Ctrl+Alt+Shift+Meta+Key`. A letter is upper case.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub key: String,
}

impl Chord {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut chord = Self {
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
            key: String::new(),
        };
        // "Ctrl++" binds the plus key: the last part is the key even when it is empty.
        let mut parts: Vec<&str> = text.split('+').collect();
        let mut key = parts.pop().unwrap_or_default();
        if key.is_empty() && text.ends_with('+') {
            parts.pop();
            key = "+";
        }
        for part in parts {
            match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "control" => chord.ctrl = true,
                "alt" => chord.alt = true,
                "shift" => chord.shift = true,
                "meta" | "cmd" | "win" => chord.meta = true,
                other => return Err(format!("unknown modifier {other}")),
            }
        }
        chord.key = normal_key(key.trim());
        if chord.key.is_empty() {
            return Err(format!("no key in {text}"));
        }
        Ok(chord)
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (held, name) in [
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
            (self.meta, "Meta+"),
        ] {
            if held {
                f.write_str(name)?;
            }
        }
        f.write_str(&self.key)
    }
}

fn normal_key(key: &str) -> String {
    let mut letters = key.chars();
    match (letters.next(), letters.next()) {
        (Some(only), None) => only.to_uppercase().collect(),
        _ if key.eq_ignore_ascii_case("esc") => "Escape".into(),
        _ => key.to_owned(),
    }
}

/// A key press as the browser reports it, with no DOM type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyInput {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    /// AltGr is held (`getModifierState("AltGraph")`). Browsers report it as Ctrl+Alt.
    pub alt_graph: bool,
    /// An input method is composing text.
    pub composing: bool,
}

impl KeyInput {
    pub fn chord(&self) -> Chord {
        Chord {
            ctrl: self.ctrl,
            alt: self.alt,
            shift: self.shift,
            meta: self.meta,
            key: normal_key(&self.key),
        }
    }

    /// The press types a character: AltGr with a printable key, or composition. It never runs a
    /// command (decision D12: character entry wins over `Ctrl+Alt+<digit>`).
    fn types_text(&self) -> bool {
        self.composing || (self.alt_graph && self.key.chars().count() == 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// Never rebound by a skin or a view: Esc, Ctrl+Z, Ctrl+Y, Alt+Q, the return to Classic.
    Safety,
    Editor,
    Component,
    Skin,
    View,
    Shell,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub layer: Layer,
    pub when: When,
    pub chord: Chord,
    pub command: CommandId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Command(CommandId),
    /// The press is text entry; leave it to the focused editor.
    Text,
    Unbound,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeymapError {
    Chord { chord: String, reason: String },
    When { when: String, reason: String },
    UnknownCommand(String),
    /// A skin tried to bind a safety key.
    Protected(Chord),
}

#[derive(Clone, Debug, Default)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Keymap {
    /// Adds a built-in binding. Within one layer a later binding wins.
    pub fn bind(&mut self, layer: Layer, when: When, chord: &str, command: CommandId) {
        let chord = Chord::parse(chord).expect("a built-in chord parses");
        self.bindings.push(Binding {
            layer,
            when,
            chord,
            command,
        });
    }

    /// Replaces the skin layer with a manifest's bindings. Every problem is reported and nothing
    /// is applied unless all bindings are valid.
    pub fn set_skin(
        &mut self,
        bindings: &[KeyBinding],
        commands: &CommandTable,
    ) -> Result<(), Vec<KeymapError>> {
        let mut errors = Vec::new();
        let mut parsed = Vec::new();
        for binding in bindings {
            let chord = Chord::parse(&binding.chord).map_err(|reason| KeymapError::Chord {
                chord: binding.chord.clone(),
                reason,
            });
            let when = When::parse(&binding.when).map_err(|reason| KeymapError::When {
                when: binding.when.clone(),
                reason,
            });
            let command = CommandId::from(binding.command.as_str());
            if commands.spec(&command).is_none() {
                errors.push(KeymapError::UnknownCommand(binding.command.clone()));
            }
            match (chord, when) {
                (Ok(chord), Ok(when)) => {
                    if self.is_protected(&chord) {
                        errors.push(KeymapError::Protected(chord));
                    } else {
                        parsed.push(Binding {
                            layer: Layer::Skin,
                            when,
                            chord,
                            command,
                        });
                    }
                }
                (chord, when) => errors.extend(chord.err().into_iter().chain(when.err())),
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        self.bindings.retain(|b| b.layer != Layer::Skin);
        self.bindings.extend(parsed);
        Ok(())
    }

    fn is_protected(&self, chord: &Chord) -> bool {
        self.bindings
            .iter()
            .any(|b| b.layer == Layer::Safety && b.chord == *chord)
    }

    pub fn resolve(&self, input: &KeyInput, context: &Context) -> Resolution {
        if input.types_text() {
            return Resolution::Text;
        }
        let chord = input.chord();
        self.lookup(&chord, context, Layer::Safety)
            .map_or(Resolution::Unbound, |b| {
                Resolution::Command(b.command.clone())
            })
    }

    /// The winning binding at or below `from` in the resolution order.
    fn lookup(&self, chord: &Chord, context: &Context, from: Layer) -> Option<&Binding> {
        self.bindings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.layer >= from && b.chord == *chord && b.when.holds(context))
            // The first layer in the order wins; within a layer the latest binding.
            .min_by_key(|(index, b)| (b.layer, std::cmp::Reverse(*index)))
            .map(|(_, b)| b)
    }

    /// The chords a command answers to in this context, for menus, tooltips and the search list.
    pub fn chords_for(&self, command: &CommandId, context: &Context) -> Vec<Chord> {
        let mut chords: Vec<Chord> = self
            .bindings
            .iter()
            .filter(|b| b.command == *command && b.when.holds(context))
            .filter(|b| {
                self.lookup(&b.chord, context, Layer::Safety)
                    .is_some_and(|winner| winner.command == *command)
            })
            .map(|b| b.chord.clone())
            .collect();
        chords.sort();
        chords.dedup();
        chords
    }

    /// The chords where the skin makes the view's own profile do something else
    /// ("Departs from Excel in 2 keys").
    pub fn departures(&self, context: &Context) -> Vec<Chord> {
        let mut chords: Vec<Chord> = self
            .bindings
            .iter()
            .filter(|b| b.layer == Layer::Skin && b.when.holds(context))
            .filter(|skin| {
                self.lookup(&skin.chord, context, Layer::View)
                    .is_some_and(|profile| profile.command != skin.command)
            })
            .map(|b| b.chord.clone())
            .collect();
        chords.sort();
        chords.dedup();
        chords
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: CommandId,
    pub label: Cow<'static, str>,
    /// When the command can run. The Deck greys it out and the search list hides it otherwise.
    pub enabled: When,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    Unknown(CommandId),
    /// The command exists and its condition does not hold here.
    NotNow(CommandId),
    /// The command ran and refused; the text is for the Strip.
    Refused(String),
}

/// One request to run a command. `args` is the command's own JSON, `null` when it takes none.
#[derive(Clone, Debug, PartialEq)]
pub struct Invocation {
    pub command: CommandId,
    pub args: serde_json::Value,
}

impl Invocation {
    pub fn of(command: CommandId) -> Self {
        Self {
            command,
            args: serde_json::Value::Null,
        }
    }
}

type Handler = Rc<dyn Fn(&Invocation) -> Result<(), String>>;

#[derive(Clone, Default)]
pub struct CommandTable {
    commands: BTreeMap<CommandId, (CommandSpec, Handler)>,
}

impl CommandTable {
    /// Registers a command. Registering an identity twice is a programming error: two owners of
    /// one command would make a key do different things depending on load order.
    pub fn register(
        &mut self,
        spec: CommandSpec,
        handler: impl Fn(&Invocation) -> Result<(), String> + 'static,
    ) {
        let id = spec.id.clone();
        let previous = self.commands.insert(id.clone(), (spec, Rc::new(handler)));
        assert!(previous.is_none(), "command {id} registered twice");
    }

    /// Registers a skin's named sequence. It runs its steps in order and stops at the first that
    /// refuses. Every step must already be registered.
    pub fn register_sequence(
        &mut self,
        id: &str,
        label: &str,
        steps: Vec<Invocation>,
    ) -> Result<(), CommandError> {
        if let Some(step) = steps.iter().find(|s| !self.commands.contains_key(&s.command)) {
            return Err(CommandError::Unknown(step.command.clone()));
        }
        let handlers: Vec<(Invocation, Handler)> = steps
            .into_iter()
            .map(|step| {
                let handler = self.commands[&step.command].1.clone();
                (step, handler)
            })
            .collect();
        self.register(
            CommandSpec {
                id: CommandId::from(id),
                label: Cow::Owned(label.to_owned()),
                enabled: When::ALWAYS,
            },
            move |_| handlers.iter().try_for_each(|(step, run)| run(step)),
        );
        Ok(())
    }

    pub fn spec(&self, id: &CommandId) -> Option<&CommandSpec> {
        self.commands.get(id).map(|(spec, _)| spec)
    }

    pub fn can_run(&self, id: &CommandId, context: &Context) -> bool {
        self.spec(id).is_some_and(|spec| spec.enabled.holds(context))
    }

    pub fn run(&self, invocation: &Invocation, context: &Context) -> Result<(), CommandError> {
        let (spec, handler) = self
            .commands
            .get(&invocation.command)
            .ok_or_else(|| CommandError::Unknown(invocation.command.clone()))?;
        if !spec.enabled.holds(context) {
            return Err(CommandError::NotNow(invocation.command.clone()));
        }
        handler(invocation).map_err(CommandError::Refused)
    }

    /// The commands that can run here whose label or identity contains every word of `text`,
    /// labels that start with the text first. An empty text lists them all.
    pub fn search(&self, text: &str, context: &Context) -> Vec<&CommandSpec> {
        let text = text.trim().to_lowercase();
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut found: Vec<&CommandSpec> = self
            .commands
            .values()
            .map(|(spec, _)| spec)
            .filter(|spec| spec.enabled.holds(context))
            .filter(|spec| {
                let label = spec.label.to_lowercase();
                words
                    .iter()
                    .all(|word| label.contains(word) || spec.id.as_str().contains(word))
            })
            .collect();
        found.sort_by_key(|spec| {
            (
                !spec.label.to_lowercase().starts_with(&text),
                spec.label.to_lowercase(),
            )
        });
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const UNDO: CommandId = CommandId::new("edit.undo");
    const DOWN: CommandId = CommandId::new("grid.move-down");
    const NEXT_ISSUE: CommandId = CommandId::new("inspector.next-issue");
    const CLASSIC: CommandId = CommandId::new("view.classic");

    fn context(view: &str, mode: InputMode) -> Context {
        Context {
            view: ViewId(view.into()),
            mode,
            read_only: false,
        }
    }

    fn press(key: &str, ctrl: bool, alt: bool) -> KeyInput {
        KeyInput {
            key: key.into(),
            ctrl,
            alt,
            shift: false,
            meta: false,
            alt_graph: false,
            composing: false,
        }
    }

    fn table(log: &Rc<RefCell<Vec<String>>>) -> CommandTable {
        let mut table = CommandTable::default();
        for (id, label, enabled) in [
            (UNDO, "Undo", When::parse("writable").unwrap()),
            (DOWN, "Move down", When::view("classic")),
            (NEXT_ISSUE, "Next issue", When::ALWAYS),
            (CLASSIC, "Show Classic view", When::ALWAYS),
        ] {
            let log = log.clone();
            let name = id.clone();
            table.register(
                CommandSpec {
                    id,
                    label: label.into(),
                    enabled,
                },
                move |call| {
                    if call.args == serde_json::json!("refuse") {
                        return Err("refused".into());
                    }
                    log.borrow_mut().push(name.to_string());
                    Ok(())
                },
            );
        }
        table
    }

    fn keymap() -> Keymap {
        let mut keymap = Keymap::default();
        keymap.bind(Layer::Safety, When::ALWAYS, "Ctrl+Z", UNDO);
        keymap.bind(Layer::Safety, When::ALWAYS, "Ctrl+Alt+1", CLASSIC);
        keymap.bind(
            Layer::View,
            When::parse("view:classic && mode:ready").unwrap(),
            "ArrowDown",
            DOWN,
        );
        keymap
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_chord_has_one_spelling() {
        let chord = Chord::parse("shift+CTRL+z").unwrap();
        assert_eq!(chord.to_string(), "Ctrl+Shift+Z");
        assert_eq!(Chord::parse("Esc").unwrap().key, "Escape");
        assert_eq!(Chord::parse("Ctrl++").unwrap().to_string(), "Ctrl++");
        assert_eq!(Chord::parse("Alt+F4").unwrap().key, "F4");
        assert!(Chord::parse("Hyper+Z").is_err());
        assert!(Chord::parse("Ctrl+").is_ok_and(|c| c.key == "+"));
        assert_eq!(press("z", true, false).chord(), Chord::parse("Ctrl+Z").unwrap());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_binding_applies_only_where_its_condition_holds() {
        let keymap = keymap();
        let ready = context("classic", InputMode::Ready);
        assert_eq!(
            keymap.resolve(&press("ArrowDown", false, false), &ready),
            Resolution::Command(DOWN)
        );
        for other in [
            context("classic", InputMode::Edit),
            context("notebook", InputMode::Ready),
        ] {
            assert_eq!(
                keymap.resolve(&press("ArrowDown", false, false), &other),
                Resolution::Unbound
            );
        }
        assert!(When::parse("view:classic && colour:red").is_err());
        assert!(When::parse("mode:flying").is_err());
        assert!(When::parse("!editing").unwrap().holds(&ready));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn alt_graph_text_and_composition_never_run_a_command() {
        let keymap = keymap();
        let ready = context("notebook", InputMode::Ready);
        let mut input = press("1", true, true);
        assert_eq!(keymap.resolve(&input, &ready), Resolution::Command(CLASSIC));
        input.alt_graph = true;
        input.key = "¹".into();
        assert_eq!(keymap.resolve(&input, &ready), Resolution::Text);
        let mut composing = press("z", true, false);
        composing.composing = true;
        assert_eq!(keymap.resolve(&composing, &ready), Resolution::Text);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_skin_binds_over_the_view_profile_and_never_over_a_safety_key() {
        let log = Rc::default();
        let commands = table(&log);
        let mut keymap = keymap();
        let skin = |when: &str, chord: &str, command: &str| KeyBinding {
            when: when.into(),
            chord: chord.into(),
            command: command.into(),
        };
        let errors = keymap
            .set_skin(
                &[
                    skin("view:classic", "Ctrl+Z", "inspector.next-issue"),
                    skin("view:classic", "Alt+Shift+X", "no.such.command"),
                    skin("mode:flying", "Hyper+X", "inspector.next-issue"),
                ],
                &commands,
            )
            .unwrap_err();
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert!(errors.contains(&KeymapError::Protected(Chord::parse("Ctrl+Z").unwrap())));
        assert!(errors.contains(&KeymapError::UnknownCommand("no.such.command".into())));
        let ready = context("classic", InputMode::Ready);
        // A refused manifest changes nothing.
        assert_eq!(
            keymap.resolve(&press("z", true, false), &ready),
            Resolution::Command(UNDO)
        );
        assert!(keymap.departures(&ready).is_empty());

        keymap
            .set_skin(
                &[
                    skin("view:classic && mode:ready", "ArrowDown", "inspector.next-issue"),
                    skin("view:classic", "Alt+Shift+X", "inspector.next-issue"),
                ],
                &commands,
            )
            .unwrap();
        assert_eq!(
            keymap.resolve(&press("ArrowDown", false, false), &ready),
            Resolution::Command(NEXT_ISSUE)
        );
        // Only the chord the profile already used is a departure.
        assert_eq!(keymap.departures(&ready), [Chord::parse("ArrowDown").unwrap()]);
        assert!(keymap.chords_for(&DOWN, &ready).is_empty());
        assert_eq!(keymap.chords_for(&NEXT_ISSUE, &ready).len(), 2);
        // Setting the skin again replaces the layer.
        keymap.set_skin(&[], &commands).unwrap();
        assert_eq!(
            keymap.resolve(&press("ArrowDown", false, false), &ready),
            Resolution::Command(DOWN)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn every_route_to_a_command_runs_the_same_handler_under_the_same_condition() {
        let log: Rc<RefCell<Vec<String>>> = Rc::default();
        let mut commands = table(&log);
        let classic = context("classic", InputMode::Ready);
        let notebook = context("notebook", InputMode::Ready);
        let mut locked = classic.clone();
        locked.read_only = true;

        commands.run(&Invocation::of(DOWN), &classic).unwrap();
        assert_eq!(
            commands.run(&Invocation::of(DOWN), &notebook),
            Err(CommandError::NotNow(DOWN))
        );
        assert_eq!(
            commands.run(&Invocation::of(UNDO), &locked),
            Err(CommandError::NotNow(UNDO))
        );
        assert_eq!(
            commands.run(&Invocation::of(CommandId::from("nope")), &classic),
            Err(CommandError::Unknown(CommandId::from("nope")))
        );
        assert_eq!(
            commands.run(
                &Invocation {
                    command: UNDO,
                    args: serde_json::json!("refuse")
                },
                &classic
            ),
            Err(CommandError::Refused("refused".into()))
        );

        commands
            .register_sequence(
                "skin.tidy",
                "Tidy",
                vec![Invocation::of(NEXT_ISSUE), Invocation::of(UNDO)],
            )
            .unwrap();
        assert_eq!(
            commands.register_sequence("skin.bad", "Bad", vec![Invocation::of("nope".into())]),
            Err(CommandError::Unknown("nope".into()))
        );
        log.borrow_mut().clear();
        commands
            .run(&Invocation::of("skin.tidy".into()), &classic)
            .unwrap();
        assert_eq!(*log.borrow(), ["inspector.next-issue", "edit.undo"]);

        let labels = |text: &str, context: &Context| -> Vec<String> {
            commands
                .search(text, context)
                .into_iter()
                .map(|spec| spec.label.to_string())
                .collect()
        };
        assert_eq!(labels("un", &classic), ["Undo"]);
        assert_eq!(labels("view show", &classic), ["Show Classic view"]);
        assert!(!labels("", &notebook).contains(&"Move down".to_owned()));
        assert!(!labels("", &locked).contains(&"Undo".to_owned()));
    }
}
