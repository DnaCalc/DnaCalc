//! Office Add-in manifests: the classic XML `OfficeApp` and the unified Microsoft 365 JSON
//! `manifest.json`, reduced to what the host needs: the task pane page, the function file,
//! ribbon commands and the shared-runtime declaration.

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
    ExecuteFunction { name: String },
    ShowTaskpane { url: String },
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
        self.hosts.is_empty() || self.hosts.iter().any(|h| h.eq_ignore_ascii_case("workbook") || h.eq_ignore_ascii_case("excel"))
    }
}

/// Load by content: a document starting with `{` is unified JSON, otherwise XML.
pub fn load(text: &str) -> Result<AddinManifest, String> {
    if text.trim_start().starts_with('{') {
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
    let mut runtime_pages: HashMap<String, String> = HashMap::new();
    for ext in doc["extensions"].as_array().into_iter().flatten() {
        for rt in ext["runtimes"].as_array().into_iter().flatten() {
            let page = s(&rt["code"]["page"]);
            let id = s(&rt["id"]);
            if !page.is_empty() {
                if manifest.task_pane_url.is_none() {
                    manifest.task_pane_url = Some(page.clone());
                }
                runtime_pages.insert(id, page);
            }
            let script = s(&rt["code"]["script"]);
            if !script.is_empty() && manifest.function_file_url.is_none() {
                manifest.function_file_url = Some(script);
            }
            if rt["actions"].as_array().is_some_and(|a| !a.is_empty()) && rt["requirements"]["capabilities"].is_null() {
                // a runtime that hosts both pages and actions is shared
            }
            if rt["lifetime"].as_str() == Some("long") || rt["code"]["page"].is_string() && rt["actions"].is_array() {
                manifest.shared_runtime = true;
            }
        }
        for h in ext["requirements"]["scopes"].as_array().into_iter().flatten() {
            manifest.hosts.push(s(h));
        }
        let mut actions: HashMap<String, CommandAction> = HashMap::new();
        for rt in ext["runtimes"].as_array().into_iter().flatten() {
            for a in rt["actions"].as_array().into_iter().flatten() {
                let id = s(&a["id"]);
                match a["type"].as_str() {
                    Some("executeFunction") => {
                        actions.insert(id, CommandAction::ExecuteFunction { name: s(&a["displayName"]).max(s(&a["id"])) });
                    }
                    Some("openPage") => {
                        let url = runtime_pages.get(&s(&a["view"])).cloned().unwrap_or_else(|| s(&a["pageUrl"]));
                        actions.insert(id, CommandAction::ShowTaskpane { url });
                    }
                    _ => {}
                }
            }
        }
        for ribbon in ext["ribbons"].as_array().into_iter().flatten() {
            for tab in ribbon["tabs"].as_array().into_iter().flatten() {
                for group in tab["groups"].as_array().into_iter().flatten() {
                    for control in group["controls"].as_array().into_iter().flatten() {
                        let action_id = s(&control["actionId"]);
                        if let Some(action) = actions.get(&action_id) {
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
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
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

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(end) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let entity = &tail[1..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            e if e.starts_with("#x") => out.extend(u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32)),
            e if e.starts_with('#') => out.extend(e[1..].parse::<u32>().ok().and_then(char::from_u32)),
            _ => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

fn parse_xml(text: &str) -> Result<Node, String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut stack: Vec<Node> = vec![Node::default()];
    while i < bytes.len() {
        if bytes[i] != b'<' {
            let end = text[i..].find('<').map_or(text.len(), |p| i + p);
            let top = stack.last_mut().unwrap();
            top.text.push_str(&unescape(&text[i..end]));
            i = end;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").ok_or("unterminated comment")? + 3;
        } else if rest.starts_with("<![CDATA[") {
            let end = rest.find("]]>").ok_or("unterminated CDATA")?;
            stack.last_mut().unwrap().text.push_str(&rest[9..end]);
            i += end + 3;
        } else if rest.starts_with("<?") {
            i += rest.find("?>").ok_or("unterminated declaration")? + 2;
        } else if rest.starts_with("<!") {
            i += rest.find('>').ok_or("unterminated DOCTYPE")? + 1;
        } else if rest.starts_with("</") {
            let end = rest.find('>').ok_or("unterminated end tag")?;
            let name = local(rest[2..end].trim()).to_string();
            let node = stack.pop().ok_or("unbalanced end tag")?;
            if node.name != name || stack.is_empty() {
                return Err(format!("mismatched end tag </{name}>"));
            }
            stack.last_mut().unwrap().children.push(node);
            i += end + 1;
        } else {
            let mut j = i + 1;
            let mut quote = None;
            while j < bytes.len() {
                match (bytes[j], quote) {
                    (b'"' | b'\'', None) => quote = Some(bytes[j]),
                    (q, Some(open)) if q == open => quote = None,
                    (b'>', None) => break,
                    _ => {}
                }
                j += 1;
            }
            if j >= bytes.len() {
                return Err("unterminated tag".into());
            }
            let inner = &text[i + 1..j];
            let empty = inner.ends_with('/');
            let inner = inner.trim_end_matches('/');
            let (name, attrs) = parse_tag(inner)?;
            let node = Node { name, attrs, ..Default::default() };
            if empty {
                stack.last_mut().unwrap().children.push(node);
            } else {
                stack.push(node);
            }
            i = j + 1;
        }
    }
    if stack.len() != 1 {
        return Err("unclosed element".into());
    }
    stack.pop().unwrap().children.into_iter().next().ok_or_else(|| "no root element".into())
}

fn parse_tag(inner: &str) -> Result<(String, Vec<(String, String)>), String> {
    let inner = inner.trim();
    let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
    let name = local(&inner[..name_end]).to_string();
    let mut attrs = Vec::new();
    let mut rest = inner[name_end..].trim_start();
    while !rest.is_empty() {
        let eq = rest.find('=').ok_or_else(|| format!("attribute without value in <{name}>"))?;
        let key = local(rest[..eq].trim()).to_string();
        let after = rest[eq + 1..].trim_start();
        let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'').ok_or("unquoted attribute")?;
        let close = after[1..].find(quote).ok_or("unterminated attribute")?;
        attrs.push((key, unescape(&after[1..1 + close])));
        rest = after[close + 2..].trim_start();
    }
    Ok((name, attrs))
}

pub fn load_xml(text: &str) -> Result<AddinManifest, String> {
    let root = parse_xml(text)?;
    if root.name != "OfficeApp" {
        return Err(format!("root element is {}, not OfficeApp", root.name));
    }
    let default = |n: &Node| n.attr("DefaultValue").unwrap_or_default().to_string();
    let text_of = |n: Option<&Node>| n.map(|n| n.text.trim().to_string()).unwrap_or_default();
    // Resources: Url / ShortString / LongString ids to their default text.
    let mut resources: HashMap<String, String> = HashMap::new();
    for kind in ["Url", "String"] {
        for n in root.all(kind) {
            if let Some(id) = n.attr("id") {
                resources.insert(id.to_string(), default(n));
            }
        }
    }
    let resolve = |resid: Option<&str>| resid.and_then(|r| resources.get(r).cloned());
    let mut manifest = AddinManifest {
        kind: Kind::Xml,
        id: text_of(root.child("Id")),
        version: text_of(root.child("Version")),
        name: root.child("DisplayName").map(default).unwrap_or_default(),
        hosts: root.child("Hosts").map(|h| h.children.iter().filter_map(|c| c.attr("Name").map(String::from)).collect()).unwrap_or_default(),
        task_pane_url: root.child("DefaultSettings").and_then(|d| d.child("SourceLocation")).map(default),
        function_file_url: None,
        commands: Vec::new(),
        shared_runtime: false,
    };
    if manifest.id.is_empty() {
        return Err("manifest has no Id".into());
    }
    for ff in root.all("FunctionFile") {
        manifest.function_file_url = resolve(ff.attr("resid"));
    }
    manifest.shared_runtime = root.all("Runtime").iter().any(|r| r.attr("lifetime") == Some("long"))
        || root.all("Requirement").iter().any(|r| r.attr("name") == Some("SharedRuntime"));
    for control in root.all("Control") {
        let Some(action) = control.child("Action") else { continue };
        let action_type = action.attr("type").unwrap_or_default();
        let command = match action_type {
            "ExecuteFunction" => CommandAction::ExecuteFunction { name: text_of(action.child("FunctionName")) },
            "ShowTaskpane" => CommandAction::ShowTaskpane {
                url: resolve(action.child("SourceLocation").and_then(|s| s.attr("resid")))
                    .or_else(|| manifest.task_pane_url.clone())
                    .unwrap_or_default(),
            },
            _ => continue,
        };
        let label = resolve(control.child("Label").and_then(|l| l.attr("resid"))).unwrap_or_default();
        manifest.commands.push(Command { id: control.attr("id").unwrap_or_default().to_string(), label, action: command });
    }
    Ok(manifest)
}
