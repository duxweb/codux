use super::*;
use codux_runtime::{
    RemoteHostDiskMetrics, RemoteHostMetrics, RemoteHostProcessMetrics, i18n::translate,
    settings::locale_from_language_setting,
};
const SERVER_INFO_POLL_INTERVAL: Duration = Duration::from_secs(5);
const SERVER_INFO_VALUE_TEXT: f32 = 0.875;
const SERVER_INFO_VALUE_LARGE_TEXT: f32 = 1.65;
#[derive(Clone, PartialEq)]
pub(in crate::app) struct ServerInfoSidebarSnapshot {
    pub(in crate::app) language: String,
    pub(in crate::app) target: ServerInfoTarget,
}

#[derive(Clone, PartialEq)]
pub(in crate::app) enum ServerInfoTarget {
    Local,
    Remote(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServerInfoCapabilityState {
    Unknown,
    Supported,
    Unsupported,
}

pub(in crate::app) struct ServerInfoSidebarView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: ServerInfoSidebarSnapshot,
    metrics: Option<RemoteHostMetrics>,
    loading: bool,
    error: Option<String>,
    capability: ServerInfoCapabilityState,
    polling: bool,
    poll_epoch: u64,
}

enum ServerInfoFetchResult {
    Metrics(Box<RemoteHostMetrics>),
    Unsupported,
    Error(String),
}

impl ServerInfoSidebarView {
    pub(in crate::app) fn new(
        app_entity: gpui::Entity<CoduxApp>,
        snapshot: ServerInfoSidebarSnapshot,
    ) -> Self {
        Self {
            app_entity,
            snapshot,
            metrics: None,
            loading: true,
            error: None,
            capability: ServerInfoCapabilityState::Unknown,
            polling: false,
            poll_epoch: 0,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: ServerInfoSidebarSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        let host_changed = self.snapshot.target != snapshot.target;
        self.snapshot = snapshot;
        if host_changed {
            self.metrics = None;
            self.error = None;
            self.loading = true;
            self.capability = ServerInfoCapabilityState::Unknown;
            self.poll_epoch = self.poll_epoch.wrapping_add(1);
            self.polling = false;
        }
        cx.notify();
    }

    pub(in crate::app) fn restart_polling(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        self.loading = true;
        if self.capability == ServerInfoCapabilityState::Unsupported {
            self.capability = ServerInfoCapabilityState::Unknown;
        }
        self.poll_epoch = self.poll_epoch.wrapping_add(1);
        self.polling = false;
        self.ensure_polling(cx);
        cx.notify();
    }

    pub(in crate::app) fn ensure_polling(&mut self, cx: &mut Context<Self>) {
        if self.polling || self.capability == ServerInfoCapabilityState::Unsupported {
            return;
        }
        self.polling = true;
        self.poll_epoch = self.poll_epoch.wrapping_add(1);
        let epoch = self.poll_epoch;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let mut first_tick = true;
            loop {
                if !first_tick {
                    timer.timer(SERVER_INFO_POLL_INTERVAL).await;
                }
                first_tick = false;

                let request = match this.update(cx, |view, cx| view.poll_request(epoch, cx)) {
                    Ok(Some(request)) => request,
                    Ok(None) | Err(_) => break,
                };
                let result = codux_runtime::async_runtime::spawn_blocking(move || {
                    fetch_server_info_metrics(request.service, request.target, request.capability)
                })
                .await
                .map_err(|error| error.to_string())
                .unwrap_or_else(ServerInfoFetchResult::Error);

                let should_continue = this
                    .update(cx, |view, cx| view.apply_fetch_result(epoch, result, cx))
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_request(
        &mut self,
        epoch: u64,
        cx: &mut Context<Self>,
    ) -> Option<ServerInfoPollRequest> {
        if !server_info_poll_accepts_epoch(self.poll_epoch, epoch) {
            return None;
        }
        let active = self.app_entity.read(cx).assistant_panel == Some(AssistantPanel::ServerInfo);
        if !server_info_poll_should_continue(active, self.capability) {
            self.polling = false;
            return None;
        };
        self.loading = self.metrics.is_none() && self.error.is_none();
        Some(ServerInfoPollRequest {
            service: self.app_entity.read(cx).runtime_service.clone(),
            target: self.snapshot.target.clone(),
            capability: self.capability,
        })
    }

    fn apply_fetch_result(
        &mut self,
        epoch: u64,
        result: ServerInfoFetchResult,
        cx: &mut Context<Self>,
    ) -> bool {
        if !server_info_poll_accepts_epoch(self.poll_epoch, epoch) {
            return false;
        }
        match result {
            ServerInfoFetchResult::Metrics(metrics) => {
                self.capability = ServerInfoCapabilityState::Supported;
                self.loading = false;
                self.error = None;
                self.metrics = Some(*metrics);
            }
            ServerInfoFetchResult::Unsupported => {
                self.capability = ServerInfoCapabilityState::Unsupported;
                self.loading = false;
                self.error = None;
                self.metrics = None;
            }
            ServerInfoFetchResult::Error(error) => {
                self.loading = false;
                self.error = Some(error);
            }
        }
        cx.notify();
        let should_continue = server_info_poll_should_continue(
            self.app_entity.read(cx).assistant_panel == Some(AssistantPanel::ServerInfo),
            self.capability,
        );
        if !should_continue {
            self.polling = false;
        }
        should_continue
    }
}

struct ServerInfoPollRequest {
    service: codux_runtime::runtime_state::RuntimeService,
    target: ServerInfoTarget,
    capability: ServerInfoCapabilityState,
}

fn fetch_server_info_metrics(
    service: codux_runtime::runtime_state::RuntimeService,
    target: ServerInfoTarget,
    capability: ServerInfoCapabilityState,
) -> ServerInfoFetchResult {
    let ServerInfoTarget::Remote(device_id) = target else {
        return ServerInfoFetchResult::Metrics(Box::new(service.local_host_metrics()));
    };
    if capability == ServerInfoCapabilityState::Unknown {
        match service.remote_host_info_blocking(&device_id) {
            Ok(info) if host_metrics_supported(&info) => {}
            Ok(_) => return ServerInfoFetchResult::Unsupported,
            Err(error) => return ServerInfoFetchResult::Error(error),
        }
    }
    service
        .remote_host_metrics(&device_id)
        .map(|metrics| ServerInfoFetchResult::Metrics(Box::new(metrics)))
        .unwrap_or_else(ServerInfoFetchResult::Error)
}

fn host_metrics_supported(info: &serde_json::Value) -> bool {
    info.get("capabilities")
        .and_then(|capabilities| capabilities.get("domains"))
        .and_then(|domains| domains.get("hostMetrics"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn server_info_poll_accepts_epoch(current_epoch: u64, request_epoch: u64) -> bool {
    current_epoch == request_epoch
}

fn server_info_poll_should_continue(
    panel_active: bool,
    capability: ServerInfoCapabilityState,
) -> bool {
    panel_active && capability != ServerInfoCapabilityState::Unsupported
}

impl Render for ServerInfoSidebarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.snapshot.language.clone();
        let title = server_text(&language, "server.panel.title", "Server Info");
        let refreshing = self.loading && self.metrics.is_some();

        div()
            .flex()
            .flex_1()
            .h_full()
            .min_h_0()
            .flex_col()
            .child(assistant_panel_header(
                title,
                HeroIconName::ServerStack,
                server_header_refresh_button(refreshing, cx),
            ))
            .child(self.render_body(&language, cx))
    }
}

impl ServerInfoSidebarView {
    fn render_body(&self, language: &str, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.capability == ServerInfoCapabilityState::Unsupported {
            return server_empty_state(
                HeroIconName::ServerStack,
                server_text(
                    language,
                    "server.unsupported",
                    "This host version does not support server metrics.",
                ),
                cx,
            )
            .into_any_element();
        }

        div()
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .p(px(12.0))
            .flex()
            .flex_col()
            .when_some(self.error.as_ref(), |this, error| {
                this.child(server_error_card(error, language, cx))
                    .child(div().h(px(12.0)))
            })
            .when(self.loading && self.metrics.is_none(), |this| {
                this.child(server_loading_card(language, cx))
            })
            .when_some(self.metrics.as_ref(), |this, metrics| {
                this.child(system_card(metrics, language, cx))
                    .child(div().mt(px(12.0)).child(cpu_card(metrics, language, cx)))
                    .child(div().mt(px(12.0)).child(memory_card(metrics, language, cx)))
                    .child(
                        div()
                            .mt(px(12.0))
                            .child(network_card(metrics, language, cx)),
                    )
                    .child(
                        div()
                            .mt(px(12.0))
                            .child(disks_card(&metrics.disks, language, cx)),
                    )
                    .child(div().mt(px(12.0)).child(processes_card(
                        &metrics.processes,
                        language,
                        cx,
                    )))
            })
            .into_any_element()
    }
}

fn server_text(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
}

fn server_card(_title: impl Into<String>, cx: &mut Context<ServerInfoSidebarView>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(12.0))
        .bg(server_surface(cx))
        .p(px(14.0))
}

fn server_surface(cx: &mut Context<ServerInfoSidebarView>) -> gpui::Hsla {
    theme::vibrancy_raised(cx.theme().sidebar)
}

fn server_track_surface(_cx: &mut Context<ServerInfoSidebarView>) -> gpui::Hsla {
    color(theme::TEXT_MUTED).opacity(0.16)
}

fn server_header_refresh_button(
    loading: bool,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    Button::new("server-info-refresh")
        .ghost()
        .loading(loading)
        .disabled(loading)
        .text_color(cx.theme().secondary_foreground)
        .icon(Icon::new(HeroIconName::ArrowPath).text_color(cx.theme().secondary_foreground))
        .on_click(cx.listener(|view, _event, _window, cx| view.restart_polling(cx)))
}

fn server_empty_state(
    icon: HeroIconName,
    message: impl Into<String>,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    div()
        .size_full()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .text_color(cx.theme().muted_foreground)
        .child(
            Icon::new(icon)
                .size_5()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .child(message.into()),
        )
}

fn server_loading_card(
    language: &str,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    server_card(
        server_text(language, "server.loading", "Loading metrics…"),
        cx,
    )
    .child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(rems(0.8125))
            .text_color(color(theme::TEXT_MUTED))
            .child(Spinner::new().small())
            .child(server_text(
                language,
                "server.polling",
                "Connecting to host",
            )),
    )
}

fn server_error_card(
    error: &str,
    language: &str,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    server_card(
        server_text(language, "server.error", "Unable to load metrics"),
        cx,
    )
    .child(
        div()
            .text_size(rems(0.75))
            .line_height(rems(1.0))
            .text_color(color(theme::TEXT_MUTED))
            .child(error.to_string()),
    )
}

fn system_card(
    metrics: &RemoteHostMetrics,
    language: &str,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    let os = format!(
        "{} {}",
        non_empty(&metrics.system.os_name, "—"),
        metrics.system.os_version
    )
    .trim()
    .to_string();
    let (os_label, os_icon) = os_badge_parts(&metrics.system.os_name);
    server_card(server_text(language, "server.system", "System"), cx)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(os_icon_badge(os_icon, cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .truncate()
                                .text_size(rems(0.9))
                                .line_height(rems(1.1))
                                .text_color(cx.theme().foreground)
                                .font_weight(FontWeight::MEDIUM)
                                .child(non_empty(&metrics.system.hostname, "—")),
                        )
                        .child(
                            div()
                                .mt(px(3.0))
                                .truncate()
                                .text_size(rems(0.72))
                                .line_height(rems(0.95))
                                .text_color(color(theme::TEXT_MUTED))
                                .child(format!("{os_label} · {}", non_empty(&os, "—"))),
                        ),
                ),
        )
        .child(
            div()
                .mt(px(12.0))
                .grid()
                .grid_cols(2)
                .gap(px(10.0))
                .child(metric_tile(
                    server_text(language, "server.kernel", "Kernel"),
                    non_empty(&metrics.system.kernel_version, "—"),
                    None,
                    cx,
                ))
                .child(metric_tile(
                    server_text(language, "server.arch", "Arch"),
                    non_empty(&metrics.system.arch, "—"),
                    None,
                    cx,
                ))
                .child(metric_tile(
                    server_text(language, "server.uptime", "Uptime"),
                    format_duration(metrics.system.uptime_seconds),
                    None,
                    cx,
                ))
                .child(metric_tile(
                    server_text(language, "server.timezone", "Timezone"),
                    format_utc_offset(metrics.system.utc_offset_seconds),
                    None,
                    cx,
                )),
        )
}

fn cpu_card(
    metrics: &RemoteHostMetrics,
    language: &str,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    let usage = metrics.cpu.total_usage_percent.clamp(0.0, 100.0);
    let idle = (100.0 - usage).clamp(0.0, 100.0);
    let cores = metrics.cpu.cores.len();
    let load = metrics.cpu.load_avg;
    let mut top_cores = metrics.cpu.cores.clone();
    top_cores.sort_by(|left, right| right.partial_cmp(left).unwrap_or(std::cmp::Ordering::Equal));
    top_cores.truncate(4);
    if top_cores.is_empty() {
        top_cores.push(usage);
    }
    server_card(server_text(language, "server.cpu", "CPU"), cx)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(big_percent(usage, cx))
                .child(
                    div()
                        .flex()
                        .gap(px(18.0))
                        .child(metric_tile(
                            "CPU",
                            format!("{usage:.0}%"),
                            Some(cpu_usage_color(usage)),
                            cx,
                        ))
                        .child(metric_tile(
                            server_text(language, "server.idle_short", "Idle"),
                            format!("{idle:.0}%"),
                            Some(server_track_surface(cx)),
                            cx,
                        )),
                ),
        )
        .child(div().mt(px(16.0)).child(core_dot_rows(&top_cores, cx)))
        .child(
            div()
                .mt(px(16.0))
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(div().flex_1().min_w_0().child(metric_tile(
                    server_text(language, "server.cores_short", "Cores"),
                    cores.to_string(),
                    None,
                    cx,
                )))
                .when_some(load, |this, load| {
                    let load_ratio = (load[0] as f32 / cores.max(1) as f32).clamp(0.0, 1.0);
                    this.child(div().flex_1().min_w_0().child(metric_tile(
                        server_text(language, "server.load_avg", "Load"),
                        format!("{:.1} {:.1} {:.1}", load[0], load[1], load[2]),
                        None,
                        cx,
                    )))
                    .child(load_rings(load_ratio, cx))
                }),
        )
}

fn memory_card(
    metrics: &RemoteHostMetrics,
    language: &str,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    let total = metrics.memory.total_bytes.max(1);
    let used = metrics.memory.used_bytes;
    let free = metrics.memory.free_bytes;
    let cache = metrics
        .memory
        .total_bytes
        .saturating_sub(metrics.memory.used_bytes)
        .saturating_sub(metrics.memory.free_bytes);
    let used_ratio = used as f32 / total as f32;
    let cache_ratio = cache as f32 / total as f32;
    let swap_value = if metrics.memory.swap_total_bytes == 0 {
        "—".to_string()
    } else {
        format_bytes(metrics.memory.swap_used_bytes)
    };
    // Shares the card anatomy of network/cpu: two equal columns + 52px figure,
    // so column reference lines match across stacked cards.
    server_card(server_text(language, "server.memory", "Memory"), cx).child(
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .flex_1()
                    .min_w_0()
                    .child(metric_tile(
                        server_text(language, "server.used_short", "Used"),
                        format_bytes(used),
                        Some(color(theme::GREEN)),
                        cx,
                    ))
                    .child(metric_tile(
                        server_text(language, "server.free_short", "Free"),
                        format_bytes(free),
                        None,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .flex_1()
                    .min_w_0()
                    .child(metric_tile(
                        server_text(language, "server.cached_short", "Cache"),
                        format_bytes(cache),
                        Some(color(theme::ORANGE)),
                        cx,
                    ))
                    .child(metric_tile(
                        server_text(language, "server.swap", "Swap"),
                        swap_value,
                        None,
                        cx,
                    )),
            )
            .child(split_donut(
                vec![
                    (used_ratio, color(theme::GREEN)),
                    (cache_ratio, color(theme::ORANGE)),
                ],
                format!("{:.0}%", used_ratio.clamp(0.0, 1.0) * 100.0),
                px(52.0),
                cx,
            )),
    )
}

fn network_card(
    metrics: &RemoteHostMetrics,
    language: &str,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    let rx_total = metrics.network.rx_total_bytes;
    let tx_total = metrics.network.tx_total_bytes;
    let (rx_ratio, tx_ratio) = network_total_ratios(rx_total, tx_total);
    let down_color = color(theme::ACCENT);
    let up_color = color(theme::GREEN);
    server_card(server_text(language, "server.network", "Network"), cx).child(
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .flex_1()
                    .min_w_0()
                    .child(metric_tile(
                        "↓",
                        format!("{}/s", format_bytes(metrics.network.rx_bytes_per_sec)),
                        Some(down_color),
                        cx,
                    ))
                    .child(metric_tile(
                        "↑",
                        format!("{}/s", format_bytes(metrics.network.tx_bytes_per_sec)),
                        Some(up_color),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .flex_1()
                    .min_w_0()
                    .child(metric_tile(
                        server_text(language, "server.download", "Down"),
                        format_bytes(metrics.network.rx_total_bytes),
                        Some(down_color),
                        cx,
                    ))
                    .child(metric_tile(
                        server_text(language, "server.upload", "Up"),
                        format_bytes(metrics.network.tx_total_bytes),
                        Some(up_color),
                        cx,
                    )),
            )
            .child(split_donut(
                vec![(rx_ratio, down_color), (tx_ratio, up_color)],
                "".to_string(),
                px(52.0),
                cx,
            )),
    )
}

fn disks_card(
    disks: &[RemoteHostDiskMetrics],
    language: &str,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    let rows = disks
        .iter()
        .take(4)
        .enumerate()
        .map(|(index, disk)| {
            div()
                .when(index > 0, |this| {
                    this.mt(px(12.0))
                        .pt(px(12.0))
                        .border_t_1()
                        .border_color(cx.theme().border.opacity(0.34))
                })
                .child(disk_row(disk, cx))
                .into_any_element()
        })
        .collect::<Vec<_>>();
    server_card(server_text(language, "server.disks", "Disks"), cx)
        .child(div().flex().flex_col().children(rows))
}

fn processes_card(
    processes: &[RemoteHostProcessMetrics],
    language: &str,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    let rows = processes
        .iter()
        .take(10)
        .map(|process| process_row(process, cx).into_any_element())
        .collect::<Vec<_>>();
    server_card(server_text(language, "server.processes", "Processes"), cx)
        .child(process_header_row(language))
        .child(div().flex().flex_col().children(rows))
}

fn metric_tile(
    label: impl Into<String>,
    value: impl Into<String>,
    swatch: Option<gpui::Hsla>,
    cx: &mut Context<ServerInfoSidebarView>,
) -> AnyElement {
    div()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(metric_label(label, swatch))
        .child(
            div()
                .text_size(rems(SERVER_INFO_VALUE_TEXT))
                .line_height(rems(1.05))
                .truncate()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().foreground)
                .child(value.into()),
        )
        .into_any_element()
}

fn metric_label(label: impl Into<String>, swatch: Option<gpui::Hsla>) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .text_size(rems(0.66))
        .line_height(rems(0.86))
        .text_color(color(theme::TEXT_MUTED))
        .when_some(swatch, |this, color| {
            this.child(div().w(px(3.0)).h(px(10.0)).rounded(px(999.0)).bg(color))
        })
        .child(label.into())
        .into_any_element()
}

fn big_percent(value: f32, cx: &mut Context<ServerInfoSidebarView>) -> AnyElement {
    div()
        .flex()
        .items_end()
        .gap(px(4.0))
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(rems(SERVER_INFO_VALUE_LARGE_TEXT))
                .line_height(rems(1.7))
                .text_color(cx.theme().foreground)
                .child(format!("{value:.0}")),
        )
        .child(
            div()
                .pb(px(5.0))
                .text_size(rems(0.8125))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_MUTED))
                .child("%"),
        )
        .into_any_element()
}

