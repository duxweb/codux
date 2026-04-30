import AppKit
import SwiftUI

@MainActor
enum PetDesktopWindowPresenter {
    private static var controller: NSWindowController?
    private static let windowSize = NSSize(width: 330, height: 185)

    static func sync(model: AppModel) {
        guard model.appSettings.pet.enabled,
              model.appSettings.pet.desktopWidgetEnabled,
              model.petStore.isClaimed else {
            close()
            return
        }
        show(model: model)
    }

    static func show(model: AppModel) {
        if let window = controller?.window {
            updateContent(model: model)
            if !window.isVisible {
                window.orderFrontRegardless()
            }
            return
        }

        let initialFrame = DesktopPetPlacementStore.initialFrame(size: windowSize)
        let window = DesktopPetPanel(
            contentRect: initialFrame,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        window.identifier = AppWindowIdentifier.desktopPet
        window.backgroundColor = .clear
        window.isOpaque = false
        window.hasShadow = false
        window.level = .floating
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        window.isReleasedWhenClosed = false
        window.isMovableByWindowBackground = false
        window.hidesOnDeactivate = false
        let hosting = NSHostingController(
            rootView: PetDesktopWidgetView(
                model: model,
                initialBubbleCorner: DesktopPetPlacementStore.bubbleCorner(for: initialFrame)
            )
        )
        makeHostingViewTransparent(hosting.view)
        window.contentViewController = hosting
        makeHostingViewTransparent(window.contentView)

        let controller = NSWindowController(window: window)
        self.controller = controller
        window.orderFrontRegardless()
    }

    static func close() {
        controller?.close()
        controller = nil
    }

    private static func updateContent(model: AppModel) {
        guard let hosting = controller?.contentViewController as? NSHostingController<PetDesktopWidgetView> else {
            return
        }
        let bubbleCorner = controller?.window.map { DesktopPetPlacementStore.bubbleCorner(for: $0.frame) } ?? .topLeading
        hosting.rootView = PetDesktopWidgetView(model: model, initialBubbleCorner: bubbleCorner)
        makeHostingViewTransparent(hosting.view)
    }

    private static func makeHostingViewTransparent(_ view: NSView?) {
        guard let view else {
            return
        }
        view.wantsLayer = true
        view.layer?.backgroundColor = NSColor.clear.cgColor
    }
}

private final class DesktopPetPanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

private enum DesktopPetBubbleCorner {
    case topLeading
    case topTrailing
    case bottomLeading
    case bottomTrailing
}

private extension DesktopPetBubbleCorner {
    private static let bubbleAnchorHeight: CGFloat = 78

    var bubbleAlignment: Alignment {
        switch self {
        case .topLeading: return .topLeading
        case .topTrailing: return .topTrailing
        case .bottomLeading: return .bottomLeading
        case .bottomTrailing: return .bottomTrailing
        }
    }

    var petAlignment: Alignment {
        switch self {
        case .topLeading: return .bottomTrailing
        case .topTrailing: return .bottomLeading
        case .bottomLeading: return .topTrailing
        case .bottomTrailing: return .topLeading
        }
    }

    var tailSide: DesktopPetBubbleTailSide {
        switch self {
        case .topLeading, .bottomLeading: return .right
        case .topTrailing, .bottomTrailing: return .left
        }
    }

    var tailVerticalPosition: PixelSpeechBubbleTailVerticalPosition {
        switch self {
        case .topLeading, .topTrailing: return .bottom
        case .bottomLeading, .bottomTrailing: return .top
        }
    }

    func topAlignmentGuide(_ dimensions: ViewDimensions) -> CGFloat {
        switch self {
        case .topLeading, .topTrailing:
            return dimensions.height - Self.bubbleAnchorHeight
        case .bottomLeading, .bottomTrailing:
            return dimensions[.top]
        }
    }
}

private struct PetDesktopWidgetView: View {
    let model: AppModel
    @State private var bubbleCorner: DesktopPetBubbleCorner
    @State private var appIsActive = NSApplication.shared.isActive
    @State private var recentActivityTick = Date()
    @State private var sleepClock = Date()

