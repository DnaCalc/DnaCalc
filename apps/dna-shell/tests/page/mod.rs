//! The page core joined to the real workbook service by packets, as a carrier joins them.
//! Shared by the shell's native session tests.
#![allow(dead_code)]

use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, blank_package};
use calc_skin::grid::{Key, Sheet};
use calc_skin::{Interest, Origin, Packet, ViewId, WritePolicy, query};
use calc_skin_host::{Connection, SkinHost};
use dna_kit::{Focus, KeyInput, Live, Registry, Session};
use leptos::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

pub struct Page {
    pub host: SkinHost,
    pub port: Connection,
    pub sent: Rc<RefCell<VecDeque<Packet>>>,
    pub session: Session,
    pub sheet: Sheet,
    pub registry: Registry,
    _owner: Owner,
}

impl Page {
    pub fn open() -> Self {
        Self::from_workbook(
            HostSession::open(
                Arc::new(MemorySource::new(blank_package())),
                &OpenPolicy::default(),
                HostOptions::default(),
            )
            .unwrap(),
        )
    }

    pub fn from_workbook(workbook: HostSession) -> Self {
        let owner = Owner::new();
        owner.set();
        let sheet = Sheet::of(workbook.host().sheets()[0].id());
        let host = SkinHost::new(workbook);
        let port = host.connect(
            Origin::User {
                view: ViewId(dna_view_classic::ID.into()),
            },
            WritePolicy::Commit,
        );
        let sent = Rc::new(RefCell::new(VecDeque::new()));
        let session = Session::new(ViewId(dna_view_classic::ID.into()), {
            let sent = sent.clone();
            move |packet| sent.borrow_mut().push_back(packet)
        });
        let mut registry = Registry::default();
        dna_view_classic::install(&mut registry);
        dna_view_inspector::install(&mut registry);
        dna_view_notebook::install(&mut registry);
        dna_view_map::install(&mut registry);
        dna_view_app::install(&mut registry);
        dna_shell::workbook::install(&registry, session);
        let page = Self {
            host,
            port,
            sent,
            session,
            sheet,
            registry,
            _owner: owner,
        };
        page.settle();
        page
    }

    /// Carries packets both ways until neither side has anything to say.
    pub fn settle(&self) {
        loop {
            let mut moved = false;
            loop {
                let Some(packet) = self.sent.borrow_mut().pop_front() else {
                    break;
                };
                self.session.deliver(self.port.handle(packet));
                moved = true;
            }
            while self.host.advance() {}
            for packet in self.port.next_packets().unwrap() {
                self.session.deliver(packet);
                moved = true;
            }
            if !moved {
                return;
            }
        }
    }

    pub fn at(&self, row: u32, column: u32) -> Key {
        Key(self.sheet.0, self.sheet.1, row, column)
    }

    pub fn press(&self, key: &str, focus: Focus) -> bool {
        let mut parts = key.split('+');
        let key = parts.next_back().unwrap();
        let modifiers: Vec<_> = parts.collect();
        let taken = self.session.key(
            &KeyInput {
                key: key.to_owned(),
                ctrl: modifiers.contains(&"Ctrl"),
                alt: modifiers.contains(&"Alt"),
                shift: modifiers.contains(&"Shift"),
                meta: false,
                alt_graph: false,
                composing: false,
            },
            focus,
        );
        self.settle();
        taken
    }

    /// What a view that watches the page of `at` is told about the cell.
    pub fn shown(&self, at: Key) -> String {
        let cells = self.session.following::<query::Cells>();
        let query = query::Cells {
            interest: Interest::Area(dna_grid::page_of(at)),
        };
        cells.at(Some(query.clone()));
        self.settle();
        let Live::Ready(window) = cells.at(Some(query)) else {
            panic!("the cells of the page did not arrive");
        };
        cells.at(None);
        self.settle();
        window
            .body
            .cells
            .into_iter()
            .find(|cell| cell.at == at)
            .map(|cell| cell.text)
            .unwrap_or_default()
    }
}
