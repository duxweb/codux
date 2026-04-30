import Foundation
import IOKit.ps
import IOKit.pwr_mgt

@MainActor
final class SleepPreventionService {
    static let shared = SleepPreventionService()

    private var mode: AppSleepPreventionMode = .off
    private var assertionID: IOPMAssertionID?
    private var refreshTimer: Timer?
    private let isUsingExternalPower: () -> Bool
    private let createAssertion: (String) -> IOPMAssertionID?
    private let releaseAssertion: (IOPMAssertionID) -> Void
    private let logger: AppDebugLog?

    init(
        isUsingExternalPower: @escaping () -> Bool = SleepPreventionService.defaultIsUsingExternalPower,
        createAssertion: @escaping (String) -> IOPMAssertionID? = SleepPreventionService.defaultCreateAssertion(reason:),
        releaseAssertion: @escaping (IOPMAssertionID) -> Void = SleepPreventionService.defaultReleaseAssertion(_:),
        logger: AppDebugLog? = AppDebugLog.shared
    ) {
        self.isUsingExternalPower = isUsingExternalPower
        self.createAssertion = createAssertion
        self.releaseAssertion = releaseAssertion
        self.logger = logger
    }

    func configure(mode: AppSleepPreventionMode) {
        self.mode = mode
        refreshAssertion()
        configureTimer()
    }

    func stop() {
        refreshTimer?.invalidate()
        refreshTimer = nil
        releaseCurrentAssertion()
        mode = .off
    }

    var isPreventingSleep: Bool {
        assertionID != nil
    }

    func refreshAssertion() {
        if shouldHoldAssertion {
            ensureAssertion()
        } else {
            releaseCurrentAssertion()
        }
    }

    private var shouldHoldAssertion: Bool {
        switch mode {
        case .always:
            return true
        case .off:
            return false
        case .powerAdapterOnly:
            return isUsingExternalPower()
        }
    }

    private func configureTimer() {
        refreshTimer?.invalidate()
        refreshTimer = nil
        guard mode == .powerAdapterOnly else {
            return
        }
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.refreshAssertion()
            }
        }
    }

    private func ensureAssertion() {
        guard assertionID == nil else {
            return
        }
        let reason = String(
            localized: "sleep_prevention.assertion_reason",
            defaultValue: "Codux is keeping this Mac awake.",
            bundle: .module
        )
        guard let nextID = createAssertion(reason) else {
            logger?.log("sleep-prevention", "assertion-create failed mode=\(mode.rawValue)")
            return
        }
        assertionID = nextID
        logger?.log("sleep-prevention", "assertion-active mode=\(mode.rawValue) id=\(nextID)")
    }

    private func releaseCurrentAssertion() {
        guard let assertionID else {
            return
        }
        releaseAssertion(assertionID)
        logger?.log("sleep-prevention", "assertion-released mode=\(mode.rawValue) id=\(assertionID)")
        self.assertionID = nil
    }

    private nonisolated static func defaultCreateAssertion(reason: String) -> IOPMAssertionID? {
        var assertionID = IOPMAssertionID(0)
        let result = IOPMAssertionCreateWithName(
            kIOPMAssertionTypeNoIdleSleep as CFString,
            IOPMAssertionLevel(kIOPMAssertionLevelOn),
            reason as CFString,
            &assertionID
        )
        guard result == kIOReturnSuccess else {
            return nil
        }
        return assertionID
    }

    private nonisolated static func defaultReleaseAssertion(_ assertionID: IOPMAssertionID) {
        IOPMAssertionRelease(assertionID)
    }

    private nonisolated static func defaultIsUsingExternalPower() -> Bool {
        if let powerSource = IOPSGetProvidingPowerSourceType(nil)?.takeRetainedValue() as String? {
            return powerSource == (kIOPSACPowerValue as String)
        }
        return IOPSCopyExternalPowerAdapterDetails()?.takeRetainedValue() != nil
    }
}
