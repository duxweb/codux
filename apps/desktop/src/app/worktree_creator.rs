use super::app_select::{CoduxSelectConfig, CoduxSelectOption, codux_select};
use super::*;
use gpui_component::input::{Input, InputEvent, InputState};

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
        let can_submit = !self.worktree_creator_loading
            && !self.worktree_creator_submitting
            && !self.worktree_creator_name.trim().is_empty()
            && !self.worktree_creator_base_branch.trim().is_empty();
        let branch_placeholder = if self.worktree_creator_loading {
            tr("worktree.create.branches_loading", "Loading branches...")
        } else {
            tr("common.choose", "Choose")
        };

        child_window_shell(title.clone(), cx)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(worktree_branch_select(
                        tr("worktree.task.base_branch", "Base Branch"),
                        &self.worktree_creator_base_branch,
                        self.worktree_creator_branch_options(),
                        branch_placeholder,
                        self.worktree_creator_loading,
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
                                .bg(color(theme::RED).opacity(0.12))
                                .px(px(10.0))
                                .py(px(8.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.125))
                                .text_color(color(theme::RED))
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
        self.state.worktrees.base_branches.clone()
    }
}

pub(super) fn resolved_worktree_creator_base_branch(summary: &WorktreeSummary) -> String {
    summary
        .base_branches
        .iter()
        .find(|branch| *branch == &summary.default_base_branch)
        .or_else(|| summary.base_branches.first())
        .cloned()
        .unwrap_or_default()
}

pub(super) fn worktree_creator_branch_error(
    summary: &WorktreeSummary,
) -> Option<(&'static str, &'static str)> {
    if !summary.base_branches.is_empty() {
        return None;
    }
    if summary.active_git.is_repository {
        Some((
            "worktree.create.initial_commit_required",
            "Create an initial commit before adding a worktree.",
        ))
    } else {
        Some(("worktree.repository.non_git", "Non-Git repository"))
    }
}

fn worktree_branch_select(
    label: String,
    value: &str,
    options: Vec<String>,
    placeholder: String,
    disabled: bool,
    _window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let select_id = "worktree-branch-select";
    let options = options
        .into_iter()
        .map(|value| CoduxSelectOption::new(value.clone(), SharedString::from(value)))
        .collect::<Vec<_>>();

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(worktree_creator_label(label))
        .child(codux_select(
            CoduxSelectConfig {
                id: select_id.to_string(),
                value: value.to_string(),
                options,
                placeholder: SharedString::from(placeholder),
                width: relative(1.0).into(),
                menu_width: px(260.0),
                disabled,
            },
            cx,
            |app, value, _window, cx| {
                app.worktree_creator_base_branch = value;
                app.worktree_creator_error = None;
                cx.notify();
            },
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creator_uses_authoritative_default_branch() {
        let summary = WorktreeSummary {
            base_branches: vec!["master".to_string(), "feature".to_string()],
            default_base_branch: "master".to_string(),
            ..Default::default()
        };

        assert_eq!(resolved_worktree_creator_base_branch(&summary), "master");
    }

    #[test]
    fn creator_falls_back_to_first_authoritative_branch() {
        let summary = WorktreeSummary {
            base_branches: vec!["trunk".to_string()],
            default_base_branch: "stale".to_string(),
            ..Default::default()
        };

        assert_eq!(resolved_worktree_creator_base_branch(&summary), "trunk");
    }

    #[test]
    fn creator_accepts_authoritative_branches_when_git_status_is_temporarily_unavailable() {
        let summary = WorktreeSummary {
            base_branches: vec!["master".to_string()],
            ..Default::default()
        };

        assert_eq!(worktree_creator_branch_error(&summary), None);
    }

    #[test]
    fn creator_requires_an_initial_commit_for_an_empty_repository() {
        let summary = WorktreeSummary {
            active_git: codux_runtime::git::GitSummary {
                is_repository: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(
            worktree_creator_branch_error(&summary),
            Some((
                "worktree.create.initial_commit_required",
                "Create an initial commit before adding a worktree."
            ))
        );
    }

    #[test]
    fn creator_rejects_a_non_git_directory_without_branches() {
        assert_eq!(
            worktree_creator_branch_error(&WorktreeSummary::default()),
            Some(("worktree.repository.non_git", "Non-Git repository"))
        );
    }
}
