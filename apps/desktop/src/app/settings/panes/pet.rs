use super::options::*;
use super::widgets::*;
use super::*;

pub(super) fn settings_pet_pane(
    settings: &SettingsSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let language = settings.language.as_str();
    let speech_disabled = settings.pet_speech_mode == "off";
    let pet_desktop_disabled = !settings.pet_enabled;
    let pet_speech_llm_provider_disabled = speech_disabled || !settings.pet_speech_llm_enabled;
    settings_form(vec![
        settings_card(
            Some(settings_text(language, "settings.pet.section.general", "General")),
            None,
            vec![
                settings_row(
                    settings_text(language, "settings.pet.enabled", "Enable Pet"),
                    None,
                    settings_toggle(
                        "settings-pet-enabled",
                        settings.pet_enabled,
                        cx,
                        |app, window, cx| app.toggle_pet_enabled(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.pet.desktop_widget", "Desktop Pet"),
                    None,
                    settings_toggle_state(
                        "settings-pet-desktop",
                        settings.pet_desktop_widget,
                        pet_desktop_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_desktop_widget(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.pet.static_mode", "Static Pet Sprite"),
                    None,
                    settings_toggle(
                        "settings-pet-static",
                        settings.pet_static_mode,
                        cx,
                        |app, window, cx| app.toggle_pet_static_mode(window, cx),
                    ),
                )
                .into_any_element(),
            ],
            cx,)
        .into_any_element(),
        settings_card(
            Some(settings_text(language, "settings.pet.speech.section", "Pet Speech")),
            None,
            vec![
                settings_row(
                    settings_text(language, "settings.pet.speech.mode", "Mode"),
                    None,
                    settings_select_impl(
                        "settings-pet-speech-mode",
                        &settings.pet_speech_mode,
                        pet_speech_mode_options(language),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_pet_speech_mode(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.pet.speech.frequency", "Frequency"),
                    Some(settings_text(
                        language,
                        "settings.pet.speech.frequency_help",
                        "Frequency is estimated per hour, not a daily cap. The shortest global cooldown is 30 seconds.",
                    )),
                    settings_select_state(
                        "settings-pet-speech-frequency",
                        &settings.pet_speech_frequency,
                        pet_speech_frequency_options(language),
                        (speech_disabled, language),
                        window,
                        cx,
                        |app, value, window, cx| app.set_pet_speech_frequency(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.speech.quiet_during_work",
                        "Speak Less During Work",
                    ),
                    None,
                    settings_toggle_state(
                        "settings-pet-speech-work",
                        settings.pet_speech_quiet_during_work,
                        speech_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_quiet_during_work(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.speech.louder_at_night",
                        "Speak More at Night",
                    ),
                    None,
                    settings_toggle_state(
                        "settings-pet-speech-night",
                        settings.pet_speech_louder_at_night,
                        speech_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_louder_at_night(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.speech.mute_on_fullscreen",
                        "Mute in Full Screen",
                    ),
                    None,
                    settings_toggle_state(
                        "settings-pet-speech-fullscreen",
                        settings.pet_speech_mute_on_fullscreen,
                        speech_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_mute_on_fullscreen(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.speech.quiet_hours",
                        "Quiet Hours 22:00-08:00",
                    ),
                    None,
                    settings_toggle_state(
                        "settings-pet-quiet-hours",
                        settings.pet_speech_quiet_hours_enabled,
                        speech_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_quiet_hours(window, cx),
                    ),
                )
                .into_any_element(),
                div()
                    .py(px(10.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(settings_small_button_state(
                        "settings-pet-mute-30",
                        settings_text(
                            language,
                            "settings.pet.speech.mute_30_minutes",
                            "Mute for 30 Minutes",
                        ),
                        false,
                        speech_disabled,
                        cx,
                        |app, _event, _window, cx| app.set_pet_speech_temporary_mute(true, cx),
                    ))
                    .child(settings_small_button_state(
                        "settings-pet-unmute",
                        settings_text(
                            language,
                            "settings.pet.speech.unmute",
                            "Clear Temporary Mute",
                        ),
                        false,
                        speech_disabled || !settings.pet_speech_temporary_muted,
                        cx,
                        |app, _event, _window, cx| app.set_pet_speech_temporary_mute(false, cx),
                    ))
                    .into_any_element(),
            ],
            cx,)
        .into_any_element(),
        settings_card(
            Some(settings_text(language, "settings.pet.llm.section", "Pet LLM")),
            Some(settings_text(
                language,
                "settings.pet.llm.help",
                "Only rhythm and milestone messages use LLM refinement. Templates are used on failure.",
            )),
            vec![
                settings_row(
                    settings_text(language, "settings.pet.llm.enabled", "Enable LLM Refinement"),
                    None,
                    settings_toggle_state(
                        "settings-pet-llm",
                        settings.pet_speech_llm_enabled,
                        speech_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_llm_enabled(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.pet.llm.channel", "LLM Provider"),
                    None,
                    settings_select_state(
                        "pet-speech-provider",
                        &settings.pet_speech_provider_id,
                        ai_provider_options(settings, "petSpeech", language),
                        (pet_speech_llm_provider_disabled, language),
                        window,
                        cx,
                        |app, value, window, cx| app.set_pet_speech_provider(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
            cx,)
        .into_any_element(),
        settings_card(
            Some(settings_text(
                language,
                "settings.pet.section.reminders",
                "Reminders",
            )),
            None,
            vec![
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.hydration",
                        "Hydration Reminder",
                    ),
                    None,
                    settings_toggle(
                        "settings-pet-reminders",
                        settings.pet_reminders,
                        cx,
                        |app, window, cx| app.toggle_pet_reminders(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.hydration_interval",
                        "Hydration Interval",
                    ),
                    None,
                    settings_select_state(
                        "settings-pet-hydration-reminder-interval",
                        &settings.pet_hydration_reminder_minutes,
                        pet_reminder_interval_options(language),
                        (!settings.pet_reminders, language),
                        window,
                        cx,
                        |app, value, window, cx| {
                            app.set_pet_hydration_reminder_minutes(value, window, cx)
                        },
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.sedentary",
                        "Sedentary Reminder",
                    ),
                    None,
                    settings_toggle(
                        "settings-pet-sedentary-reminders",
                        settings.pet_sedentary_reminders,
                        cx,
                        |app, window, cx| app.toggle_pet_sedentary_reminders(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.sedentary_interval",
                        "Sedentary Interval",
                    ),
                    None,
                    settings_select_state(
                        "settings-pet-sedentary-reminder-interval",
                        &settings.pet_sedentary_reminder_minutes,
                        pet_reminder_interval_options(language),
                        (!settings.pet_sedentary_reminders, language),
                        window,
                        cx,
                        |app, value, window, cx| {
                            app.set_pet_sedentary_reminder_minutes(value, window, cx)
                        },
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.late_night",
                        "Late-Night Reminder",
                    ),
                    None,
                    settings_toggle(
                        "settings-pet-late-night-reminders",
                        settings.pet_late_night_reminders,
                        cx,
                        |app, window, cx| app.toggle_pet_late_night_reminders(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.late_night_interval",
                        "Late-Night Interval",
                    ),
                    None,
                    settings_select_state(
                        "settings-pet-late-night-reminder-interval",
                        &settings.pet_late_night_reminder_minutes,
                        pet_reminder_interval_options(language),
                        (!settings.pet_late_night_reminders, language),
                        window,
                        cx,
                        |app, value, window, cx| {
                            app.set_pet_late_night_reminder_minutes(value, window, cx)
                        },
                    ),
                )
                .into_any_element(),
            ],
            cx,)
        .into_any_element(),
    ])
    .into_any_element()
}
