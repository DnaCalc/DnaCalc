//! Office Add-in manifests: the classic XML `OfficeApp` and the unified Microsoft 365 JSON
//! `manifest.json`, reduced to what the host needs: the task pane page, the function file,
//! ribbon commands and the shared-runtime declaration.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, encoding::Decoder};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Xml,
    UnifiedJson,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandAction {
    /// Runs a function the page registered with `Office.actions.associate(name, ...)`.
    ExecuteFunction {
        name: String,
    },
    ShowTaskpane {
        url: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub id: String,
    pub label: String,
    pub action: CommandAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddinManifest {
    pub kind: Kind,
    pub id: String,
    pub version: String,
    pub name: String,
    pub hosts: Vec<String>,
    pub task_pane_url: Option<String>,
    pub function_file_url: Option<String>,
    pub commands: Vec<Command>,
    pub shared_runtime: bool,
}

impl AddinManifest {
    pub fn runs_in_excel(&self) -> bool {
        self.hosts.is_empty() || self.hosts.iter().any(|host| excel_host(host))
    }
}

fn excel_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("workbook") || host.eq_ignore_ascii_case("excel")
}

/// Load by content: a document starting with `{` is unified JSON, otherwise XML.
pub fn load(text: &str) -> Result<AddinManifest, String> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with('{') {
        load_json(text)
    } else {
        load_xml(text)
    }
}

// ---- unified JSON ----

