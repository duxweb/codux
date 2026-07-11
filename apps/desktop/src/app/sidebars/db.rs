use super::*;
use crate::app::ui_helpers::with_codux_tooltip;
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};

pub(in crate::app) fn db_section(
    db: &DBSummary,
    selected_profile_id: Option<&str>,
    language: &str,
    _window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let locale = locale_from_language_setting(language);
    let title = translate(&locale, "db.panel.title", "Databases");
    let empty_label = translate(&locale, "db.panel.empty.title", "No Database Profiles");
    let edit_label = translate(&locale, "common.edit", "Edit");
    let remove_label = translate(&locale, "common.remove", "Remove");
    let copy_label = translate(&locale, "db.profile.copy_command", "Copy Command");
    let read_only_label = translate(&locale, "db.profile.mode.read_only", "read-only");
    let read_write_label = translate(&locale, "db.profile.mode.read_write", "read-write");
    let profile_labels = DbProfileLabels {
        edit: edit_label,
        remove: remove_label,
        copy: copy_label,
        read_only: read_only_label,
        read_write: read_write_label,
    };
    let profiles = Rc::new(db.profiles.clone());
    let selected_profile_id = selected_profile_id.map(str::to_string);
    let error_row = db.error.as_ref().map(|error| {
        div()
            .mt(px(12.0))
            .p(px(12.0))
            .rounded(px(8.0))
            .bg(ai_stats_surface(cx))
            .text_size(rems(0.75))
            .line_height(rems(1.0))
            .text_color(color(theme::ACCENT))
            .child(format!("error: {error}"))
            .into_any_element()
    });

    div()
        .flex()
        .flex_1()
        .h_full()
        .min_h_0()
        .flex_col()
        .relative()
        .child(assistant_panel_header(
            title,
            HeroIconName::CircleStack,
            header_icon_button(
                "db-add-profile",
                HeroIconName::Plus,
                cx,
                |app, _event, window, cx| app.open_db_profile_dialog(window, cx),
            ),
        ))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .p(px(12.0))
                .relative()
                .overflow_y_scrollbar()
                .child(if profiles.is_empty() {
                    db_empty_state(empty_label, cx).into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .children(profiles.iter().cloned().map(|profile| {
                            db_profile_row(
                                profile,
                                selected_profile_id.as_deref(),
                                &profile_labels,
                                cx,
                            )
                            .into_any_element()
                        }))
                        .into_any_element()
                })
                .children(error_row),
        )
}

fn db_empty_state(label: String, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .text_center()
        .gap(px(10.0))
        .child(
            div()
                .size(px(44.0))
                .rounded(px(12.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(ai_stats_surface(cx))
                .child(
                    Icon::new(HeroIconName::CircleStack)
                        .size_5()
                        .text_color(color(theme::TEXT_MUTED)),
                ),
        )
        .child(
            div()
                .text_size(rems(0.8125))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT_MUTED))
                .child(label),
        )
}

struct DbProfileLabels {
    edit: String,
    remove: String,
    copy: String,
    read_only: String,
    read_write: String,
}

fn db_profile_row(
    profile: DBProfileSummary,
    selected_profile_id: Option<&str>,
    labels: &DbProfileLabels,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let edit_label = labels.edit.clone();
    let remove_label = labels.remove.clone();
    let copy_label = labels.copy.clone();
    let read_only_label = labels.read_only.clone();
    let read_write_label = labels.read_write.clone();
    let active = selected_profile_id
        .map(|id| id == profile.id)
        .unwrap_or(false);
    let profile_id = profile.id.clone();
    let right_click_profile_id = profile.id.clone();
    let menu_profile_id = profile.id.clone();
    let hover_surface = ai_stats_track_surface(cx);
    let app_entity = cx.entity();
    div()
        .id(SharedString::from(format!("db-profile-{}", profile.id)))
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .mb(px(10.0))
        .p(px(12.0))
        .rounded(px(8.0))
        .bg(if active {
            ai_stats_track_surface(cx)
        } else {
            cx.theme().transparent
        })
        .cursor_pointer()
        .hover(move |style| style.bg(hover_surface))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_db_profile(profile_id.clone(), window, cx)
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, _event, window, cx| {
                app.select_db_profile(right_click_profile_id.clone(), window, cx)
            }),
        )
        .child(
            div()
                .size(px(40.0))
                .rounded(px(8.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(color(theme::ACCENT).opacity(0.14))
                .child(
                    Icon::new(HeroIconName::CircleStack)
                        .size_4()
                        .text_color(color(theme::ACCENT)),
                ),
        )
        .child(
            div()
                .ml(px(12.0))
                .min_w_0()
                .flex()
                .flex_1()
                .flex_col()
                .child(
                    div()
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(profile.name),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .truncate()
                        .child(format!(
                            "{} · {} · {}",
                            profile.engine,
                            profile.endpoint,
                            if profile.read_only {
                                read_only_label
                            } else {
                                read_write_label
                            }
                        )),
                ),
        )
        .child(with_codux_tooltip(
            cx.entity(),
            format!("db-copy-tooltip-{}", profile.id),
            Button::new(SharedString::from(format!("db-copy-{}", profile.id)))
                .compact()
                .ghost()
                .text_color(cx.theme().secondary_foreground)
                .icon(
                    Icon::new(HeroIconName::Clipboard)
                        .size_3p5()
                        .text_color(cx.theme().secondary_foreground),
                )
                .on_click(cx.listener({
                    let profile_id = menu_profile_id.clone();
                    move |app, _event, _window, cx| {
                        cx.stop_propagation();
                        app.copy_db_command(profile_id.clone(), cx);
                    }
                })),
            copy_label.clone(),
        ))
        .context_menu(move |menu, _window, _cx| {
            let copy_entity = app_entity.clone();
            let copy_profile_id = menu_profile_id.clone();
            let edit_entity = app_entity.clone();
            let edit_profile_id = menu_profile_id.clone();
            let remove_entity = app_entity.clone();
            let remove_profile_id = menu_profile_id.clone();

            menu.item(
                PopupMenuItem::new(copy_label.clone())
                    .icon(HeroIconName::Clipboard)
                    .on_click(move |_, _window, cx| {
                        cx.update_entity(&copy_entity, |app, cx| {
                            app.copy_db_command(copy_profile_id.clone(), cx);
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(edit_label.clone())
                    .icon(HeroIconName::Pencil)
                    .on_click(move |_, _window, cx| {
                        cx.update_entity(&edit_entity, |app, cx| {
                            app.open_selected_db_profile_editor(edit_profile_id.clone(), cx);
                        });
                    }),
            )
            .separator()
            .item(
                PopupMenuItem::new(remove_label.clone())
                    .icon(HeroIconName::Trash)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&remove_entity, |app, cx| {
                            app.select_db_profile(remove_profile_id.clone(), window, cx);
                            app.delete_selected_db_profile(window, cx);
                        });
                    }),
            )
        })
}
