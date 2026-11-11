//! The native side of OneCalc's `SessionPort` in the desktop exe: the session, the file dialogs and
//! the add-ins the page's requests reach.
//!
//! The page in WebView2 is the same page the browsers run; its link ([`dna_onecalc::bridge`]) posts
//! each request as JSON text, and [`Host::answer`] answers with the JSON of the reply. Almost every
//! request is the session's own ([`dna_onecalc::port::LocalPort`]); this host adds only what a page
//! cannot do: [`Request::OpenNative`] and [`Request::SaveNative`] show the native dialogs through
//! [`Files`], and the add-in requests ([`Request::LoadAddinNative`], [`Request::LoadAddin`],
//! [`Request::UnloadAddin`], [`Request::Addins`]) load and unload add-ins through [`Libraries`] and
//! publish what they register to the session as its one table of functions. A session saved lists
//! the add-ins this process has loaded; one opened lists the file's, loaded or not (the surface
//! offers to load them, never the host).
//!
//! It holds no window and no COM: [`Files`] and [`Libraries`] are the seams to the platform and to
//! `calc-addin-host`, so the unit tests give them scripted implementations and drive the same code
//! the window runs.

use calc_addin_host::{RtdClient, RtdInput, RtdProvider};
use calc_fml::CallableTable;
use dna_onecalc::bridge::{Answer, Call};
use dna_onecalc::port::{LoadedAddin, LocalPort, Reply, Request, SessionPort};
use dna_onecalc::{AddinRef, Session};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What the person answers when they close a window that holds unsaved changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseChoice {
    /// Save, then close.
    Save,
    /// Close and lose the changes.
    Discard,
    /// Keep the window open.
    Cancel,
}

/// The file choices a person makes in the native dialogs.
pub trait Files {
    /// The workbook the person chooses to open, with its file name; `None` when they cancel.
    fn choose_open(&mut self) -> Result<Option<(String, Vec<u8>)>, String>;

    /// The URL security zone in the Mark of the Web (`Zone.Identifier` stream) of the file
    /// `choose_open` returned last; `None` when the file has none or the host cannot tell.
    fn opened_zone(&self) -> Option<u32> {
        None
    }

    /// Asks where to save `bytes` (suggesting `suggested`), writes them there and returns the
    /// path; `None` when the person cancels.
    fn choose_save(&mut self, suggested: &str, bytes: &[u8]) -> Result<Option<String>, String>;

    /// The add-in the person chooses to load; `None` when they cancel.
    fn choose_addin(&mut self) -> Result<Option<PathBuf>, String>;

    /// The path of the file `choose_open` or `read_recent` returned last, when it has one.
    fn opened_path(&self) -> Option<PathBuf> {
        None
    }

    /// Reads the file at `path` as `choose_open` reads the one the person chooses (a recent file).
    fn read_recent(&mut self, path: &Path) -> Result<(String, Vec<u8>), String> {
        Err(format!("{} cannot be read here", path.display()))
    }

    /// The files opened or saved most recently, newest first.
    fn recent(&self) -> Vec<String> {
        Vec::new()
    }

    /// Notes that the person opened or saved the file at `path`.
    fn remember(&mut self, _path: &str) {}

    /// Writes `bytes` to the file at `path` without asking, as Save does for a file it knows;
    /// `Ok(false)` when this host always asks.
    fn write_in_place(&mut self, _path: &str, _bytes: &[u8]) -> Result<bool, String> {
        Ok(false)
    }

    /// What the person wants for the unsaved changes of `name` when they close the window.
    fn confirm_close(&mut self, _name: &str) -> CloseChoice {
        CloseChoice::Discard
    }

    /// Opens a file OneCalc refused in DNA Calc: the file at `path` when it has one, otherwise
    /// `bytes` (a file the page read) written to a place of its own first.
    fn open_in_dna_calc(
        &mut self,
        path: Option<&Path>,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        let _ = (path, name, bytes);
        Err("DNA Calc cannot be started from here".to_owned())
    }
}

/// The add-ins a process has loaded and the one table of functions they publish.
pub trait Libraries {
    /// Loads the .xll at `path`, running its open routine. A failed load leaves nothing registered.
    fn load_xll(&mut self, path: &Path) -> Result<(), String>;

    /// Unloads the .xll loaded from `path`, running its close routine.
    fn unload_xll(&mut self, path: &Path) -> Result<(), String>;

    /// The .xll add-ins loaded, in load order, with the worksheet functions each registered.
    fn loaded(&self) -> Vec<(PathBuf, Vec<String>)>;

    /// Loads a VBA add-in (an .xlam) from its project, raising `Workbook_Open`, then
    /// `Workbook_AddinInstall` when `install`. A project that does not compile loads nothing; the
    /// answer lists what its events reported.
    fn load_xlam(
        &mut self,
        path: &Path,
        project: &calc_vba::OfficeProject,
        install: bool,
    ) -> Result<Vec<String>, String>;

    /// Unloads the VBA add-in loaded from `path`.
    fn unload_xlam(&mut self, path: &Path) -> Result<(), String>;

    /// The VBA add-ins loaded, in load order, with their worksheet functions.
    fn loaded_xlam(&self) -> Vec<(PathBuf, Vec<String>)>;

    /// Compiles a VBA standard module and keeps it beside the add-ins; a module that does not
    /// compile changes nothing.
    fn load_vba(&mut self, name: &str, source: &str) -> Result<(), String>;

    /// Removes the VBA modules.
    fn unload_vba(&mut self);

    /// The one table of every function the add-ins and VBA modules publish.
    fn callables(&self) -> Result<CallableTable, String>;

    /// Whether add-ins registered or removed functions since the table was last published (an
    /// add-in may register after its open routine returns).
    fn changed(&mut self) -> bool;

    /// Tells the add-ins a cell changed (the `SheetChange` event).
    fn cell_changed(&mut self, row: u32, column: u32);

    /// Tells the add-ins a calculation finished (the `AfterCalculate` event), so the ones that
    /// wait for events can carry on.
    fn calculated(&mut self);

    /// The RTD client over the add-ins' real-time servers, when they are enabled.
    fn rtd(&self) -> Option<Rc<RtdClient>> {
        None
    }

    /// The names the add-ins registered, which the session binds to the handles an RTD
    /// declaration is keyed by.
    fn registration_names(&self) -> Vec<String> {
        Vec::new()
    }

    /// The RTD updates that have arrived; `evaluating` takes only those the evaluation just done
    /// may see.
    fn poll_rtd(&mut self, _evaluating: bool) -> Vec<RtdInput> {
        Vec::new()
    }
}

/// A file OneCalc refused, kept so that it can be opened in DNA Calc.
struct Refused {
    path: Option<PathBuf>,
    name: String,
    bytes: Vec<u8>,
}

