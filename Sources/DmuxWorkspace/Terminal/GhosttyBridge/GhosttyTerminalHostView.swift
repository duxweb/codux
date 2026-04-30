import AppKit
import Foundation
import GhosttyTerminal
import QuartzCore
import SwiftUI

@MainActor
struct GhosttyTerminalHostView: NSViewRepresentable {
    let session: TerminalSession
    let environment: [(String, String)]
    let terminalBackgroundPreset: AppTerminalBackgroundPreset
    let backgroundColorPreset: AppBackgroundColorPreset
    let terminalFontSize: Int
    let isFocused: Bool
    let isVisible: Bool
    let showsInactiveOverlay: Bool
    let shouldFocus: Bool
    var onInteraction: (() -> Void)? = nil
    var onFocusConsumed: (() -> Void)? = nil
    var onStartupSucceeded: (() -> Void)? = nil
    var onStartupFailure: ((String) -> Void)? = nil
    var onLoadingStateChanged: ((Bool) -> Void)? = nil

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> HostContainerView {
        let container = HostContainerView(frame: .zero)
        container.wantsLayer = false
        return container
    }

    func updateNSView(_ nsView: HostContainerView, context: Context) {
        let view = GhosttyTerminalRegistry.shared.containerView(
            for: session,
            environment: environment,
            terminalBackgroundPreset: terminalBackgroundPreset,
            backgroundColorPreset: backgroundColorPreset,
            terminalFontSize: terminalFontSize,
            isFocused: isFocused,
            isVisible: isVisible,
            showsInactiveOverlay: showsInactiveOverlay,
            onInteraction: onInteraction,
            onFocusConsumed: onFocusConsumed,
            onStartupSucceeded: onStartupSucceeded,
            onStartupFailure: onStartupFailure,
            onLoadingStateChanged: onLoadingStateChanged
        )
        let coordinator = context.coordinator
        let previousContainerView = coordinator.containerView
        coordinator.containerView = view
        coordinator.bindGeneration &+= 1
        let generation = coordinator.bindGeneration
        let bindVisibility = isVisible
        coordinator.boundAnchorId = ObjectIdentifier(nsView)

        let bindHostedView: (Bool) -> Void = { [weak nsView, weak view, weak coordinator] synchronizeAnchor in
            guard let nsView, let view, let coordinator else { return }
            guard coordinator.bindGeneration == generation else { return }
            guard nsView.window != nil else {
                GhosttyTerminalPortalRegistry.detach(
                    hostedView: view,
                    ifOwnedByAnchorId: coordinator.boundAnchorId
                )
                return
            }
            GhosttyTerminalPortalRegistry.bind(hostedView: view, to: nsView, visibleInUI: bindVisibility)
            if synchronizeAnchor {
                GhosttyTerminalPortalRegistry.synchronizeForAnchor(nsView)
            }
        }

        nsView.onDidMoveToWindow = {
            bindHostedView(false)
        }
        nsView.onGeometryChanged = {
            guard nsView.window != nil else {
                GhosttyTerminalPortalRegistry.detach(
                    hostedView: view,
                    ifOwnedByAnchorId: coordinator.boundAnchorId
                )
                return
            }
            GhosttyTerminalPortalRegistry.synchronizeForAnchor(nsView)
        }

        if nsView.window != nil {
            if previousContainerView !== view {
                GhosttyTerminalPortalRegistry.bind(hostedView: view, to: nsView, visibleInUI: isVisible)
                GhosttyTerminalPortalRegistry.synchronizeForAnchor(nsView)
            } else {
                GhosttyTerminalPortalRegistry.updateEntryVisibility(for: view, visibleInUI: isVisible)
            }
        } else {
            GhosttyTerminalPortalRegistry.updateEntryVisibility(for: view, visibleInUI: isVisible)
        }

        if shouldFocus, coordinator.lastShouldFocus == false {
            DispatchQueue.main.async {
                view.focusTerminal()
            }
        }
        coordinator.lastShouldFocus = shouldFocus
    }

