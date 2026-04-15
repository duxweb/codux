import AppKit
import SwiftUI

extension Color {
    init(hex: UInt, alpha: Double = 1.0) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255.0,
            green: Double((hex >> 8) & 0xFF) / 255.0,
            blue: Double(hex & 0xFF) / 255.0,
            opacity: alpha
        )
    }

    init(hexString: String, fallback: UInt = 0x6B2D73, alpha: Double = 1.0) {
        let cleaned = hexString.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
        let value = UInt(cleaned, radix: 16) ?? fallback
        self.init(hex: value, alpha: alpha)
    }

    func adjustingBrightness(_ amount: CGFloat) -> Color {
        let fallback = NSColor(self)
        guard let rgb = fallback.usingColorSpace(.extendedSRGB) else {
            return self
        }
        let nextBrightness = min(max(rgb.brightnessComponent + amount, 0), 1)
        let adjusted = NSColor(
            hue: rgb.hueComponent,
            saturation: rgb.saturationComponent,
            brightness: nextBrightness,
            alpha: rgb.alphaComponent
        )
        return Color(nsColor: adjusted)
    }
}

enum AppTheme {
    static let windowBackground = Color(nsColor: .windowBackgroundColor)
    static let chrome = Color(nsColor: .windowBackgroundColor)
    static let sidebar = Color.clear
    static let panel = Color(nsColor: .controlBackgroundColor)
    static let card = Color(nsColor: .underPageBackgroundColor)
    static let terminalSurface = Color.clear
    static let terminalChrome = Color(nsColor: .dmuxTerminalBackground)
    static let terminalDivider = Color(nsColor: .dmuxTerminalDivider)
    static let chromeDivider = Color(nsColor: .separatorColor)
    static let terminalText = Color(nsColor: .dmuxTerminalText)
    static let terminalMutedText = Color(nsColor: .dmuxTerminalMutedText)
    static let border = Color(nsColor: .separatorColor)
    static let separator = Color(nsColor: .separatorColor)
    static let focus = Color(nsColor: .controlAccentColor)
    static let textPrimary = Color(nsColor: .labelColor)
    static let textSecondary = Color(nsColor: .secondaryLabelColor)
    static let textMuted = Color(nsColor: .tertiaryLabelColor)
    static let success = Color(hex: 0x3FC17B)
    static let warning = Color(hex: 0xF4B85A)
    static let inputFill = Color(nsColor: .controlBackgroundColor).opacity(0.72)
    static let emphasizedControlFill = Color(nsColor: .tertiarySystemFill).opacity(0.92)
    static let titlebarControlHoverFill = Color(nsColor: .tertiarySystemFill)
    static let titlebarControlBorder = Color(nsColor: .separatorColor).opacity(0.32)
    static let titlebarControlHoverBorder = Color(nsColor: .separatorColor).opacity(0.44)
    static let sidebarSelectionFill = emphasizedControlFill

    static func inputBorder(isFocused: Bool, isHovered: Bool) -> Color {
        if isFocused {
            return focus.opacity(0.8)
        }
        if isHovered {
            return focus.opacity(0.8)
        }
        return Color.white.opacity(0.07)
    }
}

extension NSColor {
    static let dmuxTerminalBackground = NSColor(
        calibratedRed: 30 / 255,
        green: 30 / 255,
        blue: 30 / 255,
        alpha: 1
    )

    static let dmuxTerminalSurface = dmuxTerminalBackground
    static let dmuxTerminalChrome = dmuxTerminalBackground

    static let dmuxTerminalDivider = NSColor(
        calibratedRed: 1,
        green: 1,
        blue: 1,
        alpha: 0.14
    )

    static let dmuxTerminalText = NSColor(
        calibratedRed: 230 / 255,
        green: 237 / 255,
        blue: 243 / 255,
        alpha: 1
    )

    static let dmuxTerminalMutedText = NSColor(
        calibratedRed: 154 / 255,
        green: 164 / 255,
        blue: 178 / 255,
        alpha: 1
    )
}

struct AppInputSurfaceModifier: ViewModifier {
    let isFocused: Bool
    let isHovered: Bool
    let cornerRadius: CGFloat

    func body(content: Content) -> some View {
        content
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(AppTheme.inputFill)
            )
            .overlay {
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .strokeBorder(AppTheme.inputBorder(isFocused: isFocused, isHovered: isHovered), lineWidth: 1)
            }
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
    }
}

extension View {
    func appInputSurface(isFocused: Bool, isHovered: Bool, cornerRadius: CGFloat = 10) -> some View {
        modifier(AppInputSurfaceModifier(isFocused: isFocused, isHovered: isHovered, cornerRadius: cornerRadius))
    }
}

struct AppMultilineInputArea: View {
    @Binding var text: String
    let placeholder: String
    @Binding var isFocused: Bool
    let font: NSFont
    let horizontalInset: CGFloat
    let verticalInset: CGFloat
    let enablesSpellChecking: Bool

    @State private var isHovered = false
    @State private var isComposing = false

    var body: some View {
        ZStack(alignment: .topLeading) {
            AppMultilineEditor(
                text: $text,
                isFocused: $isFocused,
                isComposing: $isComposing,
                font: font,
                horizontalInset: horizontalInset,
                verticalInset: verticalInset,
                enablesSpellChecking: enablesSpellChecking
            )
            .padding(2)

            if text.isEmpty && !isComposing {
                Text(placeholder)
                    .font(Font(font))
                    .foregroundStyle(Color(nsColor: .placeholderTextColor))
                    .padding(.horizontal, horizontalInset + 2)
                    .padding(.top, verticalInset + 2)
                    .allowsHitTesting(false)
            }
        }
        .appInputSurface(isFocused: isFocused, isHovered: isHovered)
        .onHover { isHovered = $0 }
    }
}