    private var petStore: PetStore { model.petStore }
    private var evoPath: PetEvoPath { petStore.currentEvoPath() }
    private var info: PetProgressInfo {
        PetProgressInfo(
            totalXP: petStore.currentExperienceTokens,
            hatchTokens: petStore.currentHatchTokens,
            evoPath: evoPath
        )
    }
    private var desktopMessage: String? {
        model.petSpeechCoordinator.currentLine?.text
    }
    private var hasAnyRunningActivity: Bool {
        model.activityByProjectID.values.contains {
            switch $0 {
            case .loading, .running, .waitingInput:
                return true
            default:
                return false
            }
        }
    }
    private var isSleeping: Bool {
        if !appIsActive {
            return true
        }
        if hasAnyRunningActivity {
            return false
        }
        return sleepClock.timeIntervalSince(recentActivityTick) >= 30
    }

    init(model: AppModel, initialBubbleCorner: DesktopPetBubbleCorner) {
        self.model = model
        _bubbleCorner = State(initialValue: initialBubbleCorner)
    }

    var body: some View {
        ZStack {
            petSprite
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: bubbleCorner.petAlignment)
            if let desktopMessage {
                messageBubble(desktopMessage)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: bubbleCorner.bubbleAlignment)
                    .transition(.opacity.combined(with: .scale(scale: 0.96)))
            }
        }
        .frame(width: 330, height: 185)
        .background(Color.clear)
        .onAppear {
            recentActivityTick = Date()
            sleepClock = recentActivityTick
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            appIsActive = true
            recentActivityTick = Date()
            sleepClock = recentActivityTick
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didResignActiveNotification)) { _ in
            appIsActive = false
            recentActivityTick = Date()
            sleepClock = recentActivityTick
        }
        .onChange(of: model.activityRenderVersion) { _, _ in
            if hasAnyRunningActivity {
                recentActivityTick = Date()
                sleepClock = recentActivityTick
            }
        }
        .onReceive(Timer.publish(every: 5, on: .main, in: .common).autoconnect()) { now in
            sleepClock = now
            if hasAnyRunningActivity {
                recentActivityTick = now
            }
        }
        .overlay(
            DesktopPetDragSurface(
                hideTitle: petL("pet.desktop.hide", "Hide Desktop Pet"),
                onFrameChanged: { frame in
                    bubbleCorner = DesktopPetPlacementStore.bubbleCorner(for: frame)
                },
                onHide: {
                    model.updatePetDesktopWidgetEnabled(false)
                    PetDesktopWindowPresenter.sync(model: model)
                },
                onMute: { interval in
                    model.updatePetSpeechTemporaryMuteUntil(Date().addingTimeInterval(interval))
                },
                onMuteToday: {
                    model.updatePetSpeechTemporaryMuteUntil(Calendar.current.startOfDay(for: Date()).addingTimeInterval(86_400))
                },
                onSkip: {
                    model.petSpeechCoordinator.skipCurrentLine()
                },
                onSpeakMore: {
                    model.petSpeechCoordinator.speakMoreTemporarily()
                },
                onSpeakLess: {
                    model.petSpeechCoordinator.speakLessTemporarily()
                }
            )
        )
        .contentShape(Rectangle())
        .contextMenu {
            Button(petL("pet.desktop.mute_30_minutes", "Mute 30 Minutes")) {
                model.updatePetSpeechTemporaryMuteUntil(Date().addingTimeInterval(1800))
            }
            Button(petL("pet.desktop.mute_1_hour", "Mute 1 Hour")) {
                model.updatePetSpeechTemporaryMuteUntil(Date().addingTimeInterval(3600))
            }
            Button(petL("pet.desktop.mute_today", "Mute Today")) {
                model.updatePetSpeechTemporaryMuteUntil(Calendar.current.startOfDay(for: Date()).addingTimeInterval(86_400))
            }
            Divider()
            Button(petL("pet.desktop.skip_line", "Skip Line")) {
                model.petSpeechCoordinator.skipCurrentLine()
            }
            Button(petL("pet.desktop.speak_more", "Speak More")) {
                model.petSpeechCoordinator.speakMoreTemporarily()
            }
            Button(petL("pet.desktop.speak_less", "Speak Less")) {
                model.petSpeechCoordinator.speakLessTemporarily()
            }
            Divider()
            Button(petL("pet.desktop.hide", "Hide Desktop Pet")) {
                model.updatePetDesktopWidgetEnabled(false)
                PetDesktopWindowPresenter.sync(model: model)
            }
        }
    }

    private var petSprite: some View {
        PetSpriteView(
            species: petStore.species,
            stage: info.stage,
            sleeping: isSleeping,
            staticMode: model.appSettings.pet.staticMode,
            displaySize: 128
        )
        .frame(width: 146, height: 150)
    }

    private func messageBubble(_ desktopMessage: String) -> some View {
        let corner = bubbleCorner
        return DesktopPetMessageBubble(
            text: desktopMessage,
            tailSide: corner.tailSide,
            tailVerticalPosition: corner.tailVerticalPosition
        )
        .frame(width: 184)
        .fixedSize(horizontal: false, vertical: true)
        .alignmentGuide(.top) { dimensions in
            corner.topAlignmentGuide(dimensions)
        }
        .padding(.horizontal, 2)
    }
}