    final class Coordinator {
        weak var containerView: GhosttyTerminalContainerView?
        var bindGeneration: UInt64 = 0
        var lastShouldFocus = false
        var boundAnchorId: ObjectIdentifier?
    }

    final class HostContainerView: NSView {
        var onDidMoveToWindow: (() -> Void)?
        var onGeometryChanged: (() -> Void)?
        private var lastGeometrySignature: String = ""
        private var geometryNotificationScheduled = false

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            onDidMoveToWindow?()
            notifyGeometryChangedIfNeeded()
        }

        override func viewDidMoveToSuperview() {
            super.viewDidMoveToSuperview()
            notifyGeometryChangedIfNeeded()
        }

        override func layout() {
            super.layout()
            notifyGeometryChangedIfNeeded()
        }

        private func notifyGeometryChangedIfNeeded() {
            let signature = "\(frame.debugDescription)|\(bounds.debugDescription)|\(window?.windowNumber ?? -1)|\(superview.map { ObjectIdentifier($0).debugDescription } ?? "nil")"
            guard signature != lastGeometrySignature else {
                return
            }
            lastGeometrySignature = signature
            guard geometryNotificationScheduled == false else {
                return
            }
            geometryNotificationScheduled = true
            DispatchQueue.main.async { [weak self] in
                guard let self else {
                    return
                }
                self.geometryNotificationScheduled = false
                self.onGeometryChanged?()
            }
        }
    }

    static func dismantleNSView(_ nsView: HostContainerView, coordinator: Coordinator) {
        coordinator.bindGeneration &+= 1
        nsView.onDidMoveToWindow = nil
        nsView.onGeometryChanged = nil
        if let containerView = coordinator.containerView {
            GhosttyTerminalPortalRegistry.detach(
                hostedView: containerView,
                ifOwnedByAnchorId: coordinator.boundAnchorId
            )
        }
        coordinator.containerView = nil
        coordinator.boundAnchorId = nil
    }
}

struct GhosttyTerminalSessionResources {
    let processBridge: GhosttyPTYProcessBridge
    let controller: TerminalController
    let hasStartedProcess: Bool
    let hasReceivedInitialOutput: Bool
    let pendingFocusRequest: Bool
    let hasReportedStartupFailure: Bool
}
@MainActor
enum GhosttyPortalHostRegistry {
    private final class WeakHostBox {
        weak var view: NSView?

        init(view: NSView) {
            self.view = view
        }
    }

    private static var hostViewByWindowId: [ObjectIdentifier: WeakHostBox] = [:]

    static func register(hostView: NSView, for window: NSWindow) {
        hostViewByWindowId[ObjectIdentifier(window)] = WeakHostBox(view: hostView)
    }

    static func unregister(hostView: NSView, for window: NSWindow) {
        let key = ObjectIdentifier(window)
        guard let existing = hostViewByWindowId[key]?.view, existing === hostView else {
            return
        }
        hostViewByWindowId[key] = nil
    }

    static func hostView(for window: NSWindow) -> NSView? {
        let key = ObjectIdentifier(window)
        if let view = hostViewByWindowId[key]?.view {
            return view
        }
        hostViewByWindowId[key] = nil
        return nil
    }
}
private final class GhosttyTerminalPortalOverlayView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        for subview in subviews.reversed() where !subview.isHidden && subview.alphaValue > 0.001 {
            guard subview.frame.contains(point) else {
                continue
            }
            let converted = convert(point, to: subview)
            if let hit = subview.hitTest(converted) {
                return hit
            }
        }
        return nil
    }
}

private final class GhosttyTerminalPortalMountedView: NSView {
    let hostedView: GhosttyTerminalContainerView

