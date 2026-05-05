import SwiftUI

// MARK: - Popover

struct PetPopoverView: View {
    let model: AppModel
    let sleeping: Bool
    let petStats: PetStats
    let onInheritConfirmed: (() -> Void)?
    @State private var isEditingName = false
    @State private var showsArchiveConfirmation = false

    private var petStore: PetStore { model.petStore }
    private var species: PetSpecies { petStore.species }
    private var path: PetEvoPath { petStore.currentEvoPath() }
    private var currentXP: Int { petStore.currentExperienceTokens }
    private var info: PetProgressInfo { PetProgressInfo(totalXP: currentXP) }
    private var identity: PetResolvedIdentity { info.stage.resolvedIdentity(for: species, evoPath: path, customName: petStore.customName) }
    private var displayName: String { identity.title }
    private var hasLegacy: Bool { !petStore.legacy.isEmpty }
    private var maxStatValue: Int { PetStats.traitDisplayMaxValue }
    private var widestStatText: String { petStore.currentStats.widestCompactValueText }

    var body: some View {
        AnyView(
            mainContent
                .frame(width: 300)
                .alert(petL("pet.archive.alert.title", "Archive Current Pet"), isPresented: $showsArchiveConfirmation) {
                    Button(petL("common.cancel", "Cancel"), role: .cancel) {}
                    Button(petL("pet.archive.confirm", "Confirm Archive")) {
                        petStore.archiveCurrentPet()
                        onInheritConfirmed?()
                    }
                } message: {
                    Text(petL("pet.archive.alert.message", "Archive this pet into the dex and choose a new companion."))
                }
        )
    }

    private var mainContent: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(spacing: 0) {
                PetSpriteView(
                    species: species,
                    stage: info.stage,
                    sleeping: sleeping,
                    staticMode: model.appSettings.pet.staticMode,
                    displaySize: 110
                )
                .frame(width: 110, height: 110)

                Spacer().frame(height: 14)

                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    if isEditingName {
                        TextField(
                            petL("pet.name.placeholder", "Pet Name"),
                            text: Binding(
                                get: { petStore.customName },
                                set: { petStore.rename($0) }
                            )
                        )
                        .font(.system(size: 17, weight: .bold, design: .rounded))
                        .textFieldStyle(.roundedBorder)
                        .frame(maxWidth: 140)
                    } else {
                        Group {
                            if let subtitle = identity.subtitle {
                                Text(identity.title)
                                    .font(.system(size: 17, weight: .bold, design: .rounded))
                                    .foregroundStyle(.primary)
                                + Text("  \(subtitle)")
                                    .font(.system(size: 14, weight: .medium, design: .rounded))
                                    .foregroundStyle(.secondary)
                            } else {
                                Text(identity.title)
                                    .font(.system(size: 17, weight: .bold, design: .rounded))
                                    .foregroundStyle(.primary)
                            }
                        }
                        .onTapGesture { isEditingName = true }
                    }
                }

                Spacer().frame(height: 8)

                Text(petStore.currentStats.personaTag)
                    .font(.system(size: 12, weight: .medium, design: .rounded))
                    .foregroundStyle(info.stage.accentColor)
                    .padding(.horizontal, 9)
                    .padding(.vertical, 4)
                    .background(Capsule().fill(info.stage.accentColor.opacity(0.14)))

                Spacer().frame(height: 10)

