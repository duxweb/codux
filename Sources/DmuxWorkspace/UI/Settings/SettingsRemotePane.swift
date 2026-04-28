import CoreImage.CIFilterBuiltins
import SwiftUI

struct RemoteSettingsPane: View {
    let model: AppModel
    @ObservedObject private var remoteHostService: RemoteHostService
    @State private var serverURL = ""
    @State private var showingPairingQRCode = false
    @State private var activePendingPairing: RemoteHostService.PendingPairing?
    @FocusState private var serverURLFocused: Bool

    init(model: AppModel) {
        self.model = model
        self.remoteHostService = model.remoteHostService
    }
    @State private var refreshToken = UUID()

    private func commitServerURLIfChanged() {
        let trimmed = currentServerURL
        guard trimmed != model.appSettings.remote.serverURL else { return }
        model.updateRemoteServerURL(trimmed)
        refreshToken = UUID()
    }

    var body: some View {
        Form {
            serverSection

            if hasConfiguredServerURL {
                devicesSection
            } else {
                Section {
                    Text(String(localized: "settings.remote.configure_hint", defaultValue: "Configure a relay server URL before pairing mobile devices.", bundle: .module))
                        .foregroundStyle(.secondary)
                }
            }
        }
        .onAppear {
            serverURL = model.appSettings.remote.serverURL
            model.remoteHostService.refreshDevices()
        }
        .onChange(of: remoteHostService.snapshot.pairing) { _, pairing in
            if pairing == nil {
                showingPairingQRCode = false
            }
        }
        .onChange(of: remoteHostService.snapshot.pendingPairings) { _, pairings in
            guard let pending = pairings.first else {
                activePendingPairing = nil
                return
            }
            showingPairingQRCode = false
            activePendingPairing = pending
        }
        .sheet(isPresented: $showingPairingQRCode, onDismiss: {
            if remoteHostService.snapshot.pairing != nil {
                model.remoteHostService.cancelPairing()
            }
        }) {
            PairingQRCodeSheet(pairing: remoteHostService.snapshot.pairing)
                .frame(width: 420, height: 420)
        }
        .alert(
            String(localized: "settings.remote.confirm_pairing_title", defaultValue: "Confirm Device Pairing", bundle: .module),
            isPresented: Binding(
                get: { activePendingPairing != nil },
                set: { if !$0 { activePendingPairing = nil } }
            ),
            presenting: activePendingPairing
        ) { pending in
            Button(String(localized: "settings.remote.reject_pairing", defaultValue: "Reject", bundle: .module), role: .destructive) {
                model.remoteHostService.rejectPairing(pending.id)
                refreshToken = UUID()
            }
            Button(String(localized: "settings.remote.confirm_pairing", defaultValue: "Confirm", bundle: .module)) {
                model.remoteHostService.confirmPairing(pending.id)
                refreshToken = UUID()
            }
        } message: { pending in
            let code = pending.code.isEmpty ? "—" : pending.code
            Text(
                "\(String(localized: "settings.remote.device", defaultValue: "Device", bundle: .module)): \(pending.deviceName)\n\(String(localized: "settings.remote.match_code", defaultValue: "Match code", bundle: .module)): \(code)"
            )
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var currentServerURL: String {
        serverURL.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var hasConfiguredServerURL: Bool {
        !currentServerURL.isEmpty
    }

    private var canCreatePairing: Bool {
        hasConfiguredServerURL && model.appSettings.remote.isEnabled
            && remoteHostService.snapshot.status == .connected
    }

    private var connectionStatusColor: Color {
        if !model.appSettings.remote.isEnabled || !hasConfiguredServerURL {
            return Color(nsColor: .secondaryLabelColor)
        }
        switch remoteHostService.snapshot.status {
        case .connected: return Color(hex: 0x39D98A)
        case .registering, .connecting: return AppTheme.warning
        case .failed: return Color(hex: 0xFF5E6C)
        case .stopped: return Color(nsColor: .secondaryLabelColor)
        }
    }

    private var connectionStatusLabel: String {
        if !hasConfiguredServerURL {
            return String(localized: "remote.status.not_configured", defaultValue: "Remote not configured", bundle: .module)
        }
        if !model.appSettings.remote.isEnabled {
            return String(localized: "remote.status.disabled", defaultValue: "Remote disabled", bundle: .module)
        }
        switch remoteHostService.snapshot.status {
        case .connected:
            return String(localized: "remote.status.connected_label", defaultValue: "Connected", bundle: .module)
        case .registering, .connecting:
            return String(localized: "remote.status.connecting_label", defaultValue: "Connecting", bundle: .module)
        case .failed:
            let message = remoteHostService.snapshot.message
            return message.isEmpty
                ? String(localized: "remote.status.failed_label", defaultValue: "Error", bundle: .module)
                : message
        case .stopped:
            return String(localized: "remote.status.stopped_short", defaultValue: "Remote stopped", bundle: .module)
        }
    }

    private var serverSection: some View {
        Section(String(localized: "settings.remote.server", defaultValue: "Server", bundle: .module)) {
            TextField(String(localized: "settings.remote.server_url", defaultValue: "Relay Server URL", bundle: .module), text: Binding(
                get: { serverURL },
                set: { serverURL = $0 }
            ))
            .focused($serverURLFocused)
            .onSubmit { commitServerURLIfChanged() }
            .onChange(of: serverURLFocused) { _, focused in
                if !focused { commitServerURLIfChanged() }
            }

            Toggle(String(localized: "settings.remote.enabled", defaultValue: "Enable Remote Host", bundle: .module), isOn: Binding(
                get: { model.appSettings.remote.isEnabled },
                set: {
                    model.updateRemoteConnection(serverURL: currentServerURL, enabled: $0)
                    refreshToken = UUID()
                }
            ))

            HStack(spacing: 8) {
                Circle()
                    .fill(connectionStatusColor)
                    .frame(width: 7, height: 7)
                Text(connectionStatusLabel)
                    .font(.system(size: 11.5))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .id(refreshToken)
                Spacer()
                Button(String(localized: "settings.remote.reconnect", defaultValue: "Reconnect", bundle: .module)) {
                    commitServerURLIfChanged()
                    model.remoteHostService.start()
                    refreshToken = UUID()
                }
                .disabled(!hasConfiguredServerURL)
            }
        }
    }

    private var devicesSection: some View {
        Section {
            if remoteHostService.snapshot.devices.isEmpty {
                Text(String(localized: "settings.remote.no_devices", defaultValue: "No paired devices.", bundle: .module))
                    .foregroundStyle(.secondary)
            } else {
                ForEach(remoteHostService.snapshot.devices) { device in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(device.name)
                            Text(device.id)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button(String(localized: "settings.remote.revoke", defaultValue: "Remove", bundle: .module), role: .destructive) {
                            model.remoteHostService.revokeDevice(device.id)
                            refreshToken = UUID()
                        }
                    }
                }
            }
        } header: {
            HStack {
                Text(String(localized: "settings.remote.devices", defaultValue: "Devices", bundle: .module))
                Spacer()
                Button {
                    model.remoteHostService.createPairing()
                    showingPairingQRCode = true
                    refreshToken = UUID()
                } label: {
                    Label(
                        String(localized: "settings.remote.create_pairing", defaultValue: "Create Pairing QR", bundle: .module),
                        systemImage: "qrcode")
                }
                .labelStyle(.iconOnly)
                .help(String(localized: "settings.remote.create_pairing", defaultValue: "Create Pairing QR", bundle: .module))
                .disabled(!canCreatePairing)

                Button {
                    model.remoteHostService.refreshDevices()
                    refreshToken = UUID()
                } label: {
                    Label(
                        String(localized: "settings.remote.refresh_devices", defaultValue: "Refresh Devices", bundle: .module),
                        systemImage: "arrow.clockwise")
                }
                .labelStyle(.iconOnly)
                .help(String(localized: "settings.remote.refresh_devices", defaultValue: "Refresh Devices", bundle: .module))
            }
        }
    }
}

private struct PairingQRCodeSheet: View {
    let pairing: RemotePairingInfo?

    var body: some View {
        VStack(spacing: 0) {
            Text(String(localized: "settings.remote.pairing", defaultValue: "Pairing", bundle: .module))
                .font(.title3.weight(.semibold))
                .padding(.top, 24)

            Spacer(minLength: 18)

            if let pairing {
                QRCodeView(text: pairing.qrPayload)
                    .frame(width: 220, height: 220)
                    .padding(10)
                    .background(
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .fill(Color.white)
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .strokeBorder(Color(nsColor: .separatorColor).opacity(0.3), lineWidth: 0.5)
                    )

                Spacer(minLength: 16)

                VStack(spacing: 6) {
                    Text(String(localized: "settings.remote.waiting_scan", defaultValue: "Waiting for mobile scan…", bundle: .module))
                        .foregroundStyle(.secondary)
                        .font(.system(size: 12))
                    Text(String(localized: "settings.remote.scan_code", defaultValue: "Scan code", bundle: .module))
                        .foregroundStyle(.secondary)
                        .font(.system(size: 11))
                    Text(pairing.code)
                        .font(.system(.title3, design: .monospaced).weight(.semibold))
                        .tracking(2)
                        .foregroundStyle(.primary)
                }
            } else {
                Spacer(minLength: 0)
                VStack(spacing: 12) {
                    ProgressView()
                        .controlSize(.large)
                    Text(String(localized: "settings.remote.creating_pairing", defaultValue: "Creating pairing QR…", bundle: .module))
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
            }

            Spacer(minLength: 22)

            Button {
                dismiss()
            } label: {
                Text(String(localized: "common.close", defaultValue: "Close", bundle: .module))
                    .frame(minWidth: 96)
            }
            .controlSize(.large)
            .keyboardShortcut(.cancelAction)
            .padding(.bottom, 22)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.horizontal, 24)
    }

    @Environment(\.dismiss) private var dismiss
}

private struct QRCodeView: View {
    let text: String
    private let context = CIContext()
    private let filter = CIFilter.qrCodeGenerator()

    var body: some View {
        if let image = makeImage() {
            Image(nsImage: image)
                .interpolation(.none)
                .resizable()
        } else {
            RoundedRectangle(cornerRadius: 12).fill(.gray.opacity(0.15))
        }
    }

    private func makeImage() -> NSImage? {
        filter.message = Data(text.utf8)
        guard let output = filter.outputImage else { return nil }
        let scaled = output.transformed(by: CGAffineTransform(scaleX: 8, y: 8))
        guard let cgImage = context.createCGImage(scaled, from: scaled.extent) else { return nil }
        return NSImage(cgImage: cgImage, size: NSSize(width: 160, height: 160))
    }
}