/// The session behind the page, with the dialogs and the add-ins this process loaded.
pub struct Host<F, L> {
    port: LocalPort,
    files: F,
    libs: L,
    /// What each loaded VBA add-in's load reported, by path.
    xlam_notes: Vec<(PathBuf, Vec<String>)>,
    /// The grid's entries as the last evaluation had them, to tell the add-ins which cells changed.
    last_grid: Vec<String>,
    /// The file the session was opened from or last saved to.
    path: Option<String>,
    /// What the page last said of the session: its name and whether it has unsaved changes.
    state_name: String,
    dirty: bool,
    /// The file last refused, for Open in DNA Calc.
    refused: Option<Refused>,
    /// The formula and cell of the last evaluation, to tell a replaced formula (whose RTD
    /// connections end) from the same one calculated again.
    last_formula: Option<(String, String)>,
    /// Whether the next RTD poll follows an evaluation (see [`Libraries::poll_rtd`]).
    after_evaluation: bool,
}

impl<F: Files, L: Libraries> Host<F, L> {
    pub fn new(session: Session, files: F, libs: L) -> Self {
        let mut host = Host {
            port: LocalPort::new(session),
            files,
            libs,
            xlam_notes: Vec::new(),
            last_grid: Vec::new(),
            path: None,
            state_name: String::new(),
            dirty: false,
            refused: None,
            last_formula: None,
            after_evaluation: false,
        };
        let _ = host.publish();
        host
    }

    /// For the owner that gives the session its owner of asynchronous returns.
    #[allow(dead_code)]
    pub fn session_mut(&mut self) -> &mut Session {
        self.port.session_mut()
    }

    #[cfg(test)]
    pub fn session(&self) -> &Session {
        self.port.session()
    }

    /// Publishes the add-ins' table to the session. A table that cannot be built leaves the
    /// session with the one it had.
    fn publish(&mut self) -> Result<(), String> {
        let table = self.libs.callables()?;
        self.port.session_mut().set_user_functions(table);
        Ok(())
    }

    /// The JSON text of the answer to the JSON text of a call; `None` for text that is no call.
    pub fn answer(&mut self, text: &str) -> Option<String> {
        let Call { id, request } = serde_json::from_str(text).ok()?;
        let reply = self.handle(request);
        serde_json::to_string(&Answer { id, reply }).ok()
    }

    fn loaded_view(&self) -> Vec<LoadedAddin> {
        let xlls = self
            .libs
            .loaded()
            .into_iter()
            .map(|(path, functions)| ("xll", path, functions));
        let xlams = self
            .libs
            .loaded_xlam()
            .into_iter()
            .map(|(path, functions)| ("xlam", path, functions));
        xlls.chain(xlams)
            .map(|(kind, path, functions)| LoadedAddin {
                kind: kind.to_owned(),
                hash: file_hash(&path).unwrap_or_default(),
                notes: self
                    .xlam_notes
                    .iter()
                    .find(|(held, _)| *held == path)
                    .map(|(_, notes)| notes.clone())
                    .unwrap_or_default(),
                path: path.display().to_string(),
                functions,
            })
            .collect()
    }

    fn loaded_paths(&self) -> Vec<PathBuf> {
        self.libs
            .loaded()
            .into_iter()
            .chain(self.libs.loaded_xlam())
            .map(|(path, _)| path)
            .collect()
    }

    /// The loaded add-ins join the list the session keeps: a loaded add-in already listed (by
    /// path) takes its current hash.
    fn list_loaded(&mut self) {
        let mut listed = self.port.session().addins().to_vec();
        for add_in in self.loaded_view() {
            let entry = AddinRef {
                kind: add_in.kind,
                path: add_in.path,
                hash: add_in.hash,
            };
            match listed.iter_mut().find(|held| held.path == entry.path) {
                Some(held) => *held = entry,
                None => listed.push(entry),
            }
        }
        self.port.session_mut().set_addins(listed);
    }

    /// An opened or new session: the add-ins this process has loaded are marked as running, and
    /// their functions are the session's (a VBA module went with the session it was loaded into).
    fn finish(&mut self, reply: Reply) -> Reply {
        match reply {
            Reply::Opened(mut opened) => {
                self.libs.unload_vba();
                let _ = self.publish();
                self.release_rtd();
                opened.shown.functions_version = self.port.session().functions_version();
                opened.loaded = self
                    .loaded_paths()
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect();
                Reply::Opened(opened)
            }
            other => other,
        }
    }

    fn load_addin(&mut self, path: &Path, expected_hash: Option<&str>) -> Reply {
        let refused = |message: String| Reply::Refused { message };
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
        let is_xlam = extension.as_deref() == Some("xlam");
        if extension.as_deref() != Some("xll") && !is_xlam {
            return refused(format!(
                "{} is not an .xll or .xlam add-in; only those load here",
                path.display()
            ));
        }
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        if self.loaded_paths().contains(&path) {
            return refused(format!("{} is already loaded", path.display()));
        }
        let hash = match file_hash(&path) {
            Ok(hash) => hash,
            Err(message) => return refused(message),
        };
        if let Some(expected) = expected_hash.filter(|expected| !expected.is_empty())
            && expected != hash
        {
            return refused(format!(
                "{} is not the file the session listed (it holds {hash}, the session listed {expected}); choose it with Load add-in to accept the change",
                path.display()
            ));
        }
        if is_xlam {
            // The person loading an add-in installs it, as Excel's Add-ins dialog does; a session
            // that lists it (a reopen) only opens it.
            let install = expected_hash.is_none();
            let loaded = std::fs::read(&path)
                .map_err(|e| format!("{}: {e}", path.display()))
                .and_then(|bytes| dna_onecalc::file::read_vba_addin(&bytes))
                .and_then(|project| self.libs.load_xlam(&path, &project, install));
            match loaded {
                Ok(notes) => self.xlam_notes.push((path.clone(), notes)),
                Err(message) => return refused(format!("{}: {message}", path.display())),
            }
        } else if let Err(message) = self.libs.load_xll(&path) {
            return refused(message);
        }
        if let Err(message) = self.publish() {
            return refused(message);
        }
        self.list_loaded();
        Reply::Addins(self.loaded_view())
    }

    fn unload_addin(&mut self, path: &str) -> Reply {
        let wanted = Path::new(path);
        let path = self
            .loaded_paths()
            .into_iter()
            .find(|held| held == wanted)
            .unwrap_or_else(|| wanted.to_owned());
        let unloaded = if self
            .libs
            .loaded_xlam()
            .iter()
            .any(|(held, _)| *held == path)
        {
            self.xlam_notes.retain(|(held, _)| *held != path);
            self.libs.unload_xlam(&path)
        } else {
            self.libs.unload_xll(&path)
        };
        if let Err(message) = unloaded {
            return Reply::Refused { message };
        }
        if let Err(message) = self.publish() {
            return Reply::Refused { message };
        }
        let listed: Vec<AddinRef> = self
            .port
            .session()
            .addins()
            .iter()
            .filter(|held| Path::new(&held.path) != path)
            .cloned()
            .collect();
        self.port.session_mut().set_addins(listed);
        Reply::Addins(self.loaded_view())
    }

    /// A file chosen to open (by dialog or from the recent list): opened, and remembered when it
    /// is a session; refused with the offer when it is not.
    fn open_chosen(&mut self, name: String, bytes: Vec<u8>) -> Reply {
        let zone = self.files.opened_zone();
        let path = self.files.opened_path();
        let kept = bytes.clone();
        let reply = self.port.send(Request::Open {
            name: name.clone(),
            bytes,
            zone,
        });
        let opened = matches!(reply, Reply::Opened(_));
        self.note_refusal(&reply, path.clone(), Some(name), Some(kept));
        if opened {
            self.path = path.map(|p| p.display().to_string());
            if let Some(path) = self.path.clone() {
                self.files.remember(&path);
            }
        }
        self.finish(reply)
    }

