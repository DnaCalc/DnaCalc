//! The custom workbook ribbon controls the desktop shell can display.
use roxmltree::{Document, Node};

pub(crate) struct Ribbon {
    pub on_load: Option<String>,
    pub buttons: Vec<Button>,
}

pub(crate) struct Button {
    pub id: String,
    pub tag: String,
    pub tab: String,
    pub group: String,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub on_action: Option<String>,
    pub get_label: Option<String>,
    pub get_enabled: Option<String>,
    pub get_visible: Option<String>,
}

/// The display state of a button on a COM add-in's custom workbook ribbon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RibbonButton {
    pub token: u64,
    pub tab: String,
    pub group: String,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
}

impl Ribbon {
    pub(crate) fn parse(xml: &str) -> Result<Self, String> {
        let document = Document::parse(xml).map_err(|error| error.to_string())?;
        let root = document.root_element();
        let Some(
            ns @ ("http://schemas.microsoft.com/office/2006/01/customui"
            | "http://schemas.microsoft.com/office/2009/07/customui"),
        ) = root.tag_name().namespace()
        else {
            return Err("customUI has an unsupported namespace".into());
        };
        if !root.has_tag_name((ns, "customUI")) {
            return Err("expected a customUI document".into());
        }
        let mut buttons = Vec::new();
        for ribbon in children(root, ns, "ribbon") {
            for tabs in children(ribbon, ns, "tabs") {
                for tab in children(tabs, ns, "tab") {
                    for group in children(tab, ns, "group") {
                        for node in children(group, ns, "button") {
                            let Some(id) = node.attribute("id") else {
                                continue;
                            };
                            buttons.push(Button {
                                id: id.into(),
                                tag: node.attribute("tag").unwrap_or_default().into(),
                                tab: tab.attribute("label").unwrap_or_default().into(),
                                group: group.attribute("label").unwrap_or_default().into(),
                                label: node.attribute("label").unwrap_or(id).into(),
                                enabled: enabled(node, "enabled"),
                                visible: enabled(node, "visible"),
                                on_action: attribute(node, "onAction"),
                                get_label: attribute(node, "getLabel"),
                                get_enabled: attribute(node, "getEnabled"),
                                get_visible: attribute(node, "getVisible"),
                            });
                        }
                    }
                }
            }
        }
        Ok(Self {
            on_load: attribute(root, "onLoad"),
            buttons,
        })
    }
}

fn children<'a, 'input>(
    node: Node<'a, 'input>,
    ns: &str,
    name: &str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children()
        .filter(move |child| child.has_tag_name((ns, name)))
}

fn attribute(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn enabled(node: Node<'_, '_>, name: &str) -> bool {
    !matches!(node.attribute(name), Some("false" | "0"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: the desktop COM ribbon uses these customUI bindings.
    #[test]
    fn binds_only_custom_buttons_in_workbook_tabs() {
        let xml = r#"<customUI xmlns="http://schemas.microsoft.com/office/2009/07/customui" onLoad="Load"><ribbon><tabs>
            <tab id="t" label="Reports"><group id="g" label="Budget">
            <button id="run" label="Calculate &amp; check" onAction="Run" getEnabled="CanRun" visible="0"/>
            <button idMso="FileSave"/>
            </group></tab>
            </tabs></ribbon><backstage><button id="not-a-ribbon-button"/></backstage></customUI>"#;
        let ribbon = Ribbon::parse(xml).unwrap();
        assert_eq!(ribbon.on_load.as_deref(), Some("Load"));
        assert_eq!(ribbon.buttons.len(), 1);
        let button = &ribbon.buttons[0];
        assert_eq!(button.id, "run");
        assert_eq!(button.tab, "Reports");
        assert_eq!(button.group, "Budget");
        assert_eq!(button.label, "Calculate & check");
        assert_eq!(button.on_action.as_deref(), Some("Run"));
        assert_eq!(button.get_enabled.as_deref(), Some("CanRun"));
        assert!(!button.visible);
    }
}
