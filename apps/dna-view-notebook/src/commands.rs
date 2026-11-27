use super::*;
use calc_skin::{Batch, Change, PresentationChange, RowSort};
use dna_kit::commands::CommandId;
use dna_kit::{Invocation, Seam};
use serde::de::DeserializeOwned;

pub(super) fn install(state: State) -> Vec<ViewCommand> {
    let id = ViewId(ID.into());
    let command = |id_str, label, chords, run: fn(State, &Invocation) -> Result<(), String>| {
        ViewCommand::new(&id, id_str, label, chords, move |action| {
            if id_str != "notebook.delete-key" {
                state
                    .workspace
                    .update_value(|workspace| workspace.delete_armed = None);
            }
            run(state, action)
        })
    };
    vec![
        command(
            "notebook.link",
            "Start with selected cells",
            &[],
            |state, _| state.link(),
        )
        .when("writable"),
        command("notebook.new-formula", "New formula", &[], |state, _| {
            state.author.open(state)
        })
        .when("mode:notebook-command && writable"),
        command(
            "notebook.author-columns",
            "Set array lane width",
            &[],
            |state, action| state.author.columns(state, argument(action)?),
        )
        .when("mode:notebook-command && writable"),
        command("notebook.author", "Add formula", &[], |state, action| {
            state.author.submit(state, argument(action)?)
        })
        .when("mode:notebook-command && writable"),
        command("notebook.cancel-formula", "Cancel new formula", &[], |state, _| {
            state.author.close();
            Ok(())
        }),
        command("notebook.blank", "Blank notebook", &[], |state, _| {
            let snapshot = state.snapshot()?;
            state.submit(&snapshot, &snapshot.chain, vec![], "Start blank notebook")
        })
        .when("writable"),
        command(
            "notebook.draft",
            "Draft from workbook structure",
            &[],
            |state, _| {
                state
                    .draft
                    .set(state.session.stamp().map(|stamp| stamp.workbook));
                Ok(())
            },
        ),
        command("notebook.keep", "Keep notebook draft", &[], |state, _| {
            state.keep()
        })
        .when("writable"),
        command(
            "notebook.focus",
            "Focus notebook cell",
            &[],
            |state, action| {
                let id = argument(action)?;
                let snapshot = state.snapshot()?;
                if !snapshot.chain.cells.iter().any(|item| item.id == id) {
                    return Err("This cell has been removed from the Notebook.".into());
                }
                state.select(snapshot.part.stamp, id, snapshot.source(id));
                Ok(())
            },
        ),
        command(
            "notebook.next",
            "Next cell",
            &["ArrowDown", "J"],
            |state, _| state.step(1),
        )
        .when("mode:notebook-command"),
        command(
            "notebook.previous",
            "Previous cell",
            &["ArrowUp", "K"],
            |state, _| state.step(-1),
        )
        .when("mode:notebook-command"),
        command(
            "notebook.edit",
            "Edit cell",
            &["Enter", "F2"],
            |state, _| state.edit(),
        )
        .when("mode:notebook-command && writable"),
        command("notebook.run", "Run cell", &["Ctrl+Enter"], |state, _| {
            state.session.commit_edit();
            Ok(())
        })
        .when("editing && writable"),
        command(
            "notebook.run-next",
            "Run and advance",
            &["Shift+Enter"],
            |state, _| state.run_and_advance(),
        )
        .when("editing && writable"),
        command(
            "notebook.run-all",
            "Run all held drafts",
            &[],
            |state, _| state.run_all(),
        )
        .when("mode:notebook-command && writable"),
        command(
            "notebook.tree",
            "Toggle expression structure",
            &[],
            |state, action| {
                let snapshot = state.snapshot()?;
                let id = if action.args.is_null() {
                    state.current(&snapshot)?.id
                } else {
                    argument(action)?
                };
                let open = (snapshot.part.stamp.workbook, id);
                state
                    .tree
                    .update(|selection| *selection = (*selection != Some(open)).then_some(open));
                Ok(())
            },
        ),
        command("notebook.revert", "Revert cell", &[], |state, action| {
            let snapshot = state.snapshot()?;
            let id = if action.args.is_null() {
                state.current(&snapshot)?.id
            } else {
                argument(action)?
            };
            if let Some(at) = snapshot.source(id).and_then(first_cell) {
                state.session.revert_held(at);
            }
            Ok(())
        }),
        command(
            "notebook.fold",
            "Fold or unfold output",
            &["O"],
            |state, _| state.change_item("Fold notebook output", |item| item.folded = !item.folded),
        )
        .when("mode:notebook-command && writable"),
        command(
            "notebook.lines",
            "Toggle line numbers",
            &["L"],
            |state, _| {
                state.change_item("Toggle notebook line numbers", |item| {
                    item.line_numbers = !item.line_numbers
                })
            },
        )
        .when("mode:notebook-command && writable"),
        command(
            "notebook.layout",
            "Switch Vertical or Pipeline layout",
            &["Alt+P"],
            |state, _| {
                let snapshot = state.snapshot()?;
                let mut chain = snapshot.chain.clone();
                chain.layout = if chain.layout == Layout::Vertical {
                    Layout::Pipeline
                } else {
                    Layout::Vertical
                };
                state.submit(&snapshot, &chain, vec![], "Switch notebook layout")
            },
        )
        .when("writable"),
        command(
            "notebook.move-up",
            "Move cell up",
            &["Alt+ArrowUp"],
            |state, _| state.move_item(-1),
        )
        .when("mode:notebook-command && writable"),
        command(
            "notebook.move-down",
            "Move cell down",
            &["Alt+ArrowDown"],
            |state, _| state.move_item(1),
        )
        .when("mode:notebook-command && writable"),
        command(
            "notebook.prose-above",
            "Insert Markdown above",
            &["A"],
            |state, _| state.insert_prose(false),
        )
        .when("mode:notebook-command && writable"),
        command(
            "notebook.prose-below",
            "Insert Markdown below",
            &["B"],
            |state, _| state.insert_prose(true),
        )
        .when("mode:notebook-command && writable"),
        command(
            "notebook.delete-key",
            "Delete cell (press D twice)",
            &["D"],
            |state, _| {
                let snapshot = state.snapshot()?;
                let item = (snapshot.part.stamp.workbook, state.current(&snapshot)?.id);
                let armed = state
                    .workspace
                    .try_update_value(|workspace| workspace.delete_armed.replace(item))
                    .flatten();
                if armed == Some(item) {
                    state.delete()
                } else {
                    Ok(())
                }
            },
        )
        .when("mode:notebook-command && writable"),
        command("notebook.delete", "Delete cell", &[], |state, _| {
            state.delete()
        })
        .when("mode:notebook-command && writable"),
        command("notebook.copy", "Copy cell", &["C"], |state, _| {
            state.copy()
        })
        .when("mode:notebook-command"),
        command("notebook.cut", "Cut cell", &["X"], |state, _| {
            state.copy()?;
            state.delete()
        })
        .when("mode:notebook-command && writable"),
        command("notebook.paste", "Paste cell below", &["V"], |state, _| {
            state.paste()
        })
        .when("mode:notebook-command && writable"),
        command(
            "notebook.markdown",
            "Convert cell to Markdown",
            &["M"],
            |state, _| state.to_prose(),
        )
        .when("mode:notebook-command && writable"),
        command(
            "notebook.code",
            "Convert cell to formula",
            &["Y"],
            |state, _| state.to_formula(),
        )
        .when("mode:notebook-command && writable"),
        command(
            "notebook.save-prose",
            "Save Markdown",
            &[],
            |state, action| {
                let (id, markdown): (AnchorId, String) = argument(action)?;
                let snapshot = state.snapshot()?;
                let mut chain = snapshot.chain.clone();
                let item = chain
                    .cells
                    .iter_mut()
                    .find(|item| item.id == id)
                    .ok_or("This cell has been removed from the Notebook.")?;
                let Kind::Prose { markdown: text } = &mut item.kind else {
                    return Err("This cell is no longer Markdown.".into());
                };
                *text = markdown;
                state.submit(&snapshot, &chain, vec![], "Edit Notebook Markdown")?;
                state.prose_editor.set(None);
                Ok(())
            },
        )
        .when("writable"),
        command(
            "notebook.cancel-prose",
            "Cancel Markdown edit",
            &[],
            |state, _| {
                state.prose_editor.set(None);
                Ok(())
            },
        ),
        command(
            "notebook.jump",
            "Go to an inline value source",
            &[],
            |state, action| {
                let address = argument(action)?;
                state.session.go_to(address);
                Ok(())
            },
        ),
        command(
            "notebook.rows-page",
            "Show another page of rows",
            &[],
            |state, action| {
                let (id, offset) = argument(action)?;
                state.change_rows(id, |rows| rows.offset = offset)
            },
        ),
        command(
            "notebook.rows-limit",
            "Change rows per page",
            &[],
            |state, action| {
                let id = argument(action)?;
                state.change_rows(id, |rows| {
                    rows.offset = 0;
                    rows.limit = if rows.limit == 20 { 200 } else { 20 };
                })
            },
        ),
        command(
            "notebook.rows-sort",
            "Sort the displayed rows",
            &[],
            |state, action| {
                let (id, column) = argument(action)?;
                state.change_rows(id, |rows| {
                    rows.offset = 0;
                    let descending = rows
                        .sort
                        .is_some_and(|sort| sort.column == column && !sort.descending);
                    rows.sort = Some(RowSort { column, descending });
                })
            },
        ),
        command(
            "notebook.reveal",
            "Reveal cell in Classic",
            &[],
            |state, action| {
                let id = argument(action)?;
                let snapshot = state.snapshot()?;
                let source = snapshot
                    .source(id)
                    .ok_or("This item has no worksheet source.")?;
                state.session.show(ViewId("classic".into()));
                state.session.navigate(Target::Source(source.clone()));
                Ok(())
            },
        ),
    ]
}

