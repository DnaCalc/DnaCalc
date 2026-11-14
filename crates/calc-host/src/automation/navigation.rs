//! Workbook collections and Range geometry, resolved against the current session on every call.
use super::*;

impl WorkbookAutomation {
    pub(super) fn register_navigation(handlers: &mut Handlers<Self>) {
        handlers.on("Workbook", "Worksheets", InvokeKind::Get, Self::sheets);
        handlers.on("Workbook", "Sheets", InvokeKind::Get, Self::sheets);
        handlers.on("Workbook", "Names", InvokeKind::Get, Self::names);
        handlers.on("Worksheet", "Range", InvokeKind::Get, Self::worksheet_range);
        handlers.on("Worksheet", "Cells", InvokeKind::Get, Self::worksheet_cells);
        handlers.on("Worksheet", "Index", InvokeKind::Get, Self::worksheet_index);
        handlers.on("Range", "Cells", InvokeKind::Get, Self::range_cells);
        handlers.on("Range", "Item", InvokeKind::Get, Self::range_item);
        handlers.on("Range", "_Default", InvokeKind::Get, Self::range_default);
        handlers.on("Range", "Offset", InvokeKind::Get, Self::range_offset);
        handlers.on("Range", "Resize", InvokeKind::Get, Self::range_resize);
        handlers.on("Range", "Areas", InvokeKind::Get, Self::range_areas);
        handlers.on(
            "Range",
            "EntireRow",
            InvokeKind::Get,
            Self::range_entire_row,
        );
        handlers.on(
            "Range",
            "EntireColumn",
            InvokeKind::Get,
            Self::range_entire_column,
        );
        handlers.on("Sheets", "Count", InvokeKind::Get, Self::sheets_count);
        handlers.on(
            "Sheets",
            "Application",
            InvokeKind::Get,
            Self::sheets_application,
        );
        handlers.on("Sheets", "Parent", InvokeKind::Get, Self::sheets_parent);
        handlers.on("Sheets", "Item", InvokeKind::Get, Self::sheets_item);
        handlers.on("Sheets", "_Default", InvokeKind::Get, Self::sheets_item);
        handlers.on("Areas", "Count", InvokeKind::Get, Self::areas_count);
        handlers.on(
            "Areas",
            "Application",
            InvokeKind::Get,
            Self::areas_application,
        );
        handlers.on("Areas", "Parent", InvokeKind::Get, Self::areas_parent);
        handlers.on("Areas", "Item", InvokeKind::Get, Self::areas_item);
        handlers.on("Areas", "_Default", InvokeKind::Get, Self::areas_item);
        handlers.on("Names", "Count", InvokeKind::Get, Self::names_count);
        handlers.on(
            "Names",
            "Application",
            InvokeKind::Get,
            Self::names_application,
        );
        handlers.on("Names", "Parent", InvokeKind::Get, Self::names_parent);
        handlers.on("Names", "Item", InvokeKind::Method, Self::names_item);
        handlers.on("Names", "_Default", InvokeKind::Method, Self::names_item);
        handlers.on("Name", "Name", InvokeKind::Get, Self::name_text);
        handlers.on("Name", "RefersTo", InvokeKind::Get, Self::name_formula);
        handlers.on("Name", "RefersToRange", InvokeKind::Get, Self::name_range);
        handlers.on(
            "Name",
            "Application",
            InvokeKind::Get,
            Self::name_application,
        );
        handlers.on("Name", "Parent", InvokeKind::Get, Self::name_parent);
    }
    fn sheets(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            Ok(AutoValue::Object(object(
                "Sheets",
                ObjectKey::Sheets(meta.workbook),
            )))
        })
    }
    fn names(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            Ok(AutoValue::Object(object(
                "Names",
                ObjectKey::Names(meta.workbook),
            )))
        })
    }
    fn sheets_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            if call.args.len() != 1 {
                return Err(geometry::bad_count());
            }
            let sheets = meta.model.sheets();
            let sheet = match call.arg(0) {
                AutoValue::Text(text) => sheets.iter().find(|s| {
                    calc_value::workbook_names_equal(
                        &s.name().text().to_string_lossy(),
                        &text.to_string_lossy(),
                    )
                }),
                other => usize::try_from(geometry::integer(other)? - 1)
                    .ok()
                    .and_then(|n| sheets.get(n)),
            }
            .ok_or_else(|| AutoError::Status(0x8002_000Bu32 as i32))?;
            Ok(AutoValue::Object(object(
                "Worksheet",
                ObjectKey::Worksheet {
                    workbook: meta.workbook,
                    sheet: sheet.id(),
                },
            )))
        })
    }
    fn sheets_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            Ok(AutoValue::I4(meta.model.sheets().len() as i32))
        })
    }
    fn sheets_application(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, _| {
            self.fallback.application_member(target, call)
        })
    }
    fn sheets_parent(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.current_book(target, call)
    }
    fn worksheet_range(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |access, meta| {
            let ObjectKey::Worksheet { sheet, .. } = target.key else {
                return Err(geometry::type_mismatch());
            };
            self.get_range(access, meta, Some(sheet), call)
        })
    }
    fn worksheet_cells(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |access, _| {
            no_args(call)?;
            let ObjectKey::Worksheet { sheet, .. } = target.key else {
                return Err(geometry::type_mismatch());
            };
            self.acquire(
                access,
                &ReferenceLike::area(GridArea::full_sheet(sheet)),
                RangeAxis::Cells,
            )
        })
    }
    fn worksheet_index(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_target(target, |_, meta| {
            no_args(call)?;
            let ObjectKey::Worksheet { sheet, .. } = target.key else {
                return Err(geometry::type_mismatch());
            };
            Ok(AutoValue::I4(
                meta.model
                    .sheets()
                    .iter()
                    .position(|s| s.id() == sheet)
                    .expect("validated sheet") as i32
                    + 1,
            ))
        })
    }
    fn range_cells(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, _| {
            no_args(call)?;
            self.acquire(access, range, RangeAxis::Cells)
        })
    }
    fn range_areas(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, _| {
            no_args(call)?;
            let AutoValue::Object(acquired) = self.acquire(access, range, RangeAxis::Cells)? else {
                unreachable!()
            };
            let ObjectKey::Range(reference) = acquired.key else {
                unreachable!()
            };
            Ok(AutoValue::Object(object(
                "Areas",
                ObjectKey::Areas(reference),
            )))
        })
    }
    fn range_default(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, meta, range, axis| {
            if call.args.len() > 2 {
                return Err(geometry::bad_count());
            }
            if call.arg(0).is_missing() && call.arg(1).is_missing() {
                return self.read(access, meta, range, false);
            }
            self.index_range(access, range, axis, call)
        })
    }
    fn range_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, axis| {
            self.index_range(access, range, axis, call)
        })
    }
    fn index_range(
        &self,
        access: &mut Access<'_>,
        range: &ReferenceLike,
        axis: RangeAxis,
        call: &Call<'_>,
    ) -> Reply {
        if call.args.len() > 2 || call.arg(0).is_missing() {
            return Err(geometry::bad_count());
        }
        let range = geometry::item(range, axis, call.arg(0), call.arg(1))?;
        self.acquire(access, &range, RangeAxis::Cells)
    }
    fn range_offset(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, _| {
            if call.args.len() > 2 {
                return Err(geometry::bad_count());
            }
            self.acquire(
                access,
                &geometry::offset(range, call.arg(0), call.arg(1))?,
                RangeAxis::Cells,
            )
        })
    }
    fn range_resize(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, _| {
            if call.args.len() > 2 {
                return Err(geometry::bad_count());
            }
            self.acquire(
                access,
                &geometry::resize(range, call.arg(0), call.arg(1))?,
                RangeAxis::Cells,
            )
        })
    }
    fn range_entire_row(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, _| {
            no_args(call)?;
            self.acquire(
                access,
                &geometry::entire_row(range, call.arg(0), call.arg(1))?,
                RangeAxis::Cells,
            )
        })
    }
    fn range_entire_column(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, _| {
            no_args(call)?;
            self.acquire(
                access,
                &geometry::entire_column(range, call.arg(0), call.arg(1))?,
                RangeAxis::Cells,
            )
        })
    }
    fn areas_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, _| {
            if call.args.len() != 1 {
                return Err(geometry::bad_count());
            }
            let index = usize::try_from(geometry::integer(call.arg(0))? - 1).ok();
            let area = index
                .and_then(|i| range.areas().get(i))
                .ok_or_else(|| AutoError::excel(1004, ""))?;
            self.acquire(access, &ReferenceLike::area(*area), RangeAxis::Cells)
        })
    }
    fn areas_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |_, _, range, _| {
            no_args(call)?;
            Ok(AutoValue::I4(range.area_count() as i32))
        })
    }
    fn areas_parent(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.range_cells(target, call)
    }
    fn areas_application(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.range_application(target, call)
    }
    fn with_names(
        &self,
        target: &ObjectRef,
        mut f: impl FnMut(&Snapshot, &[&calc_doc_model::NameNode]) -> Reply,
    ) -> Reply {
        self.with_target(target, |_, meta| {
            // Workbook.Names includes sheet-local names in Excel. Never report a
            // truncated Count or shifted indexes while only global names are modelled.
            if meta
                .model
                .definitions()
                .name_table()
                .nodes()
                .any(|n| n.definition().scope != calc_doc_model::NameScope::Workbook)
            {
                return Err(AutoError::not_modelled(
                    "Workbook.Names with worksheet-scoped names",
                    "dc-hsa5.6",
                ));
            }
            let mut names = meta
                .model
                .definitions()
                .name_table()
                .nodes()
                .filter(|n| n.definition().scope == calc_doc_model::NameScope::Workbook)
                .collect::<Vec<_>>();
            let unknown_order = Cell::new(false);
            names.sort_by(|a, b| {
                match calc_func::text_order(
                    &a.definition().name.encode_utf16().collect::<Vec<_>>(),
                    &b.definition().name.encode_utf16().collect::<Vec<_>>(),
                ) {
                    Some(order) => order,
                    None => {
                        unknown_order.set(true);
                        std::cmp::Ordering::Equal
                    }
                }
            });
            if unknown_order.get() {
                return Err(AutoError::not_modelled(
                    "Names.Item name ordering",
                    "dc-zsn.2",
                ));
            }
            f(meta, &names)
        })
    }
    fn names_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_names(target, |meta, names| {
            if call.args.len() > 3 {
                return Err(geometry::bad_count());
            }
            if !call.arg(1).is_missing() || !call.arg(2).is_missing() {
                return Err(AutoError::not_modelled(
                    "Names.Item IndexLocal/RefersTo",
                    "dc-1nbp.6",
                ));
            }
            let node = match call.arg(0) {
                AutoValue::Text(text) => names.iter().copied().find(|n| {
                    calc_value::workbook_names_equal(&n.definition().name, &text.to_string_lossy())
                }),
                other => usize::try_from(geometry::integer(other)? - 1)
                    .ok()
                    .and_then(|i| names.get(i).copied()),
            }
            .ok_or_else(|| AutoError::excel(1004, ""))?;
            Ok(AutoValue::Object(object(
                "Name",
                ObjectKey::Name {
                    workbook: meta.workbook,
                    id: node.id(),
                },
            )))
        })
    }
    fn names_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_names(target, |_, names| {
            no_args(call)?;
            Ok(AutoValue::I4(names.len() as i32))
        })
    }
    fn names_parent(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_names(target, |meta, _| {
            no_args(call)?;
            Ok(AutoValue::Object(object(
                "Workbook",
                ObjectKey::Workbook(meta.workbook),
            )))
        })
    }
    fn names_application(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_names(target, |_, _| {
            self.fallback.application_member(target, call)
        })
    }
    fn with_name(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        mut f: impl FnMut(&mut Access<'_>, &Snapshot, &calc_doc_model::DefinedName) -> Reply,
    ) -> Reply {
        self.with_target(target, |access, meta| {
            no_args(call)?;
            let ObjectKey::Name { id, .. } = target.key else {
                return Err(geometry::type_mismatch());
            };
            let node = meta
                .model
                .definitions()
                .name_table()
                .get(id)
                .ok_or_else(|| AutoError::excel(1004, ""))?;
            f(access, meta, node.definition())
        })
    }
    fn name_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_name(target, call, |_, _, definition| {
            Ok(AutoValue::text(&definition.name))
        })
    }
    fn name_formula(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_name(target, call, |_, _, definition| {
            Ok(AutoValue::text(&format!("={}", definition.formula)))
        })
    }
    fn name_application(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_name(target, call, |_, _, _| {
            self.fallback.application_member(target, call)
        })
    }
    fn name_parent(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_name(target, call, |_, meta, _| {
            Ok(AutoValue::Object(object(
                "Workbook",
                ObjectKey::Workbook(meta.workbook),
            )))
        })
    }
    fn name_range(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_name(target, call, |access, meta, definition| {
            let at = meta.active.ok_or_else(|| {
                AutoError::not_modelled("Name.RefersToRange without active worksheet", OWNER)
            })?;
            if meta
                .model
                .definitions()
                .name_table()
                .lookup(
                    calc_doc_model::NameScope::Sheet(at.sheet()),
                    &definition.name,
                )
                .is_some()
            {
                return Err(AutoError::not_modelled(
                    "Name.RefersToRange with a worksheet-local shadow",
                    "dc-hsa5.6",
                ));
            }
            let result = match &access {
                Access::Session(session) => {
                    session.host().resolve_reference_text(at, &definition.name)
                }
                _ => crate::resolve_reference_text(&meta.model, at, &definition.name),
            };
            match result {
                ReferenceTarget::Area(area) => {
                    self.acquire(access, &ReferenceLike::area(area), RangeAxis::Cells)
                }
                ReferenceTarget::Invalid => Err(AutoError::excel(1004, "")),
                _ => Err(AutoError::not_modelled(
                    "Name.RefersToRange definition form",
                    "dc-hsa5.6",
                )),
            }
        })
    }
}