fn os_icon_badge(icon_path: &'static str, cx: &mut Context<ServerInfoSidebarView>) -> AnyElement {
    div()
        .size(px(34.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(999.0))
        .border_1()
        .border_color(color(theme::BORDER_SOFT).opacity(0.65))
        .bg(color(theme::TEXT_MUTED).opacity(0.08))
        .child(
            Icon::empty()
                .path(icon_path)
                .size_4()
                .text_color(cx.theme().secondary_foreground),
        )
        .into_any_element()
}

fn os_badge_parts(os_name: &str) -> (&'static str, &'static str) {
    let normalized = os_name.to_ascii_lowercase();
    if normalized.contains("mac") || normalized.contains("darwin") {
        ("macOS", "icons/os-apple.svg")
    } else if normalized.contains("windows") {
        ("Windows", "icons/os-windows.svg")
    } else {
        ("Linux", "icons/os-linux.svg")
    }
}

fn core_dot_rows(usages: &[f32], cx: &mut Context<ServerInfoSidebarView>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .children(usages.iter().map(|usage| core_dot_row(*usage, cx)))
}

fn core_dot_row(usage: f32, cx: &mut Context<ServerInfoSidebarView>) -> AnyElement {
    const DOTS: usize = 24;
    let active = ((usage.clamp(0.0, 100.0) / 100.0) * DOTS as f32).round() as usize;
    let fill = cpu_usage_color(usage);
    div()
        .flex()
        .justify_between()
        .children((0..DOTS).map(move |index| {
            div()
                .w(px(6.0))
                .h(px(10.0))
                .rounded(px(999.0))
                .bg(if index < active {
                    fill
                } else {
                    server_track_surface(cx)
                })
                .into_any_element()
        }))
        .into_any_element()
}

fn cpu_usage_color(usage: f32) -> gpui::Hsla {
    if usage >= 85.0 {
        color(theme::RED)
    } else if usage >= 60.0 {
        color(theme::ORANGE)
    } else {
        color(theme::GREEN)
    }
}

fn load_rings(ratio: f32, cx: &mut Context<ServerInfoSidebarView>) -> AnyElement {
    div()
        .relative()
        .size(px(54.0))
        .flex_none()
        .child(load_rings_canvas(
            ratio.clamp(0.0, 1.0),
            color(theme::GREEN),
            server_track_surface(cx),
        ))
        .into_any_element()
}

fn load_rings_canvas(ratio: f32, fill: gpui::Hsla, track: gpui::Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            for (index, stroke_width) in [5.0, 5.0, 5.0].into_iter().enumerate() {
                let inset = px(index as f32 * 9.0);
                let ring_bounds = Bounds {
                    origin: point(bounds.origin.x + inset, bounds.origin.y + inset),
                    size: size(
                        (bounds.size.width - inset * 2.0).max(px(1.0)),
                        (bounds.size.height - inset * 2.0).max(px(1.0)),
                    ),
                };
                if let Ok(path) = ring_segment_path(ring_bounds, 0.0, 1.0, px(stroke_width)) {
                    window.paint_path(path, track);
                }
                if let Ok(path) = ring_segment_path(
                    ring_bounds,
                    0.30 + index as f32 * 0.02,
                    ratio,
                    px(stroke_width),
                ) {
                    window.paint_path(path, fill);
                }
            }
        },
    )
    .absolute()
    .inset_0()
}

