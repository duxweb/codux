import AppKit
import SwiftUI

struct AppearanceSettingsPane: View {
    let model: AppModel

    private var darkPresets: [AppTerminalBackgroundPreset] {
        AppTerminalBackgroundPreset.allCases.filter { !$0.isAutomatic && $0.isLight == false }
    }

    private var lightPresets: [AppTerminalBackgroundPreset] {
        AppTerminalBackgroundPreset.allCases.filter { !$0.isAutomatic && $0.isLight == true }
    }

    var body: some View {
        Form {
            Section(String(localized: "settings.theme", defaultValue: "Theme", bundle: .module)) {
                VStack(alignment: .leading, spacing: 14) {
                    let autoPreset = AppTerminalBackgroundPreset.automatic
                    themeGrid(header: nil, presets: [autoPreset])
                    themeGrid(
                        header: String(localized: "settings.theme.group.dark", defaultValue: "Dark", bundle: .module),
                        presets: darkPresets
                    )
                    themeGrid(
                        header: String(localized: "settings.theme.group.light", defaultValue: "Light", bundle: .module),
                        presets: lightPresets
                    )
                }
                .padding(.vertical, 6)
            }

            Section(String(localized: "settings.background_color", defaultValue: "Background Color", bundle: .module)) {
                let fallback = model.appSettings.terminalBackgroundPreset
                    .effectiveAppearance(
                        backgroundColorPreset: .automatic,
                        automaticAppearance: model.automaticTerminalAppearance
                    )
                    .backgroundColor
                ColorSwatchGrid(
                    presets: AppBackgroundColorPreset.allCases,
                    selectedPreset: model.appSettings.backgroundColorPreset,
                    fallbackColor: fallback
                ) { model.updateBackgroundColorPreset($0) }
                .padding(.vertical, 4)
            }

            Section(String(localized: "settings.terminal_text", defaultValue: "Terminal Text", bundle: .module)) {
                LabeledContent(String(localized: "settings.terminal_font_size", defaultValue: "Terminal Font Size", bundle: .module)) {
                    HStack(spacing: 8) {
                        TextField(
                            "",
                            value: Binding(
                                get: { model.appSettings.terminalFontSize },
                                set: { model.updateTerminalFontSize($0) }
                            ),
                            format: .number
                        )
                        .labelsHidden()
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 58)

                        Stepper(
                            "",
                            value: Binding(
                                get: { model.appSettings.terminalFontSize },
                                set: { model.updateTerminalFontSize($0) }
                            ),
                            in: 10...28
                        )
                        .labelsHidden()
                    }
                }
            }

            Section(String(localized: "settings.app_icon", defaultValue: "App Icon", bundle: .module)) {
                HStack(spacing: 16) {
                    ForEach(AppIconStyle.allCases) { style in
                        AppIconPreviewCard(
                            title: style.title,
                            style: style,
                            isSelected: model.appSettings.iconStyle == style
                        ) {
                            model.updateAppIconStyle(style)
                        }
                    }
                }
                .padding(.vertical, 4)
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    @ViewBuilder
    private func themeGrid(header: String?, presets: [AppTerminalBackgroundPreset]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if let header {
                Text(header)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 96, maximum: 120), spacing: 8)],
                alignment: .leading,
                spacing: 8
            ) {
                ForEach(presets) { preset in
                    ThemePreviewCard(
                        title: preset.title,
                        appearance: preset.effectiveAppearance(
                            backgroundColorPreset: .automatic,
                            automaticAppearance: model.automaticTerminalAppearance
                        ),
                        isSelected: model.appSettings.terminalBackgroundPreset == preset
                    ) {
                        model.updateTerminalBackgroundPreset(preset)
                    }
                }
            }
        }
    }
}