private enum DesktopPetBubbleTailSide {
    case left
    case right
}

private enum PixelSpeechBubbleTailVerticalPosition {
    case top
    case center
    case bottom
}

private struct DesktopPetMessageBubble: View {
    let text: String
    let tailSide: DesktopPetBubbleTailSide
    let tailVerticalPosition: PixelSpeechBubbleTailVerticalPosition

    @Environment(\.colorScheme) private var colorScheme

    private var fillColor: Color {
        colorScheme == .dark
            ? Color(red: 0.16, green: 0.17, blue: 0.21)
            : Color(red: 0.98, green: 0.96, blue: 0.90)
    }

    private var strokeColor: Color {
        colorScheme == .dark
            ? Color(red: 0.92, green: 0.90, blue: 0.84)
            : Color(red: 0.16, green: 0.15, blue: 0.13)
    }

    private var highlightColor: Color {
        colorScheme == .dark
            ? Color.white.opacity(0.12)
            : Color.white.opacity(0.55)
    }

    private var textColor: Color {
        colorScheme == .dark
            ? Color(red: 0.94, green: 0.93, blue: 0.88)
            : Color(red: 0.18, green: 0.17, blue: 0.15)
    }

    var body: some View {
        let shape = PixelSpeechBubbleShape(
            tailSide: tailSide,
            tailVerticalPosition: tailVerticalPosition
        )
        Text(text)
            .font(.system(size: 12, weight: .bold, design: .monospaced))
            .tracking(0.4)
            .foregroundStyle(textColor)
            .lineLimit(4)
            .multilineTextAlignment(.center)
            .minimumScaleFactor(0.78)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
            .padding(.leading, tailSide == .left ? 24 : 15)
            .padding(.trailing, tailSide == .right ? 24 : 15)
            .frame(minHeight: 44)
            .background(shape.fill(fillColor))
            .overlay(
                shape
                    .inset(by: 3)
                    .stroke(highlightColor, lineWidth: 1)
            )
            .overlay(shape.stroke(strokeColor, lineWidth: 2))
    }
}

private struct PixelSpeechBubbleShape: InsettableShape {
    let tailSide: DesktopPetBubbleTailSide
    var tailVerticalPosition: PixelSpeechBubbleTailVerticalPosition = .center
    var insetAmount: CGFloat = 0

    private let pixel: CGFloat = 3

