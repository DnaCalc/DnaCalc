//! The native side of OneCalc's `SessionPort` in the desktop exe: the session, the file dialogs and
//! the add-ins the page's requests reach.
//!
//! The page in WebView2 is the same page the browsers run; its link ([`dna_onecalc::bridge`]) posts
//! each request as JSON text, and [`Host::answer`] answers with the JSON of the reply. Almost every
//! request is the session's own ([`dna_onecalc::port::LocalPort`]); this host adds only what a page
//! cannot do: [`Request::OpenNative`] and [`Request::SaveNative`] show the native dialogs through
//! [`Files`], and a session saved or opened lists the add-ins this process has loaded.
//!
//! It holds no window and no COM: [`Files`] is the one seam to the platform, so the unit tests give
//! it a scripted chooser and drive the same code the window runs.

use dna_onecalc::bridge::{Answer, Call};
use dna_onecalc::port::{LocalPort, Reply, Request, SessionPort};
use dna_onecalc::{AddinRef, Session};

/// The file choices a person makes in the native dialogs.
pub trait Files {
    /// The workbook the person chooses to open, with its file name; `None` when they cancel.
    fn choose_open(&mut self) -> Result<Option<(String, Vec<u8>)>, String>;

    /// Asks where to save `bytes` (suggesting `suggested`), writes them there and returns the
    /// path; `None` when the person cancels.
    fn choose_save(&mut self, suggested: &str, bytes: &[u8]) -> Result<Option<String>, String>;
}

/// The session behind the page, with the dialogs and the add-ins this process loaded.
pub struct Host<F> {
    port: LocalPort,
    files: F,
    loaded: Vec<AddinRef>,
}

impl<F: Files> Host<F> {
    pub fn new(session: Session, files: F) -> Self {
        Host {
            port: LocalPort::new(session),
            files,
            loaded: Vec::new(),
        }
    }

    /// The add-ins this process has loaded, which a saved session lists.
    pub fn set_loaded(&mut self, loaded: Vec<AddinRef>) {
        self.loaded = loaded;
    }

    /// For the owner that publishes add-in functions between requests.
    pub fn session_mut(&mut self) -> &mut Session {
        self.port.session_mut()
    }

    pub fn session(&self) -> &Session {
        self.port.session()
    }

    /// The JSON text of the answer to the JSON text of a call; `None` for text that is no call.
    pub fn answer(&mut self, text: &str) -> Option<String> {
        let Call { id, request } = serde_json::from_str(text).ok()?;
        let reply = self.handle(request);
        serde_json::to_string(&Answer { id, reply }).ok()
    }

    /// The loaded add-ins join the list the session keeps: a loaded add-in already listed (by
    /// path) takes its current hash.
    fn list_loaded(&mut self) {
        let mut listed = self.port.session().addins().to_vec();
        for add_in in &self.loaded {
            match listed.iter_mut().find(|held| held.path == add_in.path) {
                Some(held) => *held = add_in.clone(),
                None => listed.push(add_in.clone()),
            }
        }
        self.port.session_mut().set_addins(listed);
    }

    /// An opened or new session: the add-ins this process has loaded are marked as running.
    fn finish(&mut self, reply: Reply) -> Reply {
        match reply {
            Reply::Opened(mut opened) => {
                opened.loaded = self.loaded.iter().map(|a| a.path.clone()).collect();
                Reply::Opened(opened)
            }
            other => other,
        }
    }

