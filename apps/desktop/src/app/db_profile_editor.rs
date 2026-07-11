use super::app_select::{CoduxSelectConfig, CoduxSelectOption, codux_select};
use super::*;

#[derive(Clone)]
struct DbProfileEditorLabels {
    add: String,
    edit: String,
    name: String,
    name_placeholder: String,
    engine: String,
    host: String,
    port: String,
    database: String,
    username: String,
    password: String,
    password_placeholder: String,
    ssl_mode: String,
    ssl_disable: String,
    ssl_prefer: String,
    ssl_require: String,
    read_only: String,
    select: String,
    cancel: String,
    test: String,
    testing: String,
    save: String,
}

impl DbProfileEditorLabels {
    fn load(language: &str) -> Self {
        let locale = locale_from_language_setting(language);
        let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
        Self {
            add: tr("db.profile.add", "Add Database"),
            edit: tr("db.profile.edit", "Edit Database"),
            name: tr("db.profile.name", "Name"),
            name_placeholder: tr("db.profile.name.placeholder", "Production DB"),
            engine: tr("db.profile.engine", "Engine"),
            host: tr("db.profile.host", "Host"),
            port: tr("db.profile.port", "Port"),
            database: tr("db.profile.database", "Database"),
            username: tr("db.profile.username", "Username"),
            password: tr("db.profile.password", "Password"),
            password_placeholder: tr("db.profile.password.placeholder", "Stored locally"),
            ssl_mode: tr("db.profile.ssl_mode", "SSL Mode"),
            ssl_disable: tr("db.profile.ssl.disable", "Disable"),
            ssl_prefer: tr("db.profile.ssl.prefer", "Prefer"),
            ssl_require: tr("db.profile.ssl.require", "Require"),
            read_only: tr("db.profile.read_only", "Read Only"),
            select: tr("common.choose", "Choose"),
            cancel: tr("common.cancel", "Cancel"),
            test: tr("common.test", "Test"),
            testing: tr("db.profile.test.testing", "Testing..."),
            save: tr("common.save", "Save"),
        }
    }
}

fn db_engine_options() -> Vec<CoduxSelectOption> {
    vec![
        CoduxSelectOption::new("postgres", "PostgreSQL"),
        CoduxSelectOption::new("mysql", "MySQL / MariaDB"),
        CoduxSelectOption::new("sqlite", "SQLite"),
    ]
}

fn db_ssl_options(labels: &DbProfileEditorLabels) -> Vec<CoduxSelectOption> {
    vec![
        CoduxSelectOption::new("disable", labels.ssl_disable.clone()),
        CoduxSelectOption::new("prefer", labels.ssl_prefer.clone()),
        CoduxSelectOption::new("require", labels.ssl_require.clone()),
    ]
}