fn network_total_ratios(rx_total: u64, tx_total: u64) -> (f32, f32) {
    let total = rx_total.saturating_add(tx_total);
    if total == 0 {
        return (0.0, 0.0);
    }
    (
        rx_total as f32 / total as f32,
        tx_total as f32 / total as f32,
    )
}

fn split_donut(
    segments: Vec<(f32, gpui::Hsla)>,
    center: String,
    size: Pixels,
    cx: &mut Context<ServerInfoSidebarView>,
) -> AnyElement {
    div()
        .relative()
        .size(size)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(split_donut_canvas(
            segments,
            color(theme::TEXT_MUTED).opacity(0.16),
        ))
        .when(!center.is_empty(), |this| {
            this.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(rems(SERVER_INFO_VALUE_TEXT))
                    .text_color(color(theme::TEXT_MUTED))
                    .child(center),
            )
        })
        .into_any_element()
}

fn split_donut_canvas(segments: Vec<(f32, gpui::Hsla)>, track: gpui::Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            if let Ok(path) = ring_segment_path(bounds, 0.0, 1.0, px(6.0)) {
                window.paint_path(path, track);
            }
            let mut start = 0.0f32;
            for (ratio, color) in &segments {
                let sweep = ratio.clamp(0.0, 1.0);
                if sweep <= 0.0 {
                    continue;
                }
                if let Ok(path) = ring_segment_path(bounds, start, sweep, px(6.0)) {
                    window.paint_path(path, *color);
                }
                start = (start + sweep).clamp(0.0, 1.0);
            }
        },
    )
    .absolute()
    .inset_0()
}

