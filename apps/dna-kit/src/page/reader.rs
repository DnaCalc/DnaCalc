//! Plain-text reader prose with explicitly bound, live workbook values.

use std::collections::BTreeMap;

use calc_skin::Bound;
use leptos::prelude::*;

use crate::{Live, Session};

pub(super) fn prose(
    session: Session,
    text: &str,
    bindings: &BTreeMap<String, Bound>,
) -> impl IntoView + use<> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("{=") {
        let Some((key, tail)) = after.split_once('}') else {
            break;
        };
        pieces.push(view! { <span>{before.to_owned()}</span> }.into_any());
        let key = key.trim();
        let value = bindings.get(key).map_or_else(super::missing, |bound| {
            chip(session, key.to_owned(), bound.clone()).into_any()
        });
        pieces.push(value);
        rest = tail;
    }
    pieces.push(view! { <span>{rest.to_owned()}</span> }.into_any());
    view! { <p class="dna-page-prose">{pieces}</p> }
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
