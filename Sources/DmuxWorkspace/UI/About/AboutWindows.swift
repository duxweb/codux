import AppKit
import SwiftUI

@MainActor
enum AboutWindowPresenter {
    private static var controller: NSWindowController?

    static func show(model: AppModel) {
        if let window = controller?.window {
            if let hosting = controller?.contentViewController as? NSHostingController<AnyView> {
                hosting.rootView = AnyView(
                    AboutWindowView(model: model)
                )
            }
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 320, height: 380),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.identifier = AppWindowIdentifier.about
        applyStandardWindowChrome(window, title: String(format: String(localized: "menu.app.about_format", defaultValue: "About %@", bundle: .module), model.appDisplayName))
        window.center()
        window.isReleasedWhenClosed = false
        window.setContentSize(NSSize(width: 320, height: 380))
        window.minSize = NSSize(width: 320, height: 380)
        window.maxSize = NSSize(width: 320, height: 380)
        window.standardWindowButton(.miniaturizeButton)?.isHidden = true
        window.standardWindowButton(.zoomButton)?.isHidden = true

        let hosting = NSHostingController(
            rootView: AnyView(
                AboutWindowView(model: model)
            )
        )
        window.contentViewController = hosting
        let controller = NSWindowController(window: window)
        self.controller = controller
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}

@MainActor
enum UserAgreementWindowPresenter {
    private static var controller: NSWindowController?

    static func show(model: AppModel) {
        if let window = controller?.window {
            if let hosting = controller?.contentViewController as? NSHostingController<AnyView> {
                hosting.rootView = AnyView(
                    UserAgreementView(model: model)
                )
            }
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 520, height: 460),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.identifier = AppWindowIdentifier.agreement
        applyStandardWindowChrome(window, title: String(localized: "about.user_agreement", defaultValue: "User Agreement", bundle: .module))
        window.center()
        window.isReleasedWhenClosed = false
        let hosting = NSHostingController(
            rootView: AnyView(
                UserAgreementView(model: model)
            )
        )
        window.contentViewController = hosting
        let controller = NSWindowController(window: window)
        self.controller = controller
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}

struct AboutWindowView: View {
    let model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            Spacer().frame(height: 24)

            Image(nsImage: model.appIconImage)
                .resizable()
                .interpolation(.high)
                .antialiased(true)
                .frame(width: 96, height: 96)

            Spacer().frame(height: 14)

            Text(model.appDisplayName)
                .font(.system(size: 20, weight: .bold))

            Spacer().frame(height: 4)

            Text(model.appVersionDescription)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)

            Spacer().frame(height: 20)

            VStack(spacing: 3) {
                Text(String(localized: "about.tagline", defaultValue: "AI-Powered Terminal Workspace", bundle: .module))
                .font(.system(size: 12))
                .foregroundStyle(.secondary)

                Text(String(localized: "about.copyright", defaultValue: "Copyright © 2025 Codux contributors", bundle: .module))
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
            }

            Spacer().frame(height: 20)

            HStack(spacing: 12) {
                Button(String(localized: "about.agreement", defaultValue: "Agreement", bundle: .module)) {
                    UserAgreementWindowPresenter.show(model: model)
                }
                .font(.system(size: 12, weight: .semibold))

                Button(String(localized: "about.website", defaultValue: "Website", bundle: .module)) {
                    model.openURL(AppSupportLinks.website)
                }
                .font(.system(size: 12, weight: .semibold))

                Button(model.isCheckingForUpdates ? String(localized: "about.checking_updates", defaultValue: "Checking...", bundle: .module) : String(localized: "about.updates", defaultValue: "Updates", bundle: .module)) {
                    model.checkForUpdates()
                }
                .font(.system(size: 12, weight: .semibold))
                .disabled(model.isCheckingForUpdates)
            }
            .controlSize(.regular)

            Spacer().frame(height: 24)
        }
        .frame(width: 320, height: 380)
    }
}

struct UserAgreementView: View {
    let model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                Text(String(localized: "about.user_agreement", defaultValue: "User Agreement", bundle: .module))
                    .font(.system(size: 20, weight: .bold, design: .rounded))
                    .foregroundStyle(.primary)

                Text("GPL-3.0")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 24)
            .padding(.top, 20)
            .padding(.bottom, 16)

            Divider()

            LegalDocumentTextView(
                text: model.localizedUserAgreementDocument,
                backgroundColor: .windowBackgroundColor,
                textColor: .textColor
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(minWidth: 520, minHeight: 460)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

private struct LegalDocumentTextView: NSViewRepresentable {
    let text: String
    let backgroundColor: NSColor
    let textColor: NSColor

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true

        let textView = NSTextView()
        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = true
        textView.backgroundColor = backgroundColor
        textView.textContainerInset = NSSize(width: 24, height: 18)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        applyDocumentStyle(to: textView)

        scrollView.documentView = textView
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? NSTextView else {
            return
        }
        textView.backgroundColor = backgroundColor
        applyDocumentStyle(to: textView)
    }

    private func applyDocumentStyle(to textView: NSTextView) {
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.lineSpacing = 4
        paragraphStyle.paragraphSpacing = 10
        paragraphStyle.alignment = .left

        let attributed = NSAttributedString(
            string: text,
            attributes: [
                .font: NSFont.systemFont(ofSize: 13, weight: .regular),
                .foregroundColor: textColor,
                .paragraphStyle: paragraphStyle
            ]
        )
        textView.textStorage?.setAttributedString(attributed)
    }
}