                Text("Lv.\(info.level)")
                    .font(.system(size: 26, weight: .black, design: .rounded))
                    .foregroundStyle(.primary)
                    .contentTransition(.numericText())
            }
            .frame(maxWidth: .infinity)
            .padding(.top, 18)
            .padding(.bottom, 14)
            .padding(.horizontal, 14)
            .overlay(alignment: .topTrailing) {
                Button {
                    PetDexWindowPresenter.show(model: model)
                } label: {
                    Image(systemName: "books.vertical.fill")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(info.stage.accentColor.opacity(0.8))
                        .frame(width: 28, height: 28)
                        .background(
                            RoundedRectangle(cornerRadius: 7, style: .continuous)
                                .fill(info.stage.accentColor.opacity(0.1))
                        )
                }
                .buttonStyle(.plain)
                .help(petL("pet.dex.open", "Open Dex"))
                .padding(14)
            }

                    Divider().padding(.horizontal, 14)

                    VStack(spacing: 6) {
                        HStack {
                            Text(petL("pet.xp.label", "Experience"))
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.secondary)
                            Spacer()
                            Text("\(petFormatCompactNumber(info.xpInLevel)) / \(petFormatCompactNumber(info.xpForLevel))")
                                .font(.system(size: 12, weight: .medium, design: .rounded))
                                .foregroundStyle(.secondary)
                        }
                        GeometryReader { geo in
                            ZStack(alignment: .leading) {
                                RoundedRectangle(cornerRadius: 4, style: .continuous)
                                    .fill(info.stage.accentColor.opacity(0.15))
                                    .frame(height: 7)
                                RoundedRectangle(cornerRadius: 4, style: .continuous)
                                    .fill(
                                        LinearGradient(
                                            colors: [info.stage.accentColor, info.stage.accentColor.adjustingBrightness(-0.15)],
                                            startPoint: .leading,
                                            endPoint: .trailing
                                        )
                                    )
                                    .frame(width: geo.size.width * info.xpProgress, height: 7)
                                    .animation(.easeInOut(duration: 0.4), value: info.xpProgress)
                            }
                        }
                        .frame(height: 7)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 12)

                    Divider().padding(.horizontal, 14)

                    VStack(alignment: .leading, spacing: 7) {
                        Text(petL("pet.stats.title", "Traits"))
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(.tertiary)
                        PetAttributeRow(
                            emoji: "🧠",
                            name: petL("pet.attribute.wisdom", "Wisdom"),
                            value: petStore.currentStats.wisdom,
                            maxValue: maxStatValue,
                            color: info.stage.accentColor,
                            widestValueText: widestStatText,
                            helpText: petL("pet.attribute.wisdom.help", "Reflects deeper, denser sessions with more substantial exchanges.")
                        )
                        PetAttributeRow(
                            emoji: "🔥",
                            name: petL("pet.attribute.chaos", "Chaos"),
                            value: petStore.currentStats.chaos,
                            maxValue: maxStatValue,
                            color: Color(hex: 0xFF6030),
                            widestValueText: widestStatText,
                            helpText: petL("pet.attribute.chaos.help", "Reflects fast, jumpy, high-tempo sessions with frequent bursts.")
                        )
                        PetAttributeRow(
                            emoji: "🌙",
                            name: petL("pet.attribute.night", "Night"),
                            value: petStore.currentStats.night,
                            maxValue: maxStatValue,
                            color: Color(hex: 0x6060CC),
                            widestValueText: widestStatText,
                            helpText: petL("pet.attribute.night.help", "Reflects how much of your recent activity leans into late-night hours.")
                        )
                        PetAttributeRow(
                            emoji: "💪",
                            name: petL("pet.attribute.stamina", "Stamina"),
                            value: petStore.currentStats.stamina,
                            maxValue: maxStatValue,
                            color: Color(hex: 0x20A060),
                            widestValueText: widestStatText,
                            helpText: petL("pet.attribute.stamina.help", "Reflects steadier sessions that hold focus across more sustained back-and-forth.")
                        )
                        PetAttributeRow(
                            emoji: "🩹",
                            name: petL("pet.attribute.empathy", "Empathy"),
                            value: petStore.currentStats.empathy,
                            maxValue: maxStatValue,
                            color: Color(hex: 0xE060A0),
                            widestValueText: widestStatText,
                            helpText: petL("pet.attribute.empathy.help", "Reflects patient repair work, iterative debugging, and careful refinement.")
                        )
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 12)

                    Divider().padding(.horizontal, 14)

                    PetStatCell(label: petL("pet.total_xp", "Total XP"), value: petFormatCompactNumber(info.totalXP))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 10)

                    if info.hasUnlockedArchive {
                        Divider().padding(.horizontal, 14)

                        HStack {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(petL("pet.archive.unlocked", "Archive Unlocked"))
                                    .font(.system(size: 13, weight: .semibold, design: .rounded))
                                Text(petL("pet.archive.unlocked.detail", "Archive unlocks at level 100, but levels can keep increasing."))
                                    .font(.system(size: 12))
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button(petL("pet.archive.action", "Archive")) {
                                showsArchiveConfirmation = true
                            }
                            .buttonStyle(.borderedProminent)
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 10)
                    }

                    if hasLegacy {
                        Divider().padding(.horizontal, 14)

                        VStack(alignment: .leading, spacing: 8) {
                            Text(petL("pet.archive.history", "Archive History"))
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.tertiary)

                            ForEach(Array(petStore.legacy.prefix(2))) { record in
                                PetLegacyRow(
                                    record: record,
                                    stage: PetProgressInfo(totalXP: record.totalXP).stage
                                )
                            }
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 10)
                    }
        }
    }
}

private struct PetLegacyRow: View {
    let record: PetLegacyRecord
    let stage: PetStage

    private var identity: PetResolvedIdentity {
        record.resolvedIdentity(for: stage)
    }

    private var subtitleText: String {
        let parts = [identity.subtitle, "\(petFormatCompactNumber(record.totalXP)) XP"]
            .compactMap { $0 }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: record.species.placeholderSymbol)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(width: 16)

            VStack(alignment: .leading, spacing: 2) {
                Text(identity.title)
                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                Text(subtitleText)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }

            Spacer()
        }
        .padding(.vertical, 4)
    }
}

