//! Workbook collections and Range geometry, resolved against the current session on every call.
use super::*;

impl WorkbookAutomation {
    pub(super) fn register_navigation(handlers: &mut Handlers<Self>) {
        for (interface, names) in [
            ("Workbook", &["Worksheets", "Sheets", "Names"][..]),
            ("Worksheet", &["Range", "Cells", "Index"][..]),
            (
                "Range",
                &[
                    "Cells",
                    "Item",
                    "_Default",
                    "Offset",
                    "Resize",
                    "Areas",
                    "EntireRow",
                    "EntireColumn",
                ][..],
            ),
            (
                "Sheets",
                &["Count", "Item", "_Default", "Application", "Parent"][..],
            ),
            (
                "Areas",
                &["Count", "Item", "_Default", "Application", "Parent"][..],
            ),
            ("Names", &["Count", "Application", "Parent"][..]),
            (
                "Name",
                &["Name", "RefersTo", "RefersToRange", "Application", "Parent"][..],
            ),
        ] {
            for name in names {
                handlers.on(interface, name, InvokeKind::Get, Self::navigate);
            }
        }
        for name in ["Item", "_Default"] {
            handlers.on("Names", name, InvokeKind::Method, Self::navigate);
        }
    }

    pub(super) fn navigate(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_access(|mut access| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            let name = call.member.name.as_str();
            let book = || AutoValue::Object(object("Workbook", ObjectKey::Workbook(meta.workbook)));
            match &target.key {
                ObjectKey::Workbook(id) => {
                    no_args(call)?;
                    Ok(AutoValue::Object(match name {
                        "Sheets" | "Worksheets" => object("Sheets", ObjectKey::Sheets(*id)),
                        "Names" => object("Names", ObjectKey::Names(*id)),
                        _ => return Err(AutoError::not_modelled(call.label(), OWNER)),
                    }))
                }
                ObjectKey::Sheets(workbook) => match name {
                    "Item" | "_Default" => {
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
                                workbook: *workbook,
                                sheet: sheet.id(),
                            },
                        )))
                    }
                    _ => {
                        no_args(call)?;
                        Ok(match name {
                            "Count" => AutoValue::I4(meta.model.sheets().len() as i32),
                            "Parent" => book(),
                            "Application" => AutoValue::Object(self.application()),
                            _ => return Err(AutoError::not_modelled(call.label(), OWNER)),
                        })
                    }
                },
                ObjectKey::Worksheet { sheet, .. } => match name {
                    "Index" => {
                        no_args(call)?;
                        Ok(AutoValue::I4(
                            meta.model
                                .sheets()
                                .iter()
                                .position(|s| s.id() == *sheet)
                                .expect("validated sheet") as i32
                                + 1,
                        ))
                    }
                    "Cells" => {
                        no_args(call)?;
                        self.acquire(
                            &mut access,
                            &ReferenceLike::area(GridArea::full_sheet(*sheet)),
                            RangeAxis::Cells,
                        )
                    }
                    "Range" => self.get_range(&mut access, &meta, Some(*sheet), call),
                    _ => Err(AutoError::not_modelled(call.label(), OWNER)),
                },
                ObjectKey::Range(held)
                | ObjectKey::RangeView {
                    reference: held, ..
                } => {
                    let range = self.resolve(&mut access, held)?;
                    let axis = match target.key {
                        ObjectKey::RangeView { axis, .. } => axis,
                        _ => RangeAxis::Cells,
                    };
                    match name {
                        "Cells" => {
                            no_args(call)?;
                            self.acquire(&mut access, &range, RangeAxis::Cells)
                        }
                        "Areas" => {
                            no_args(call)?;
                            let AutoValue::Object(acquired) =
                                self.acquire(&mut access, &range, RangeAxis::Cells)?
                            else {
                                unreachable!()
                            };
                            let ObjectKey::Range(reference) = acquired.key else {
                                unreachable!()
                            };
                            Ok(AutoValue::Object(object(
                                "Areas",
                                ObjectKey::Areas(reference),
                            )))
                        }
                        "Item" | "_Default" => {
                            if call.args.len() > 2 {
                                return Err(geometry::bad_count());
                            }
                            if name == "_Default"
                                && call.arg(0).is_missing()
                                && call.arg(1).is_missing()
                            {
                                return self.read(&mut access, &meta, &range, false);
                            }
                            if call.arg(0).is_missing() {
                                return Err(geometry::bad_count());
                            }
                            let range = geometry::item(&range, axis, call.arg(0), call.arg(1))?;
                            self.acquire(&mut access, &range, RangeAxis::Cells)
                        }
                        _ => {
                            if matches!(name, "Offset" | "Resize") {
                                if call.args.len() > 2 {
                                    return Err(geometry::bad_count());
                                }
                            } else {
                                no_args(call)?;
                            }
                            self.acquire(
                                &mut access,
                                &geometry::transform(&range, name, call.arg(0), call.arg(1))?,
                                RangeAxis::Cells,
                            )
                        }
                    }
                }
                ObjectKey::Areas(held) => {
                    let range = self.resolve(&mut access, held)?;
                    match name {
                        "Item" | "_Default" => {
                            if call.args.len() != 1 {
                                return Err(geometry::bad_count());
                            }
                            let index = usize::try_from(geometry::integer(call.arg(0))? - 1).ok();
                            let area = index
                                .and_then(|i| range.areas().get(i))
                                .ok_or_else(|| AutoError::excel(1004, ""))?;
                            self.acquire(&mut access, &ReferenceLike::area(*area), RangeAxis::Cells)
                        }
                        _ => {
                            no_args(call)?;
                            match name {
                                "Count" => Ok(AutoValue::I4(range.area_count() as i32)),
                                "Parent" => self.acquire(&mut access, &range, RangeAxis::Cells),
                                "Application" => Ok(AutoValue::Object(self.application())),
                                _ => Err(AutoError::not_modelled(call.label(), OWNER)),
                            }
                        }
                    }
                }
                ObjectKey::Names(workbook) => {
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
                    match name {
                        "Item" | "_Default" => {
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
                                    calc_value::workbook_names_equal(
                                        &n.definition().name,
                                        &text.to_string_lossy(),
                                    )
                                }),
                                other => usize::try_from(geometry::integer(other)? - 1)
                                    .ok()
                                    .and_then(|i| names.get(i).copied()),
                            }
                            .ok_or_else(|| AutoError::excel(1004, ""))?;
                            Ok(AutoValue::Object(object(
                                "Name",
                                ObjectKey::Name {
                                    workbook: *workbook,
                                    id: node.id(),
                                },
                            )))
                        }
                        _ => {
                            no_args(call)?;
                            Ok(match name {
                                "Count" => AutoValue::I4(names.len() as i32),
                                "Parent" => book(),
                                "Application" => AutoValue::Object(self.application()),
                                _ => return Err(AutoError::not_modelled(call.label(), OWNER)),
                            })
                        }
                    }
                }
                ObjectKey::Name { id, .. } => {
                    no_args(call)?;
                    let node = meta
                        .model
                        .definitions()
                        .name_table()
                        .get(*id)
                        .ok_or_else(|| AutoError::excel(1004, ""))?;
                    let definition = node.definition();
                    match name {
                        "Name" => Ok(AutoValue::text(&definition.name)),
                        "RefersTo" => Ok(AutoValue::text(&format!("={}", definition.formula))),
                        "RefersToRange" => {
                            let at = meta.active.ok_or_else(|| {
                                AutoError::not_modelled(
                                    "Name.RefersToRange without active worksheet",
                                    OWNER,
                                )
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
                                _ => {
                                    crate::resolve_reference_text(&meta.model, at, &definition.name)
                                }
                            };
                            match result {
                                ReferenceTarget::Area(area) => self.acquire(
                                    &mut access,
                                    &ReferenceLike::area(area),
                                    RangeAxis::Cells,
                                ),
                                ReferenceTarget::Invalid => Err(AutoError::excel(1004, "")),
                                _ => Err(AutoError::not_modelled(
                                    "Name.RefersToRange definition form",
                                    "dc-hsa5.6",
                                )),
                            }
                        }
                        "Parent" => Ok(book()),
                        "Application" => Ok(AutoValue::Object(self.application())),
                        _ => Err(AutoError::not_modelled(call.label(), OWNER)),
                    }
                }
                _ => Err(AutoError::not_modelled(call.label(), OWNER)),
            }
        })
    }
}
