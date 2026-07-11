use super::*;

pub(super) fn git_empty_repository_panel(
    git: &GitSummary,
    labels: Rc<GitSidebarLabels>,
    running_operation: Option<&GitRunningOperation>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let cloning = running_operation.is_some_and(|operation| operation.label == "clone");
    let trust_required = git
        .error
        .as_deref()
        .is_some_and(codux_runtime::git::git_repository_owner_mismatch);
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .p(px(18.0))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .text_center()
                .child(
                    div()
                        .size(px(42.0))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(color(theme::ORANGE).opacity(0.12))
                        .text_color(color(theme::ORANGE))
                        .child(
                            Icon::new(if trust_required {
                                HeroIconName::ShieldExclamation
                            } else {
                                HeroIconName::Folder
                            })
                            .size_5(),
                        ),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .child(if trust_required {
                            labels.trust_directory_title.clone()
                        } else {
                            labels.no_repository.clone()
                        }),
                )
                .child(
                    div()
                        .mt(px(6.0))
                        .max_w(px(220.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0625))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(if trust_required {
                            labels.trust_directory_description.clone()
                        } else if cloning {
                            labels.clone_preparing.clone()
                        } else {
                            labels.no_repository_description.clone()
                        }),
                )
                .when(cloning, |this| {
                    this.child(git_clone_indeterminate_progress())
                })
                .child(
                    div()
                        .mt(px(14.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .when(trust_required, |this| {
                            this.child(
                                git_empty_action_button(
                                    labels.trust_directory_action.clone(),
                                    true,
                                )
                                .on_click(cx.listener(
                                    |app, _event, window, cx| {
                                        app.trust_project_git_directory(window, cx)
                                    },
                                )),
                            )
                        })
                        .when(!trust_required && !cloning, |this| {
                            this.child(
                                git_empty_action_button(labels.init_repository.clone(), true)
                                    .on_click(cx.listener(|app, _event, window, cx| {
                                        app.init_project_git(window, cx)
                                    })),
                            )
                            .child(
                                git_empty_action_button(labels.clone_repository.clone(), false)
                                    .on_click(cx.listener(|app, _event, window, cx| {
                                        app.open_git_clone_dialog(window, cx)
                                    })),
                            )
                        }),
                ),
        )
}

fn git_clone_indeterminate_progress() -> impl IntoElement {
    Progress::new("git-clone-progress")
        .mt(px(8.0))
        .loading(true)
        .with_size(Size::XSmall)
        .color(color(theme::ACCENT))
}

fn git_empty_action_button(label: String, primary: bool) -> Stateful<Div> {
    div()
        .id(ElementId::Name(SharedString::from(format!(
            "git-empty-action-{label}"
        ))))
        .h(px(24.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .when(primary, |this| {
            this.bg(color(theme::ACCENT))
                .text_color(color(0xFFFFFF))
                .hover(|style| style.bg(color(theme::ACCENT).opacity(0.88)))
        })
        .when(!primary, |this| {
            this.bg(color(theme::BG_PANEL))
                .text_color(color(theme::TEXT))
                .hover(|style| style.bg(color(theme::BG_ROW_HOVER)))
        })
        .child(label)
}

pub(in crate::app) fn git_clone_window_workspace(
    clone_remote_url: &str,
    running_operation: Option<&GitRunningOperation>,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let labels = Rc::new(GitSidebarLabels::load(language));
    let cloning = running_operation.is_some_and(|operation| operation.label == "clone");
    let value = clone_remote_url.to_string();
    let input_state = window.use_keyed_state("git-clone-remote-url", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .placeholder(labels.remote_url.clone())
    });
    input_state.update(cx, |state, cx| {
        if state.value().as_ref() != clone_remote_url {
            state.set_value(clone_remote_url.to_string(), window, cx);
        }
    });
    cx.subscribe_in(
        &input_state,
        window,
        |app, state, event, window, cx| match event {
            InputEvent::Change => {
                app.set_git_clone_remote_url(state.read(cx).value().to_string(), window, cx);
            }
            InputEvent::PressEnter { .. } => app.clone_project_git(window, cx),
            _ => {}
        },
    )
    .detach();

    child_window_shell(labels.clone_repository.clone(), cx)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .p(px(18.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(git_clone_input_label(labels.remote_url.clone()))
                .child(
                    div()
                        .child(
                            Input::new(&input_state)
                                .disabled(cloning)
                                .with_size(gpui_component::Size::Medium),
                        )
                        .when(cloning, |this| {
                            this.child(git_clone_indeterminate_progress())
                        }),
                ),
        )
        .child(dialog_footer_bar(
            div().flex().items_center().gap(px(8.0)).child(
                dialog_primary_button(
                    "git-clone-confirm",
                    labels.confirm.clone(),
                    cx,
                    |app, _event, window, cx| app.clone_project_git(window, cx),
                )
                .loading(cloning)
                .disabled(cloning || clone_remote_url.trim().is_empty()),
            ),
            cx,
        ))
}

fn git_clone_input_label(label: impl Into<String>) -> impl IntoElement {
    div()
        .text_size(rems(0.875))
        .line_height(rems(1.125))
        .text_color(color(theme::TEXT))
        .child(label.into())
}

pub(in crate::app) fn git_credentials_window_workspace(
    app: &CoduxApp,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let labels = Rc::new(GitSidebarLabels::load(language));
    let retrying = app.git_credential_retrying
        || app
            .git_running_operation
            .as_ref()
            .is_some_and(|operation| operation.label == "clone");

    child_window_shell(labels.credentials_title.clone(), cx)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .p(px(16.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .mb(px(14.0))
                        .text_size(rems(0.875))
                        .line_height(rems(1.25))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(labels.credentials_message.clone()),
                )
                .child(git_credentials_input(
                    "username",
                    labels.credential_username.clone(),
                    &app.git_credential_username,
                    (false, retrying),
                    window,
                    cx,
                    |app, value, window, cx| app.set_git_credential_username(value, window, cx),
                ))
                .child(git_credentials_input(
                    "password-or-token",
                    labels.credential_password_or_token.clone(),
                    &app.git_credential_password_or_token,
                    (true, retrying),
                    window,
                    cx,
                    |app, value, window, cx| {
                        app.set_git_credential_password_or_token(value, window, cx)
                    },
                ))
                .when_some(app.git_credential_error.clone(), |this, error| {
                    this.child(
                        div()
                            .mt(px(8.0))
                            .text_size(rems(0.75))
                            .line_height(rems(1.0))
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
                .child(
                    dialog_cancel_button(
                        "git-credentials-cancel",
                        labels.cancel.clone(),
                        cx,
                        |app, _event, window, cx| app.close_git_credentials_dialog(window, cx),
                    )
                    .disabled(retrying),
                )
                .child(
                    dialog_primary_button(
                        "git-credentials-confirm",
                        labels.confirm.clone(),
                        cx,
                        |app, _event, window, cx| app.retry_git_clone_with_credentials(window, cx),
                    )
                    .loading(retrying)
                    .disabled(retrying),
                ),
            cx,
        ))
}

fn git_credentials_input(
    id: &'static str,
    label: String,
    value: &str,
    state: (bool, bool),
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let (masked, disabled) = state;
    let value = value.to_string();
    let state = window.use_keyed_state(SharedString::from(format!("git-credential-{id}")), cx, {
        let value = value.clone();
        move |window, cx| {
            InputState::new(window, cx)
                .default_value(value.clone())
                .masked(masked)
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
        .mb(px(14.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .child(label),
        )
        .child(
            Input::new(&state)
                .disabled(disabled)
                .with_size(gpui_component::Size::Medium),
        )
}