fn ring_segment_path(
    bounds: Bounds<Pixels>,
    start_ratio: f32,
    sweep_ratio: f32,
    stroke_width: Pixels,
) -> Result<gpui::Path<Pixels>, anyhow::Error> {
    let width: f32 = bounds.size.width.into();
    let height: f32 = bounds.size.height.into();
    let origin_x: f32 = bounds.origin.x.into();
    let origin_y: f32 = bounds.origin.y.into();
    let radius = width.min(height).max(1.0) / 2.0 - stroke_width.as_f32() - 1.0;
    let center_x = origin_x + width / 2.0;
    let center_y = origin_y + height / 2.0;
    let start_angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * start_ratio;
    let sweep = std::f32::consts::TAU * sweep_ratio.clamp(0.0, 1.0);
    let segments = ((96.0 * sweep_ratio.clamp(0.0, 1.0)).ceil() as usize).clamp(2, 96);
    let mut builder = PathBuilder::stroke(stroke_width);
    for index in 0..=segments {
        let t = index as f32 / segments as f32;
        let angle = start_angle + sweep * t;
        let point = point(
            px(center_x + radius * angle.cos()),
            px(center_y + radius * angle.sin()),
        );
        if index == 0 {
            builder.move_to(point);
        } else {
            builder.line_to(point);
        }
    }
    builder.build()
}

