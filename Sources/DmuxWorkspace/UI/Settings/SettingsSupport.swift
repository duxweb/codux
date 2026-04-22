import AppKit
import SwiftUI

struct SettingsWindowConfigurator: NSViewRepresentable {
    let title: String
    let contentSize: NSSize

    func makeNSView(context: Context) -> NSView {
        ConfigView(title: title, contentSize: contentSize)
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard let configView = nsView as? ConfigView else {
            return
        }
        configView.title = title
        configView.contentSize = contentSize
        configView.applyWindowConfigurationIfNeeded()
    }

    private final class ConfigView: NSView {
        var title: String
        var contentSize: NSSize
        private var lastAppliedFrameSize: NSSize?

        init(title: String, contentSize: NSSize) {
            self.title = title
            self.contentSize = contentSize
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) {
            fatalError("init(coder:) has not been implemented")
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            applyWindowConfigurationIfNeeded()
            DispatchQueue.main.async { [weak self] in
                self?.applyWindowConfigurationIfNeeded()
            }
        }

        func applyWindowConfigurationIfNeeded() {
            guard let window else {
                return
            }
            window.identifier = AppWindowIdentifier.settings
            applyStandardWindowChrome(window, title: title, toolbarStyle: .preference)

            let targetContentRect = NSRect(origin: .zero, size: contentSize)
            let targetFrame = window.frameRect(forContentRect: targetContentRect)
            let targetFrameSize = targetFrame.size

            guard lastAppliedFrameSize != targetFrameSize
                || abs(window.frame.size.width - targetFrameSize.width) > 0.5
                || abs(window.frame.size.height - targetFrameSize.height) > 0.5 else {
                return
            }

            var nextFrame = window.frame
            nextFrame.origin.y += nextFrame.height - targetFrameSize.height
            nextFrame.size = targetFrameSize

            lastAppliedFrameSize = targetFrameSize
            window.setFrame(nextFrame, display: true, animate: false)
        }
    }
}

struct RefreshIntervalOption {
    let seconds: TimeInterval

    @MainActor
    func title(model: AppModel) -> String {
        let intValue = Int(seconds)
        if intValue % 60 == 0 {
            let minutes = intValue / 60
            return String(format: String(localized: "settings.interval.minutes_format", defaultValue: "%@ min", bundle: .module), "\(minutes)")
        }
        return String(format: String(localized: "settings.interval.seconds_format", defaultValue: "%@ sec", bundle: .module), "\(intValue)")
    }

    static let gitOptions = [30, 60, 120, 300, 600].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
    static let aiOptions = [60, 120, 180, 300, 600].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
    static let backgroundAIOptions = [300, 600, 900, 1800].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
    static let performanceMonitorOptions = [1, 2, 3, 5, 10].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
    static let petReminderOptions = [900, 1800, 2700, 3600, 5400, 7200, 10800].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
}
