import AppKit
import SwiftUI

struct NotificationSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            ForEach(AppNotificationChannel.allCases) { channel in
                NotificationChannelCard(model: model, channel: channel)
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

private struct NotificationChannelCard: View {
    let model: AppModel
    let channel: AppNotificationChannel

    private var configuration: AppNotificationChannelConfiguration {
        channel.configuration(from: model.appSettings.notifications)
    }

    private var isEnabled: Bool {
        configuration.isEnabled
    }

    var body: some View {
        Section {
            headerRow

            if isEnabled {
                fieldBlock(
                    label: channel.endpointLabel,
                    placeholder: channel.endpointPlaceholder,
                    isSecure: false,
                    text: Binding(
                        get: { configuration.endpoint },
                        set: { model.updateNotificationChannelEndpoint($0, for: channel) }
                    )
                )

                if channel.showsTokenField {
                    fieldBlock(
                        label: channel.tokenLabel,
                        placeholder: channel.tokenPlaceholder,
                        isSecure: true,
                        text: Binding(
                            get: { configuration.token },
                            set: { model.updateNotificationChannelToken($0, for: channel) }
                        )
                    )
                }
            }
        }
    }

    private var headerRow: some View {
        HStack(spacing: 12) {
            ZStack {
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .fill(channel.accentColor.opacity(0.15))
                    .frame(width: 30, height: 30)

                Image(systemName: channel.symbolName)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(channel.accentColor)
            }

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Text(channel.localizedTitle)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(.primary)

                    if let url = channel.websiteURL {
                        Button {
                            model.openURL(url)
                        } label: {
                            Image(systemName: "arrow.up.right")
                                .font(.system(size: 10, weight: .semibold))
                                .foregroundStyle(.tertiary)
                                .padding(2)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .help(url.absoluteString)
                    }
                }

                Text(channel.descriptionText)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer()

            Toggle(
                "",
                isOn: Binding(
                    get: { isEnabled },
                    set: { model.updateNotificationChannelEnabled($0, for: channel) }
                )
            )
            .labelsHidden()
            .toggleStyle(.switch)
            .controlSize(.small)
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder
    private func fieldBlock(label: String, placeholder: String, isSecure: Bool, text: Binding<String>) -> some View {
        HStack(alignment: .center, spacing: 14) {
            Text(label)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .frame(width: 118, alignment: .leading)

            Spacer(minLength: 0)

            Group {
                if isSecure {
                    SecureField(
                        "",
                        text: text,
                        prompt: Text(placeholder)
                    )
                } else {
                    TextField(
                        "",
                        text: text,
                        prompt: Text(placeholder)
                    )
                }
            }
            .labelsHidden()
            .textFieldStyle(.plain)
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(Color(nsColor: .textBackgroundColor))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .stroke(Color(nsColor: .separatorColor).opacity(0.5), lineWidth: 0.5)
            )
            .frame(width: 360, alignment: .trailing)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private extension AppNotificationChannel {
    var localizedTitle: String {
        switch self {
        case .bark:
            return String(localized: "settings.notifications.channel.bark.title", defaultValue: "Bark", bundle: .module)
        case .ntfy:
            return String(localized: "settings.notifications.channel.ntfy.title", defaultValue: "ntfy", bundle: .module)
        case .wxpusher:
            return String(localized: "settings.notifications.channel.wxpusher.title", defaultValue: "WxPusher", bundle: .module)
        case .feishu:
            return String(localized: "settings.notifications.channel.feishu.title", defaultValue: "Feishu", bundle: .module)
        case .dingTalk:
            return String(localized: "settings.notifications.channel.dingtalk.title", defaultValue: "DingTalk", bundle: .module)
        case .weCom:
            return String(localized: "settings.notifications.channel.wecom.title", defaultValue: "WeCom", bundle: .module)
        case .telegram:
            return String(localized: "settings.notifications.channel.telegram.title", defaultValue: "Telegram", bundle: .module)
        case .discord:
            return String(localized: "settings.notifications.channel.discord.title", defaultValue: "Discord", bundle: .module)
        case .slack:
            return String(localized: "settings.notifications.channel.slack.title", defaultValue: "Slack", bundle: .module)
        case .webhook:
            return String(localized: "settings.notifications.channel.webhook.title", defaultValue: "Webhook", bundle: .module)
        }
    }

    var descriptionText: String {
        switch self {
        case .bark:
            return String(localized: "settings.notifications.channel.bark.description", defaultValue: "Send push alerts through a Bark server with your device key.", bundle: .module)
        case .ntfy:
            return String(localized: "settings.notifications.channel.ntfy.description", defaultValue: "Publish messages to an ntfy topic. Add a bearer token only when your server requires it.", bundle: .module)
        case .wxpusher:
            return String(localized: "settings.notifications.channel.wxpusher.description", defaultValue: "Send notifications to a WxPusher SPT target. No extra token is required.", bundle: .module)
        case .feishu:
            return String(localized: "settings.notifications.channel.feishu.description", defaultValue: "Post messages with a Feishu bot webhook. You can fill either the full URL or the hook token.", bundle: .module)
        case .dingTalk:
            return String(localized: "settings.notifications.channel.dingtalk.description", defaultValue: "Post messages with a DingTalk robot webhook. You can fill either the full URL or the access token.", bundle: .module)
        case .weCom:
            return String(localized: "settings.notifications.channel.wecom.description", defaultValue: "Post messages to a WeCom group bot. You can fill either the full URL or the webhook key.", bundle: .module)
        case .telegram:
            return String(localized: "settings.notifications.channel.telegram.description", defaultValue: "Send messages with a Telegram bot token and target chat ID.", bundle: .module)
        case .discord:
            return String(localized: "settings.notifications.channel.discord.description", defaultValue: "Deliver notifications to a Discord webhook. Optional auth token is only needed for custom gateways.", bundle: .module)
        case .slack:
            return String(localized: "settings.notifications.channel.slack.description", defaultValue: "Deliver notifications to a Slack incoming webhook. Optional auth token is only needed for custom gateways.", bundle: .module)
        case .webhook:
            return String(localized: "settings.notifications.channel.webhook.description", defaultValue: "Send JSON POST requests to your own endpoint. Add a bearer token if the receiver requires authorization.", bundle: .module)
        }
    }

    var endpointLabel: String {
        switch self {
        case .bark:
            return String(localized: "settings.notifications.channel.bark.endpoint", defaultValue: "Server URL", bundle: .module)
        case .ntfy:
            return String(localized: "settings.notifications.channel.ntfy.endpoint", defaultValue: "Topic URL", bundle: .module)
        case .wxpusher:
            return String(localized: "settings.notifications.channel.wxpusher.endpoint", defaultValue: "SPT Token", bundle: .module)
        case .feishu:
            return String(localized: "settings.notifications.channel.feishu.endpoint", defaultValue: "Webhook URL", bundle: .module)
        case .dingTalk:
            return String(localized: "settings.notifications.channel.dingtalk.endpoint", defaultValue: "Webhook URL", bundle: .module)
        case .weCom:
            return String(localized: "settings.notifications.channel.wecom.endpoint", defaultValue: "Webhook URL", bundle: .module)
        case .telegram:
            return String(localized: "settings.notifications.channel.telegram.endpoint", defaultValue: "Chat ID", bundle: .module)
        case .discord:
            return String(localized: "settings.notifications.channel.discord.endpoint", defaultValue: "Webhook URL", bundle: .module)
        case .slack:
            return String(localized: "settings.notifications.channel.slack.endpoint", defaultValue: "Webhook URL", bundle: .module)
        case .webhook:
            return String(localized: "settings.notifications.channel.webhook.endpoint", defaultValue: "Request URL", bundle: .module)
        }
    }

    var tokenLabel: String {
        switch self {
        case .bark:
            return String(localized: "settings.notifications.channel.bark.token", defaultValue: "Device Key", bundle: .module)
        case .ntfy:
            return String(localized: "settings.notifications.channel.ntfy.token", defaultValue: "Bearer Token", bundle: .module)
        case .wxpusher:
            return String(localized: "settings.notifications.channel.wxpusher.token", defaultValue: "Token", bundle: .module)
        case .feishu:
            return String(localized: "settings.notifications.channel.feishu.token", defaultValue: "Hook Token", bundle: .module)
        case .dingTalk:
            return String(localized: "settings.notifications.channel.dingtalk.token", defaultValue: "Access Token", bundle: .module)
        case .weCom:
            return String(localized: "settings.notifications.channel.wecom.token", defaultValue: "Webhook Key", bundle: .module)
        case .telegram:
            return String(localized: "settings.notifications.channel.telegram.token", defaultValue: "Bot Token", bundle: .module)
        case .discord:
            return String(localized: "settings.notifications.channel.discord.token", defaultValue: "Optional Auth Token", bundle: .module)
        case .slack:
            return String(localized: "settings.notifications.channel.slack.token", defaultValue: "Optional Auth Token", bundle: .module)
        case .webhook:
            return String(localized: "settings.notifications.channel.webhook.token", defaultValue: "Bearer Token", bundle: .module)
        }
    }

    var symbolName: String {
        switch self {
        case .bark: return "bell.badge.fill"
        case .ntfy: return "bell.and.waves.left.and.right.fill"
        case .wxpusher: return "message.fill"
        case .feishu: return "sparkles"
        case .dingTalk: return "megaphone.fill"
        case .weCom: return "person.3.fill"
        case .telegram: return "paperplane.fill"
        case .discord: return "bubble.left.and.bubble.right.fill"
        case .slack: return "number.square.fill"
        case .webhook: return "link"
        }
    }

    var accentColor: Color {
        switch self {
        case .bark: return .orange
        case .ntfy: return .blue
        case .wxpusher: return .green
        case .feishu: return .teal
        case .dingTalk: return .cyan
        case .weCom: return .mint
        case .telegram: return Color(red: 0.15, green: 0.53, blue: 0.88)
        case .discord: return Color(red: 0.44, green: 0.43, blue: 0.87)
        case .slack: return Color(red: 0.89, green: 0.20, blue: 0.53)
        case .webhook: return .gray
        }
    }
}