    /// Keeps a refused file for Open in DNA Calc, and forgets the last one when a file opens.
    fn note_refusal(
        &mut self,
        reply: &Reply,
        path: Option<PathBuf>,
        name: Option<String>,
        bytes: Option<Vec<u8>>,
    ) {
        match (reply, name, bytes) {
            (Reply::NotASession { .. }, Some(name), Some(bytes)) => {
                self.refused = Some(Refused { path, name, bytes });
            }
            (Reply::Opened(_), ..) => self.refused = None,
            _ => {}
        }
    }

    /// Save: to the file the session came from when there is one of the kind the session saves as
    /// now, otherwise (and for Save as) through the dialog.
    fn save_native(&mut self, ask: bool) -> Reply {
        self.list_loaded();
        match self.port.send(Request::Save) {
            Reply::File { name, bytes } => {
                if !ask
                    && let Some(path) = self.path.clone()
                    && same_extension(&path, &name)
                {
                    match self.files.write_in_place(&path, &bytes) {
                        Ok(true) => {
                            self.dirty = false;
                            self.files.remember(&path);
                            return Reply::Saved { path };
                        }
                        Ok(false) => {}
                        Err(message) => return Reply::Refused { message },
                    }
                }
                match self.files.choose_save(&name, &bytes) {
                    Ok(Some(path)) => {
                        self.path = Some(path.clone());
                        self.dirty = false;
                        self.files.remember(&path);
                        Reply::Saved { path }
                    }
                    Ok(None) => Reply::Cancelled,
                    Err(message) => Reply::Refused { message },
                }
            }
            other => other,
        }
    }

    /// The window is closing: with unsaved changes the person is asked, and `false` keeps it open
    /// (they cancelled, or the save they asked for did not happen).
    pub fn close_requested(&mut self) -> bool {
        if !self.dirty {
            return true;
        }
        let name = if self.state_name.is_empty() {
            "Untitled".to_owned()
        } else {
            self.state_name.clone()
        };
        match self.files.confirm_close(&name) {
            CloseChoice::Discard => true,
            CloseChoice::Cancel => false,
            CloseChoice::Save => matches!(self.save_native(false), Reply::Saved { .. }),
        }
    }

    /// The handles the session gives the add-ins' registrations: what an RTD declaration is keyed by.
    fn rtd_providers(&self) -> BTreeSet<RtdProvider> {
        self.libs
            .registration_names()
            .iter()
            .filter_map(|name| self.port.session().bind_function(name))
            .collect()
    }

    /// The formula is being replaced: its topics leave the ledger now and disconnect once the new
    /// formula has been calculated ([`Self::settle_rtd`]).
    fn retire_rtd(&mut self) {
        let Some(rtd) = self.libs.rtd() else { return };
        let (providers, at) = (self.rtd_providers(), self.port.session().formula_cell());
        match rtd.begin_replacement(&providers, at) {
            Ok(retired) => {
                for (topic, lease) in retired {
                    self.port.session_mut().retire_rtd(topic, lease);
                }
            }
            Err(message) => eprintln!("dna-onecalc-desktop: RTD: {message}"),
        }
    }

    /// An evaluation ended: the replaced formula's topics disconnect, and the client keeps only
    /// the connections the evaluation declared.
    fn settle_rtd(&mut self) {
        let Some(rtd) = self.libs.rtd() else { return };
        let (providers, at) = (self.rtd_providers(), self.port.session().formula_cell());
        let invocations = self.port.session().rtd_invocations().clone();
        if trace() {
            eprintln!(
                "onecalc-trace: settle: {} invocation(s), {} read(s)",
                invocations.len(),
                self.port.session().rtd_reads().len()
            );
        }
        for result in [
            rtd.finish_replacement(&providers, at),
            rtd.reconcile_evaluation(&providers, at, &invocations),
        ] {
            if let Err(message) = result {
                eprintln!("dna-onecalc-desktop: RTD: {message}");
            }
        }
        // Admissions are the ledger's record; nothing here lists them.
        self.port.session_mut().take_rtd_admissions();
        self.after_evaluation = true;
    }

    /// The session was replaced: the old formula's topics disconnect.
    fn release_rtd(&mut self) {
        self.last_formula = None;
        if self.libs.rtd().is_some() {
            self.retire_rtd();
            self.settle_rtd();
        }
    }

    /// Takes the real-time values that arrived since the last call, between requests. `true` when
    /// the formula read one of them: the page evaluates it again.
    pub fn poll_rtd(&mut self) -> bool {
        if self.libs.rtd().is_none() {
            return false;
        }
        let mut changed = false;
        let evaluating = std::mem::take(&mut self.after_evaluation);
        let inputs = self.libs.poll_rtd(evaluating);
        if trace() && (!inputs.is_empty() || evaluating) {
            eprintln!(
                "onecalc-trace: poll (after evaluation {evaluating}) gave {} input(s)",
                inputs.len()
            );
        }
        for input in inputs {
            let session = self.port.session_mut();
            let disposition = session.update_rtd(
                input.topic.clone(),
                input.lease,
                input.sequence,
                input.value,
            );
            if trace() {
                eprintln!(
                    "onecalc-trace: update {:?} lease {:?} sequence {} admitted as {disposition:?}, read {}",
                    input.topic,
                    input.lease,
                    input.sequence,
                    session.reads_rtd(&input.topic, input.lease)
                );
            }
            // The session already admitted the connection's own first value as it was returned.
            if disposition == calc_fml::rtd::RtdDisposition::Accepted
                && session.reads_rtd(&input.topic, input.lease)
            {
                changed = true;
            }
        }
        self.port.session_mut().take_rtd_admissions();
        changed
    }