pub fn load_json(text: &str) -> Result<AddinManifest, String> {
    let doc: Value = serde_json::from_str(text).map_err(|e| format!("manifest.json: {e}"))?;
    let s = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let mut manifest = AddinManifest {
        kind: Kind::UnifiedJson,
        id: s(&doc["id"]),
        version: s(&doc["version"]),
        name: s(&doc["name"]["short"]),
        hosts: Vec::new(),
        task_pane_url: None,
        function_file_url: None,
        commands: Vec::new(),
        shared_runtime: false,
    };
    if manifest.id.is_empty() {
        return Err("manifest.json has no id".into());
    }
    for ext in doc["extensions"].as_array().into_iter().flatten() {
        let scopes = ext["requirements"]["scopes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        manifest
            .hosts
            .extend(scopes.iter().map(|scope| (*scope).to_owned()));
        if !scopes.is_empty() && !scopes.iter().any(|scope| excel_host(scope)) {
            continue;
        }
        let mut actions = HashMap::new();
        for runtime in ext["runtimes"].as_array().into_iter().flatten() {
            let page = runtime["code"]["page"]
                .as_str()
                .filter(|url| !url.is_empty());
            let script = runtime["code"]["script"]
                .as_str()
                .filter(|url| !url.is_empty());
            manifest.shared_runtime |=
                page.is_some() && runtime["lifetime"].as_str() == Some("long");
            for action in runtime["actions"].as_array().into_iter().flatten() {
                let Some(id) = action["id"].as_str().filter(|id| !id.is_empty()) else {
                    continue;
                };
                let command = match action["type"].as_str() {
                    Some("executeFunction") => {
                        if let Some(url) = script.or(page) {
                            manifest
                                .function_file_url
                                .get_or_insert_with(|| url.to_owned());
                        }
                        CommandAction::ExecuteFunction {
                            name: id.to_owned(),
                        }
                    }
                    Some("openPage") => {
                        let Some(url) = page else { continue };
                        if action["view"].as_str() == Some("Office.AutoShowTaskpaneWithDocument") {
                            manifest.task_pane_url = Some(url.to_owned());
                        } else {
                            manifest.task_pane_url.get_or_insert_with(|| url.to_owned());
                        }
                        CommandAction::ShowTaskpane {
                            url: url.to_owned(),
                        }
                    }
                    _ => continue,
                };
                actions.insert(id, command);
            }
        }
        for ribbon in ext["ribbons"].as_array().into_iter().flatten() {
            for tab in ribbon["tabs"].as_array().into_iter().flatten() {
                for group in tab["groups"].as_array().into_iter().flatten() {
                    for control in group["controls"].as_array().into_iter().flatten() {
                        if let Some(action) =
                            control["actionId"].as_str().and_then(|id| actions.get(id))
                        {
                            manifest.commands.push(Command {
                                id: s(&control["id"]),
                                label: s(&control["label"]),
                                action: action.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(manifest)
}

// ---- XML ----

#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
    text: String,
}

impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    fn walk<'a>(&'a self, out: &mut Vec<&'a Node>) {
        out.push(self);
        for c in &self.children {
            c.walk(out);
        }
    }
    fn all(&self, name: &str) -> Vec<&Node> {
        let mut all = Vec::new();
        self.walk(&mut all);
        all.retain(|n| n.name == name);
        all
    }
}

fn node(start: BytesStart<'_>, decoder: Decoder) -> Result<Node, String> {
    let attr = |attribute: quick_xml::events::attributes::Attribute<'_>| {
        let key = String::from_utf8_lossy(attribute.key.local_name().as_ref()).into_owned();
        let value = attribute
            .decode_and_unescape_value(decoder)
            .map_err(|e| e.to_string())?;
        Ok((key, value.into_owned()))
    };
    let attrs = start
        .attributes()
        .map(|a| attr(a.map_err(|e| e.to_string())?))
        .collect::<Result<_, String>>()?;
    Ok(Node {
        name: String::from_utf8_lossy(start.local_name().as_ref()).into_owned(),
        attrs,
        ..Default::default()
    })
}

fn parse_xml(text: &str) -> Result<Node, String> {
    let mut reader = Reader::from_str(text);
    let mut stack = vec![Node::default()];
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(start) => {
                if stack.len() >= 64 {
                    return Err("manifest XML nesting exceeds 63 elements".into());
                }
                stack.push(node(start, reader.decoder())?);
            }
            Event::Empty(start) => {
                stack
                    .last_mut()
                    .unwrap()
                    .children
                    .push(node(start, reader.decoder())?);
            }
            Event::End(_) => {
                if stack.len() < 2 {
                    return Err("unexpected closing element".into());
                }
                let closed = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(closed);
            }
            Event::Text(text) => {
                let text = text.unescape().map_err(|error| error.to_string())?;
                stack.last_mut().unwrap().text.push_str(&text);
            }
            Event::CData(text) => {
                let text = text.decode().map_err(|error| error.to_string())?;
                stack.last_mut().unwrap().text.push_str(&text);
            }
            Event::DocType(_) => return Err("office manifests do not support a DTD".into()),
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 {
        return Err("unclosed element".into());
    }
    let mut outside = stack.pop().unwrap();
    if outside.children.len() != 1 || !outside.text.trim().is_empty() {
        return Err("manifest must contain one root element".into());
    }
    Ok(outside.children.pop().unwrap())
}

pub fn load_xml(text: &str) -> Result<AddinManifest, String> {
    let root = parse_xml(text)?;
    if root.name != "OfficeApp" {
        return Err(format!("root element is {}, not OfficeApp", root.name));
    }
    let default = |n: &Node| n.attr("DefaultValue").unwrap_or_default().to_string();
    let text_of = |n: Option<&Node>| n.map(|n| n.text.trim().to_string()).unwrap_or_default();
    let mut manifest = AddinManifest {
        kind: Kind::Xml,
        id: text_of(root.child("Id")),
        version: text_of(root.child("Version")),
        name: root.child("DisplayName").map(default).unwrap_or_default(),
        hosts: root
            .child("Hosts")
            .map(|h| {
                h.children
                    .iter()
                    .filter_map(|c| c.attr("Name").map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        task_pane_url: root
            .child("DefaultSettings")
            .and_then(|d| d.child("SourceLocation"))
            .map(default),
        function_file_url: None,
        commands: Vec::new(),
        shared_runtime: false,
    };
    if manifest.id.is_empty() {
        return Err("manifest has no Id".into());
    }
    manifest.shared_runtime = root.child("Requirements").is_some_and(|requirements| {
        requirements
            .all("Set")
            .iter()
            .any(|set| set.attr("Name") == Some("SharedRuntime"))
    });
    let excel_override =
        root.all("VersionOverrides")
            .into_iter()
            .rev()
            .find_map(|overrides| {
                let workbook_host = overrides.child("Hosts")?.children.iter().find(|host| {
                    host.name == "Host" && host.attr("type").is_some_and(excel_host)
                })?;
                Some((overrides, workbook_host))
            });
    let Some((overrides, host)) = excel_override else {
        return Ok(manifest);
    };

    let mut resources = HashMap::new();
    if let Some(resource_root) = overrides.child("Resources") {
        for kind in ["Url", "String"] {
            for resource in resource_root.all(kind) {
                if let Some(id) = resource.attr("id") {
                    resources.insert(id, default(resource));
                }
            }
        }
    }
    let resolve = |id: Option<&str>| id.and_then(|id| resources.get(id).cloned());
    let runtime = host
        .all("Runtime")
        .into_iter()
        .find(|r| r.attr("lifetime") == Some("long"));
    manifest.shared_runtime |= runtime.is_some();
    let mut pane = runtime.and_then(|r| resolve(r.attr("resid")));
    let Some(desktop) = host.child("DesktopFormFactor") else {
        return Ok(manifest);
    };
    manifest.function_file_url = desktop
        .child("FunctionFile")
        .and_then(|file| resolve(file.attr("resid")));
    for control in desktop.all("Control") {
        let Some(action) = control.child("Action") else {
            continue;
        };
        let command = match action.attr("type") {
            Some("ExecuteFunction") => CommandAction::ExecuteFunction {
                name: text_of(action.child("FunctionName")),
            },
            Some("ShowTaskpane") => {
                let source = action
                    .child("SourceLocation")
                    .and_then(|source| resolve(source.attr("resid")));
                if let Some(url) = &source {
                    pane.get_or_insert_with(|| url.clone());
                }
                let Some(url) = source.or_else(|| manifest.task_pane_url.clone()) else {
                    continue;
                };
                CommandAction::ShowTaskpane { url }
            }
            _ => continue,
        };
        let label =
            resolve(control.child("Label").and_then(|l| l.attr("resid"))).unwrap_or_default();
        manifest.commands.push(Command {
            id: control.attr("id").unwrap_or_default().to_string(),
            label,
            action: command,
        });
    }
    manifest.task_pane_url = pane.or(manifest.task_pane_url);
    Ok(manifest)
}