struct AppMultilineEditor: NSViewRepresentable {
    @Binding var text: String
    @Binding var isFocused: Bool
    @Binding var isComposing: Bool
    let font: NSFont
    let horizontalInset: CGFloat
    let verticalInset: CGFloat
    let enablesSpellChecking: Bool

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text, isFocused: $isFocused, isComposing: $isComposing)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.borderType = .noBorder
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true

        let textView = AppCompositionAwareTextView()
        textView.delegate = context.coordinator
        textView.onCompositionChange = { isComposing in
            DispatchQueue.main.async {
                context.coordinator.isComposing = isComposing
            }
        }
        textView.drawsBackground = false
        textView.isRichText = false
        textView.importsGraphics = false
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isGrammarCheckingEnabled = false
        textView.isContinuousSpellCheckingEnabled = enablesSpellChecking
        textView.textContainerInset = NSSize(width: horizontalInset, height: verticalInset)
        textView.textContainer?.lineFragmentPadding = 0
        textView.font = font
        textView.textColor = NSColor.labelColor
        textView.string = text

        scrollView.documentView = textView
        context.coordinator.textView = textView
        return scrollView
    }

    func updateNSView(_ nsView: NSScrollView, context: Context) {
        guard let textView = context.coordinator.textView else { return }

        let isComposingMarkedText = textView.hasMarkedText()
        if context.coordinator.isComposing != isComposingMarkedText {
            DispatchQueue.main.async {
                context.coordinator.isComposing = isComposingMarkedText
            }
        }

        if textView.string != text,
           !context.coordinator.isUpdatingFromTextView,
           !isComposingMarkedText {
            textView.string = text
        }

        textView.font = font
        textView.textContainerInset = NSSize(width: horizontalInset, height: verticalInset)
        textView.isContinuousSpellCheckingEnabled = enablesSpellChecking

        if isFocused,
           textView.window?.firstResponder !== textView,
           !context.coordinator.didRequestInitialFocus {
            context.coordinator.didRequestInitialFocus = true
            DispatchQueue.main.async {
                textView.window?.makeFirstResponder(textView)
            }
        } else if !isFocused {
            context.coordinator.didRequestInitialFocus = false
        }
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        @Binding var text: String
        @Binding var isFocused: Bool
        @Binding var isComposing: Bool
        weak var textView: NSTextView?
        var didRequestInitialFocus = false
        var isUpdatingFromTextView = false

        init(text: Binding<String>, isFocused: Binding<Bool>, isComposing: Binding<Bool>) {
            _text = text
            _isFocused = isFocused
            _isComposing = isComposing
        }

        func textDidChange(_ notification: Notification) {
            guard let textView else { return }
            isUpdatingFromTextView = true
            isComposing = textView.hasMarkedText()
            text = textView.string
            DispatchQueue.main.async {
                self.isUpdatingFromTextView = false
            }
        }

        func textView(_ textView: NSTextView, shouldChangeTextIn affectedCharRange: NSRange, replacementString: String?) -> Bool {
            true
        }

        func textDidBeginEditing(_ notification: Notification) {
            isFocused = true
            didRequestInitialFocus = true
            isComposing = textView?.hasMarkedText() ?? false
        }

        func textDidEndEditing(_ notification: Notification) {
            isFocused = false
            didRequestInitialFocus = false
            isComposing = false
        }
    }
}

private final class AppCompositionAwareTextView: NSTextView {
    var onCompositionChange: ((Bool) -> Void)?

    override func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        super.setMarkedText(string, selectedRange: selectedRange, replacementRange: replacementRange)
        onCompositionChange?(hasMarkedText())
    }

    override func unmarkText() {
        super.unmarkText()
        onCompositionChange?(hasMarkedText())
    }

    override func insertText(_ string: Any, replacementRange: NSRange) {
        super.insertText(string, replacementRange: replacementRange)
        onCompositionChange?(hasMarkedText())
    }
}

struct AppVisualEffectBackground: NSViewRepresentable {
    let material: NSVisualEffectView.Material
    let blendingMode: NSVisualEffectView.BlendingMode

    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = material
        view.blendingMode = blendingMode
        view.state = .active
        return view
    }

    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {}
}

struct AppWindowGlassBackground: View {
    var body: some View {
        AppVisualEffectBackground(material: .underWindowBackground, blendingMode: .behindWindow)
    }
}

struct AppPinnedHeaderBackground: View {
    var body: some View {
        AppVisualEffectBackground(material: .headerView, blendingMode: .withinWindow)
    }
}

struct TerminalShellShape: Shape {
    func path(in rect: CGRect) -> Path {
        UnevenRoundedRectangle(
            cornerRadii: .init(topLeading: 22, bottomLeading: 0, bottomTrailing: 0, topTrailing: 0),
            style: .continuous
        ).path(in: rect)
    }
}

private struct AppCursorModifier: ViewModifier {
    let cursor: NSCursor

    func body(content: Content) -> some View {
        content.onHover { hovering in
            if hovering {
                cursor.set()
            } else {
                NSCursor.arrow.set()
            }
        }
    }
}

extension View {
    func appCursor(_ cursor: NSCursor) -> some View {
        modifier(AppCursorModifier(cursor: cursor))
    }
}
