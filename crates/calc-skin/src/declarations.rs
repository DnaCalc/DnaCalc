//! Declarative skins and pages. Component bindings carry source addresses and anchor IDs.

use crate::{Bound, ViewId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkinManifest {
    pub schema: SkinSchema,
    pub id: String,
    pub name: String,
    pub extends: Option<String>,
    #[serde(default)]
    pub level: Level,
    pub theme: ThemeSettings,
    #[serde(default)]
    pub shell: ShellSettings,
    #[serde(default)]
    pub keymap: Option<Keymap>,
    #[serde(default)]
    pub commands: Vec<CommandSequence>,
    #[serde(default)]
    pub views: BTreeMap<ViewId, Value>,
    #[serde(default)]
    pub pages: BTreeMap<String, Page>,
    #[serde(default)]
    pub requires: Requirements,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkinSchema {
    #[serde(rename = "dna.skin/1")]
    V1,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    Formula,
    #[default]
    Workbook,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeSettings {
    pub base: String,
    pub density: String,
    #[serde(default)]
    pub overrides: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSettings {
    pub deck: Option<String>,
    #[serde(default)]
    pub panels: BTreeMap<String, Vec<DockPanel>>,
    #[serde(default)]
    pub strip_pins: Vec<StripPin>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DockPanel {
    Id(String),
    Sized {
        id: String,
        width_px: std::num::NonZeroU16,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StripPin {
    pub address: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymap {
    pub profile: String,
    #[serde(default)]
    pub bindings: Vec<KeyBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub when: String,
    pub chord: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandSequence {
    pub id: String,
    pub label: String,
    pub steps: Vec<(String, Value)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirements {
    #[serde(default)]
    pub extensions: Vec<ExtensionRequirement>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionRequirement {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageSchema {
    #[serde(rename = "dna.page/1")]
    V1,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputPolicy {
    #[default]
    Live,
    Batched,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub schema: PageSchema,
    pub title: String,
    #[serde(default)]
    pub input: InputPolicy,
    pub root: PageNode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PageNode {
    #[serde(alias = "Stack")]
    Stack { children: Vec<PageNode> },
    #[serde(alias = "Row")]
    Row { children: Vec<PageNode> },
    #[serde(alias = "Grid")]
    Grid {
        columns: std::num::NonZeroU8,
        children: Vec<PageNode>,
    },
    #[serde(alias = "Section")]
    Section {
        title: String,
        #[serde(default)]
        folded: bool,
        children: Vec<PageNode>,
    },
    #[serde(alias = "Tabs")]
    Tabs { tabs: Vec<PageTab> },
    #[serde(alias = "Part")]
    Part {
        id: String,
        component: String,
        #[serde(default)]
        bind: BTreeMap<String, Bound>,
        #[serde(default)]
        props: BTreeMap<String, Value>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageTab {
    pub label: String,
    pub body: PageNode,
}

impl PageNode {
    /// Visit component parts in authored reading order, including all tabs.
    pub fn try_for_each_part<'a, E>(
        &'a self,
        visit: &mut impl FnMut(&'a str, &'a str, &'a BTreeMap<String, Bound>) -> Result<(), E>,
    ) -> Result<(), E> {
        match self {
            Self::Stack { children }
            | Self::Row { children }
            | Self::Grid { children, .. }
            | Self::Section { children, .. } => children
                .iter()
                .try_for_each(|part| part.try_for_each_part(visit)),
            Self::Tabs { tabs } => tabs
                .iter()
                .try_for_each(|tab| tab.body.try_for_each_part(visit)),
            Self::Part {
                id,
                component,
                bind,
                ..
            } => visit(id, component, bind),
        }
    }

    pub fn for_each_part(&self, visit: &mut impl FnMut(&str, &str, &BTreeMap<String, Bound>)) {
        let _: Result<(), std::convert::Infallible> =
            self.try_for_each_part(&mut |id, component, bind| {
                visit(id, component, bind);
                Ok(())
            });
    }
}
