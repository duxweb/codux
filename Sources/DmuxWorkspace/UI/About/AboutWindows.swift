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
                        .preferredColorScheme(model.appSettings.themeMode.colorScheme)
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
        applyStandardWindowChrome(window, title: String(format: model.i18n("menu.app.about_format", fallback: "About %@"), model.appDisplayName))
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
                    .preferredColorScheme(model.appSettings.themeMode.colorScheme)
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
                        .preferredColorScheme(model.appSettings.themeMode.colorScheme)
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
        applyStandardWindowChrome(window, title: model.i18n("about.user_agreement", fallback: "User Agreement"))
        window.center()
        window.isReleasedWhenClosed = false
        let hosting = NSHostingController(
            rootView: AnyView(
                UserAgreementView(model: model)
                    .preferredColorScheme(model.appSettings.themeMode.colorScheme)
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
                Text(model.i18n("about.tagline", fallback: "AI-Powered Terminal Workspace"))
                .font(.system(size: 12))
                .foregroundStyle(.secondary)

                Text(model.i18n("about.copyright", fallback: "Copyright © 2025 dmux contributors"))
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
            }

            Spacer().frame(height: 20)

            HStack(spacing: 12) {
                Button(model.i18n("about.agreement", fallback: "Agreement")) {
                    UserAgreementWindowPresenter.show(model: model)
                }

                Button(model.i18n("about.website", fallback: "Website")) {
                    model.openURL(AppSupportLinks.website)
                }

                Button(model.i18n("about.updates", fallback: "Updates")) {
                    model.checkForUpdates()
                }
            }
            .controlSize(.small)

            Spacer().frame(height: 24)
        }
        .frame(width: 320, height: 380)
    }
}

struct UserAgreementView: View {
    let model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Text(model.i18n("about.user_agreement", fallback: "User Agreement"))
                    .font(.system(size: 20, weight: .bold, design: .rounded))
                    .foregroundStyle(.primary)

                Text(model.i18n("about.user_agreement_body", fallback: "This app is currently a development preview. By using it, you understand that terminal, Git, and AI activity features read local project metadata and runtime state, but do not proactively upload your project contents. You are responsible for the safety of your local environment, permissions, third-party CLIs, and repository credentials. Continued use means you accept that this experimental software may change behavior, interface, and compatibility over time."))
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }
            .padding(24)
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }
}