private struct ThemePreviewCard: View {
    let title: String
    let appearance: AppEffectiveTerminalAppearance
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 5) {
                ZStack(alignment: .topLeading) {
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(Color(nsColor: appearance.backgroundColor))
                        .frame(height: 46)

                    VStack(alignment: .leading, spacing: 3) {
                        Capsule()
                            .fill(Color(nsColor: appearance.mutedForegroundColor)
                                .opacity(appearance.isLight ? 0.30 : 0.25))
                            .frame(width: 14, height: 2.5)
                        RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                            .fill(Color(nsColor: appearance.foregroundColor)
                                .opacity(appearance.isLight ? 0.80 : 0.90))
                            .frame(width: 28, height: 3)
                        RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                            .fill(Color(nsColor: appearance.mutedForegroundColor)
                                .opacity(appearance.isLight ? 0.55 : 0.65))
                            .frame(width: 20, height: 2.5)
                    }
                    .padding(7)
                }
                .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .stroke(
                            isSelected ? Color.accentColor : Color(nsColor: .separatorColor).opacity(0.5),
                            lineWidth: isSelected ? 2 : 0.5
                        )
                )

                Text(title)
                    .font(.system(size: 11, weight: isSelected ? .semibold : .regular))
                    .foregroundStyle(isSelected ? Color.primary : Color.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
                    .frame(maxWidth: .infinity, alignment: .center)
            }
        }
        .buttonStyle(.plain)
    }
}

private struct ColorSwatchGrid: View {
    let presets: [AppBackgroundColorPreset]
    let selectedPreset: AppBackgroundColorPreset
    let fallbackColor: NSColor
    let onSelect: (AppBackgroundColorPreset) -> Void

    private let columns = [GridItem(.adaptive(minimum: 44, maximum: 52), spacing: 8)]

    var body: some View {
        LazyVGrid(columns: columns, alignment: .leading, spacing: 10) {
            ForEach(presets) { preset in
                ColorSwatch(
                    title: preset.title,
                    swatchColor: preset.swatchColor ?? fallbackColor,
                    isAutomatic: preset.isAutomatic,
                    isSelected: selectedPreset == preset
                ) { onSelect(preset) }
            }
        }
    }
}

private struct ColorSwatch: View {
    let title: String
    let swatchColor: NSColor
    let isAutomatic: Bool
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 4) {
                ZStack {
                    Circle()
                        .fill(Color(nsColor: swatchColor))
                        .frame(width: 28, height: 28)
                        .overlay(
                            Circle()
                                .stroke(
                                    isSelected ? Color.accentColor : Color(nsColor: .separatorColor).opacity(0.4),
                                    lineWidth: isSelected ? 2.5 : 0.5
                                )
                        )
                        .shadow(color: .black.opacity(0.12), radius: 2, x: 0, y: 1)

                    if isAutomatic {
                        Text("A")
                            .font(.system(size: 11, weight: .bold, design: .rounded))
                            .foregroundStyle(Color(nsColor: swatchColor.dmuxPreviewTextColor))
                    }
                }

                Text(title)
                    .font(.system(size: 9.5, weight: isSelected ? .semibold : .regular))
                    .foregroundStyle(isSelected ? Color.primary : Color.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                    .frame(width: 44)
                    .multilineTextAlignment(.center)
            }
        }
        .buttonStyle(.plain)
    }
}

private extension NSColor {
    var dmuxSettingsPerceivedBrightness: CGFloat {
        let resolved = usingColorSpace(.deviceRGB) ?? self
        return (resolved.redComponent * 0.299) + (resolved.greenComponent * 0.587) + (resolved.blueComponent * 0.114)
    }

    var dmuxPreviewTextColor: NSColor {
        if dmuxSettingsPerceivedBrightness >= 0.72 {
            return NSColor(calibratedRed: 40 / 255, green: 39 / 255, blue: 38 / 255, alpha: 1)
        }
        return NSColor(calibratedRed: 1, green: 252 / 255, blue: 240 / 255, alpha: 1)
    }
}

private struct AppIconPreviewCard: View {
    let title: String
    let style: AppIconStyle
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 6) {
                Image(nsImage: AppIconRenderer.image(for: style, size: 96))
                    .resizable()
                    .interpolation(.high)
                    .antialiased(true)
                    .frame(width: 48, height: 48)
                    .overlay(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .stroke(isSelected ? Color.accentColor : Color.clear, lineWidth: 2)
                    )

                Text(title)
                    .font(.system(size: 11))
                    .foregroundStyle(isSelected ? .primary : .secondary)
            }
        }
        .buttonStyle(.plain)
    }
}