    pub fn handle(&mut self, request: Request) -> Reply {
        if self.libs.changed() {
            let _ = self.publish();
        }
        match request {
            Request::OpenNative => match self.files.choose_open() {
                Err(message) => Reply::Refused { message },
                Ok(None) => Reply::Cancelled,
                Ok(Some((name, bytes))) => self.open_chosen(name, bytes),
            },
            Request::OpenRecent { path } => match self.files.read_recent(Path::new(&path)) {
                Err(message) => Reply::Refused { message },
                Ok((name, bytes)) => self.open_chosen(name, bytes),
            },
            Request::Recent => Reply::Recent(self.files.recent()),
            Request::SessionState { dirty, name } => {
                self.dirty = dirty;
                self.state_name = name;
                Reply::Accepted
            }
            Request::OpenInDnaCalc => match &self.refused {
                None => Reply::Refused {
                    message: "No file was refused, so there is nothing to open in DNA Calc."
                        .to_owned(),
                },
                Some(file) => {
                    match self
                        .files
                        .open_in_dna_calc(file.path.as_deref(), &file.name, &file.bytes)
                    {
                        Ok(()) => Reply::Accepted,
                        Err(message) => Reply::Refused { message },
                    }
                }
            },
            // The person enabled macros for this file: the project compiles and `Workbook_Open`
            // runs; the add-ins the host loaded stay loaded (this is not a new file).
            Request::EnableMacros => match self.port.send(Request::EnableMacros) {
                Reply::Opened(mut opened) => {
                    let _ = self.publish();
                    opened.shown.functions_version = self.port.session().functions_version();
                    opened.loaded = self
                        .loaded_paths()
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect();
                    Reply::Opened(opened)
                }
                other => other,
            },
            Request::SaveNative => self.save_native(false),
            Request::SaveAsNative => self.save_native(true),
            Request::Save => {
                self.list_loaded();
                self.port.send(Request::Save)
            }
            request @ (Request::Open { .. } | Request::New) => {
                let (name, bytes) = match &request {
                    Request::Open { name, bytes, .. } => (Some(name.clone()), Some(bytes.clone())),
                    _ => (None, None),
                };
                let reply = self.port.send(request);
                if matches!(reply, Reply::Opened(_)) {
                    self.path = None;
                }
                self.note_refusal(&reply, None, name, bytes);
                self.finish(reply)
            }
            Request::LoadAddinNative => match self.files.choose_addin() {
                Err(message) => Reply::Refused { message },
                Ok(None) => Reply::Cancelled,
                Ok(Some(path)) => self.load_addin(&path, None),
            },
            Request::LoadAddin {
                path,
                expected_hash,
            } => self.load_addin(Path::new(&path), expected_hash.as_deref()),
            Request::UnloadAddin { path } => self.unload_addin(&path),
            Request::Addins => Reply::Addins(self.loaded_view()),
            // A VBA module lives beside the add-ins, so that loading one leaves the add-ins'
            // functions callable and loading an add-in leaves the module's.
            // In an .xlsm session the module is the file project's: the session edits it.
            request @ Request::LoadVba { .. }
                if self
                    .port
                    .session()
                    .project()
                    .is_some_and(|project| !project.modules().is_empty()) =>
            {
                self.port.send(request)
            }
            Request::LoadVba { name, source } => {
                let before = self.port.session().user_function_names();
                if let Err(message) = self.libs.load_vba(&name, &source) {
                    return Reply::Refused { message };
                }
                if let Err(message) = self.publish() {
                    return Reply::Refused { message };
                }
                self.port.session_mut().set_vba_loaded(true);
                let names = self
                    .port
                    .session()
                    .user_function_names()
                    .into_iter()
                    .filter(|name| !before.contains(name))
                    .collect();
                Reply::Functions { names }
            }
            Request::UnloadVba => {
                self.libs.unload_vba();
                let _ = self.publish();
                self.port.session_mut().set_vba_loaded(false);
                Reply::Accepted
            }
            Request::Evaluate(authored) => {
                // The add-ins hear of the cells the person changed (the grid's entries) and of the
                // formula's cell, then that a calculation finished.
                let columns = dna_onecalc::grid::COLUMNS as usize;
                let changed: Vec<(u32, u32)> = authored
                    .grid
                    .iter()
                    .enumerate()
                    .filter(|(index, entry)| {
                        self.last_grid
                            .get(*index)
                            .map_or(!entry.is_empty(), |last| last != *entry)
                    })
                    .map(|(index, _)| ((index / columns) as u32 + 1, (index % columns) as u32 + 1))
                    .collect();
                self.last_grid = authored.grid.clone();
                // A replaced formula lets go of the real-time topics it read; the topics the new
                // one reads connect first and the old ones disconnect after (as Excel does).
                let key = (authored.formula.clone(), authored.formula_cell.clone());
                let replaced = self
                    .last_formula
                    .replace(key.clone())
                    .is_some_and(|last| last != key);
                if replaced {
                    self.retire_rtd();
                }
                let reply = self.port.send(Request::Evaluate(authored));
                self.settle_rtd();
                let cell = self.port.session().formula_cell();
                for (row, column) in changed {
                    self.libs.cell_changed(row, column);
                }
                self.libs.cell_changed(cell.row(), cell.column());
                self.libs.calculated();
                reply
            }
            other => self.port.send(other),
        }
    }
}

/// Whether `DNACALC_ONECALC_TRACE` asks for a trace of the real-time updates on stderr.
fn trace() -> bool {
    std::env::var_os("DNACALC_ONECALC_TRACE").is_some()
}

/// Whether the file at `path` and the file name `name` have the same extension.
fn same_extension(path: &str, name: &str) -> bool {
    let extension = |text: &str| text.rsplit('.').next().map(str::to_ascii_lowercase);
    extension(path) == extension(name)
}

