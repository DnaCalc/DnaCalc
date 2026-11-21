//! The page's text and optional bytes, shared by browser and desktop worker transports.

use crate::server::Server;
use crate::wire::{self, AddinList, Event, FromServer, Reply, Request, ToServer};

pub struct PageMessage {
    message: ToServer,
    bytes: Option<Vec<u8>>,
}

pub enum DesktopCommand<'a> {
    OpenStartupFile,
    ChooseWorkbook,
    Addin(AddinCommand<'a>),
}

pub enum AddinCommand<'a> {
    List,
    Choose,
    Load(&'a str),
    Unload(&'a str),
}

impl PageMessage {
    pub fn decode(text: &str, bytes: Option<Vec<u8>>) -> Result<Self, String> {
        Ok(Self {
            message: wire::decode(text)?,
            bytes,
        })
    }

    /// Classifies platform services before the native compositor borrows the workbook server.
    pub fn desktop_command(&self, has_startup_file: bool) -> Option<DesktopCommand<'_>> {
        if self.bytes.is_none() {
            let command = match &self.message.request {
                Request::Addins => Some(AddinCommand::List),
                Request::LoadAddinNative => Some(AddinCommand::Choose),
                Request::LoadAddin { path } => Some(AddinCommand::Load(path)),
                Request::UnloadAddin { path } => Some(AddinCommand::Unload(path)),
                _ => None,
            };
            if let Some(command) = command {
                return Some(DesktopCommand::Addin(command));
            }
        }

        let Request::Command(envelope) = &self.message.request else {
            return None;
        };
        match *envelope.request {
            Request::OpenNative if !has_startup_file => Some(DesktopCommand::ChooseWorkbook),
            Request::OpenNative | Request::New | Request::OpenTemplate { .. }
                if has_startup_file =>
            {
                Some(DesktopCommand::OpenStartupFile)
            }
            _ => None,
        }
    }

    /// Supplies the selected file in the original page command and request identity.
    pub fn open_selected(&mut self, name: String, bytes: Vec<u8>) {
        let Request::Command(envelope) = &mut self.message.request else {
            return;
        };
        *envelope.request = Request::Open { name };
        self.bytes = Some(bytes);
    }

    pub fn cancelled_open(&self) -> String {
        reply(self.message.id, Ok(Reply::OpenCancelled))
    }

    pub fn failed_open(&self, message: String) -> String {
        failure(Some(self.message.id), message)
    }

    pub fn addins(&self, result: Result<AddinList, String>) -> String {
        reply(self.message.id, result.map(Reply::Addins))
    }
}

pub fn answer(
    server: &mut Server,
    message: Result<PageMessage, String>,
) -> (String, Option<Vec<u8>>) {
    match message {
        Ok(PageMessage { message, bytes }) => {
            let answer = server.handle(message.request, bytes);
            (reply(message.id, answer.reply), answer.bytes)
        }
        Err(message) => (failure(None, message), None),
    }
}

fn reply(id: u64, result: Result<Reply, String>) -> String {
    match result {
        Ok(reply) => wire::encode(&FromServer::Reply { id, reply }),
        Err(message) => failure(Some(id), message),
    }
}

pub fn failure(id: Option<u64>, message: String) -> String {
    wire::encode(&FromServer::Failed { id, message })
}

pub fn started() -> String {
    wire::encode(&FromServer::Started)
}

pub fn event(event: Event) -> Result<(String, Option<Vec<u8>>), String> {
    match event {
        Event::Tiles(batch) => {
            wire::encode_tiles(&batch).map(|bytes| (wire::TILE_TAG.to_owned(), Some(bytes)))
        }
        control => Ok((wire::encode(&FromServer::Event(control)), None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(request: Request, bytes: Option<Vec<u8>>) -> PageMessage {
        PageMessage::decode(&wire::encode(&ToServer { id: 19, request }), bytes).unwrap()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn native_open_keeps_the_original_command_envelope() {
        let server = Server::new();
        let editing = Request::OpenForEditing {
            name: "t.xltx".into(),
        };
        assert!(call(Request::Summary, None).desktop_command(true).is_none());
        assert!(
            call(server.envelope(editing), None)
                .desktop_command(true)
                .is_none()
        );
        assert!(PageMessage::decode("not json", None).is_err());

        for startup in [
            Request::New,
            Request::OpenTemplate {
                name: "Book.xltx".into(),
            },
            Request::OpenNative,
        ] {
            let mut page = call(server.envelope(startup), None);
            assert!(matches!(
                page.desktop_command(true),
                Some(DesktopCommand::OpenStartupFile)
            ));
            page.open_selected("named.xlsx".into(), vec![2, 3]);
            assert_eq!(page.message.id, 19);
            assert_eq!(
                page.message.request,
                server.envelope(Request::Open {
                    name: "named.xlsx".into()
                })
            );
            assert_eq!(page.bytes, Some(vec![2, 3]));
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn native_services_and_worker_fallback_use_the_same_replies() {
        let mut server = Server::new();
        let open = call(server.envelope(Request::OpenNative), None);
        assert!(matches!(
            open.desktop_command(false),
            Some(DesktopCommand::ChooseWorkbook)
        ));
        assert_eq!(
            wire::decode(&open.cancelled_open()),
            Ok(FromServer::Reply {
                id: 19,
                reply: Reply::OpenCancelled
            })
        );
        let (text, bytes) = answer(&mut server, Ok(call(Request::OpenNative, None)));
        assert!(bytes.is_none());
        assert_eq!(
            wire::decode(&text),
            Ok(FromServer::Failed {
                id: Some(19),
                message: "Native file selection is unavailable in this worker".into(),
            })
        );

        let query = call(Request::Addins, None);
        assert!(matches!(
            query.desktop_command(false),
            Some(DesktopCommand::Addin(AddinCommand::List))
        ));
        let desktop = query.addins(Ok(AddinList::default()));
        assert_eq!(answer(&mut server, Ok(query)), (desktop, None));

        let load = Request::LoadAddin {
            path: "local.xll".into(),
        };
        assert!(
            call(load.clone(), Some(vec![]))
                .desktop_command(false)
                .is_none()
        );
        let load = call(load, None);
        assert!(matches!(
            load.desktop_command(false),
            Some(DesktopCommand::Addin(AddinCommand::Load("local.xll")))
        ));
        let (text, _) = answer(&mut server, Ok(load));
        assert_eq!(
            wire::decode(&text),
            Ok(FromServer::Failed {
                id: Some(19),
                message: crate::server::ADDINS_DESKTOP_ONLY.to_owned(),
            })
        );
    }
}