    func path(in rect: CGRect) -> Path {
        let p = pixel
        let cornerNotch = 2 * p
        let tailExtent = 3 * p
        let tailHalfHeight = 3 * p

        let body: CGRect
        switch tailSide {
        case .right:
            body = CGRect(
                x: rect.minX,
                y: rect.minY,
                width: rect.width - tailExtent,
                height: rect.height
            ).insetBy(dx: insetAmount, dy: insetAmount)
        case .left:
            body = CGRect(
                x: rect.minX + tailExtent,
                y: rect.minY,
                width: rect.width - tailExtent,
                height: rect.height
            ).insetBy(dx: insetAmount, dy: insetAmount)
        }

        var path = Path()
        let r = body
        let tailEdgeMargin = cornerNotch + tailHalfHeight + p
        let tipY: CGFloat = {
            switch tailVerticalPosition {
            case .top: return r.minY + tailEdgeMargin
            case .center: return r.midY
            case .bottom: return r.maxY - tailEdgeMargin
            }
        }()

        path.move(to: CGPoint(x: r.minX, y: r.minY + cornerNotch))
        path.addLine(to: CGPoint(x: r.minX + p, y: r.minY + cornerNotch))
        path.addLine(to: CGPoint(x: r.minX + p, y: r.minY + p))
        path.addLine(to: CGPoint(x: r.minX + cornerNotch, y: r.minY + p))
        path.addLine(to: CGPoint(x: r.minX + cornerNotch, y: r.minY))

        path.addLine(to: CGPoint(x: r.maxX - cornerNotch, y: r.minY))

        path.addLine(to: CGPoint(x: r.maxX - cornerNotch, y: r.minY + p))
        path.addLine(to: CGPoint(x: r.maxX - p, y: r.minY + p))
        path.addLine(to: CGPoint(x: r.maxX - p, y: r.minY + cornerNotch))
        path.addLine(to: CGPoint(x: r.maxX, y: r.minY + cornerNotch))

        if tailSide == .right {
            path.addLine(to: CGPoint(x: r.maxX, y: tipY - tailHalfHeight))
            path.addLine(to: CGPoint(x: r.maxX + p, y: tipY - tailHalfHeight))
            path.addLine(to: CGPoint(x: r.maxX + p, y: tipY - 2 * p))
            path.addLine(to: CGPoint(x: r.maxX + 2 * p, y: tipY - 2 * p))
            path.addLine(to: CGPoint(x: r.maxX + 2 * p, y: tipY - p))
            path.addLine(to: CGPoint(x: r.maxX + 3 * p, y: tipY - p))
            path.addLine(to: CGPoint(x: r.maxX + 3 * p, y: tipY + p))
            path.addLine(to: CGPoint(x: r.maxX + 2 * p, y: tipY + p))
            path.addLine(to: CGPoint(x: r.maxX + 2 * p, y: tipY + 2 * p))
            path.addLine(to: CGPoint(x: r.maxX + p, y: tipY + 2 * p))
            path.addLine(to: CGPoint(x: r.maxX + p, y: tipY + tailHalfHeight))
            path.addLine(to: CGPoint(x: r.maxX, y: tipY + tailHalfHeight))
        }

        path.addLine(to: CGPoint(x: r.maxX, y: r.maxY - cornerNotch))

        path.addLine(to: CGPoint(x: r.maxX - p, y: r.maxY - cornerNotch))
        path.addLine(to: CGPoint(x: r.maxX - p, y: r.maxY - p))
        path.addLine(to: CGPoint(x: r.maxX - cornerNotch, y: r.maxY - p))
        path.addLine(to: CGPoint(x: r.maxX - cornerNotch, y: r.maxY))

        path.addLine(to: CGPoint(x: r.minX + cornerNotch, y: r.maxY))

        path.addLine(to: CGPoint(x: r.minX + cornerNotch, y: r.maxY - p))
        path.addLine(to: CGPoint(x: r.minX + p, y: r.maxY - p))
        path.addLine(to: CGPoint(x: r.minX + p, y: r.maxY - cornerNotch))
        path.addLine(to: CGPoint(x: r.minX, y: r.maxY - cornerNotch))

        if tailSide == .left {
            path.addLine(to: CGPoint(x: r.minX, y: tipY + tailHalfHeight))
            path.addLine(to: CGPoint(x: r.minX - p, y: tipY + tailHalfHeight))
            path.addLine(to: CGPoint(x: r.minX - p, y: tipY + 2 * p))
            path.addLine(to: CGPoint(x: r.minX - 2 * p, y: tipY + 2 * p))
            path.addLine(to: CGPoint(x: r.minX - 2 * p, y: tipY + p))
            path.addLine(to: CGPoint(x: r.minX - 3 * p, y: tipY + p))
            path.addLine(to: CGPoint(x: r.minX - 3 * p, y: tipY - p))
            path.addLine(to: CGPoint(x: r.minX - 2 * p, y: tipY - p))
            path.addLine(to: CGPoint(x: r.minX - 2 * p, y: tipY - 2 * p))
            path.addLine(to: CGPoint(x: r.minX - p, y: tipY - 2 * p))
            path.addLine(to: CGPoint(x: r.minX - p, y: tipY - tailHalfHeight))
            path.addLine(to: CGPoint(x: r.minX, y: tipY - tailHalfHeight))
        }

        path.addLine(to: CGPoint(x: r.minX, y: r.minY + cornerNotch))
        path.closeSubpath()
        return path
    }