    init(hostedView: GhosttyTerminalContainerView) {
        self.hostedView = hostedView
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = true
        autoresizingMask = []
        wantsLayer = true
        layer?.masksToBounds = true
        layer?.backgroundColor = NSColor.clear.cgColor
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, alphaValue > 0.001 else {
            return nil
        }
        guard bounds.contains(point) else {
            return nil
        }

        if hostedView.isReadyForInteraction, let event = NSApp.currentEvent {
            switch event.type {
            case .leftMouseDown, .rightMouseDown, .otherMouseDown:
                hostedView.prepareForPointerInteraction()
            default:
                break
            }
        }

        let converted = convert(point, to: hostedView)
        return hostedView.hitTest(converted) ?? hostedView
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func mouseDown(with event: NSEvent) {
        hostedView.forwardMouseDown(event)
    }

    override func rightMouseDown(with event: NSEvent) {
        hostedView.forwardRightMouseDown(event)
    }

    override func otherMouseDown(with event: NSEvent) {
        hostedView.forwardOtherMouseDown(event)
    }

    override func scrollWheel(with event: NSEvent) {
        hostedView.forwardScrollWheel(event)
    }

    override func layout() {
        super.layout()
        if hostedView.superview !== self {
            addSubview(hostedView)
        }
        if hostedView.frame != bounds {
            hostedView.frame = bounds
        }
        hostedView.portalDidUpdateFrame()
    }
}

@MainActor
private final class GhosttyWindowPortal {
    struct Entry {
        weak var anchorView: NSView?
        let hostedView: GhosttyTerminalContainerView
        let mountedView: GhosttyTerminalPortalMountedView
        var visibleInUI: Bool
    }

    weak var window: NSWindow?
    weak var hostView: NSView?
    let overlayView: GhosttyTerminalPortalOverlayView
    private var entries: [ObjectIdentifier: Entry] = [:]
    private var closeObserver: Any?

    init(window: NSWindow, hostView: NSView) {
        self.window = window
        self.hostView = hostView
        let overlayView = GhosttyTerminalPortalOverlayView(frame: .zero)
        overlayView.translatesAutoresizingMaskIntoConstraints = true
        overlayView.autoresizingMask = [.width, .height]
        overlayView.frame = hostView.bounds
        overlayView.wantsLayer = false
        self.overlayView = overlayView

        hostView.addSubview(overlayView, positioned: .above, relativeTo: nil)

        closeObserver = NotificationCenter.default.addObserver(
            forName: NSWindow.willCloseNotification,
            object: window,
            queue: .main
        ) { [weak window] _ in
            guard let window else { return }
            Task { @MainActor in
                GhosttyTerminalPortalRegistry.removePortal(for: window)
            }
        }
    }

    func tearDown(retainedHostedIds: Set<ObjectIdentifier>) {
        if let closeObserver {
            NotificationCenter.default.removeObserver(closeObserver)
            self.closeObserver = nil
        }

        // Wrap all view-hierarchy mutations in a single disabled-animation
        // CATransaction. Removing a Metal-backed view while a drawFrame
        // is in flight (under an open CATransaction) corrupts the
        // os_unfair_lock that Ghostty uses to guard the renderer, causing
        // _os_unfair_lock_corruption_abort. Disabling actions prevents
        // implicit animations from opening nested transactions.
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        for (hostedId, entry) in entries {
            if retainedHostedIds.contains(hostedId) {
                // This hosted view is about to be rebound to a new window.
                // Do NOT touch it here — the new window's bind() call will
                // reparent it. Hiding or removing it would detach the Metal
                // surface and trigger the lock-corruption crash.
            } else {
                // Truly discarded: hide and remove from the hierarchy.
                entry.hostedView.isHidden = true
                entry.hostedView.removeFromSuperview()
            }
            entry.mountedView.isHidden = true
            entry.mountedView.removeFromSuperview()
        }

        CATransaction.commit()

        entries.removeAll()
        overlayView.removeFromSuperview()
    }

    func bind(hostedView: GhosttyTerminalContainerView, to anchorView: NSView, visibleInUI: Bool) {
        let hostedId = ObjectIdentifier(hostedView)
        let mountedView: GhosttyTerminalPortalMountedView
        if let existing = entries[hostedId]?.mountedView {
            mountedView = existing
        } else {
            mountedView = GhosttyTerminalPortalMountedView(hostedView: hostedView)
        }
        entries[hostedId] = Entry(
            anchorView: anchorView,
            hostedView: hostedView,
            mountedView: mountedView,
            visibleInUI: visibleInUI
        )

        if mountedView.superview !== overlayView {
            mountedView.removeFromSuperview()
            overlayView.addSubview(mountedView)
        }

        if hostedView.superview !== mountedView {
            hostedView.removeFromSuperview()
            mountedView.addSubview(hostedView)
        }

        synchronizeHostedView(withId: hostedId)
    }

