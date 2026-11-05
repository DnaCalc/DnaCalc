//! Named commands, availability and the shared shell keymap. A skin submits a command;
//! the application adapter executes the returned effect against its single transport.
use crate::{FindMode, Model, SkinId};
use calc_value::{CellKey, GridArea, NameId, SheetId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandId {
    PasteSpecial,
    FlashFill,
    FormatCells,
    NameManager,
    Undo,
    Redo,
    Find,
    Replace,
    GoTo,
    Recalculate,
    Reveal,
    SwitchSkin,
}
impl CommandId {
    pub const ALL: [Self; 12] = [
        Self::PasteSpecial,
        Self::FlashFill,
        Self::FormatCells,
        Self::NameManager,
        Self::Undo,
        Self::Redo,
        Self::Find,
        Self::Replace,
        Self::GoTo,
        Self::Recalculate,
        Self::Reveal,
        Self::SwitchSkin,
    ];
    pub const fn stable_id(self) -> &'static str {
        match self {
            Self::PasteSpecial => "edit.paste-special",
            Self::FlashFill => "edit.flash-fill",
            Self::FormatCells => "format.cells",
            Self::NameManager => "name.manager",
            Self::Undo => "edit.undo",
            Self::Redo => "edit.redo",
            Self::Find => "edit.find",
            Self::Replace => "edit.replace",
            Self::GoTo => "view.go-to",
            Self::Recalculate => "calculate.now",
            Self::Reveal => "view.reveal",
            Self::SwitchSkin => "view.skin",
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::PasteSpecial => "Paste Special",
            Self::FlashFill => "Flash Fill",
            Self::FormatCells => "Format Cells",
            Self::NameManager => "Name Manager",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Find => "Find",
            Self::Replace => "Replace",
            Self::GoTo => "Go to",
            Self::Recalculate => "Calculate now",
            Self::Reveal => "Show source",
            Self::SwitchSkin => "Switch skin",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Cells(Vec<GridArea>),
    Name(NameId),
    Sheet(SheetId),
    Object(String),
    LayoutItem(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevealTarget {
    Cell(CellKey),
    Reference(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    PasteSpecial,
    FlashFill,
    FormatCells,
    NameManager,
    Undo,
    Redo,
    Find,
    Replace,
    GoTo,
    Recalculate,
    Reveal { target: RevealTarget, skin: SkinId },
    SwitchSkin(SkinId),
}
impl Command {
    pub const fn id(&self) -> CommandId {
        match self {
            Self::PasteSpecial => CommandId::PasteSpecial,
            Self::FlashFill => CommandId::FlashFill,
            Self::FormatCells => CommandId::FormatCells,
            Self::NameManager => CommandId::NameManager,
            Self::Undo => CommandId::Undo,
            Self::Redo => CommandId::Redo,
            Self::Find => CommandId::Find,
            Self::Replace => CommandId::Replace,
            Self::GoTo => CommandId::GoTo,
            Self::Recalculate => CommandId::Recalculate,
            Self::Reveal { .. } => CommandId::Reveal,
            Self::SwitchSkin(_) => CommandId::SwitchSkin,
        }
    }
    pub fn simple(id: CommandId) -> Option<Self> {
        Some(match id {
            CommandId::PasteSpecial => Self::PasteSpecial,
            CommandId::FlashFill => Self::FlashFill,
            CommandId::FormatCells => Self::FormatCells,
            CommandId::NameManager => Self::NameManager,
            CommandId::Undo => Self::Undo,
            CommandId::Redo => Self::Redo,
            CommandId::Find => Self::Find,
            CommandId::Replace => Self::Replace,
            CommandId::GoTo => Self::GoTo,
            CommandId::Recalculate => Self::Recalculate,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandContext {
    pub ready: bool,
    pub readonly: bool,
    pub undo: bool,
    pub redo: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    NoWorkbook,
    ReadOnly,
    NothingToUndo,
    NothingToRedo,
    EditInProgress,
}
impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoWorkbook => "Wait for the workbook",
            Self::ReadOnly => "Wait for the current entry decision",
            Self::NothingToUndo => "There is nothing to undo",
            Self::NothingToRedo => "There is nothing to redo",
            Self::EditInProgress => "Finish or cancel the current edit first",
        })
    }
}
impl std::error::Error for Unavailable {}

/// Captures focus ownership at submission, before a worker or edit reply can defer dispatch.
/// Only the model constructs this request; all adapters dispatch it through the same semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandRequest {
    command: Command,
    focus: Option<crate::FocusTicket>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    PasteSpecial,
    FlashFill,
    FormatCells,
    NameManager,
    Undo {
        editor: bool,
    },
    Redo {
        editor: bool,
    },
    Recalculate,
    Dialog,
    Reveal {
        target: RevealTarget,
        skin: SkinId,
        ticket: crate::FocusTicket,
    },
    FocusSkin {
        skin: SkinId,
        ticket: crate::FocusTicket,
    },
}
impl Model {
    pub fn availability(&self, id: CommandId, context: CommandContext) -> Result<(), Unavailable> {
        if !context.ready {
            return Err(Unavailable::NoWorkbook);
        }
        if context.readonly {
            return Err(Unavailable::ReadOnly);
        }
        if id == CommandId::Undo && !context.undo {
            return Err(Unavailable::NothingToUndo);
        }
        if id == CommandId::Redo && !context.redo {
            return Err(Unavailable::NothingToRedo);
        }
        if self.edit.surface.is_some()
            && matches!(
                id,
                CommandId::PasteSpecial | CommandId::FlashFill | CommandId::FormatCells
                    | CommandId::NameManager
                    | CommandId::Find
                    | CommandId::Replace
                    | CommandId::GoTo
                    | CommandId::Recalculate
            )
        {
            return Err(Unavailable::EditInProgress);
        }
        Ok(())
    }
    pub fn prepare_command(&mut self, command: Command) -> CommandRequest {
        self.prepare_command_with_focus(command, None)
    }
    /// A browser replay can inherit ownership from the native gesture that queued it.
    pub fn prepare_command_with_focus(
        &mut self,
        command: Command,
        captured: Option<crate::FocusTicket>,
    ) -> CommandRequest {
        let focus = matches!(command, Command::Reveal { .. } | Command::SwitchSkin(_))
            .then(|| captured.unwrap_or_else(|| self.focus.request()));
        CommandRequest { command, focus }
    }
    pub fn dispatch(
        &mut self,
        command: Command,
        context: CommandContext,
    ) -> Result<Effect, Unavailable> {
        let request = self.prepare_command(command);
        self.dispatch_request(request, context)
    }
    pub fn dispatch_request(
        &mut self,
        request: CommandRequest,
        context: CommandContext,
    ) -> Result<Effect, Unavailable> {
        let CommandRequest { command, focus } = request;
        self.availability(command.id(), context)?;
        Ok(match command {
            Command::PasteSpecial => Effect::PasteSpecial,
            Command::FlashFill => Effect::FlashFill,
            Command::NameManager => {
                if !self.names.open {
                    self.names = crate::names::NameState {
                        open: true,
                        ..Default::default()
                    };
                }
                Effect::NameManager
            }
            Command::FormatCells => {
                if !self.formatting.open {
                    self.formatting = crate::format::FormatState {
                        open: true,
                        ..Default::default()
                    };
                }
                Effect::FormatCells
            }
            Command::Undo => Effect::Undo {
                editor: self.edit.surface.is_some(),
            },
            Command::Redo => Effect::Redo {
                editor: self.edit.surface.is_some(),
            },
            Command::Find | Command::Replace | Command::GoTo => {
                self.find = Some(match command {
                    Command::Replace => FindMode::Replace,
                    Command::GoTo => FindMode::GoTo,
                    _ => FindMode::Find,
                });
                Effect::Dialog
            }
            Command::Recalculate => Effect::Recalculate,
            Command::Reveal { target, skin } => Effect::Reveal {
                target,
                skin,
                ticket: focus.expect("prepared focus command"),
            },
            Command::SwitchSkin(skin) => {
                self.edit.transfer(crate::SurfaceId::Bar);
                self.reveal_completed(skin);
                Effect::FocusSkin {
                    skin,
                    ticket: focus.expect("prepared focus command"),
                }
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandBinding {
    pub key: &'static str,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub command: CommandId,
}
pub const SHELL_KEYMAP: &[CommandBinding] = &[
    CommandBinding {key:"v",ctrl:true,shift:false,alt:true,command:CommandId::PasteSpecial},
    CommandBinding {key:"e",ctrl:true,shift:false,alt:false,command:CommandId::FlashFill},
    CommandBinding {
        key: "F3",
        ctrl: true,
        shift: false,
        alt: false,
        command: CommandId::NameManager,
    },
    CommandBinding {
        key: "1",
        ctrl: true,
        shift: false,
        alt: false,
        command: CommandId::FormatCells,
    },
    CommandBinding {
        key: "z",
        ctrl: true,
        shift: false,
        alt: false,
        command: CommandId::Undo,
    },
    CommandBinding {
        key: "z",
        ctrl: true,
        shift: true,
        alt: false,
        command: CommandId::Redo,
    },
    CommandBinding {
        key: "y",
        ctrl: true,
        shift: false,
        alt: false,
        command: CommandId::Redo,
    },
    CommandBinding {
        key: "f",
        ctrl: true,
        shift: false,
        alt: false,
        command: CommandId::Find,
    },
    CommandBinding {
        key: "h",
        ctrl: true,
        shift: false,
        alt: false,
        command: CommandId::Replace,
    },
    CommandBinding {
        key: "g",
        ctrl: true,
        shift: false,
        alt: false,
        command: CommandId::GoTo,
    },
    CommandBinding {
        key: "F5",
        ctrl: false,
        shift: false,
        alt: false,
        command: CommandId::GoTo,
    },
    CommandBinding {
        key: "F9",
        ctrl: false,
        shift: false,
        alt: false,
        command: CommandId::Recalculate,
    },
];
pub fn resolve_shell_key(chord: &crate::selection::KeyChord) -> Option<CommandId> {
    SHELL_KEYMAP
        .iter()
        .find(|binding| {
            binding.key == chord.key
                && binding.ctrl == chord.ctrl
                && binding.shift == chord.shift
                && binding.alt == chord.alt
        })
        .map(|binding| binding.command)
}