    func inset(by amount: CGFloat) -> PixelSpeechBubbleShape {
        var copy = self
        copy.insetAmount += amount
        return copy
    }
}

private struct DesktopPetDragSurface: NSViewRepresentable {
    let hideTitle: String
    let onFrameChanged: (NSRect) -> Void
    let onHide: () -> Void
    let onMute: (TimeInterval) -> Void
    let onMuteToday: () -> Void
    let onSkip: () -> Void
    let onSpeakMore: () -> Void
    let onSpeakLess: () -> Void

    func makeNSView(context: Context) -> NSView {
        DesktopPetDragView(
            hideTitle: hideTitle,
            onFrameChanged: onFrameChanged,
            onHide: onHide,
            onMute: onMute,
            onMuteToday: onMuteToday,
            onSkip: onSkip,
            onSpeakMore: onSpeakMore,
            onSpeakLess: onSpeakLess
        )
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard let view = nsView as? DesktopPetDragView else {
            return
        }
        view.hideTitle = hideTitle
        view.onFrameChanged = onFrameChanged
        view.onHide = onHide
        view.onMute = onMute
        view.onMuteToday = onMuteToday
        view.onSkip = onSkip
        view.onSpeakMore = onSpeakMore
        view.onSpeakLess = onSpeakLess
    }
}

private final class DesktopPetDragView: NSView {
    var hideTitle: String
    var onFrameChanged: (NSRect) -> Void
    var onHide: () -> Void
    var onMute: (TimeInterval) -> Void
    var onMuteToday: () -> Void
    var onSkip: () -> Void
    var onSpeakMore: () -> Void
    var onSpeakLess: () -> Void

    private var dragStartMouseLocation: CGPoint?
    private var dragStartWindowOrigin: CGPoint?

