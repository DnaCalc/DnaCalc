use super::*;
use crate::ribbon::{Button, Ribbon, RibbonButton};
use calc_object_model::Descriptor;

pub(super) struct LiveRibbon {
    ui: Rc<Node>,
    buttons: Vec<LiveButton>,
}

struct LiveButton {
    spec: Button,
    control: Rc<Node>,
    revision: Cell<u64>,
    view: RefCell<Option<RibbonButton>>,
}

impl LiveButton {
    fn invalidate(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
        self.view.take();
    }
}

impl Office {
    pub(super) fn mark_ribbon_changed(&self) {
        self.ribbon_revision
            .set(self.ribbon_revision.get().wrapping_add(1));
    }

    pub(super) fn ribbon_handlers(handlers: &mut Handlers<Self>) {
        handlers.on(
            "IRibbonControl",
            "Id",
            InvokeKind::Get,
            Self::control_caption,
        );
        handlers.on(
            "IRibbonControl",
            "Tag",
            InvokeKind::Get,
            Self::control_action,
        );
        handlers.on(
            "IRibbonControl",
            "Context",
            InvokeKind::Get,
            Self::ribbon_context,
        );
        for name in ["Invalidate", "InvalidateControl"] {
            handlers.on(
                "IRibbonUI",
                name,
                InvokeKind::Method,
                Self::ribbon_invalidate,
            );
        }
    }

    fn ribbon_context(&self, target: &ObjectRef, _: &Call<'_>) -> Result<AutoValue> {
        self.node(target)?;
        let state = self.state()?;
        let application = state.application();
        let descriptor = Descriptor::get();
        let interface = descriptor.default_interface(application.interface);
        let member = descriptor
            .member(interface, "ActiveWindow", InvokeKind::Get)
            .unwrap();
        state.call(
            &application,
            &Call {
                interface,
                member,
                args: vec![],
                lcid: 0,
            },
        )
    }

    fn ribbon_invalidate(&self, target: &ObjectRef, call: &Call<'_>) -> Result<AutoValue> {
        let ui = self.node(target)?;
        let node = self.node(&ui.parent)?;
        let data = node.addin.borrow().clone().ok_or(AutoError::Disconnected)?;
        let ribbon = data
            .ribbon
            .borrow()
            .clone()
            .ok_or(AutoError::Disconnected)?;
        let id = match call.member.name.as_str() {
            "Invalidate" => None,
            _ => match call.arg(0) {
                AutoValue::Text(id) => Some(id.to_string_lossy()),
                _ => return values::mismatch(),
            },
        };
        for button in &ribbon.buttons {
            if id.as_deref().is_none_or(|id| id == button.spec.id) {
                button.invalidate();
            }
        }
        self.mark_ribbon_changed();
        Ok(AutoValue::Empty)
    }

    pub(super) fn forget_ribbon(&self, addin: &Addin) {
        if let Some(ribbon) = addin.ribbon.take() {
            let mut nodes = self.nodes.borrow_mut();
            for node in std::iter::once(&ribbon.ui).chain(ribbon.buttons.iter().map(|b| &b.control))
            {
                node.deleted.set(true);
                nodes.remove(&Self::key(node));
            }
        }
        addin.ribbon_initialized.set(false);
    }

    fn initialize_ribbon(&self, node: &Node, data: &Addin) -> Result<()> {
        let xml = data.xml.borrow().clone();
        if xml.is_empty() || data.ribbon_initialized.replace(true) {
            return Ok(());
        }
        let parsed = Ribbon::parse(&xml).map_err(|error| {
            AutoError::excel(1004, format!("{} ribbon XML: {error}", data.progid))
        })?;
        let ui = self.make(node.target.clone(), Role::RibbonUi)?;
        let mut buttons = Vec::new();
        for spec in parsed.buttons {
            let control = self.make(node.target.clone(), Role::RibbonControl)?;
            control.caption.replace(text(&spec.id));
            control.action.replace(text(&spec.tag));
            buttons.push(LiveButton {
                spec,
                control,
                revision: Cell::new(0),
                view: RefCell::default(),
            });
        }
        let ribbon = Rc::new(LiveRibbon { ui, buttons });
        data.ribbon.replace(Some(ribbon.clone()));
        if let Some(name) = parsed.on_load {
            self.ribbon_invoke(data, &name, &ribbon.ui)?;
        }
        Ok(())
    }

