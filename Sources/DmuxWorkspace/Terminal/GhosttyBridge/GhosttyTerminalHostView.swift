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

    @MainActor
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
        private var pendingGeometryNotification: DispatchWorkItem?

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
            let signature = geometrySignature
            guard signature != lastGeometrySignature else {
                return
            }
            lastGeometrySignature = signature
            pendingGeometryNotification?.cancel()
            let workItem = DispatchWorkItem { [weak self] in
                guard let self else {
                    return
                }
                self.pendingGeometryNotification = nil
                self.onGeometryChanged?()
            }
            pendingGeometryNotification = workItem
            DispatchQueue.main.async(execute: workItem)
        }

        private var geometrySignature: String {
            let frameSignature = "\(Int(frame.minX.rounded())):\(Int(frame.minY.rounded())):\(Int(frame.width.rounded()))x\(Int(frame.height.rounded()))"
            let boundsSignature = "\(Int(bounds.width.rounded()))x\(Int(bounds.height.rounded()))"
            return "\(frameSignature)|\(boundsSignature)|\(window?.windowNumber ?? -1)|\(superview.map { ObjectIdentifier($0).debugDescription } ?? "nil")"
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
struct GhosttyTerminalPortalNativeAccessoryView: NSViewRepresentable {
    let isVisible: Bool
    let preferredSize: NSSize
    let makeAccessoryView: () -> NSView
    let updateAccessoryView: (NSView) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(accessoryView: makeAccessoryView(), preferredSize: preferredSize)
    }

    func makeNSView(context: Context) -> GhosttyTerminalPortalAccessoryAnchorView {
        let anchorView = GhosttyTerminalPortalAccessoryAnchorView(frame: .zero)
        anchorView.wantsLayer = false
        return anchorView
    }

    func updateNSView(_ nsView: GhosttyTerminalPortalAccessoryAnchorView, context: Context) {
        let coordinator = context.coordinator
        updateAccessoryView(coordinator.accessoryView)
        coordinator.visibleInUI = isVisible
        coordinator.preferredSize = preferredSize
        coordinator.boundAnchorId = ObjectIdentifier(nsView)

        nsView.onDidMoveToWindow = { [weak nsView, weak coordinator] in
            guard let nsView, let coordinator else { return }
            coordinator.bind(to: nsView, synchronizeAnchor: false)
        }
        nsView.onGeometryChanged = { [weak nsView, weak coordinator] in
            guard let nsView, let coordinator else { return }
            coordinator.bind(to: nsView, synchronizeAnchor: true)
        }

        coordinator.bind(to: nsView, synchronizeAnchor: nsView.window != nil)
    }

    static func dismantleNSView(_ nsView: GhosttyTerminalPortalAccessoryAnchorView, coordinator: Coordinator) {
        nsView.onDidMoveToWindow = nil
        nsView.onGeometryChanged = nil
        GhosttyTerminalPortalRegistry.detachAccessory(
            accessoryView: coordinator.accessoryView,
            ifOwnedByAnchorId: coordinator.boundAnchorId
        )
        coordinator.boundAnchorId = nil
    }

    @MainActor
    final class Coordinator {
        let accessoryView: NSView
        var visibleInUI = false
        var boundAnchorId: ObjectIdentifier?
        var preferredSize: NSSize

        init(accessoryView: NSView, preferredSize: NSSize) {
            self.accessoryView = accessoryView
            self.preferredSize = preferredSize
            accessoryView.translatesAutoresizingMaskIntoConstraints = true
            accessoryView.autoresizingMask = [.width, .height]
        }

        func bind(to anchorView: NSView, synchronizeAnchor: Bool) {
            guard anchorView.window != nil else {
                GhosttyTerminalPortalRegistry.detachAccessory(
                    accessoryView: accessoryView,
                    ifOwnedByAnchorId: boundAnchorId
                )
                return
            }

            GhosttyTerminalPortalRegistry.bindAccessory(
                accessoryView: accessoryView,
                to: anchorView,
                visibleInUI: visibleInUI,
                preferredSize: preferredSize
            )
            if synchronizeAnchor {
                GhosttyTerminalPortalRegistry.synchronizeAccessoryForAnchor(anchorView)
            }
        }
    }
}