pub(super) fn invoke<T: Serialize>(session: Session, command: &'static str, args: T) {
    if let Ok(args) = serde_json::to_value(args) {
        let _ = session.run(&Invocation {
            command: CommandId::new(command),
            args,
        });
    }
}

fn argument<T: DeserializeOwned>(action: &Invocation) -> Result<T, String> {
    serde_json::from_value(action.args.clone()).map_err(|error| error.to_string())
}

impl State {
    fn current(self, snapshot: &Snapshot) -> Result<Item, String> {
        self.selected(snapshot)
            .and_then(|position| snapshot.chain.cells.get(position))
            .cloned()
            .ok_or_else(|| "Choose a Notebook cell first.".into())
    }

    fn submit(
        self,
        snapshot: &Snapshot,
        chain: &Chain,
        changes: Vec<Change>,
        label: &str,
    ) -> Result<(), String> {
        let batch = self.prepare_batch(snapshot, chain, changes, label)?;
        self.session.submit(batch);
        Ok(())
    }

    pub(super) fn prepare_batch(
        self,
        snapshot: &Snapshot,
        chain: &Chain,
        mut changes: Vec<Change>,
        label: &str,
    ) -> Result<Batch, String> {
        if !matches!(self.session.seam(), Seam::Idle) {
            return Err("Finish the change in progress first.".into());
        }
        let body = serde_json::to_value(chain).map_err(|error| error.to_string())?;
        let doc = if let Some(doc) = snapshot.doc {
            PresentationChange::PatchDoc {
                doc,
                expect_view_rev: snapshot.part.stamp.view,
                body,
            }
        } else {
            let id = snapshot.next_id()?;
            PresentationChange::CreateDoc {
                doc: DocId(id),
                expect_view_rev: Some(snapshot.part.stamp.view),
                view: ViewId(ID.into()),
                sheet: None,
                schema: SCHEMA,
                body,
            }
        };
        changes.push(Change::Presentation(doc));
        let stamp = snapshot.part.stamp;
        Ok(Batch {
            id: String::new(),
            workbook: Some(stamp.workbook),
            base: Some(stamp.model),
            expects: vec![],
            selection_seq: None,
            label: Some(label.into()),
            changes,
        })
    }

