use super::app_select::{CoduxSelectOption, codux_select};
use super::*;

#[derive(Clone)]
struct SshProfileEditorLabels {
    add: String,
    edit: String,
    name: String,
    name_placeholder: String,
    host: String,
    port: String,
    username: String,
    credential: String,
    credential_none: String,
    credential_password: String,
    credential_private_key: String,
    password: String,
    password_placeholder: String,
    private_key: String,
    key_passphrase: String,
    key_passphrase_placeholder: String,
    choose: String,
    select: String,
    cancel: String,
    test: String,
    testing: String,
    save: String,
}

impl SshProfileEditorLabels {
    fn load(language: &str) -> Self {
        let locale = locale_from_language_setting(language);
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        Self {
            add: tr("ssh.profile.add", "Add SSH Connection"),
            edit: tr("ssh.profile.edit", "Edit SSH Connection"),
            name: tr("ssh.profile.name", "Name"),
            name_placeholder: tr("ssh.profile.name.placeholder", "Production Server"),
            host: tr("ssh.profile.host", "Host"),
            port: tr("ssh.profile.port", "Port"),
            username: tr("ssh.profile.username", "Username"),
            credential: tr("ssh.profile.credential", "Credential"),
            credential_none: tr("ssh.credential.none", "None / SSH Agent"),
            credential_password: tr("ssh.credential.password", "Password"),
            credential_private_key: tr("ssh.credential.private_key", "Private Key"),
            password: tr("ssh.profile.password", "Password"),
            password_placeholder: tr("ssh.profile.password.placeholder", "Stored locally"),
            private_key: tr("ssh.profile.private_key", "Private Key"),
            key_passphrase: tr("ssh.profile.key_passphrase", "Key Passphrase"),
            key_passphrase_placeholder: tr(
                "ssh.profile.key_passphrase.placeholder",
                "Optional, stored locally",
            ),
            choose: tr("common.choose", "Choose"),
            select: tr("common.choose", "Choose"),
            cancel: tr("common.cancel", "Cancel"),
            test: tr("common.test", "Test"),
            testing: tr("ssh.profile.test.testing", "Testing..."),
            save: tr("common.save", "Save"),
        }
    }
}

fn ssh_credential_options(labels: &SshProfileEditorLabels) -> Vec<CoduxSelectOption> {
    vec![
        CoduxSelectOption::new("none", labels.credential_none.clone()),
        CoduxSelectOption::new("password", labels.credential_password.clone()),
        CoduxSelectOption::new("privateKey", labels.credential_private_key.clone()),
    ]
}

pub(in crate::app) fn ssh_profile_editor_workspace(
    app: &CoduxApp,
    ssh_testing: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let labels = SshProfileEditorLabels::load(&app.state.settings.language);
    let test_result = app.ssh_test_result.clone();
    let footer =
        div()
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(div().min_w_0().flex_1().children(test_result.map(|result| {
                ssh_test_result_message(result.message, result.ok).into_any_element()
            })))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(dialog_cancel_button(
                        "ssh-editor-cancel",
                        labels.cancel.clone(),
                        cx,
                        |_app, _event, window, _cx| {
                            window.remove_window();
                        },
                    ))
                    .child(
                        dialog_secondary_button(
                            "ssh-editor-test",
                            if ssh_testing {
                                labels.testing.clone()
                            } else {
                                labels.test.clone()
                            },
                            cx,
                            |app, _event, window, cx| app.test_ssh_profile_draft(window, cx),
                        )
                        .loading(ssh_testing)
                        .disabled(ssh_testing),
                    )
                    .child(dialog_primary_button(
                        "ssh-editor-save",
                        labels.save.clone(),
                        cx,
                        |app, _event, window, cx| app.save_ssh_profile_draft(window, cx),
                    )),
            );
    child_window_shell(
        if app.ssh_draft_id.is_some() {
            labels.edit.clone()
        } else {
            labels.add.clone()
        },
        cx,
    )
    .child(
        div()
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .p(px(18.0))
            .flex()
            .flex_col()
            .child(ssh_dialog_input(
                "name",
                labels.name.clone(),
                &app.ssh_draft_name,
                labels.name_placeholder.clone(),
                false,
                window,
                cx,
                |app, value, window, cx| app.set_ssh_draft_name(value, window, cx),
            ))
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(8.0))
                    .mb(px(16.0))
                    .child(ssh_dialog_input(
                        "host",
                        labels.host.clone(),
                        &app.ssh_draft_host,
                        "example.com".to_string(),
                        false,
                        window,
                        cx,
                        |app, value, window, cx| app.set_ssh_draft_host(value, window, cx),
                    ))
                    .child(ssh_dialog_input(
                        "port",
                        labels.port.clone(),
                        &app.ssh_draft_port,
                        "22".to_string(),
                        false,
                        window,
                        cx,
                        |app, value, window, cx| app.set_ssh_draft_port(value, window, cx),
                    )),
            )
            .child(ssh_dialog_input(
                "username",
                labels.username.clone(),
                &app.ssh_draft_username,
                "root".to_string(),
                false,
                window,
                cx,
                |app, value, window, cx| app.set_ssh_draft_username(value, window, cx),
            ))
            .child(ssh_dialog_select(
                &app.ssh_draft_credential_kind,
                &labels,
                window,
                cx,
            ))
            .when(app.ssh_draft_credential_kind == "password", |this| {
                this.child(ssh_dialog_input(
                    "password",
                    labels.password.clone(),
                    &app.ssh_draft_password,
                    labels.password_placeholder.clone(),
                    true,
                    window,
                    cx,
                    |app, value, window, cx| app.set_ssh_draft_password(value, window, cx),
                ))
            })
            .when(app.ssh_draft_credential_kind == "privateKey", |this| {
                this.child(ssh_private_key_path_input(
                    &app.ssh_draft_private_key_path,
                    &labels,
                    window,
                    cx,
                ))
                .child(ssh_dialog_input(
                    "key-passphrase",
                    labels.key_passphrase.clone(),
                    &app.ssh_draft_key_passphrase,
                    labels.key_passphrase_placeholder.clone(),
                    true,
                    window,
                    cx,
                    |app, value, window, cx| app.set_ssh_draft_key_passphrase(value, window, cx),
                ))
            }),
    )
    .child(dialog_footer_bar(footer, cx))
}

