import Foundation
import Sparkle

@MainActor
final class AppUpdaterService {
    static var isSupportedConfiguration: Bool {
        guard let info = Bundle.main.infoDictionary else {
            return false
        }
        let feedURL = (info["SUFeedURL"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let publicKey = (info["SUPublicEDKey"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return !feedURL.isEmpty && !publicKey.isEmpty
    }

    private let controller: SPUStandardUpdaterController
    private let isEnabled: Bool
    private var didStartUpdater = false
    private var didPerformInitialBackgroundCheck = false
    private var canCheckObservation: NSKeyValueObservation?

    var onCanCheckForUpdatesChanged: ((Bool) -> Void)?

    init(isEnabled: Bool) {
        self.isEnabled = isEnabled
        self.controller = SPUStandardUpdaterController(
            startingUpdater: false,
            updaterDelegate: nil,
            userDriverDelegate: nil
        )

        canCheckObservation = controller.updater.observe(\.canCheckForUpdates, options: [.initial, .new]) { [weak self] updater, _ in
            guard let self else {
                return
            }
            Task { @MainActor in
                self.onCanCheckForUpdatesChanged?(updater.canCheckForUpdates)
            }
        }
    }

    var canCheckForUpdates: Bool {
        isEnabled && controller.updater.canCheckForUpdates
    }

    var isAvailable: Bool {
        isEnabled
    }

    func checkForUpdates() throws {
        try startIfNeeded()
        controller.checkForUpdates(nil)
    }

    func performLaunchBackgroundCheckIfNeeded() {
        guard isEnabled, !didPerformInitialBackgroundCheck else {
            return
        }

        do {
            try startIfNeeded()
        } catch {
            return
        }

        didPerformInitialBackgroundCheck = true
        guard controller.updater.automaticallyChecksForUpdates else {
            return
        }
        controller.updater.checkForUpdatesInBackground()
    }

    private func startIfNeeded() throws {
        guard isEnabled, !didStartUpdater else {
            return
        }
        try controller.updater.start()
        didStartUpdater = true
    }
}
