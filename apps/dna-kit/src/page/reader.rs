//! Plain-text reader prose with explicitly bound, live workbook values.

use std::collections::BTreeMap;

use calc_skin::{BindingKind, BindingSnapshot, BindingValue, Bound};
use leptos::prelude::*;

use crate::{Live, Session};

pub(super) fn prose(
    session: Session,
    text: &str,
    bindings: &BTreeMap<String, Bound>,
    snapshot: Option<StoredValue<BindingSnapshot>>,
) -> impl IntoView + use<> {
    let pieces = live_values(text, |name| {
        bindings.get(&name).map_or_else(super::missing, |bound| {
            if let Some(snapshot) = snapshot {
                snapshot.with_value(|snapshot| match snapshot.get(bound, BindingKind::Value) {
                    Some(BindingValue::Cell(cell)) => view! {
                        <span class="dna-page-live-value">{super::shown_value(cell.clone(), Some(name))}</span>
                    }.into_any(),
                    _ => super::missing(),
                })
            } else {
                chip(session, name, bound.clone()).into_any()
            }
        })
    });
    view! { <p class="dna-page-prose">{pieces}</p> }
}

/// Split a prose line into text and the view's live-value chips.
pub fn live_values(text: &str, mut value: impl FnMut(String) -> AnyView) -> Vec<AnyView> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("{=") {
        let Some((key, tail)) = after.split_once('}') else {
            break;
        };
        pieces.push(before.to_owned().into_any());
        pieces.push(value(key.trim().to_owned()));
        rest = tail;
    }
    pieces.push(rest.to_owned().into_any());
    pieces
}

fn chip(session: Session, label: String, bound: Bound) -> impl IntoView {
    let source = super::resolve_bound(session, bound);
    let value = move || match source.get() {
        Live::Ready(source) if super::single_cell(&source).is_some() => {
            let read = super::first_cell(session, source);
            let label = label.clone();
            view! {
                {move || read.get().map(|cell| super::shown_value(cell, Some(label.clone())))}
            }
            .into_any()
        }
        Live::Refused(_) | Live::Ready(_) => super::missing(),
        _ => view! { <small aria-busy="true">"Reading value…"</small> }.into_any(),
    };
    view! { <span class="dna-page-live-value">{value}</span> }
}