pub(in crate::app) fn db_profile_editor_workspace(
    app: &CoduxApp,
    db_testing: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let labels = DbProfileEditorLabels::load(&app.state.settings.language);
    let test_result = app.db_test_result.clone();
    let footer =
        div()
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(div().min_w_0().flex_1().children(test_result.map(|result| {
                db_test_result_message(result.message, result.ok).into_any_element()
            })))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(dialog_cancel_button(
                        "db-editor-cancel",
                        labels.cancel.clone(),
                        cx,
                        |_app, _event, window, _cx| {
                            window.remove_window();
                        },
                    ))
                    .child(
                        dialog_secondary_button(
                            "db-editor-test",
                            if db_testing {
                                labels.testing.clone()
                            } else {
                                labels.test.clone()
                            },
                            cx,
                            |app, _event, window, cx| app.test_db_profile_draft(window, cx),
                        )
                        .loading(db_testing)
                        .disabled(db_testing),
                    )
                    .child(dialog_primary_button(
                        "db-editor-save",
                        labels.save.clone(),
                        cx,
                        |app, _event, window, cx| app.save_db_profile_draft(window, cx),
                    )),
            );

    child_window_shell(
        if app.db_draft_id.is_some() {
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
            .child(db_dialog_input(
                "name",
                labels.name.clone(),
                &app.db_draft_name,
                DbDialogInputOptions {
                    placeholder: labels.name_placeholder.clone(),
                    masked: false,
                },
                window,
                cx,
                |app, value, window, cx| app.set_db_draft_field("name", value, window, cx),
            ))
            .child(db_dialog_select(
                "engine",
                labels.engine.clone(),
                &app.db_draft_engine,
                (db_engine_options(), labels.select.clone()),
                window,
                cx,
                |app, value, window, cx| app.set_db_draft_field("engine", value, window, cx),
            ))
            .when(app.db_draft_engine != "sqlite", |this| {
                this.child(
                    div()
                        .grid()
                        .grid_cols(2)
                        .gap(px(8.0))
                        .mb(px(16.0))
                        .child(db_dialog_input(
                            "host",
                            labels.host.clone(),
                            &app.db_draft_host,
                            DbDialogInputOptions {
                                placeholder: "localhost".to_string(),
                                masked: false,
                            },
                            window,
                            cx,
                            |app, value, window, cx| {
                                app.set_db_draft_field("host", value, window, cx)
                            },
                        ))
                        .child(db_dialog_input(
                            "port",
                            labels.port.clone(),
                            &app.db_draft_port,
                            DbDialogInputOptions {
                                placeholder: if app.db_draft_engine == "mysql" {
                                    "3306"
                                } else {
                                    "5432"
                                }
                                .to_string(),
                                masked: false,
                            },
                            window,
                            cx,
                            |app, value, window, cx| {
                                app.set_db_draft_field("port", value, window, cx)
                            },
                        )),
                )
                .child(db_dialog_input(
                    "username",
                    labels.username.clone(),
                    &app.db_draft_username,
                    DbDialogInputOptions {
                        placeholder: "app".to_string(),
                        masked: false,
                    },
                    window,
                    cx,
                    |app, value, window, cx| app.set_db_draft_field("username", value, window, cx),
                ))
                .child(db_dialog_input(
                    "password",
                    labels.password.clone(),
                    &app.db_draft_password,
                    DbDialogInputOptions {
                        placeholder: labels.password_placeholder.clone(),
                        masked: true,
                    },
                    window,
                    cx,
                    |app, value, window, cx| app.set_db_draft_field("password", value, window, cx),
                ))
                .child(db_dialog_select(
                    "ssl",
                    labels.ssl_mode.clone(),
                    &app.db_draft_ssl_mode,
                    (db_ssl_options(&labels), labels.select.clone()),
                    window,
                    cx,
                    |app, value, window, cx| app.set_db_draft_field("sslMode", value, window, cx),
                ))
            })
            .child(db_dialog_input(
                "database",
                labels.database.clone(),
                &app.db_draft_database,
                DbDialogInputOptions {
                    placeholder: if app.db_draft_engine == "sqlite" {
                        "/path/to/app.sqlite3"
                    } else {
                        "app"
                    }
                    .to_string(),
                    masked: false,
                },
                window,
                cx,
                |app, value, window, cx| app.set_db_draft_field("database", value, window, cx),
            ))
            .child(read_only_toggle(app.db_draft_read_only, &labels, cx)),
    )
    .child(dialog_footer_bar(footer, cx))
}

fn db_test_result_message(message: String, ok: bool) -> impl IntoElement {
    let tone = if ok { theme::GREEN } else { theme::RED };
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(rems(0.75))
        .text_color(color(tone))
        .child(
            Icon::new(if ok {
                HeroIconName::CheckCircle
            } else {
                HeroIconName::XCircle
            })
            .size_4(),
        )
        .child(div().min_w_0().truncate().child(message))
}

struct DbDialogInputOptions {
    placeholder: String,
    masked: bool,
}

fn db_dialog_input(
    id: &'static str,
    label: String,
    value: &str,
    options: DbDialogInputOptions,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let DbDialogInputOptions {
        placeholder,
        masked,
    } = options;
    let value = value.to_string();
    let state = window.use_keyed_state(SharedString::from(format!("db-input-{id}")), cx, {
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

fn db_dialog_select(
    id: &'static str,
    label: String,
    value: &str,
    select: (Vec<CoduxSelectOption>, String),
    _window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let (options, select_label) = select;
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
        .child(codux_select(
            CoduxSelectConfig {
                id: format!("db-select-{id}"),
                value: value.to_string(),
                options,
                placeholder: select_label.into(),
                width: relative(1.0).into(),
                menu_width: px(220.0),
                disabled: false,
            },
            cx,
            action,
        ))
}

fn read_only_toggle(
    read_only: bool,
    labels: &DbProfileEditorLabels,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .mb(px(16.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(labels.read_only.clone()),
        )
        .child(
            Switch::new("db-read-only-toggle")
                .checked(read_only)
                .on_click(cx.listener(move |app, _event, _window, cx| {
                    app.set_db_draft_read_only(!read_only, cx)
                })),
        )
}