/// `sha256:` and the hex digest of the file, as a session lists an add-in.
fn file_hash(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("sha256:{}", calc_verdict::sha256_hex(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_fml::{CallableDescriptor, CallableParameter, UserFunction};
    use calc_value::CalcValue;
    use dna_onecalc::shown::Authored;
    use std::rc::Rc;

    /// A chooser the test scripts: what the person would pick, and what was written.
    #[derive(Default)]
    struct Scripted {
        open: Option<(String, Vec<u8>)>,
        save_to: Option<String>,
        addin: Option<PathBuf>,
        written: Vec<(String, Vec<u8>)>,
        zone: Option<u32>,
        /// The path `choose_open` and `read_recent` report as the file they read.
        path: Option<PathBuf>,
        /// What the recent list holds, and what `remember` was told.
        recent: Vec<String>,
        /// Whether Save may write in place; the paths it wrote to, with their bytes.
        in_place: bool,
        in_place_written: Vec<(String, Vec<u8>)>,
        /// What the person answers when asked about unsaved changes, and the names asked about.
        close: Option<CloseChoice>,
        asked: Vec<String>,
        /// A message when DNA Calc is absent; the files it was asked to open (path, name, bytes).
        dna_calc: Option<String>,
        launched: Vec<(Option<PathBuf>, String, usize)>,
    }

    impl Files for Scripted {
        fn choose_open(&mut self) -> Result<Option<(String, Vec<u8>)>, String> {
            Ok(self.open.take())
        }

        fn opened_zone(&self) -> Option<u32> {
            self.zone
        }

        fn choose_save(&mut self, suggested: &str, bytes: &[u8]) -> Result<Option<String>, String> {
            Ok(self.save_to.take().map(|path| {
                self.written.push((path.clone(), bytes.to_vec()));
                assert_eq!(suggested, "onecalc-session.xlsx");
                path
            }))
        }

        fn choose_addin(&mut self) -> Result<Option<PathBuf>, String> {
            Ok(self.addin.take())
        }

        fn opened_path(&self) -> Option<PathBuf> {
            self.path.clone()
        }

        fn read_recent(&mut self, path: &Path) -> Result<(String, Vec<u8>), String> {
            self.path = Some(path.to_owned());
            self.open
                .take()
                .ok_or_else(|| format!("{} is gone", path.display()))
        }

        fn recent(&self) -> Vec<String> {
            self.recent.clone()
        }

        fn remember(&mut self, path: &str) {
            self.recent.insert(0, path.to_owned());
        }

        fn write_in_place(&mut self, path: &str, bytes: &[u8]) -> Result<bool, String> {
            if self.in_place {
                self.in_place_written
                    .push((path.to_owned(), bytes.to_vec()));
            }
            Ok(self.in_place)
        }

        fn confirm_close(&mut self, name: &str) -> CloseChoice {
            self.asked.push(name.to_owned());
            self.close.expect("the test scripts the answer")
        }

        fn open_in_dna_calc(
            &mut self,
            path: Option<&Path>,
            name: &str,
            bytes: &[u8],
        ) -> Result<(), String> {
            if let Some(message) = &self.dna_calc {
                return Err(message.clone());
            }
            self.launched
                .push((path.map(Path::to_owned), name.to_owned(), bytes.len()));
            Ok(())
        }
    }

    /// An add-in function that doubles its argument.
    #[derive(Debug)]
    struct Double;
    impl UserFunction for Double {
        fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String> {
            Ok(CalcValue::number(
                arguments
                    .first()
                    .and_then(CalcValue::as_number)
                    .unwrap_or(0.)
                    * 2.,
            ))
        }
    }

    /// The add-in set the tests script: loading a file named `*.xll` registers `Double` (documented
    /// as `xlfRegister` would), and a file called `broken.xll` fails to load.
    #[derive(Default)]
    struct Fake {
        xlls: Vec<PathBuf>,
        /// Each VBA add-in loaded: its path, its module count and whether it was installed.
        xlams: Vec<(PathBuf, usize, bool)>,
        vba: Vec<String>,
        generation: u32,
        seen: u32,
        events: Vec<(u32, u32)>,
        calculations: u32,
    }

    impl Libraries for Fake {
        fn load_xll(&mut self, path: &Path) -> Result<(), String> {
            if path.file_name().is_some_and(|n| n == "broken.xll") {
                return Err("xlAutoOpen failed; after rollback: 0 active registrations".into());
            }
            self.xlls.push(path.to_owned());
            self.generation += 1;
            Ok(())
        }

        fn unload_xll(&mut self, path: &Path) -> Result<(), String> {
            let before = self.xlls.len();
            self.xlls.retain(|held| held != path);
            if self.xlls.len() == before {
                return Err(format!("{} is not loaded", path.display()));
            }
            self.generation += 1;
            Ok(())
        }

        fn loaded(&self) -> Vec<(PathBuf, Vec<String>)> {
            self.xlls
                .iter()
                .map(|path| (path.clone(), vec!["Double".to_owned()]))
                .collect()
        }

        fn load_xlam(
            &mut self,
            path: &Path,
            project: &calc_vba::OfficeProject,
            install: bool,
        ) -> Result<Vec<String>, String> {
            self.xlams
                .push((path.to_owned(), project.modules().len(), install));
            self.generation += 1;
            Ok(vec![format!("{} module(s)", project.modules().len())])
        }

        fn unload_xlam(&mut self, path: &Path) -> Result<(), String> {
            self.xlams.retain(|(held, _, _)| held != path);
            self.generation += 1;
            Ok(())
        }

        fn loaded_xlam(&self) -> Vec<(PathBuf, Vec<String>)> {
            self.xlams
                .iter()
                .map(|(path, _, _)| (path.clone(), vec!["DnaAdd".to_owned()]))
                .collect()
        }

        fn load_vba(&mut self, name: &str, source: &str) -> Result<(), String> {
            if source.contains("Function (") {
                return Err("does not compile".into());
            }
            self.vba.push(name.to_owned());
            Ok(())
        }

        fn unload_vba(&mut self) {
            self.vba.clear();
        }

        fn callables(&self) -> Result<CallableTable, String> {
            let mut table = CallableTable::new();
            if !self.xlls.is_empty() {
                table.register_described(
                    None,
                    "Double",
                    Rc::new(Double),
                    CallableDescriptor {
                        name: "Double".into(),
                        origin: "xll fake.xll".into(),
                        category: "Fakes".into(),
                        description: "Twice the number.".into(),
                        parameters: vec![CallableParameter {
                            name: "number".into(),
                            description: "What to double.".into(),
                        }],
                        ..CallableDescriptor::default()
                    },
                )?;
            }
            for name in &self.vba {
                table.register(&format!("{name}.Add2"), Rc::new(Double))?;
            }
            Ok(table)
        }

        fn changed(&mut self) -> bool {
            let changed = self.generation != self.seen;
            self.seen = self.generation;
            changed
        }

        fn cell_changed(&mut self, row: u32, column: u32) {
            self.events.push((row, column));
        }

        fn calculated(&mut self) {
            self.calculations += 1;
        }
    }

    fn host() -> Host<Scripted, Fake> {
        Host::new(
            Session::new("Sheet1").unwrap(),
            Scripted::default(),
            Fake::default(),
        )
    }

    fn shown(reply: Reply) -> dna_onecalc::shown::Shown {
        match reply {
            Reply::Shown(shown) => *shown,
            other => panic!("{other:?}"),
        }
    }

    fn addins(reply: Reply) -> Vec<LoadedAddin> {
        match reply {
            Reply::Addins(list) => list,
            other => panic!("{other:?}"),
        }
    }

    /// A file on disk the host can hash, named `name`, in a directory of its own.
    fn file_named(name: &str, content: &[u8]) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "dnacalc-onecalc-host-{}-{}",
            std::process::id(),
            calc_verdict::sha256_hex(name.as_bytes())
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn formula(text: &str) -> Authored {
        Authored {
            formula: text.to_owned(),
            ..Authored::default()
        }
    }

    /// Native-only: the exe's host. A session saved through the native dialog is a workbook the
    /// same host opens through its native open dialog, with the same formula, constants and
    /// options and the add-ins this process loaded, listed.
    #[test]
    fn a_session_saved_through_the_dialog_opens_through_the_dialog() {
        let path = file_named("Add.xll", b"MZ first");
        let mut first = host();
        let loaded = addins(first.handle(Request::LoadAddin {
            path: path.display().to_string(),
            expected_hash: None,
        }));
        assert_eq!(loaded.len(), 1);
        let mut authored = Authored::example_sum();
        authored.locale = "de-DE".into();
        authored.number_format = "0,00".into();
        assert_eq!(
            shown(first.handle(Request::Evaluate(authored.clone()))).result,
            "number 3.5"
        );
        first.files.save_to = Some("C:\\work\\session.xlsx".into());
        let saved = first.handle(Request::SaveNative);
        assert_eq!(
            saved,
            Reply::Saved {
                path: "C:\\work\\session.xlsx".into()
            }
        );
        let (_, bytes) = first.files.written.remove(0);

        let mut second = host();
        second.libs.xlls = first.libs.xlls.clone();
        second.files.open = Some(("session.xlsx".into(), bytes));
        let Reply::Opened(opened) = second.handle(Request::OpenNative) else {
            panic!("not opened");
        };
        assert_eq!(opened.name, "session.xlsx");
        assert_eq!(opened.authored, authored);
        assert_eq!(opened.shown.result, "number 3.5");
        assert_eq!(
            opened.addins,
            [AddinRef {
                kind: "xll".into(),
                path: loaded[0].path.clone(),
                hash: loaded[0].hash.clone(),
            }]
        );
        assert_eq!(opened.loaded, [loaded[0].path.clone()]);
    }

    /// A session workbook made by a host of its own, for the tests that open one.
    fn session_bytes(text: &str) -> Vec<u8> {
        let mut maker = host();
        maker.handle(Request::Evaluate(formula(text)));
        maker.files.save_to = Some("C:\\made\\made.xlsx".into());
        assert!(matches!(
            maker.handle(Request::SaveNative),
            Reply::Saved { .. }
        ));
        maker.files.written.remove(0).1
    }

    /// Native-only: Save writes to the file the session came from without asking, Save as asks, and
    /// a session with no file asks; every file opened or saved is remembered as recent.
    #[test]
    fn save_goes_to_the_known_file_and_save_as_asks() {
        let mut host = host();
        host.files.in_place = true;
        host.files.open = Some(("one.xlsx".into(), session_bytes("=1+1")));
        host.files.path = Some("C:\\work\\one.xlsx".into());
        assert!(matches!(host.handle(Request::OpenNative), Reply::Opened(_)));
        assert_eq!(host.files.recent, ["C:\\work\\one.xlsx"]);
        host.handle(Request::Evaluate(formula("=2+2")));
        // Save: in place, no dialog (a dialog here would answer Cancelled: nothing is scripted).
        assert_eq!(
            host.handle(Request::SaveNative),
            Reply::Saved {
                path: "C:\\work\\one.xlsx".into()
            }
        );
        assert_eq!(host.files.in_place_written.len(), 1);
        assert!(host.files.written.is_empty());
        // Save as asks, takes the new path, and Save then goes there.
        host.files.save_to = Some("C:\\work\\two.xlsx".into());
        assert_eq!(
            host.handle(Request::SaveAsNative),
            Reply::Saved {
                path: "C:\\work\\two.xlsx".into()
            }
        );
        assert_eq!(host.files.written.len(), 1);
        assert_eq!(
            host.handle(Request::SaveNative),
            Reply::Saved {
                path: "C:\\work\\two.xlsx".into()
            }
        );
        assert_eq!(host.files.in_place_written[1].0, "C:\\work\\two.xlsx");
        assert_eq!(
            host.handle(Request::Recent),
            Reply::Recent(host.files.recent.clone())
        );
        assert_eq!(host.files.recent[0], "C:\\work\\two.xlsx");
        // A new session has no file: Save asks.
        host.handle(Request::New);
        assert_eq!(host.handle(Request::SaveNative), Reply::Cancelled);
        // A session that saves as .xlsm does not go to an .xlsx path.
        assert!(super::same_extension("C:\\a\\b.XLSX", "b.xlsx"));
        assert!(!super::same_extension("C:\\a\\b.xlsx", "b.xlsm"));
    }

    /// Native-only: a recent file opens by its path; a path that is gone is a refusal that leaves the
    /// session as it was.
    #[test]
    fn a_recent_file_opens_by_path() {
        let mut host = host();
        host.handle(Request::Evaluate(formula("=SUM(1,2)")));
        match host.handle(Request::OpenRecent {
            path: "C:\\gone.xlsx".into(),
        }) {
            Reply::Refused { message } => assert!(message.contains("gone.xlsx"), "{message}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(host.session().formula(), "=SUM(1,2)");
        host.files.open = Some(("seven.xlsx".into(), session_bytes("=3+4")));
        let Reply::Opened(opened) = host.handle(Request::OpenRecent {
            path: "C:\\work\\seven.xlsx".into(),
        }) else {
            panic!("not opened");
        };
        assert_eq!(
            (opened.name.as_str(), opened.shown.result.as_str()),
            ("seven.xlsx", "number 7")
        );
        assert_eq!(host.files.recent, ["C:\\work\\seven.xlsx"]);
    }

    /// Native-only: closing a clean session asks nothing; with unsaved changes the person's answer
    /// decides: discard closes, cancel keeps the window, save writes first and keeps the window if
    /// the save does not happen.
    #[test]
    fn closing_with_unsaved_changes_asks_first() {
        let mut host = host();
        assert!(host.close_requested());
        assert!(host.files.asked.is_empty());
        host.handle(Request::Evaluate(formula("=1+1")));
        host.handle(Request::SessionState {
            dirty: false,
            name: String::new(),
        });
        assert!(host.close_requested());
        host.handle(Request::SessionState {
            dirty: true,
            name: String::new(),
        });
        host.files.close = Some(CloseChoice::Cancel);
        assert!(!host.close_requested());
        assert_eq!(host.files.asked, ["Untitled"]);
        host.files.close = Some(CloseChoice::Discard);
        assert!(host.close_requested());
        // Save with no file: the dialog is asked; cancelling it keeps the window open.
        host.handle(Request::SessionState {
            dirty: true,
            name: "calc.xlsx".into(),
        });
        host.files.close = Some(CloseChoice::Save);
        assert!(!host.close_requested());
        assert_eq!(
            host.files.asked.last().map(String::as_str),
            Some("calc.xlsx")
        );
        host.files.save_to = Some("C:\\work\\calc.xlsx".into());
        assert!(host.close_requested());
        assert_eq!(host.files.written.len(), 1);
        // Saved: the next close asks nothing.
        let asked = host.files.asked.len();
        assert!(host.close_requested());
        assert_eq!(host.files.asked.len(), asked);
    }

    /// Native-only: a file OneCalc refuses can be opened in DNA Calc, by its path when the dialog
    /// gave one; a file the page read goes with its bytes; with no file refused, or no DNA Calc,
    /// the answer says so.
    #[test]
    fn a_refused_file_opens_in_dna_calc() {
        let mut host = host();
        match host.handle(Request::OpenInDnaCalc) {
            Reply::Refused { message } => assert!(message.contains("nothing to open"), "{message}"),
            other => panic!("{other:?}"),
        }
        host.files.open = Some(("big.xlsx".into(), b"not a workbook".to_vec()));
        host.files.path = Some("C:\\work\\big.xlsx".into());
        assert!(matches!(
            host.handle(Request::OpenNative),
            Reply::NotASession { .. }
        ));
        assert_eq!(host.handle(Request::OpenInDnaCalc), Reply::Accepted);
        assert_eq!(
            host.files.launched,
            [(
                Some(PathBuf::from("C:\\work\\big.xlsx")),
                "big.xlsx".to_owned(),
                14
            )]
        );
        // The page read this one (a drop): no path.
        host.handle(Request::Open {
            name: "drop.xlsx".into(),
            bytes: b"zip?".to_vec(),
            zone: None,
        });
        assert_eq!(host.handle(Request::OpenInDnaCalc), Reply::Accepted);
        assert_eq!(host.files.launched[1], (None, "drop.xlsx".to_owned(), 4));
        host.files.dna_calc = Some("DNA Calc is not in the folder".into());
        match host.handle(Request::OpenInDnaCalc) {
            Reply::Refused { message } => assert_eq!(message, "DNA Calc is not in the folder"),
            other => panic!("{other:?}"),
        }
        // A file that opens clears the refusal.
        host.files.open = Some(("ok.xlsx".into(), session_bytes("=5")));
        assert!(matches!(host.handle(Request::OpenNative), Reply::Opened(_)));
        match host.handle(Request::OpenInDnaCalc) {
            Reply::Refused { message } => assert!(message.contains("nothing to open"), "{message}"),
            other => panic!("{other:?}"),
        }
    }

    /// Native-only: cancelling a dialog changes nothing; a file outside the constraints is the
    /// typed refusal with the offer; a VBA module refuses the save naming its bead.
    #[test]
    fn cancel_refusal_and_vba_leave_the_session_as_it_was() {
        let mut host = host();
        host.handle(Request::Evaluate(Authored::example_sum()));
        assert_eq!(host.handle(Request::OpenNative), Reply::Cancelled);
        assert_eq!(host.handle(Request::SaveNative), Reply::Cancelled);
        assert_eq!(host.handle(Request::LoadAddinNative), Reply::Cancelled);
        host.files.open = Some(("bad.xlsx".into(), b"not a workbook".to_vec()));
        match host.handle(Request::OpenNative) {
            Reply::NotASession { offer, .. } => assert_eq!(offer, "Open in DNA Calc"),
            other => panic!("{other:?}"),
        }
        assert_eq!(host.session().formula(), "=SUM(A1:A3)");
        host.handle(Request::LoadVba {
            name: "Module1".into(),
            source: "Public Function Add2(a, b)\n Add2 = a + b\nEnd Function\n".to_owned(),
        });
        match host.handle(Request::SaveNative) {
            Reply::Refused { message } => assert!(message.contains("dc-0f1k.4"), "{message}"),
            other => panic!("{other:?}"),
        }
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

    /// Native-only: an add-in loads from the dialog or by path, its functions and what it
    /// documented reach the page's next answer, a session saved lists it, and unloading removes the
    /// functions and the listing.
    #[test]
    fn an_add_in_loads_calculates_is_listed_and_unloads() {
        let path = file_named("Fake.xll", b"MZ fake");
        let mut host = host();
        assert_eq!(
            shown(host.handle(Request::Evaluate(formula("=Double(21)")))).result,
            "error #NAME?"
        );
        host.files.addin = Some(path.clone());
        let loaded = addins(host.handle(Request::LoadAddinNative));
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].kind, "xll");
        assert_eq!(loaded[0].functions, ["Double"]);
        assert!(loaded[0].hash.starts_with("sha256:") && loaded[0].hash.len() == 71);
        let answer = shown(host.handle(Request::Evaluate(formula("=Double(21)"))));
        assert_eq!(answer.result, "number 42");
        let Reply::FunctionDocs(docs) = host.handle(Request::FunctionDocs) else {
            panic!("no function docs");
        };
        assert_eq!(docs.version, answer.functions_version);
        assert_eq!(docs.names, ["DOUBLE"]);
        assert_eq!(docs.docs.len(), 1);
        assert_eq!(docs.docs[0].signature(), "Double(number)");
        // The add-ins were told of the formula's cell and that a calculation finished, once per
        // evaluation, and of a grid cell only when its entry changed.
        let (events, calculations) = (host.libs.events.len(), host.libs.calculations);
        assert_eq!((events, calculations), (2, 2));
        let mut with_input = formula("=Double(A1)");
        with_input.grid[0] = "5".into();
        host.handle(Request::Evaluate(with_input.clone()));
        assert_eq!(&host.libs.events[events..], [(1, 1), (1, 8)]);
        host.handle(Request::Evaluate(with_input));
        assert_eq!(&host.libs.events[events + 2..], [(1, 8)]);
        // The loaded add-in is listed by a saved session, with the hash of the file.
        assert_eq!(addins(host.handle(Request::Addins)), loaded);
        host.handle(Request::Save);
        let listed = host.session().addins().to_vec();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].hash, loaded[0].hash);
        // Loading it again is refused; so is a file that is not an .xll or an .xlam.
        match host.handle(Request::LoadAddin {
            path: loaded[0].path.clone(),
            expected_hash: None,
        }) {
            Reply::Refused { message } => assert!(message.contains("already loaded"), "{message}"),
            other => panic!("{other:?}"),
        }
        match host.handle(Request::LoadAddin {
            path: "C:\\libs\\Add.dll".into(),
            expected_hash: None,
        }) {
            Reply::Refused { message } => {
                assert!(message.contains("not an .xll or .xlam add-in"), "{message}")
            }
            other => panic!("{other:?}"),
        }
        // Unloading removes the function and the listing.
        assert!(
            addins(host.handle(Request::UnloadAddin {
                path: loaded[0].path.clone()
            }))
            .is_empty()
        );
        assert_eq!(
            shown(host.handle(Request::Evaluate(formula("=Double(21)")))).result,
            "error #NAME?"
        );
        assert!(host.session().addins().is_empty());
        assert!(matches!(
            host.handle(Request::UnloadAddin {
                path: loaded[0].path.clone()
            }),
            Reply::Refused { .. }
        ));
    }

    /// Native-only: a load that fails leaves nothing loaded and says why; a listed add-in whose
    /// file changed is not loaded by the offer, only by choosing the file.
    #[test]
    fn a_failed_load_and_a_changed_file_load_nothing() {
        let broken = file_named("broken.xll", b"MZ broken");
        let mut host = host();
        match host.handle(Request::LoadAddin {
            path: broken.display().to_string(),
            expected_hash: None,
        }) {
            Reply::Refused { message } => assert!(message.contains("rollback"), "{message}"),
            other => panic!("{other:?}"),
        }
        assert!(addins(host.handle(Request::Addins)).is_empty());
        let path = file_named("Changed.xll", b"MZ original");
        let listed = format!("sha256:{}", calc_verdict::sha256_hex(b"MZ original"));
        std::fs::write(&path, b"MZ replaced").unwrap();
        match host.handle(Request::LoadAddin {
            path: path.display().to_string(),
            expected_hash: Some(listed.clone()),
        }) {
            Reply::Refused { message } => {
                assert!(
                    message.contains("not the file the session listed"),
                    "{message}"
                )
            }
            other => panic!("{other:?}"),
        }
        assert!(addins(host.handle(Request::Addins)).is_empty());
        std::fs::write(&path, b"MZ original").unwrap();
        assert_eq!(
            addins(host.handle(Request::LoadAddin {
                path: path.display().to_string(),
                expected_hash: Some(listed),
            }))
            .len(),
            1
        );
    }

    /// Native-only: a VBA module loads beside the add-ins instead of replacing their functions,
    /// opening a file drops the module and keeps the add-ins, and functions an add-in registers
    /// after its open routine returned appear at the next request.
    #[test]
    fn vba_modules_and_late_registrations_share_the_one_table() {
        let path = file_named("Late.xll", b"MZ late");
        let mut host = host();
        host.handle(Request::LoadAddin {
            path: path.display().to_string(),
            expected_hash: None,
        });
        match host.handle(Request::LoadVba {
            name: "Module1".into(),
            source: "Public Function Add2(a, b)\n Add2 = a + b\nEnd Function\n".into(),
        }) {
            Reply::Functions { names } => assert_eq!(names, ["MODULE1.ADD2"]),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            shown(host.handle(Request::Evaluate(formula("=Double(4)")))).result,
            "number 8"
        );
        assert!(host.session().has_vba_module());
        assert!(matches!(
            host.handle(Request::LoadVba {
                name: "Broken".into(),
                source: "Function (".into()
            }),
            Reply::Refused { .. }
        ));
        assert_eq!(
            shown(host.handle(Request::Evaluate(formula("=Double(4)")))).result,
            "number 8"
        );
        // Opening a new session ends the module, not the add-in.
        let Reply::Opened(opened) = host.handle(Request::New) else {
            panic!("not opened")
        };
        let Reply::FunctionDocs(docs) = host.handle(Request::FunctionDocs) else {
            panic!("no function docs");
        };
        assert_eq!(
            (docs.names, docs.version),
            (vec!["DOUBLE".to_owned()], opened.shown.functions_version)
        );
        assert!(!host.session().has_vba_module() && host.libs.vba.is_empty());
        // A registration after the open routine: the next request publishes it.
        host.libs.xlls.clear();
        host.libs.generation += 1;
        assert_eq!(
            shown(host.handle(Request::Evaluate(formula("=Double(4)")))).result,
            "error #NAME?"
        );
    }

    /// Native-only: a VBA add-in (.xlam) loads by path: CalcVBA reads the project calc-doc found
    /// in the file, the person's load installs it, a load from the session's list only opens it,
    /// and unloading removes it.
    #[test]
    fn a_vba_add_in_loads_installs_is_listed_and_unloads() {
        let path = file_named(
            "DnaExamples.xlam",
            include_bytes!("../../../crates/calc-xll-reference/fixture/vba/DnaExamples.xlam"),
        );
        let mut host = host();
        let loaded = addins(host.handle(Request::LoadAddin {
            path: path.display().to_string(),
            expected_hash: None,
        }));
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].kind, "xlam");
        assert_eq!(loaded[0].functions, ["DnaAdd"]);
        // ThisWorkbook, Sheet1, DnaExamples, DnaSimple and DnaCounter.
        assert_eq!(loaded[0].notes, ["5 module(s)"]);
        assert_eq!(host.libs.xlams, [(path.clone(), 5, true)]);
        assert_eq!(host.session().addins().len(), 1);
        assert_eq!(host.session().addins()[0].kind, "xlam");
        // Loading it twice is refused.
        assert!(matches!(
            host.handle(Request::LoadAddin {
                path: path.display().to_string(),
                expected_hash: None
            }),
            Reply::Refused { .. }
        ));
        // Unloaded, then loaded from the session's list (a reopen): opened, not installed.
        let hash = loaded[0].hash.clone();
        assert!(
            addins(host.handle(Request::UnloadAddin {
                path: path.display().to_string()
            }))
            .is_empty()
        );
        assert!(host.libs.xlams.is_empty());
        host.handle(Request::LoadAddin {
            path: path.display().to_string(),
            expected_hash: Some(hash),
        });
        assert_eq!(host.libs.xlams, [(path.clone(), 5, false)]);
        // A file without a VBA project is refused, naming it.
        let empty = file_named("Empty.xlam", b"not a workbook");
        assert!(
            matches!(host.handle(Request::LoadAddin { path: empty.display().to_string(), expected_hash: None }), Reply::Refused { message } if message.contains("Empty.xlam"))
        );
    }

    /// Native-only: an .xlsm session opened through the dialog compiles its project; a module the
    /// VBA panel loads goes into that project (not beside the add-ins), and Save writes the changed
    /// project as an .xlsm under the file's name.
    #[test]
    fn an_xlsm_session_keeps_panel_modules_in_its_project_and_saves_it() {
        let xlsm =
            include_bytes!("../../../crates/calc-xll-reference/fixture/vba/DnaExamples.xlsm");
        let mut host = host();
        host.files.open = Some(("DnaExamples.xlsm".into(), xlsm.to_vec()));
        let Reply::Opened(opened) = host.handle(Request::OpenNative) else {
            panic!("opened")
        };
        // Excel's default: the file opens with macros disabled; nothing is compiled.
        let view = opened.project.expect("the file's project");
        assert!(view.functions.is_empty(), "{view:?}");
        assert_eq!(view.macros, dna_onecalc::port::MacrosView::Disabled);
        let refused = host.handle(Request::LoadVba {
            name: "Module1".into(),
            source: "Public Function X()\r\nEnd Function\r\n".into(),
        });
        assert!(matches!(refused, Reply::Refused { .. }), "{refused:?}");
        let Reply::Opened(enabled) = host.handle(Request::EnableMacros) else {
            panic!("enabled")
        };
        assert!(enabled.project.is_some_and(|p| !p.functions.is_empty()
            && p.notes.is_empty()
            && p.macros == dna_onecalc::port::MacrosView::Enabled));
        let reply = host.handle(Request::LoadVba {
            name: "Module1".into(),
            source: "Public Function DnaTwice(ByVal x As Double) As Double\r\n    DnaTwice = 2 * x\r\nEnd Function\r\n".into(),
        });
        assert!(
            matches!(&reply, Reply::Functions { names } if names.iter().any(|n| n == "DNATWICE")),
            "{reply:?}"
        );
        assert!(
            host.libs.vba.is_empty(),
            "the module went into the project, not beside the add-ins"
        );
        assert_eq!(
            shown(host.handle(Request::Evaluate(formula("=DnaTwice(21)")))).result,
            "number 42"
        );
        let Reply::File { name, bytes } = host.handle(Request::Save) else {
            panic!("saved")
        };
        assert_eq!(name, "DnaExamples.xlsm");
        let file = calc_doc_read(&bytes);
        assert!(file.contains("DnaTwice = 2 * x"));
    }

    /// Native-only: a file with the Mark of the Web (zone 3 or 4) never runs its VBA project and
    /// the person cannot enable it; zone 2 or no mark opens disabled and can be enabled.
    #[test]
    fn a_file_with_the_mark_of_the_web_never_runs_its_project() {
        let xlsm =
            include_bytes!("../../../crates/calc-xll-reference/fixture/vba/DnaExamples.xlsm");
        for (zone, blocked) in [
            (Some(3), true),
            (Some(4), true),
            (Some(2), false),
            (None, false),
        ] {
            let mut host = host();
            host.files.open = Some(("DnaExamples.xlsm".into(), xlsm.to_vec()));
            host.files.zone = zone;
            let Reply::Opened(opened) = host.handle(Request::OpenNative) else {
                panic!("opened")
            };
            let view = opened.project.expect("the file's project");
            assert!(
                view.functions.is_empty() && !view.notes.is_empty(),
                "{view:?}"
            );
            let reply = host.handle(Request::EnableMacros);
            if blocked {
                assert_eq!(view.macros, dna_onecalc::port::MacrosView::Blocked);
                assert!(view.notes.join(" ").contains("Mark of the Web"), "{view:?}");
                assert!(
                    matches!(reply, Reply::Refused { .. }),
                    "{zone:?}: {reply:?}"
                );
                // The project is still the file's: a save writes it back.
                assert!(matches!(host.handle(Request::Save), Reply::File { .. }));
            } else {
                assert_eq!(view.macros, dna_onecalc::port::MacrosView::Disabled);
                assert!(matches!(reply, Reply::Opened(_)), "{zone:?}: {reply:?}");
            }
        }
    }

    /// The code of the saved file's Module1, read the way the session reads it.
    fn calc_doc_read(bytes: &[u8]) -> String {
        let (session, vba) = dna_onecalc::file::open(bytes).unwrap();
        drop(session);
        let project = calc_vba::read_office_project(&vba.unwrap().project).unwrap();
        project
            .modules()
            .iter()
            .find(|m| m.name == "Module1")
            .unwrap()
            .source
            .clone()
    }
}