    func detachHostedView(withId hostedId: ObjectIdentifier) {
        guard var entry = entries[hostedId] else { return }
        entry.anchorView = nil
        entry.visibleInUI = false
        // Do NOT remove hostedView from mountedView here. We only detach the
        // mounted wrapper from the overlay so temporary rebinds keep the
        // terminal/surface alive without leaving stale pixels behind.
        entry.mountedView.isHidden = true
        if entry.mountedView.superview != nil {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            entry.mountedView.removeFromSuperview()
            CATransaction.commit()
        }
        entries[hostedId] = entry
    }

    func updateEntryVisibility(for hostedId: ObjectIdentifier, visibleInUI: Bool) {
        guard var entry = entries[hostedId] else { return }
        entry.visibleInUI = visibleInUI
        entries[hostedId] = entry
        synchronizeHostedView(withId: hostedId)
    }

    func synchronizeHostedViewForAnchor(_ anchorView: NSView) {
        for hostedId in entries.keys where entries[hostedId]?.anchorView === anchorView {
            synchronizeHostedView(withId: hostedId)
        }
    }

    private func synchronizeHostedView(withId hostedId: ObjectIdentifier) {
        guard let window, let hostView, let entry = entries[hostedId] else {
            return
        }

        if overlayView.superview !== hostView {
            overlayView.removeFromSuperview()
            hostView.addSubview(overlayView, positioned: .above, relativeTo: nil)
        }
        overlayView.frame = hostView.bounds

        guard entry.visibleInUI,
              let anchorView = entry.anchorView,
              anchorView.window === window,
              anchorView.superview != nil,
              !anchorView.bounds.isEmpty,
              !anchorView.dmuxHasHiddenAncestor else {
            entry.hostedView.isHidden = true
            entry.mountedView.isHidden = true
            if entry.mountedView.superview != nil {
                CATransaction.begin()
                CATransaction.setDisableActions(true)
                entry.mountedView.removeFromSuperview()
                CATransaction.commit()
                hostView.setNeedsDisplay(hostView.bounds)
            }
            return
        }

        let frameInHost = anchorView.convert(anchorView.bounds, to: hostView).integral
        guard frameInHost.width > 1, frameInHost.height > 1 else {
            entry.hostedView.isHidden = true
            entry.mountedView.isHidden = true
            if entry.mountedView.superview != nil {
                CATransaction.begin()
                CATransaction.setDisableActions(true)
                entry.mountedView.removeFromSuperview()
                CATransaction.commit()
                hostView.setNeedsDisplay(hostView.bounds)
            }
            return
        }

        // Guard all view-hierarchy mutations and frame changes inside a
        // disabled-action CATransaction so they land atomically between
        // Metal drawFrame calls, avoiding os_unfair_lock corruption.
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        let previousFrame = entry.mountedView.frame
        if entry.mountedView.superview !== overlayView {
            overlayView.addSubview(entry.mountedView)
        }
        if entry.hostedView.superview !== entry.mountedView {
            entry.hostedView.removeFromSuperview()
            entry.mountedView.addSubview(entry.hostedView)
        }
        if entry.mountedView.frame != frameInHost {
            entry.mountedView.frame = frameInHost
        }
        entry.mountedView.isHidden = false
        entry.hostedView.isHidden = false

        CATransaction.commit()

        entry.hostedView.portalDidUpdateFrame()

        if previousFrame != frameInHost {
            hostView.setNeedsDisplay(previousFrame.union(frameInHost))
        }
    }
}

@MainActor
enum GhosttyTerminalPortalRegistry {
    private static var portalsByWindowId: [ObjectIdentifier: GhosttyWindowPortal] = [:]
    private static var hostedToWindowId: [ObjectIdentifier: ObjectIdentifier] = [:]
    private static var hostedToAnchorId: [ObjectIdentifier: ObjectIdentifier] = [:]

