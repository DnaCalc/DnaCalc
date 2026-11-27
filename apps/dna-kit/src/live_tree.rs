//! Inline expression structure over the worksheet authoring context.

use crate::{Live, Session};
use calc_skin::grid::Key;
use calc_skin::{Evidence, query};
use leptos::prelude::*;

pub fn live_tree(session: Session, at: Key) -> impl IntoView {
    let tree = session.following::<query::ExpressionTree>();
    view! {
        <section class="dna-live-tree" aria-label="Expression structure">
            <h3>"Expression structure"</h3>
            {move || match tree.at(Some(query::ExpressionTree { at })) {
                Live::Ready(answer) => match answer.body {
                    Evidence::Unavailable { reason } => view! { <p role="status">{reason}</p> }.into_any(),
                    Evidence::Derived { body, .. } | Evidence::Observed { body, .. } => {
                        let mut steps = body.steps;
                        steps.sort_by_key(|step| (step.start, std::cmp::Reverse(step.len)));
                        let rows = steps.into_iter().map(|step| view! {
                            <tr data-depth=step.depth>
                                <td style:padding-inline-start=format!("{}ch", 2 * step.depth.min(12))>
                                    <code>{step.text}</code>
                                </td>
                                <td>{step.value}</td>
                            </tr>
                        }).collect_view();
                        view! {
                            <table><thead><tr><th scope="col">"Expression"</th><th scope="col">"Value"</th></tr></thead>
                                <tbody>{rows}</tbody>
                            </table>
                            <p>{body.note}</p>
                        }.into_any()
                    }
                },
                Live::Refused(reason) => view! { <p role="status">{format!("Expression structure unavailable: {reason:?}")}</p> }.into_any(),
                _ => view! { <p role="status">"Reading expression structure…"</p> }.into_any(),
            }}
        </section>
    }
}