struct PetClaimDialogState: Equatable {
    var selectedOption: PetClaimOption
}

struct PetClaimResult {
    let option: PetClaimOption
    let customName: String
}

@MainActor
enum PetClaimDialogPresenter {
    static func present(
        dialog: PetClaimDialogState,
        staticMode: Bool,
        parentWindow: NSWindow,
        completion: @escaping (PetClaimResult?) -> Void
    ) {
        let controller = PetClaimDialogController(dialog: dialog, staticMode: staticMode)
        controller.beginSheet(for: parentWindow, completion: completion)
    }
}

@MainActor
private final class PetClaimDialogViewModel: ObservableObject {
    @Published var selectedOption: PetClaimOption
    @Published var customName: String = ""

    init(dialog: PetClaimDialogState) {
        self.selectedOption = dialog.selectedOption
    }
}

private struct PetClaimDialogView: View {
    static let dialogWidth: CGFloat = 640
    static let dialogHeight: CGFloat = 420

    @ObservedObject var viewModel: PetClaimDialogViewModel
    let staticMode: Bool
    let onCancel: () -> Void
    let onConfirm: () -> Void

    private var selectedOption: PetClaimOption { viewModel.selectedOption }

    private var accentColor: Color {
        switch selectedOption {
        case .voidcat:
            return Color(hex: 0x2A80FF)
        case .rusthound:
            return Color(hex: 0xFF6030)
        case .goose:
            return Color(hex: 0xF5DEB3)
        case .chaossprite:
            return Color(hex: 0xFF4FA3)
        case .code:
            return Color(hex: 0x2F8FFF)
        case .sheep:
            return Color(hex: 0xF28FB8)
        case .ox:
            return Color(hex: 0xF3B43F)
        case .dragon:
            return Color(hex: 0xE04435)
        case .phoenix:
            return Color(hex: 0xFF7A22)
        case .dolphin:
            return Color(hex: 0x1E9BFF)
        case .penguin:
            return Color(hex: 0x5C6D85)
        case .panda:
            return Color(hex: 0x6A6F78)
        case .random:
            return .purple
        }
    }

    private var description: String {
        switch selectedOption {
        case .voidcat:
            return petL("pet.claim.voidcat.description", "A black cat that loves long thoughts in the middle of the night.")
        case .rusthound:
            return petL("pet.claim.rusthound.description", "It falls over, gets up again, and keeps going anyway.")
        case .goose:
            return petL("pet.claim.goose.description", "A steady companion with a relaxed rhythm.")
        case .chaossprite:
            return petL("pet.claim.chaossprite.description", "A bright companion for messy ideas and sudden turns.")
        case .code:
            return petL("pet.claim.code.description", "A coding companion that keeps your terminal alive.")
        case .sheep:
            return petL("pet.claim.sheep.description", "A soft companion that keeps long work calm.")
        case .ox:
            return petL("pet.claim.ox.description", "A steady companion for reliable, grounded progress.")
        case .dragon:
            return petL("pet.claim.dragon.description", "A brave companion for big pushes and hot paths.")
        case .phoenix:
            return petL("pet.claim.phoenix.description", "A bright companion that makes every restart feel lighter.")
        case .dolphin:
            return petL("pet.claim.dolphin.description", "A nimble companion that moves quickly through ideas.")
        case .penguin:
            return petL("pet.claim.penguin.description", "A calm companion for focused work and cool heads.")
        case .panda:
            return petL("pet.claim.panda.description", "A round, gentle companion that keeps the pace steady.")
        case .random:
            return petL("pet.claim.random.description", "Let Codux choose one companion for you.")
        }
    }