    init(
        hideTitle: String,
        onFrameChanged: @escaping (NSRect) -> Void,
        onHide: @escaping () -> Void,
        onMute: @escaping (TimeInterval) -> Void,
        onMuteToday: @escaping () -> Void,
        onSkip: @escaping () -> Void,
        onSpeakMore: @escaping () -> Void,
        onSpeakLess: @escaping () -> Void
    ) {
        self.hideTitle = hideTitle
        self.onFrameChanged = onFrameChanged
        self.onHide = onHide
        self.onMute = onMute
        self.onMuteToday = onMuteToday
        self.onSkip = onSkip
        self.onSpeakMore = onSpeakMore
        self.onSpeakLess = onSpeakLess
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var acceptsFirstResponder: Bool { true }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func mouseDown(with event: NSEvent) {
        dragStartMouseLocation = NSEvent.mouseLocation
        dragStartWindowOrigin = window?.frame.origin
    }

    override func mouseDragged(with event: NSEvent) {
        guard let window,
              let dragStartMouseLocation,
              let dragStartWindowOrigin else {
            return
        }
        let mouseLocation = NSEvent.mouseLocation
        let nextOrigin = CGPoint(
            x: dragStartWindowOrigin.x + mouseLocation.x - dragStartMouseLocation.x,
            y: dragStartWindowOrigin.y + mouseLocation.y - dragStartMouseLocation.y
        )
        window.setFrameOrigin(nextOrigin)
        onFrameChanged(window.frame)
    }

    override func mouseUp(with event: NSEvent) {
        if let window {
            DesktopPetPlacementStore.save(window.frame.origin)
            onFrameChanged(window.frame)
        }
        dragStartMouseLocation = nil
        dragStartWindowOrigin = nil
    }

    override func rightMouseDown(with event: NSEvent) {
        let menu = NSMenu()
        menu.addItem(menuItem(petL("pet.desktop.mute_30_minutes", "Mute 30 Minutes"), #selector(mute30Minutes)))
        menu.addItem(menuItem(petL("pet.desktop.mute_1_hour", "Mute 1 Hour"), #selector(muteOneHour)))
        menu.addItem(menuItem(petL("pet.desktop.mute_today", "Mute Today"), #selector(muteToday)))
        menu.addItem(.separator())
        menu.addItem(menuItem(petL("pet.desktop.skip_line", "Skip Line"), #selector(skipCurrentLine)))
        menu.addItem(menuItem(petL("pet.desktop.speak_more", "Speak More"), #selector(speakMore)))
        menu.addItem(menuItem(petL("pet.desktop.speak_less", "Speak Less"), #selector(speakLess)))
        menu.addItem(.separator())
        let item = menuItem(hideTitle, #selector(hideDesktopPet))
        menu.addItem(item)
        menu.popUp(positioning: item, at: convert(event.locationInWindow, from: nil), in: self)
    }

    private func menuItem(_ title: String, _ action: Selector) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
        item.target = self
        return item
    }

    @objc private func hideDesktopPet() {
        onHide()
    }

    @objc private func mute30Minutes() {
        onMute(1800)
    }

    @objc private func muteOneHour() {
        onMute(3600)
    }

    @objc private func muteToday() {
        onMuteToday()
    }

    @objc private func skipCurrentLine() {
        onSkip()
    }

    @objc private func speakMore() {
        onSpeakMore()
    }

    @objc private func speakLess() {
        onSpeakLess()
    }
}

private enum DesktopPetPlacementStore {
    private static let xKey = "pet.desktop_widget.origin_x"
    private static let yKey = "pet.desktop_widget.origin_y"

    static func initialFrame(size: NSSize) -> NSRect {
        let origin = savedOrigin() ?? defaultOrigin(size: size)
        return NSRect(origin: clamped(origin, size: size), size: size)
    }

    static func bubbleCorner(for frame: NSRect) -> DesktopPetBubbleCorner {
        guard let screen = screen(containing: frame) else {
            return .topLeading
        }
        let onRightHalf = frame.midX > screen.midX
        let onTopHalf = frame.midY > screen.midY
        switch (onRightHalf, onTopHalf) {
        case (true, true): return .bottomLeading
        case (true, false): return .topLeading
        case (false, true): return .bottomTrailing
        case (false, false): return .topTrailing
        }
    }

    static func save(_ origin: CGPoint) {
        UserDefaults.standard.set(origin.x, forKey: xKey)
        UserDefaults.standard.set(origin.y, forKey: yKey)
    }

    private static func savedOrigin() -> CGPoint? {
        let defaults = UserDefaults.standard
        guard defaults.object(forKey: xKey) != nil,
              defaults.object(forKey: yKey) != nil else {
            return nil
        }
        return CGPoint(
            x: defaults.double(forKey: xKey),
            y: defaults.double(forKey: yKey)
        )
    }

    private static func defaultOrigin(size: NSSize) -> CGPoint {
        let frame = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        return CGPoint(
            x: frame.maxX - size.width - 42,
            y: frame.minY + 110
        )
    }

    private static func clamped(_ origin: CGPoint, size: NSSize) -> CGPoint {
        let screens = NSScreen.screens.map(\.visibleFrame)
        guard !screens.isEmpty else {
            return origin
        }
        if let screen = screen(containing: NSRect(origin: origin, size: size)) {
            return CGPoint(
                x: min(max(origin.x, screen.minX), screen.maxX - size.width),
                y: min(max(origin.y, screen.minY), screen.maxY - size.height)
            )
        }
        return defaultOrigin(size: size)
    }

    private static func screen(containing frame: NSRect) -> NSRect? {
        NSScreen.screens
            .map(\.visibleFrame)
            .max { lhs, rhs in
                lhs.intersection(frame).area < rhs.intersection(frame).area
            }
    }
}

private extension NSRect {
    var area: CGFloat {
        guard !isNull, width > 0, height > 0 else {
            return 0
        }
        return width * height
    }
}