fn disk_row(
    disk: &RemoteHostDiskMetrics,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    let used = disk.total_bytes.saturating_sub(disk.available_bytes);
    let ratio = if disk.total_bytes == 0 {
        0.0
    } else {
        used as f32 / disk.total_bytes as f32
    };
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .truncate()
                        .text_size(rems(SERVER_INFO_VALUE_TEXT))
                        .line_height(rems(1.2))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().foreground)
                        .child(if disk.mount_point.is_empty() {
                            disk.name.clone()
                        } else {
                            disk.mount_point.clone()
                        }),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .truncate()
                        .text_size(rems(0.72))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(format!(
                            "{} · {} / {}",
                            non_empty(&disk.fs_type, "—"),
                            format_bytes(used),
                            format_bytes(disk.total_bytes)
                        )),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .w(px(92.0))
                .flex_none()
                .child(disk_rate_line("R", disk.read_bytes_per_sec, cx))
                .child(disk_rate_line("W", disk.write_bytes_per_sec, cx)),
        )
        .child(vertical_capacity_bar(ratio, capacity_color(ratio), cx))
}

fn disk_rate_line(
    label: &'static str,
    bytes_per_sec: u64,
    cx: &mut Context<ServerInfoSidebarView>,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(
            div()
                .w(px(12.0))
                .flex_none()
                .text_size(rems(0.66))
                .text_color(color(theme::TEXT_MUTED))
                .child(label),
        )
        .child(
            div()
                .text_size(rems(0.75))
                .whitespace_nowrap()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().foreground)
                .child(format!("{}/s", format_bytes(bytes_per_sec))),
        )
        .into_any_element()
}

