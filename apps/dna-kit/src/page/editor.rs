//! Shared literal editors for App inputs, through the same draft and apply commands.
use super::PageInput;
use crate::commands::{CommandId, Invocation};
use crate::{Live, Session};
use calc_skin::grid::Key;
use calc_skin::query::{self, InputChoice, InputEditor};
use calc_skin::{Bound, Coverage, WorkbookCell};
use leptos::prelude::*;

#[derive(Clone, Copy)]
struct Field {
    session: Session,
    input: PageInput,
    bound: StoredValue<Bound>,
    value: Memo<String>,
    enabled: Memo<bool>,
    focused: RwSignal<bool>,
}

impl Field {
    fn focus(self, on: bool) {
        self.focused.set(on);
        (self.input.focus)(self.session, on.then(|| self.bound.get_value()));
    }

    fn draft(self, text: String) -> bool {
        self.session.run(&Invocation {
            command: CommandId::new(self.input.draft_command),
            args: serde_json::to_value((self.bound.get_value(), text))
                .expect("the input draft serializes"),
        }).is_ok()
    }

    fn apply(self) {
        let _ = self.session.run(&Invocation {
            command: CommandId::new(self.input.command),
            args: serde_json::to_value(self.bound.get_value()).expect("a binding serializes"),
        });
    }

    fn choose(self, text: String) {
        if self.draft(text) {
            self.apply();
        }
    }

    fn render(self, editor: InputEditor) -> AnyView {
        match editor {
            InputEditor::Field => view! {
                <input data-dna-action-field="" disabled=move || !self.enabled.get()
                    prop:value=move || self.value.get()
                    on:focus=move |_| self.focus(true)
                    on:blur=move |_| self.focus(false)
                    on:input=move |event| { self.draft(event_target_value(&event)); }/>
            }.into_any(),
            InputEditor::Checkbox => view! {
                <input type="checkbox" data-dna-action-field="" disabled=move || !self.enabled.get()
                    prop:checked=move || self.value.with(|value| value.eq_ignore_ascii_case("TRUE"))
                    on:focus=move |_| self.focus(true)
                    on:blur=move |_| self.focus(false)
                    on:change=move |event| self.choose(
                        if event_target_checked(&event) { "TRUE" } else { "FALSE" }.into()
                    )/>
            }.into_any(),
            InputEditor::Slider { min, max, step } => self.number("range", min, max, step),
            InputEditor::Stepper { min, max, step } => self.number("number", min, max, step),
            InputEditor::Choices(choices) => self.choices(choices),
        }
    }

    fn number(self, kind: &'static str, min: i64, max: i64, step: i64) -> AnyView {
        view! {
            <input type=kind min=min.to_string() max=max.to_string() step=step.to_string()
                data-dna-action-field="" disabled=move || !self.enabled.get()
                prop:value=move || self.value.get()
                on:focus=move |_| self.focus(true)
                on:blur=move |_| self.focus(false)
                on:input=move |event| {
                    if kind == "range" {
                        self.draft(event_target_value(&event));
                    }
                }
                on:change=move |event| {
                    let target: leptos::web_sys::HtmlInputElement = event_target(&event);
                    if target.check_validity() && !target.value().is_empty() {
                        self.choose(target.value());
                    }
                }/>
            {(kind == "range").then(|| view! { <output>{move || self.value.get()}</output> })}
        }.into_any()
    }

    fn choices(self, choices: Vec<InputChoice>) -> AnyView {
        const UNSELECTED: &str = "unselected";
        let choices = StoredValue::new(choices);
        let selected = move || {
            let current = self.value.get();
            choices.with_value(|choices| choices.iter().find(|choice| {
                choice.entry == current || choice.entry.strip_prefix("'") == Some(current.as_str())
            }).map(|choice| choice.entry.clone()).unwrap_or_else(|| UNSELECTED.into()))
        };
        view! {
            <select data-dna-action-field="" disabled=move || !self.enabled.get()
                prop:value=selected
                on:focus=move |_| self.focus(true)
                on:blur=move |_| self.focus(false)
                on:change=move |event| self.choose(event_target_value(&event))>
                <option value=UNSELECTED disabled=true>"Choose…"</option>
                {choices.get_value().into_iter().map(|choice| view! {
                    <option value=choice.entry>{choice.label}</option>
                }).collect_view()}
            </select>
        }.into_any()
    }
}

pub(super) fn mount(
    session: Session,
    label: String,
    bound: Bound,
    at: Key,
    read: Memo<Option<WorkbookCell>>,
    input: PageInput,
) -> AnyView {
    let rules = session.following::<query::Rules>();
    let facts = Memo::new(move |_| match rules.at(Some(query::Rules { at })) {
        Live::Ready(answer) if answer.coverage == Coverage::Complete => Some(answer.body),
        _ => None,
    });
    let bound = StoredValue::new(bound);
    let field = Field {
        session, input, bound,
        value: Memo::new(move |_| (input.draft)(session, &bound.get_value())
            .or_else(|| read.get().map(|cell| cell.input)).unwrap_or_default()),
        enabled: Memo::new(move |_| facts.with(|facts| facts.as_ref().is_some_and(|facts| facts.refusal.is_none()))
            && read.get().is_some_and(|cell| !cell.input.starts_with('='))),
        focused: RwSignal::new(false),
    };
    let editor = Memo::new(move |_| {
        let editor = facts.with(|facts| facts.as_ref()
            .map(|facts| facts.input_editor.clone()).unwrap_or_default());
        let compatible = match editor {
            InputEditor::Checkbox => field.value.with(|value| {
                value.eq_ignore_ascii_case("TRUE") || value.eq_ignore_ascii_case("FALSE")
            }),
            InputEditor::Slider { min, max, step } | InputEditor::Stepper { min, max, step } =>
                field.value.with(|value| value.parse::<i64>().is_ok_and(|value| {
                    step > 0 && (min..=max).contains(&value) && (value - min) % step == 0
                })),
            _ => true,
        };
        if compatible { editor } else { InputEditor::Field }
    });
    view! {
        <form on:submit=move |event: leptos::ev::SubmitEvent| {
            event.prevent_default();
            field.apply();
        }>
            <label><strong>{label}</strong>{move || field.render(editor.get())}</label>
            <Show when=move || field.focused.get()>
                <small>{move || format!("Model change · {}", bound.get_value().address)}</small>
            </Show>
            {move || (input.error)(session, &bound.get_value(), at).map(|error| view! {
                <p class="dna-page-error" role="alert">{error}</p>
            })}
        </form>
    }.into_any()
}
