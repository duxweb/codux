import AppKit
import SwiftUI

struct MainWorkspaceWindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        ConfigView()
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        (nsView as? ConfigView)?.applyWindowConfigurationIfNeeded()
    }

    private final class ConfigView: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            applyWindowConfigurationIfNeeded()
        }

        func applyWindowConfigurationIfNeeded() {
            guard let window else {
                return
            }
            window.identifier = AppWindowIdentifier.main
            applyImmersiveWindowChrome(window)
        }
    }
}

struct TitlebarZoomSurface: NSViewRepresentable {
    func makeNSView(context: Context) -> TitlebarZoomNSView {
        TitlebarZoomNSView()
    }

    func updateNSView(_ nsView: TitlebarZoomNSView, context: Context) {
    }
}

final class TitlebarZoomNSView: NSView {
    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var mouseDownCanMoveWindow: Bool {
        true
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func mouseUp(with event: NSEvent) {
        if event.clickCount == 2 {
            window?.performZoom(nil)
            return
        }
        super.mouseUp(with: event)
    }
}