final class GhosttyTerminalPortalAccessoryAnchorView: NSView {
    var onDidMoveToWindow: (() -> Void)?
    var onGeometryChanged: (() -> Void)?
    private var lastGeometrySignature: String = ""
    private var pendingGeometryNotification: DispatchWorkItem?

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

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
        let signature = geometrySignature
        guard signature != lastGeometrySignature else {
            return
        }
        lastGeometrySignature = signature
        pendingGeometryNotification?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            guard let self else {
                return
            }
            self.pendingGeometryNotification = nil
            self.onGeometryChanged?()
        }
        pendingGeometryNotification = workItem
        DispatchQueue.main.async(execute: workItem)
    }

    private var geometrySignature: String {
        let frameSignature = "\(Int(frame.minX.rounded())):\(Int(frame.minY.rounded())):\(Int(frame.width.rounded()))x\(Int(frame.height.rounded()))"
        let boundsSignature = "\(Int(bounds.width.rounded()))x\(Int(bounds.height.rounded()))"
        return "\(frameSignature)|\(boundsSignature)|\(window?.windowNumber ?? -1)|\(superview.map { ObjectIdentifier($0).debugDescription } ?? "nil")"
    }

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
    var accessoryViews: [NSView] = []

    override func hitTest(_ point: NSPoint) -> NSView? {
        for accessoryView in accessoryViews.reversed() where !accessoryView.isHidden && accessoryView.alphaValue > 0.001 {
            guard accessoryView.superview === self, accessoryView.frame.contains(point) else {
                continue
            }
            let converted = convert(point, to: accessoryView)
            if let hit = accessoryView.hitTest(converted) {
                return hit
            }
        }

        for subview in subviews.reversed() where !subview.isHidden && subview.alphaValue > 0.001 {
            guard !accessoryViews.contains(where: { $0 === subview }),
                  subview.frame.contains(point) else {
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
    private var lastNotifiedSize: NSSize = .zero

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
        let previousFrame = hostedView.frame
        if hostedView.frame != bounds {
            hostedView.frame = bounds
        }
        if previousFrame.size != bounds.size, lastNotifiedSize != bounds.size {
            lastNotifiedSize = bounds.size
            hostedView.portalDidUpdateFrame()
        }
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

    struct AccessoryEntry {
        weak var anchorView: NSView?
        let accessoryView: NSView
        var visibleInUI: Bool
        var preferredSize: NSSize
    }

    weak var window: NSWindow?
    weak var hostView: NSView?
    let overlayView: GhosttyTerminalPortalOverlayView
    private var entries: [ObjectIdentifier: Entry] = [:]
    private var accessories: [ObjectIdentifier: AccessoryEntry] = [:]
    private var closeObserver: Any?

    init(window: NSWindow, hostView: NSView) {
        self.window = window
        self.hostView = hostView
        let overlayView = GhosttyTerminalPortalOverlayView(frame: .zero)
        overlayView.translatesAutoresizingMaskIntoConstraints = true
        overlayView.autoresizingMask = [.width, .height]
        if overlayView.frame != hostView.bounds {
            overlayView.frame = hostView.bounds
        }
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
        for entry in accessories.values {
            entry.accessoryView.removeFromSuperview()
        }

        CATransaction.commit()

        entries.removeAll()
        accessories.removeAll()
        overlayView.accessoryViews = []
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

    func bindAccessory(accessoryView: NSView, to anchorView: NSView, visibleInUI: Bool, preferredSize: NSSize) {
        let accessoryId = ObjectIdentifier(accessoryView)
        accessories[accessoryId] = AccessoryEntry(
            anchorView: anchorView,
            accessoryView: accessoryView,
            visibleInUI: visibleInUI,
            preferredSize: preferredSize
        )

        if accessoryView.superview !== overlayView {
            accessoryView.removeFromSuperview()
            overlayView.addSubview(accessoryView, positioned: .above, relativeTo: nil)
        }
        if !overlayView.accessoryViews.contains(where: { $0 === accessoryView }) {
            overlayView.accessoryViews.append(accessoryView)
        }

        synchronizeAccessory(withId: accessoryId)
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

    func detachAccessory(withId accessoryId: ObjectIdentifier) {
        guard let entry = accessories.removeValue(forKey: accessoryId) else { return }
        overlayView.accessoryViews.removeAll { $0 === entry.accessoryView }
        entry.accessoryView.removeFromSuperview()
    }

    func accessoryAnchorId(for accessoryId: ObjectIdentifier) -> ObjectIdentifier? {
        accessories[accessoryId]?.anchorView.map(ObjectIdentifier.init)
    }

    func synchronizeHostedViewForAnchor(_ anchorView: NSView) {
        for hostedId in entries.keys where entries[hostedId]?.anchorView === anchorView {
            synchronizeHostedView(withId: hostedId)
        }
    }

    func synchronizeAccessoryForAnchor(_ anchorView: NSView) {
        for accessoryId in accessories.keys where accessories[accessoryId]?.anchorView === anchorView {
            synchronizeAccessory(withId: accessoryId)
        }
    }

    private func synchronizeAccessory(withId accessoryId: ObjectIdentifier) {
        guard let window, let hostView, let entry = accessories[accessoryId] else {
            return
        }

        if overlayView.superview !== hostView {
            overlayView.removeFromSuperview()
            hostView.addSubview(overlayView, positioned: .above, relativeTo: nil)
        }
        if overlayView.frame != hostView.bounds {
            overlayView.frame = hostView.bounds
        }

        guard entry.visibleInUI,
              let anchorView = entry.anchorView,
              anchorView.window === window,
              anchorView.superview != nil,
              !anchorView.bounds.isEmpty,
              !anchorView.dmuxHasHiddenAncestor else {
            entry.accessoryView.isHidden = true
            entry.accessoryView.removeFromSuperview()
            return
        }

        let anchorFrameInHost = anchorView.convert(anchorView.bounds, to: hostView).integral
        guard anchorFrameInHost.width > 1, anchorFrameInHost.height > 1 else {
            entry.accessoryView.isHidden = true
            entry.accessoryView.removeFromSuperview()
            return
        }
        let accessoryWidth = min(anchorFrameInHost.width, max(1, entry.preferredSize.width))
        let accessoryHeight = min(anchorFrameInHost.height, max(1, entry.preferredSize.height))
        let frameInHost = NSRect(
            x: anchorFrameInHost.maxX - accessoryWidth,
            y: anchorFrameInHost.maxY - accessoryHeight,
            width: accessoryWidth,
            height: accessoryHeight
        ).integral

        CATransaction.begin()
        CATransaction.setDisableActions(true)

        if entry.accessoryView.superview !== overlayView {
            overlayView.addSubview(entry.accessoryView, positioned: .above, relativeTo: nil)
        }
        if !overlayView.accessoryViews.contains(where: { $0 === entry.accessoryView }) {
            overlayView.accessoryViews.append(entry.accessoryView)
        }
        if entry.accessoryView.frame != frameInHost {
            entry.accessoryView.frame = frameInHost
        }
        entry.accessoryView.isHidden = false

        CATransaction.commit()
    }

    private func synchronizeHostedView(withId hostedId: ObjectIdentifier) {
        guard let window, let hostView, let entry = entries[hostedId] else {
            return
        }

        if overlayView.superview !== hostView {
            overlayView.removeFromSuperview()
            hostView.addSubview(overlayView, positioned: .above, relativeTo: nil)
        }
        if overlayView.frame != hostView.bounds {
            overlayView.frame = hostView.bounds
        }

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
        let wasMounted = entry.mountedView.superview === overlayView
        let wasHosted = entry.hostedView.superview === entry.mountedView
        if !wasMounted {
            overlayView.addSubview(entry.mountedView)
        }
        if !wasHosted {
            entry.hostedView.removeFromSuperview()
            entry.mountedView.addSubview(entry.hostedView)
        }
        if entry.mountedView.frame != frameInHost {
            entry.mountedView.frame = frameInHost
        }
        positionMountedViewBelowAccessoriesIfNeeded(entry.mountedView)
        entry.mountedView.isHidden = false
        entry.hostedView.isHidden = false

        CATransaction.commit()

        if !wasMounted || !wasHosted {
            entry.hostedView.portalDidUpdateFrame()
        }

        if previousFrame != frameInHost {
            hostView.setNeedsDisplay(previousFrame.union(frameInHost))
        }
    }

    private func positionMountedViewBelowAccessoriesIfNeeded(_ mountedView: NSView) {
        guard let mountedIndex = overlayView.subviews.firstIndex(where: { $0 === mountedView }) else {
            overlayView.addSubview(mountedView)
            return
        }

        guard let firstAccessory = overlayView.subviews.first(where: { candidate in
            accessories.keys.contains(ObjectIdentifier(candidate))
        }) else {
            return
        }

        guard let firstAccessoryIndex = overlayView.subviews.firstIndex(where: { $0 === firstAccessory }),
              mountedIndex > firstAccessoryIndex else {
            return
        }

        overlayView.addSubview(mountedView, positioned: .below, relativeTo: firstAccessory)
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

    static func bindAccessory(accessoryView: NSView, to anchorView: NSView, visibleInUI: Bool, preferredSize: NSSize) {
        guard let window = anchorView.window else {
            accessoryView.removeFromSuperview()
            return
        }

        portal(for: window).bindAccessory(
            accessoryView: accessoryView,
            to: anchorView,
            visibleInUI: visibleInUI,
            preferredSize: preferredSize
        )
    }

    static func detachAccessory(accessoryView: NSView, ifOwnedByAnchorId ownerAnchorId: ObjectIdentifier?) {
        guard let ownerAnchorId else {
            return
        }

        guard let window = accessoryView.window ?? accessoryView.superview?.window else {
            accessoryView.removeFromSuperview()
            return
        }

        let accessoryId = ObjectIdentifier(accessoryView)
        let portal = portal(for: window)
        guard portal.accessoryAnchorId(for: accessoryId) == ownerAnchorId else {
            return
        }
        portal.detachAccessory(withId: accessoryId)
    }

    static func synchronizeForAnchor(_ anchorView: NSView) {
        guard let window = anchorView.window else { return }
        portal(for: window).synchronizeHostedViewForAnchor(anchorView)
    }

    static func synchronizeAccessoryForAnchor(_ anchorView: NSView) {
        guard let window = anchorView.window else { return }
        portal(for: window).synchronizeAccessoryForAnchor(anchorView)
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