    static func bind(
        hostedView: GhosttyTerminalContainerView,
        to anchorView: NSView,
        visibleInUI: Bool
    ) {
        guard let window = anchorView.window else {
            detach(hostedView: hostedView)
            return
        }

        let hostedId = ObjectIdentifier(hostedView)
        let windowId = ObjectIdentifier(window)

        if let oldWindowId = hostedToWindowId[hostedId], oldWindowId != windowId {
            portalsByWindowId[oldWindowId]?.detachHostedView(withId: hostedId)
        }

        let portal = portal(for: window)
        portal.bind(hostedView: hostedView, to: anchorView, visibleInUI: visibleInUI)
        hostedToWindowId[hostedId] = windowId
        hostedToAnchorId[hostedId] = ObjectIdentifier(anchorView)
    }

    static func detach(hostedView: GhosttyTerminalContainerView) {
        let hostedId = ObjectIdentifier(hostedView)
        if let windowId = hostedToWindowId[hostedId] {
            portalsByWindowId[windowId]?.detachHostedView(withId: hostedId)
        }
    }

    static func detach(
        hostedView: GhosttyTerminalContainerView,
        ifOwnedByAnchorId ownerAnchorId: ObjectIdentifier?
    ) {
        guard let ownerAnchorId else {
            return
        }
        let hostedId = ObjectIdentifier(hostedView)
        guard hostedToAnchorId[hostedId] == ownerAnchorId else {
            return
        }
        guard let windowId = hostedToWindowId[hostedId] else {
            return
        }
        portalsByWindowId[windowId]?.detachHostedView(withId: hostedId)
    }

    static func updateEntryVisibility(for hostedView: GhosttyTerminalContainerView, visibleInUI: Bool) {
        let hostedId = ObjectIdentifier(hostedView)
        guard let windowId = hostedToWindowId[hostedId] else {
            hostedView.isHidden = !visibleInUI
            return
        }
        portalsByWindowId[windowId]?.updateEntryVisibility(for: hostedId, visibleInUI: visibleInUI)
    }

    static func synchronizeForAnchor(_ anchorView: NSView) {
        guard let window = anchorView.window else { return }
        portal(for: window).synchronizeHostedViewForAnchor(anchorView)
    }

    static func removePortal(for window: NSWindow) {
        let windowId = ObjectIdentifier(window)
        let retainedHostedIds = Set(
            hostedToWindowId.compactMap { hostedId, mappedWindowId in
                mappedWindowId == windowId ? hostedId : nil
            }
        )
        portalsByWindowId.removeValue(forKey: windowId)?.tearDown(retainedHostedIds: retainedHostedIds)
        hostedToWindowId = hostedToWindowId.filter { $0.value != windowId }
        hostedToAnchorId = hostedToAnchorId.filter { hostedId, _ in
            hostedToWindowId[hostedId] != nil
        }
    }

    private static func portal(for window: NSWindow) -> GhosttyWindowPortal {
        let windowId = ObjectIdentifier(window)
        guard let hostView = GhosttyPortalHostRegistry.hostView(for: window) ?? window.contentView else {
            fatalError("Ghostty portal host unavailable")
        }
        if let existing = portalsByWindowId[windowId] {
            if existing.hostView !== hostView {
                let retainedHostedIds = Set(
                    hostedToWindowId.compactMap { hostedId, mappedWindowId in
                        mappedWindowId == windowId ? hostedId : nil
                    }
                )
                existing.tearDown(retainedHostedIds: retainedHostedIds)
                let replacement = GhosttyWindowPortal(window: window, hostView: hostView)
                portalsByWindowId[windowId] = replacement
                return replacement
            }
            return existing
        }
        let created = GhosttyWindowPortal(window: window, hostView: hostView)
        portalsByWindowId[windowId] = created
        return created
    }
}

private extension NSView {
    var dmuxHasHiddenAncestor: Bool {
        var current: NSView? = self
        while let view = current {
            if view.isHidden || view.alphaValue <= 0.001 {
                return true
            }
            current = view.superview
        }
        return false
    }
}