fn vertical_capacity_bar(
    ratio: f32,
    fill: gpui::Hsla,
    cx: &mut Context<ServerInfoSidebarView>,
) -> AnyElement {
    div()
        .w(px(16.0))
        .h(px(40.0))
        .flex_none()
        .flex()
        .items_end()
        .rounded(px(6.0))
        .overflow_hidden()
        .bg(server_track_surface(cx))
        .child(div().w_full().h(relative(ratio.clamp(0.0, 1.0))).bg(fill))
        .into_any_element()
}

fn capacity_color(ratio: f32) -> gpui::Hsla {
    cpu_usage_color(ratio.clamp(0.0, 1.0) * 100.0)
}

fn process_header_row(language: &str) -> impl IntoElement {
    div()
        .h(px(22.0))
        .flex()
        .items_center()
        .gap_2()
        .text_size(rems(0.66))
        .line_height(rems(0.86))
        .text_color(color(theme::TEXT_MUTED))
        .child(div().flex_1().min_w_0().truncate().child(server_text(
            language,
            "server.process.name",
            "Name",
        )))
        .child(div().w(px(48.0)).text_right().child(server_text(
            language,
            "server.process.cpu",
            "CPU",
        )))
        .child(div().w(px(64.0)).text_right().child(server_text(
            language,
            "server.process.memory",
            "Memory",
        )))
}

