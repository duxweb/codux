import AppKit
import Foundation
import SwiftUI

@MainActor
private final class AppDialogLifecycle {
    static var activeControllers: [ObjectIdentifier: AnyObject] = [:]

    static func retain(_ controller: AnyObject) {
        activeControllers[ObjectIdentifier(controller)] = controller
    }

    static func release(_ controller: AnyObject) {
        activeControllers.removeValue(forKey: ObjectIdentifier(controller))
    }
}

class AppDialogPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

@MainActor
class AppDialogController<Result>: NSWindowController, NSWindowDelegate {
    private var isFinishing = false
    var responseValue: Result?

    init(panel: NSPanel) {
        super.init(window: panel)
        panel.delegate = self
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func prepareForPresentation() {}

    func beginSheet(for parentWindow: NSWindow, completion: @escaping (Result?) -> Void) {
        guard let window else {
            completion(nil)
            return
        }

        isFinishing = false
        responseValue = nil
        prepareForPresentation()
        AppDialogLifecycle.retain(self)
        NSApp.activate(ignoringOtherApps: true)
        parentWindow.beginSheet(window) { response in
            window.orderOut(nil)
            let result = response == .continue ? self.responseValue : nil
            AppDialogLifecycle.release(self)
            completion(result)
        }
    }

    func finish(with response: NSApplication.ModalResponse, value: Result? = nil) {
        guard let window, !isFinishing else { return }

        isFinishing = true
        responseValue = response == .continue ? value : nil

        if let parent = window.sheetParent {
            parent.endSheet(window, returnCode: response)
        } else {
            NSApp.stopModal(withCode: response)
            window.close()
        }
    }

    func windowWillClose(_ notification: Notification) {
        if !isFinishing {
            finish(with: .abort)
        }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        finish(with: .abort)
        return false
    }
}

func systemAccentHexString() -> String {
    let color = NSColor.controlAccentColor.usingColorSpace(.deviceRGB) ?? NSColor.controlAccentColor
    let red = Int(round(color.redComponent * 255))
    let green = Int(round(color.greenComponent * 255))
    let blue = Int(round(color.blueComponent * 255))
    return String(format: "#%02X%02X%02X", red, green, blue)
}

struct AutofocusTextField: NSViewRepresentable {
    @Binding var text: String
    let placeholder: String
    let autofocus: Bool
    let onViewCreated: ((NSTextField) -> Void)?

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: AutofocusTextField
        var didRequestInitialFocus = false

        init(parent: AutofocusTextField) {
            self.parent = parent
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            parent.text = field.stringValue
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField(string: text)
        field.placeholderString = placeholder
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = .systemFont(ofSize: 13, weight: .regular)
        field.delegate = context.coordinator
        onViewCreated?(field)
        return field
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {
        if nsView.stringValue != text {
            nsView.stringValue = text
        }

        if autofocus,
           nsView.window?.firstResponder !== nsView.currentEditor(),
           !context.coordinator.didRequestInitialFocus {
            context.coordinator.didRequestInitialFocus = true
            DispatchQueue.main.async {
                guard let window = nsView.window else { return }
                window.makeFirstResponder(nsView)
                if let editor = window.fieldEditor(true, for: nsView) as? NSTextView {
                    editor.selectedRange = NSRange(location: nsView.stringValue.count, length: 0)
                    editor.insertionPointColor = .labelColor
                }
            }
        } else if !autofocus {
            context.coordinator.didRequestInitialFocus = false
        }
    }
}

struct AppDialogHeaderSpec {
    let title: String
    let message: String
    let icon: String?
    let iconColor: Color
}

private struct AppDialogHeaderView: View {
    let header: AppDialogHeaderSpec

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            if let icon = header.icon {
                ZStack {
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(header.iconColor.opacity(0.16))
                        .frame(width: 40, height: 40)

                    Image(systemName: icon)
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(header.iconColor)
                }
            }

            VStack(alignment: .leading, spacing: 3) {
                Text(header.title)
                    .font(.system(size: 15, weight: .bold))
                    .foregroundStyle(Color(nsColor: .labelColor))

                Text(header.message)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(Color(nsColor: .secondaryLabelColor))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

struct AppDialogFormLayout<Content: View, Actions: View>: View {
    let header: AppDialogHeaderSpec
    let width: CGFloat?
    let chromeTopInset: CGFloat
    let contentSpacing: CGFloat
    let headerTopPadding: CGFloat
    let headerBottomPadding: CGFloat
    let contentTopPadding: CGFloat
    let contentBottomPadding: CGFloat
    let footerTopPadding: CGFloat
    let footerBottomPadding: CGFloat
    let content: Content
    let actions: Actions

    init(
        header: AppDialogHeaderSpec,
        width: CGFloat? = nil,
        chromeTopInset: CGFloat = 8,
        contentSpacing: CGFloat = 14,
        headerTopPadding: CGFloat = 20,
        headerBottomPadding: CGFloat = 14,
        contentTopPadding: CGFloat = 0,
        contentBottomPadding: CGFloat = 18,
        footerTopPadding: CGFloat = 0,
        footerBottomPadding: CGFloat = 18,
        @ViewBuilder content: () -> Content,
        @ViewBuilder actions: () -> Actions
    ) {
        self.header = header
        self.width = width
        self.chromeTopInset = chromeTopInset
        self.contentSpacing = contentSpacing
        self.headerTopPadding = headerTopPadding
        self.headerBottomPadding = headerBottomPadding
        self.contentTopPadding = contentTopPadding
        self.contentBottomPadding = contentBottomPadding
        self.footerTopPadding = footerTopPadding
        self.footerBottomPadding = footerBottomPadding
        self.content = content()
        self.actions = actions()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Color.clear
                .frame(height: chromeTopInset)

            AppDialogHeaderView(header: header)
                .padding(.horizontal, 24)
                .padding(.top, headerTopPadding)
                .padding(.bottom, headerBottomPadding)

            VStack(alignment: .leading, spacing: contentSpacing) {
                content
            }
            .padding(.horizontal, 24)
            .padding(.top, contentTopPadding)
            .padding(.bottom, contentBottomPadding)

            HStack(spacing: 10) {
                Spacer()
                actions
            }
            .padding(.horizontal, 24)
            .padding(.top, footerTopPadding)
            .padding(.bottom, footerBottomPadding)
        }
        .frame(width: width, alignment: .topLeading)
        .frame(maxWidth: width == nil ? .infinity : nil, alignment: .topLeading)
        .background(AppTheme.windowBackground)
    }
}

struct AppDialogPrimaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    var tint: Color = AppTheme.focus

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 18)
            .frame(height: 32)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(tint.opacity(configuration.isPressed ? 0.7 : 1.0))
            )
            .opacity(isEnabled ? 1.0 : 0.45)
            .animation(.easeInOut(duration: 0.1), value: configuration.isPressed)
    }
}

struct AppDialogSecondaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Color(nsColor: .labelColor).opacity(0.85))
            .padding(.horizontal, 18)
            .frame(height: 32)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color(nsColor: .quaternarySystemFill).opacity(configuration.isPressed ? 1.0 : 0.7))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(Color(nsColor: .separatorColor).opacity(0.3), lineWidth: 0.5)
            )
            .animation(.easeInOut(duration: 0.1), value: configuration.isPressed)
    }
}
