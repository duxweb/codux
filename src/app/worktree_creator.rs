use super::*;
use gpui_component::input::{Input, InputEvent, InputState};

#[derive(Clone)]
struct WorktreeBranchOption {
    value: String,
    label: SharedString,
}

impl WorktreeBranchOption {
    fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            label: SharedString::from(value.clone()),
            value,
        }
    }
}

impl SelectItem for WorktreeBranchOption {
    type Value = String;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }
}

impl CoduxApp {
    pub(in crate::app) fn worktree_creator_workspace(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let language = self.state.settings.language.as_str();
        let locale = locale_from_language_setting(language);
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        let title = tr("worktree.create.title", "New Worktree");
        let project_name = self.worktree_creator_project_name.clone();
        let can_submit = !self.worktree_creator_submitting
            && !self.worktree_creator_name.trim().is_empty()
            && !self.worktree_creator_base_branch.trim().is_empty();

        child_window_shell(title.clone(), cx)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(
                        div()
                            .text_size(rems(0.75))
                            .line_height(rems(1.0))
                            .text_color(color(theme::TEXT_DIM))
                            .truncate()
                            .child(project_name),
                    )
                    .child(worktree_branch_select(
                        tr("worktree.task.base_branch", "Base Branch"),
                        &self.worktree_creator_base_branch,
                        self.worktree_creator_branch_options(),
                        window,
                        cx,
                    ))
                    .child(worktree_creator_input(
                        tr("worktree.task.title", "Worktree Name"),
                        "worktree-name",
                        &self.worktree_creator_name,
                        tr("worktree.task.default_title", "New Worktree"),
                        window,
                        cx,
                        |app, value, _window, cx| {
                            app.worktree_creator_name = value;
                            app.worktree_creator_error = None;
                            cx.notify();
                        },
                    ))
                    .when_some(self.worktree_creator_error.clone(), |this, error| {
                        this.child(
                            div()
                                .rounded(px(8.0))
                                .bg(color(0xFF5C68).opacity(0.12))
                                .px(px(10.0))
                                .py(px(8.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.125))
                                .text_color(color(0xFF5C68))
                                .child(error),
                        )
                    }),
            )
            .child(dialog_footer_bar(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(dialog_cancel_button(
                        "worktree-create-cancel",
                        tr("common.cancel", "Cancel"),
                        cx,
                        |_app, _event, window, _cx| {
                            window.remove_window();
                        },
                    ))
                    .child(
                        dialog_primary_button(
                            "worktree-create-confirm",
                            tr("common.create", "Create"),
                            cx,
                            |app, _event, window, cx| {
                                app.submit_worktree_creator(window, cx);
                            },
                        )
                        .disabled(!can_submit)
                        .loading(self.worktree_creator_submitting),
                    ),
                cx,
            ))
    }

    fn worktree_creator_branch_options(&self) -> Vec<String> {
        let mut values = Vec::new();
        push_unique_branch(&mut values, self.worktree_creator_base_branch.as_str());
        for branch in &self.state.git.branches {
            push_unique_branch(&mut values, branch.name.as_str());
        }
        push_unique_branch(&mut values, self.state.git.branch.as_str());
        if let Some(worktree) = super::ai_runtime_status::selected_worktree_info(&self.state) {
            push_unique_branch(&mut values, worktree.branch.as_str());
        }
        values
    }
}

fn push_unique_branch(values: &mut Vec<String>, value: &str) {
    let branch = value.trim();
    if branch.is_empty() || values.iter().any(|item| item == branch) {
        return;
    }
    values.push(branch.to_string());
}

fn worktree_branch_select(
    label: String,
    value: &str,
    options: Vec<String>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let items = options
        .into_iter()
        .map(WorktreeBranchOption::new)
        .collect::<Vec<_>>();
    let selected_index = items.iter().position(|item| item.value == value);
    let state = window.use_keyed_state("worktree-branch-select", cx, {
        let items = items.clone();
        move |window, cx| {
            SelectState::new(
                items,
                selected_index.map(|row| gpui_component::IndexPath::default().row(row)),
                window,
                cx,
            )
        }
    });
    state.update(cx, |state, cx| {
        state.set_items(items, window, cx);
        state.set_selected_index(
            selected_index.map(|row| gpui_component::IndexPath::default().row(row)),
            window,
            cx,
        );
    });
    cx.subscribe_in(&state, window, |app, _, event, _window, cx| {
        let SelectEvent::Confirm(selected) = event;
        if let Some(value) = selected {
            app.worktree_creator_base_branch = value.to_string();
            app.worktree_creator_error = None;
            cx.notify();
        }
    })
    .detach();

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(worktree_creator_label(label))
        .child(
            Select::new(&state)
                .menu_width(px(260.0))
                .with_size(Size::Medium),
        )
}

fn worktree_creator_input(
    label: String,
    id: &'static str,
    value: &str,
    placeholder: String,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let value = value.to_string();
    let state = window.use_keyed_state(SharedString::from(format!("worktree-input-{id}")), cx, {
        let value = value.clone();
        move |window, cx| {
            InputState::new(window, cx)
                .default_value(value.clone())
                .placeholder(placeholder)
        }
    });
    state.update(cx, |state, cx| {
        if state.value().as_ref() != value.as_str() {
            state.set_value(value.clone(), window, cx);
        }
    });
    cx.subscribe_in(&state, window, move |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            action(app, state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(worktree_creator_label(label))
        .child(Input::new(&state).with_size(Size::Medium))
}

fn worktree_creator_label(label: String) -> impl IntoElement {
    div()
        .text_size(rems(0.875))
        .line_height(rems(1.125))
        .text_color(color(theme::TEXT))
        .child(label)
}
