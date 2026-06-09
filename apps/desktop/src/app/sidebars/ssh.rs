use super::*;
use crate::app::ui_helpers::with_codux_tooltip;
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};

pub(in crate::app) fn ssh_section(
    ssh: &SSHSummary,
    selected_profile_id: Option<&str>,
    scroll_handle: UniformListScrollHandle,
    language: &str,
    _window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let locale = locale_from_language_setting(language);
    let title = translate(&locale, "ssh.panel.title", "SSH");
    let empty_label = translate(&locale, "ssh.panel.empty.title", "No SSH Connections");
    let connect_label = translate(&locale, "ssh.profile.connect", "Connect");
    let open_label = translate(&locale, "common.open", "Open");
    let edit_label = translate(&locale, "common.edit", "Edit");
    let remove_label = translate(&locale, "common.remove", "Remove");
    let profiles = Rc::new(ssh.profiles.clone());
    let selected_profile_id = selected_profile_id.map(str::to_string);
    let error_row = ssh.error.as_ref().map(|error| {
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
            HeroIconName::CommandLine,
            header_icon_button(
                "ssh-add-profile",
                HeroIconName::Plus,
                cx,
                |app, _event, window, cx| app.open_ssh_profile_dialog(window, cx),
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
                    ssh_empty_state(empty_label.clone(), cx).into_any_element()
                } else {
                    let _ = scroll_handle;
                    div()
                        .flex()
                        .flex_col()
                        .children(profiles.iter().cloned().map(|profile| {
                            ssh_profile_row(
                                profile,
                                selected_profile_id.as_deref(),
                                connect_label.clone(),
                                open_label.clone(),
                                edit_label.clone(),
                                remove_label.clone(),
                                cx,
                            )
                            .into_any_element()
                        }))
                        .into_any_element()
                })
                .children(error_row),
        )
}

fn ssh_empty_state(label: String, cx: &mut Context<CoduxApp>) -> impl IntoElement {
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
                    Icon::new(HeroIconName::CommandLine)
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

fn ssh_profile_row(
    profile: SSHProfileSummary,
    selected_profile_id: Option<&str>,
    connect_label: String,
    open_label: String,
    edit_label: String,
    remove_label: String,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let active = selected_profile_id
        .map(|id| id == profile.id)
        .unwrap_or(false);
    let profile_id = profile.id.clone();
    let connect_profile_id = profile.id.clone();
    let right_click_profile_id = profile.id.clone();
    let menu_profile_id = profile.id.clone();
    let hover_surface = ai_stats_track_surface(cx);
    let app_entity = cx.entity();
    div()
        .id(SharedString::from(format!("ssh-profile-{}", profile.id)))
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
            app.select_ssh_profile(profile_id.clone(), window, cx)
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, _event, window, cx| {
                app.select_ssh_profile(right_click_profile_id.clone(), window, cx)
            }),
        )
        .child(
            div()
                .size(px(40.0))
                .rounded(px(8.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(color(theme::ORANGE).opacity(0.14))
                .child(
                    Icon::new(HeroIconName::CommandLine)
                        .size_4()
                        .text_color(color(theme::ORANGE)),
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
                        .child(profile.endpoint),
                ),
        )
        .child(with_codux_tooltip(
            cx.entity(),
            format!("ssh-connect-tooltip-{}", profile.id),
            Button::new(SharedString::from(format!("ssh-connect-{}", profile.id)))
                .compact()
                .ghost()
                .text_color(cx.theme().secondary_foreground)
                .icon(
                    Icon::new(HeroIconName::ArrowTopRightOnSquare)
                        .size_3p5()
                        .text_color(cx.theme().secondary_foreground),
                )
                .on_click(cx.listener(move |app, _event, window, cx| {
                    cx.stop_propagation();
                    app.select_ssh_profile(connect_profile_id.clone(), window, cx);
                    app.connect_selected_ssh_profile(window, cx);
                })),
            connect_label,
        ))
        .context_menu(move |menu, _window, _cx| {
            let open_entity = app_entity.clone();
            let open_profile_id = menu_profile_id.clone();
            let edit_entity = app_entity.clone();
            let edit_profile_id = menu_profile_id.clone();
            let remove_entity = app_entity.clone();
            let remove_profile_id = menu_profile_id.clone();

            menu.item(
                PopupMenuItem::new(open_label.clone())
                    .icon(HeroIconName::ArrowTopRightOnSquare)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&open_entity, |app, cx| {
                            app.select_ssh_profile(open_profile_id.clone(), window, cx);
                            app.connect_selected_ssh_profile(window, cx);
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(edit_label.clone())
                    .icon(HeroIconName::Pencil)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&edit_entity, |app, cx| {
                            app.select_ssh_profile(edit_profile_id.clone(), window, cx);
                            app.open_selected_ssh_profile_editor(edit_profile_id.clone(), cx);
                        });
                    }),
            )
            .separator()
            .item(
                PopupMenuItem::new(remove_label.clone())
                    .icon(HeroIconName::Trash)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&remove_entity, |app, cx| {
                            app.select_ssh_profile(remove_profile_id.clone(), window, cx);
                            app.delete_selected_ssh_profile(window, cx);
                        });
                    }),
            )
        })
}