    @ViewBuilder
    private func optionRow(_ option: PetClaimOption) -> some View {
        let isSelected = viewModel.selectedOption == option
        Button {
            withAnimation(.easeInOut(duration: 0.15)) {
                viewModel.selectedOption = option
            }
        } label: {
            HStack(spacing: 10) {
                PetClaimPreview(option: option, staticMode: staticMode)
                    .frame(width: 44, height: 44)
                VStack(alignment: .leading, spacing: 2) {
                    Text(option.title)
                        .font(.system(size: 13, weight: .semibold, design: .rounded))
                        .foregroundStyle(.primary)
                    Text(option.subtitle)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if isSelected {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(Color.accentColor)
                        .font(.system(size: 14))
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(isSelected ? Color.accentColor.opacity(0.1) : Color.primary.opacity(0.035))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .stroke(isSelected ? Color.accentColor.opacity(0.6) : Color.clear, lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                ZStack {
                    Circle()
                        .fill(
                            LinearGradient(
                                colors: [accentColor.opacity(0.3), accentColor.opacity(0.1)],
                                startPoint: .topLeading,
                                endPoint: .bottomTrailing
                            )
                        )
                    Image(systemName: "pawprint.fill")
                        .font(.system(size: 18, weight: .bold))
                        .foregroundStyle(accentColor)
                }
                .frame(width: 40, height: 40)
                VStack(alignment: .leading, spacing: 2) {
                    Text(petL("pet.claim.dialog.title", "Choose a Pet"))
                        .font(.system(size: 15, weight: .bold, design: .rounded))
                    Text(petL("pet.claim.dialog.subtitle", "A tiny coding partner for your workspace"))
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            .padding(.horizontal, 20)
            .padding(.top, 20)
            .padding(.bottom, 16)

            Divider()

            HStack(alignment: .top, spacing: 0) {
                ScrollView(.vertical, showsIndicators: true) {
                    VStack(spacing: 8) {
                        ForEach(PetClaimOption.allCases) { option in
                            optionRow(option)
                        }
                    }
                    .padding(14)
                }
                .frame(width: 220)

                Divider()

                VStack(alignment: .center, spacing: 12) {
                    ZStack {
                        Circle()
                            .fill(accentColor.opacity(0.08))
                            .frame(width: 100, height: 100)
                        if let species = selectedOption.previewSpecies {
                            PetSpriteView(
                                species: species,
                                stage: .companion,
                                staticMode: true,
                                displaySize: 84
                            )
                            .frame(width: 84, height: 84)
                        } else {
                            Image(systemName: "sparkles")
                                .font(.system(size: 32, weight: .semibold))
                                .foregroundStyle(accentColor.opacity(0.7))
                        }
                    }
                    .animation(.easeInOut(duration: 0.2), value: selectedOption)

                    Text(selectedOption.title)
                        .font(.system(size: 15, weight: .bold, design: .rounded))
                        .foregroundStyle(.primary)

                    Text(description)
                        .font(.system(size: 12, design: .rounded))
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .lineLimit(3)
                        .frame(maxWidth: .infinity)

                    Spacer(minLength: 0)

                    Divider()

                    VStack(alignment: .leading, spacing: 6) {
                        Text(petL("pet.claim.name.label", "Give it a name (optional)"))
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(.secondary)
                        TextField(
                            petL("pet.claim.name.placeholder", "Leave empty to use the species name"),
                            text: Binding(
                                get: { viewModel.customName },
                                set: { viewModel.customName = $0 }
                            )
                        )
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 13))
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity)
            }

            Divider()

            HStack {
                Button(petL("common.cancel", "Cancel")) { onCancel() }
                    .keyboardShortcut(.escape)
                Spacer()
                Button(petL("pet.claim.confirm", "Confirm Claim")) { onConfirm() }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.return)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 14)
        }
        .frame(width: Self.dialogWidth, height: Self.dialogHeight)
    }
}

@MainActor
private final class PetClaimDialogController: AppDialogController<PetClaimResult> {
    private let viewModel: PetClaimDialogViewModel

    init(dialog: PetClaimDialogState, staticMode: Bool) {
        self.viewModel = PetClaimDialogViewModel(dialog: dialog)

        let panel = AppDialogPanel(
            contentRect: NSRect(
                x: 0,
                y: 0,
                width: PetClaimDialogView.dialogWidth,
                height: PetClaimDialogView.dialogHeight
            ),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        panel.title = petL("pet.claim.window.title", "Claim Pet")
        panel.isReleasedWhenClosed = false
        panel.isFloatingPanel = false
        panel.hidesOnDeactivate = false
        panel.level = .modalPanel
        panel.collectionBehavior = [.fullScreenAuxiliary]
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true
        panel.setContentSize(NSSize(width: PetClaimDialogView.dialogWidth, height: PetClaimDialogView.dialogHeight))
        panel.minSize = NSSize(width: PetClaimDialogView.dialogWidth, height: PetClaimDialogView.dialogHeight)
        panel.maxSize = NSSize(width: PetClaimDialogView.dialogWidth, height: PetClaimDialogView.dialogHeight)

        super.init(panel: panel)

        let contentView = PetClaimDialogView(
            viewModel: viewModel,
            staticMode: staticMode,
            onCancel: { [weak self] in
                self?.finish(with: .abort)
            },
            onConfirm: { [weak self] in
                guard let self else { return }
                let result = PetClaimResult(
                    option: viewModel.selectedOption,
                    customName: viewModel.customName.trimmingCharacters(in: .whitespacesAndNewlines)
                )
                self.finish(with: .continue, value: result)
            }
        )
        let hostingController = NSHostingController(rootView: contentView)
        hostingController.view.frame = NSRect(
            x: 0,
            y: 0,
            width: PetClaimDialogView.dialogWidth,
            height: PetClaimDialogView.dialogHeight
        )
        hostingController.view.autoresizingMask = [.width, .height]
        panel.contentViewController = hostingController
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