fn process_row(
    process: &RemoteHostProcessMetrics,
    cx: &mut Context<ServerInfoSidebarView>,
) -> impl IntoElement {
    let cpu_color = if process.cpu_percent >= 50.0 {
        cpu_usage_color(process.cpu_percent)
    } else {
        color(theme::TEXT_MUTED)
    };
    div()
        .h(px(27.0))
        .flex()
        .items_center()
        .gap_2()
        .border_t_1()
        .border_color(cx.theme().border.opacity(0.34))
        .text_size(rems(0.75))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(color(theme::TEXT))
                .child(process.name.clone()),
        )
        .child(
            div()
                .w(px(48.0))
                .text_right()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cpu_color)
                .child(format!("{:.0}%", process.cpu_percent)),
        )
        .child(
            div()
                .w(px(64.0))
                .text_right()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(color(theme::TEXT_MUTED))
                .child(format_bytes(process.memory_bytes)),
        )
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn format_utc_offset(seconds: i32) -> String {
    let sign = if seconds < 0 { '-' } else { '+' };
    let total = seconds.unsigned_abs();
    let hours = total / 3_600;
    let minutes = (total % 3_600) / 60;
    format!("UTC{sign}{hours:02}:{minutes:02}")
}

fn non_empty(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_metrics_capability_reads_soft_gate() {
        assert!(host_metrics_supported(&json!({
            "capabilities": { "domains": { "hostMetrics": true } }
        })));
        assert!(!host_metrics_supported(&json!({
            "capabilities": { "domains": { "aiStats": true } }
        })));
    }

    #[test]
    fn byte_format_is_compact() {
        assert_eq!(format_bytes(900), "900 B");
        assert_eq!(format_bytes(2 * 1024), "2.0 KB");
        assert_eq!(format_bytes(12 * 1024 * 1024), "12 MB");
    }

    #[test]
    fn network_total_ratios_stay_empty_for_zero_totals() {
        assert_eq!(network_total_ratios(0, 0), (0.0, 0.0));
        assert_eq!(network_total_ratios(75, 25), (0.75, 0.25));
    }

    #[test]
    fn poll_state_rejects_stale_epoch_and_unsupported_hosts() {
        assert!(server_info_poll_accepts_epoch(7, 7));
        assert!(!server_info_poll_accepts_epoch(7, 6));
        assert!(server_info_poll_should_continue(
            true,
            ServerInfoCapabilityState::Supported
        ));
        assert!(!server_info_poll_should_continue(
            false,
            ServerInfoCapabilityState::Supported
        ));
        assert!(!server_info_poll_should_continue(
            true,
            ServerInfoCapabilityState::Unsupported
        ));
    }
}