    pub fn handle(&mut self, request: Request) -> Reply {
        match request {
            Request::OpenNative => match self.files.choose_open() {
                Err(message) => Reply::Refused { message },
                Ok(None) => Reply::Cancelled,
                Ok(Some((name, bytes))) => {
                    let reply = self.port.send(Request::Open { name, bytes });
                    self.finish(reply)
                }
            },
            Request::SaveNative => {
                self.list_loaded();
                match self.port.send(Request::Save) {
                    Reply::File { name, bytes } => match self.files.choose_save(&name, &bytes) {
                        Ok(Some(path)) => Reply::Saved { path },
                        Ok(None) => Reply::Cancelled,
                        Err(message) => Reply::Refused { message },
                    },
                    other => other,
                }
            }
            Request::Save => {
                self.list_loaded();
                self.port.send(Request::Save)
            }
            request @ (Request::Open { .. } | Request::New) => {
                let reply = self.port.send(request);
                self.finish(reply)
            }
            other => self.port.send(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dna_onecalc::shown::Authored;

    /// A chooser the test scripts: what the person would pick, and what was written.
    #[derive(Default)]
    struct Scripted {
        open: Option<(String, Vec<u8>)>,
        save_to: Option<String>,
        written: Vec<(String, Vec<u8>)>,
    }

    impl Files for Scripted {
        fn choose_open(&mut self) -> Result<Option<(String, Vec<u8>)>, String> {
            Ok(self.open.take())
        }

        fn choose_save(&mut self, suggested: &str, bytes: &[u8]) -> Result<Option<String>, String> {
            Ok(self.save_to.take().map(|path| {
                self.written.push((path.clone(), bytes.to_vec()));
                assert_eq!(suggested, "onecalc-session.xlsx");
                path
            }))
        }
    }

    fn host() -> Host<Scripted> {
        Host::new(Session::new("Sheet1").unwrap(), Scripted::default())
    }

    fn shown(reply: Reply) -> dna_onecalc::shown::Shown {
        match reply {
            Reply::Shown(shown) => *shown,
            other => panic!("{other:?}"),
        }
    }

    /// Native-only: the exe's host. A session saved through the native dialog is a workbook the
    /// same host opens through its native open dialog, with the same formula, constants and
    /// options and the add-ins this process loaded, listed.
    #[test]
    fn a_session_saved_through_the_dialog_opens_through_the_dialog() {
        let mut first = host();
        first.set_loaded(vec![AddinRef {
            kind: "xll".into(),
            path: "C:\\libs\\Add.xll".into(),
            hash: "sha256:00".into(),
        }]);
        let mut authored = Authored::example_sum();
        authored.locale = "de-DE".into();
        authored.number_format = "0,00".into();
        assert_eq!(shown(first.handle(Request::Evaluate(authored.clone()))).result, "number 3.5");
        first.files.save_to = Some("C:\\work\\session.xlsx".into());
        let saved = first.handle(Request::SaveNative);
        assert_eq!(saved, Reply::Saved { path: "C:\\work\\session.xlsx".into() });
        let (_, bytes) = first.files.written.remove(0);

        let mut second = host();
        second.set_loaded(first.loaded.clone());
        second.files.open = Some(("session.xlsx".into(), bytes));
        let Reply::Opened(opened) = second.handle(Request::OpenNative) else {
            panic!("not opened");
        };
        assert_eq!(opened.name, "session.xlsx");
        assert_eq!(opened.authored, authored);
        assert_eq!(opened.shown.result, "number 3.5");
        assert_eq!(opened.addins, first.loaded);
        assert_eq!(opened.loaded, ["C:\\libs\\Add.xll"]);
    }

    /// Native-only: cancelling a dialog changes nothing; a file outside the constraints is the
    /// typed refusal with the offer; a VBA module refuses the save naming its bead.
    #[test]
    fn cancel_refusal_and_vba_leave_the_session_as_it_was() {
        let mut host = host();
        host.handle(Request::Evaluate(Authored::example_sum()));
        assert_eq!(host.handle(Request::OpenNative), Reply::Cancelled);
        assert_eq!(host.handle(Request::SaveNative), Reply::Cancelled);
        host.files.open = Some(("bad.xlsx".into(), b"not a workbook".to_vec()));
        match host.handle(Request::OpenNative) {
            Reply::NotASession { offer, .. } => assert_eq!(offer, "Open in DNA Calc"),
            other => panic!("{other:?}"),
        }
        assert_eq!(host.session().formula(), "=SUM(A1:A3)");
        host.handle(Request::LoadVba {
            name: "Module1".into(),
            source: calc_vba_example(),
        });
        match host.handle(Request::SaveNative) {
            Reply::Refused { message } => assert!(message.contains("dc-0f1k.4"), "{message}"),
            other => panic!("{other:?}"),
        }
    }

    fn calc_vba_example() -> String {
        "Public Function Add2(a, b)\n Add2 = a + b\nEnd Function\n".to_owned()
    }

    /// Native-only: the wire text a page posts is answered with the same id, in JSON.
    #[test]
    fn a_posted_call_is_answered_with_its_id() {
        let mut host = host();
        let call = Call {
            id: 41,
            request: Request::Evaluate(Authored::example_sum()),
        };
        let text = serde_json::to_string(&call).unwrap();
        let answer: Answer = serde_json::from_str(&host.answer(&text).unwrap()).unwrap();
        assert_eq!(answer.id, 41);
        assert!(matches!(answer.reply, Reply::Shown(_)));
        assert!(host.answer("not json").is_none());
    }
}