    fn link(self) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let selection = self
            .session
            .selection()
            .ok_or("Select a worksheet cell first.")?;
        let (kind, source) = match selection.target {
            SourceRef::Cells { ref areas, .. } if areas.len() == 1 => {
                let kind = if areas[0].0 == areas[0].1 {
                    Kind::Formula
                } else {
                    Kind::Table { name: None }
                };
                (kind, selection.target)
            }
            SourceRef::Spill { .. } => (Kind::Formula, selection.target),
            _ => return Err("Select a cell, a spill, or a single range.".into()),
        };
        self.add(&snapshot, kind, Some(source), true)
    }

    fn keep(self) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        if snapshot.doc.is_some() {
            return Err("This workbook already has a Notebook.".into());
        }
        let rows = self
            .structure()
            .ok_or("The workbook structure has not arrived yet.")?;
        let mut next = snapshot.next_id()?;
        let mut chain = Chain::default();
        let mut changes = Vec::new();
        for (kind, source) in rows {
            let id = AnchorId(next);
            chain.cells.push(Item::new(id, kind));
            changes.push(bind(id, source));
            next = next
                .get()
                .checked_add(1)
                .and_then(NonZeroU128::new)
                .ok_or("The workbook has no available Notebook item ID.")?;
        }
        self.submit(&snapshot, &chain, changes, "Keep workbook structure draft")
    }

    fn add(
        self,
        snapshot: &Snapshot,
        kind: Kind,
        source: Option<SourceRef>,
        after: bool,
    ) -> Result<(), String> {
        let mut chain = snapshot.chain.clone();
        let at = self
            .selected(snapshot)
            .map_or(0, |index| index + usize::from(after));
        let id = AnchorId(snapshot.next_id()?);
        chain.cells.insert(at, Item::new(id, kind));
        let changes = source.into_iter().map(|source| bind(id, source)).collect();
        self.submit(snapshot, &chain, changes, "Add Notebook cell")?;
        self.item.set(Some((snapshot.part.stamp.workbook, id)));
        Ok(())
    }

    fn insert_prose(self, after: bool) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        self.add(
            &snapshot,
            Kind::Prose {
                markdown: String::new(),
            },
            None,
            after,
        )
    }

    fn change_item(self, label: &str, change: impl FnOnce(&mut Item)) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let mut chain = snapshot.chain.clone();
        let at = self
            .selected(&snapshot)
            .ok_or("Choose a Notebook cell first.")?;
        change(&mut chain.cells[at]);
        self.submit(&snapshot, &chain, vec![], label)
    }

    fn step(self, by: isize) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let index = self
            .selected(&snapshot)
            .ok_or("Choose a Notebook cell first.")?;
        let next = index
            .saturating_add_signed(by)
            .min(snapshot.chain.cells.len() - 1);
        let item = &snapshot.chain.cells[next];
        self.select(snapshot.part.stamp, item.id, snapshot.source(item.id));
        stage::reveal(item.id);
        Ok(())
    }

    fn edit(self) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let item = self.current(&snapshot)?;
        if matches!(item.kind, Kind::Table { .. }) {
            return Err("Open the linked worksheet to edit this range.".into());
        }
        if matches!(item.kind, Kind::Prose { .. }) {
            self.prose_editor
                .set(Some((snapshot.part.stamp.workbook, item.id)));
            return Ok(());
        }
        let at = snapshot
            .source(item.id)
            .and_then(first_cell)
            .ok_or("The worksheet cell is unavailable.")?;
        self.session.begin_edit(at, None);
        Ok(())
    }

    fn run_all(self) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let mut seen = std::collections::BTreeSet::new();
        let drafts = snapshot
            .chain
            .cells
            .iter()
            .filter_map(|item| snapshot.source(item.id).and_then(first_cell))
            .filter(|at| self.session.held_edit(*at).is_some() && seen.insert(*at))
            .collect::<Vec<_>>();
        if drafts.is_empty() {
            return Err("There are no unrun formula drafts in this Notebook.".into());
        }
        self.run_drafts(drafts.into_iter());
        Ok(())
    }

    fn run_drafts(self, mut drafts: std::vec::IntoIter<Key>) {
        if let Some(at) = drafts.next()
            && self.session.resume_held(at)
        {
            self.session
                .commit_edit_then(move || self.run_drafts(drafts));
        }
    }

    fn run_and_advance(self) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let current = self
            .selected(&snapshot)
            .ok_or("Choose a Notebook cell first.")?;
        let next = snapshot.chain.cells.get(current + 1).map(|item| item.id);
        self.session.commit_edit_then(move || {
            if let Some(next) = next {
                self.select(snapshot.part.stamp, next, snapshot.source(next));
                stage::reveal(next);
            }
        });
        Ok(())
    }

    fn move_item(self, by: isize) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let mut chain = snapshot.chain.clone();
        let index = self
            .selected(&snapshot)
            .ok_or("Choose a Notebook cell first.")?;
        let next = index.saturating_add_signed(by).min(chain.cells.len() - 1);
        chain.cells.swap(index, next);
        self.submit(&snapshot, &chain, vec![], "Move Notebook cell")
    }

    fn delete(self) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let mut chain = snapshot.chain.clone();
        let index = self
            .selected(&snapshot)
            .ok_or("Choose a Notebook cell first.")?;
        let item = chain.cells.remove(index);
        self.workspace
            .update_value(|state| state.delete_armed = None);
        let changes = snapshot
            .anchor(item.id)
            .map(|_| Change::Presentation(PresentationChange::RemoveAnchor { anchor: item.id }))
            .into_iter()
            .collect();
        self.submit(&snapshot, &chain, changes, "Delete Notebook cell")
    }

    fn copy(self) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let item = self.current(&snapshot)?;
        self.workspace.update_value(|workspace| {
            workspace.owner = Some(snapshot.part.stamp.workbook);
            workspace.copied = Some(CopiedCell {
                kind: item.kind,
                binding: snapshot.source(item.id).cloned(),
            });
        });
        Ok(())
    }

    fn paste(self) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let copy = self
            .workspace
            .with_value(|workspace| {
                (workspace.owner == Some(snapshot.part.stamp.workbook))
                    .then(|| workspace.copied.clone())
                    .flatten()
            })
            .ok_or("Copy or cut a Notebook cell first.")?;
        self.add(&snapshot, copy.kind, copy.binding, true)
    }

    fn to_prose(self) -> Result<(), String> {
        self.change_item("Convert Notebook cell to Markdown", |item| {
            if !matches!(item.kind, Kind::Prose { .. }) {
                item.kind = Kind::Prose {
                    markdown: String::new(),
                };
            }
        })
    }

    fn to_formula(self) -> Result<(), String> {
        let snapshot = self.snapshot()?;
        let item = self.current(&snapshot)?;
        if snapshot.source(item.id).is_none() {
            return Err("Link a worksheet cell for this formula first.".into());
        }
        self.change_item("Convert Notebook cell to formula", |item| {
            item.kind = Kind::Formula
        })
    }
}

fn bind(id: AnchorId, source: SourceRef) -> Change {
    Change::Presentation(PresentationChange::Bind { anchor: id, source })
}