    fn ribbon_invoke(&self, data: &Addin, name: &str, argument: &Node) -> Result<VARIANT> {
        let generation = data.generation.get();
        let _lease = data.lease.borrow().clone();
        let receiver = Self::extension(data)?.cast::<IDispatch>().map_err(status)?;
        let name = BSTR::from(name);
        let mut dispid = 0;
        unsafe {
            receiver.GetIDsOfNames(&GUID::zeroed(), &PCWSTR(name.as_ptr()), 1, 0, &mut dispid)
        }
        .map_err(status)?;
        if generation != data.generation.get() || !data.connected.get() {
            return failure(E_ABORT);
        }
        let mut arg = VARIANT::from(self.state()?.wrap(&argument.target)?);
        let params = DISPPARAMS {
            rgvarg: &mut arg,
            cArgs: 1,
            ..Default::default()
        };
        let mut result = VARIANT::default();
        let mut exception = EXCEPINFO::default();
        let invoked = unsafe {
            receiver.Invoke(
                dispid,
                &GUID::zeroed(),
                0,
                DISPATCH_METHOD,
                &params,
                Some(&mut result),
                Some(&mut exception),
                None,
            )
        };
        unsafe { crate::events::clear_exception(&mut exception) };
        invoked.map_err(status)?;
        if generation != data.generation.get() || !data.connected.get() {
            return failure(E_ABORT);
        }
        Ok(result)
    }

    fn ribbon_button(&self, data: &Addin, button: &LiveButton) -> Result<RibbonButton> {
        if let Some(view) = button.view.borrow().as_ref() {
            return Ok(view.clone());
        }
        let revision = button.revision.get();
        let spec = &button.spec;
        let get_bool = |callback: &Option<String>, default| -> Result<bool> {
            if let Some(callback) = callback {
                let result = self.ribbon_invoke(data, callback, &button.control)?;
                return bool::try_from(&result).map_err(status);
            }
            Ok(default)
        };
        let view = RibbonButton {
            token: Self::key(&button.control),
            tab: spec.tab.clone(),
            group: spec.group.clone(),
            label: if let Some(callback) = &spec.get_label {
                let result = self.ribbon_invoke(data, callback, &button.control)?;
                BSTR::try_from(&result).map_err(status)?.to_string()
            } else {
                spec.label.clone()
            },
            enabled: get_bool(&spec.get_enabled, spec.enabled)? && spec.on_action.is_some(),
            visible: get_bool(&spec.get_visible, spec.visible)?,
        };
        if revision == button.revision.get() {
            button.view.replace(Some(view.clone()));
        }
        Ok(view)
    }

    pub fn ribbon_buttons(&self) -> Result<Vec<RibbonButton>> {
        if self.addin_mode.get() == AddinMode::Disabled || self.addins.get().is_none() {
            return Ok(Vec::new());
        }
        self.ribbons()?;
        let collection = self.addins()?;
        let mut buttons = Vec::new();
        for node in self.children(&collection) {
            let Some(data) = node
                .addin
                .borrow()
                .clone()
                .filter(|data| data.connected.get())
            else {
                continue;
            };
            self.initialize_ribbon(&node, &data)?;
            let Some(ribbon) = data.ribbon.borrow().clone() else {
                continue;
            };
            for button in &ribbon.buttons {
                buttons.push(self.ribbon_button(&data, button)?);
            }
        }
        Ok(buttons)
    }

    pub fn click_ribbon(&self, token: u64) -> Result<()> {
        if self.addin_mode.get() == AddinMode::Disabled {
            return failure(E_ACCESSDENIED);
        }
        let control = self.node(&ObjectRef::new(
            id("IRibbonControl"),
            ObjectKey::Host(token),
        ))?;
        if control.target.interface != id("IRibbonControl") {
            return failure(E_INVALIDARG);
        }
        let addin = self.node(&control.parent)?;
        let data = addin
            .addin
            .borrow()
            .clone()
            .filter(|data| data.connected.get())
            .ok_or(AutoError::Disconnected)?;
        let ribbon = data
            .ribbon
            .borrow()
            .clone()
            .ok_or(AutoError::Disconnected)?;
        let button = ribbon
            .buttons
            .iter()
            .find(|button| Self::key(&button.control) == token)
            .ok_or(AutoError::Disconnected)?;
        let view = self.ribbon_button(&data, button)?;
        if !view.enabled || !view.visible {
            return failure(E_ACCESSDENIED);
        }
        let action = button
            .spec
            .on_action
            .as_deref()
            .ok_or(AutoError::Status(E_ACCESSDENIED.0))?;
        self.ribbon_invoke(&data, action, &control)?;
        Ok(())
    }
}