fn ssh_test_result_message(message: String, ok: bool) -> impl IntoElement {
    let tone = if ok { theme::GREEN } else { 0xFF5C68 };
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(color(theme::TEXT_MUTED))
        .child(div().size(px(7.0)).rounded_full().bg(color(tone)))
        .child(div().min_w_0().truncate().child(message))
}

fn ssh_dialog_input(
    id: &'static str,
    label: String,
    value: &str,
    placeholder: String,
    masked: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let value = value.to_string();
    let state = window.use_keyed_state(SharedString::from(format!("ssh-input-{id}")), cx, {
        let value = value.clone();
        move |window, cx| {
            InputState::new(window, cx)
                .default_value(value.clone())
                .placeholder(placeholder)
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
        .mb(px(16.0))
        .flex()
        .flex_col()
        .gap(px(5.0))
        .child(
            div()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(label),
        )
        .child(Input::new(&state).with_size(Size::Medium))
}

fn ssh_private_key_path_input(
    value: &str,
    labels: &SshProfileEditorLabels,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let value = value.to_string();
    let state = window.use_keyed_state("ssh-input-private-key", cx, {
        let value = value.clone();
        move |window, cx| {
            InputState::new(window, cx)
                .default_value(value.clone())
                .placeholder("~/.ssh/id_ed25519")
        }
    });
    state.update(cx, |state, cx| {
        if state.value().as_ref() != value.as_str() {
            state.set_value(value.clone(), window, cx);
        }
    });
    cx.subscribe_in(&state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_ssh_draft_private_key_path(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .mb(px(16.0))
        .flex()
        .flex_col()
        .gap(px(5.0))
        .child(
            div()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(labels.private_key.clone()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&state).with_size(Size::Medium)),
                )
                .child(
                    Button::new("ssh-editor-choose-key")
                        .secondary()
                        .child(dialog_button_label(labels.choose.clone()))
                        .on_click(cx.listener(|app, _event, window, cx| {
                            app.choose_ssh_private_key_path(window, cx)
                        })),
                ),
        )
}

fn ssh_dialog_select(
    value: &str,
    labels: &SshProfileEditorLabels,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let select_id = "ssh-credential-select";
    let options = ssh_credential_options(labels);

    div()
        .mb(px(16.0))
        .flex()
        .flex_col()
        .gap(px(5.0))
        .child(
            div()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(labels.credential.clone()),
        )
        .child(codux_select(
            select_id,
            value,
            options,
            labels.select.clone(),
            relative(1.0),
            px(220.0),
            false,
            window,
            cx,
            |app, value, window, cx| app.set_ssh_draft_credential_kind(value, window, cx),
        ))
}
