import SwiftUI

// MARK: - Popover

struct PetPopoverView: View {
    let model: AppModel
    let allTimeTokens: Int
    let sleeping: Bool
    let petStats: PetStats
    let onInheritConfirmed: (() -> Void)?
    @State private var isEditingName = false
    @State private var showsInheritanceConfirmation = false

    private var petStore: PetStore { model.petStore }
    private var species: PetSpecies { petStore.species }
    private var path: PetEvoPath { petStore.currentEvoPath() }
    private var currentXP: Int { petStore.currentExperienceTokens }
    private var hatchTokens: Int { petStore.currentHatchTokens }
    private var info: PetProgressInfo { PetProgressInfo(totalXP: currentXP, hatchTokens: hatchTokens, evoPath: path) }
    private var identity: PetResolvedIdentity { info.stage.resolvedIdentity(for: species, evoPath: path, customName: petStore.customName) }
    private var displayName: String { identity.title }
    private var hasLegacy: Bool { !petStore.legacy.isEmpty }
    private var maxStatValue: Int { max(1, petStore.currentStats.maxValue) }
    private var widestStatText: String { petStore.currentStats.widestCompactValueText }

    var body: some View {
        AnyView(
            mainContent
                .frame(width: 300)
                .alert(petL("pet.inherit.alert.title", "Inherit Current Pet"), isPresented: $showsInheritanceConfirmation) {
                    Button(petL("common.cancel", "Cancel"), role: .cancel) {}
                    Button(petL("pet.inherit.confirm", "Confirm Inheritance")) {
                        petStore.inheritCurrentPet()
                        onInheritConfirmed?()
                    }
                } message: {
                    Text(petL("pet.inherit.alert.message", "Inheritance will archive the current pet into the dex and return you to egg selection."))
                }
        )
    }

    private var mainContent: some View {
        Group {
            if info.isHatching {
                VStack(spacing: 0) {
                    HStack(alignment: .center, spacing: 12) {
                        PetSpriteView(
                            species: species,
                            stage: .egg,
                            staticMode: model.appSettings.pet.staticMode,
                            displaySize: 80
                        )
                        .frame(width: 80, height: 80)

                        VStack(alignment: .leading, spacing: 5) {
                            Text(displayName)
                                .font(.system(size: 16, weight: .bold, design: .rounded))
                                .foregroundStyle(.primary)
                            Text(petL("pet.stage.egg", "Hatching"))
                                .font(.system(size: 12, weight: .semibold, design: .rounded))
                                .foregroundStyle(info.stage.accentColor)
                                .padding(.horizontal, 8)
                                .padding(.vertical, 3)
                                .background(Capsule().fill(info.stage.accentColor.opacity(0.14)))
                            Text(petL("pet.hatch.description", "Hatches after reaching the token threshold"))
                                .font(.system(size: 12))
                                .foregroundStyle(.tertiary)
                        }

                        Spacer(minLength: 0)

                        VStack(spacing: 2) {
                            Text("\(Int((info.hatchProgress * 100).rounded()))%")
                                .font(.system(size: 26, weight: .black, design: .rounded))
                                .foregroundStyle(.primary)
                                .contentTransition(.numericText())
                            Text(petL("pet.hatch.progress.short", "Hatch"))
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.tertiary)
                        }
                        .frame(minWidth: 64, alignment: .center)
                    }
                    .padding(14)

                    Divider()
                        .padding(.horizontal, 14)

                    VStack(spacing: 6) {
                        HStack {
                            Text(petL("pet.hatch.progress.title", "Hatch Progress"))
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.secondary)
                            Spacer()
                            Text("\(petFormatCompactNumber(info.hatchTokens)) / \(petFormatCompactNumber(PetProgressInfo.hatchThreshold))")
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
                                            colors: [info.stage.accentColor, info.stage.accentColor.adjustingBrightness(-0.12)],
                                            startPoint: .leading,
                                            endPoint: .trailing
                                        )
                                    )
                                    .frame(width: max(0, geo.size.width * info.hatchProgress), height: 7)
                                    .animation(.easeInOut(duration: 0.4), value: info.hatchProgress)
                            }
                        }
                        .frame(height: 7)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 12)
                    .appCursor(.arrow)
                }
            } else {
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
                        PetAttributeRow(emoji: "🧠", name: petL("pet.attribute.wisdom", "Wisdom"), value: petStore.currentStats.wisdom, maxValue: maxStatValue, color: info.stage.accentColor, widestValueText: widestStatText)
                        PetAttributeRow(emoji: "🔥", name: petL("pet.attribute.chaos", "Chaos"), value: petStore.currentStats.chaos, maxValue: maxStatValue, color: Color(hex: 0xFF6030), widestValueText: widestStatText)
                        PetAttributeRow(emoji: "🌙", name: petL("pet.attribute.night", "Night"), value: petStore.currentStats.night, maxValue: maxStatValue, color: Color(hex: 0x6060CC), widestValueText: widestStatText)
                        PetAttributeRow(emoji: "💪", name: petL("pet.attribute.stamina", "Stamina"), value: petStore.currentStats.stamina, maxValue: maxStatValue, color: Color(hex: 0x20A060), widestValueText: widestStatText)
                        PetAttributeRow(emoji: "🩹", name: petL("pet.attribute.empathy", "Empathy"), value: petStore.currentStats.empathy, maxValue: maxStatValue, color: Color(hex: 0xE060A0), widestValueText: widestStatText)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 12)

                    Divider().padding(.horizontal, 14)

                    HStack(spacing: 0) {
                        PetStatCell(label: petL("pet.total_xp", "Total XP"), value: petFormatCompactNumber(info.totalXP))
                            .frame(maxWidth: .infinity)
                        Divider().frame(height: 32)
                        PetStatCell(label: petL("pet.next_stage", "Next Stage"), value: nextStageLabel(info: info))
                            .frame(maxWidth: .infinity)
                    }
                    .padding(.vertical, 10)

                    if info.hasUnlockedInheritance {
                        Divider().padding(.horizontal, 14)

                        HStack {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(petL("pet.inherit.unlocked", "Inheritance Unlocked"))
                                    .font(.system(size: 13, weight: .semibold, design: .rounded))
                                Text(petL("pet.inherit.unlocked.detail", "Inheritance unlocks at level 100, but levels can keep increasing."))
                                    .font(.system(size: 12))
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button(petL("pet.inherit.action", "Inherit")) {
                                showsInheritanceConfirmation = true
                            }
                            .buttonStyle(.borderedProminent)
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 10)
                    }

                    if hasLegacy {
                        Divider().padding(.horizontal, 14)

                        VStack(alignment: .leading, spacing: 8) {
                            Text(petL("pet.inherit.history", "Inheritance History"))
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.tertiary)

                            ForEach(Array(petStore.legacy.prefix(2))) { record in
                                PetLegacyRow(
                                    record: record,
                                    stage: PetProgressInfo(
                                        totalXP: record.totalXP,
                                        hatchTokens: PetProgressInfo.hatchThreshold,
                                        evoPath: record.evoPath
                                    ).stage
                                )
                            }
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 10)
                    }
                }
            }
        }
    }

    private func nextStageLabel(info: PetProgressInfo) -> String {
        let next: PetStage
        switch info.stage {
        case .egg:
            next = .infant
        case .infant:
            next = .child
        case .child:
            next = .adult
        case .adult:
            next = path == .pathA ? .evoA : .evoB
        case .evoA, .evoB:
            next = path == .pathA ? .megaA : .megaB
        case .megaA, .megaB:
            return petL("pet.stage.awakened_complete", "Awakened")
        }
        return next.displayName
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

struct PetEggSelectionDialogState: Equatable {
    var selectedOption: PetClaimOption
}

struct PetEggClaimResult {
    let option: PetClaimOption
    let customName: String
}

@MainActor
enum PetEggSelectionDialogPresenter {
    static func present(
        dialog: PetEggSelectionDialogState,
        staticMode: Bool,
        parentWindow: NSWindow,
        completion: @escaping (PetEggClaimResult?) -> Void
    ) {
        let controller = PetEggSelectionDialogController(dialog: dialog, staticMode: staticMode)
        controller.beginSheet(for: parentWindow, completion: completion)
    }
}

@MainActor
private final class PetEggSelectionDialogViewModel: ObservableObject {
    @Published var selectedOption: PetClaimOption
    @Published var customName: String = ""

    init(dialog: PetEggSelectionDialogState) {
        self.selectedOption = dialog.selectedOption
    }
}

private struct PetEggSelectionDialogView: View {
    static let dialogWidth: CGFloat = 640
    static let dialogHeight: CGFloat = 420

    @ObservedObject var viewModel: PetEggSelectionDialogViewModel
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
            return petL("pet.claim.goose.description", "It doesn't fully get what's happening, but it also doesn't mind.")
        case .random:
            return petL("pet.claim.random.description", "Leave it to fate. A random egg can hatch the hidden species.")
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
                PetClaimEggPreview(option: option, staticMode: staticMode)
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
                    Text(petL("pet.claim.dialog.title", "Choose a Pet Egg"))
                        .font(.system(size: 15, weight: .bold, design: .rounded))
                    Text(petL("pet.claim.dialog.subtitle", "A tiny coding partner that hatches by your side"))
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
                VStack(spacing: 8) {
                    ForEach(PetClaimOption.allCases) { option in
                        optionRow(option)
                    }
                }
                .padding(14)
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
                                stage: .egg,
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
private final class PetEggSelectionDialogController: AppDialogController<PetEggClaimResult> {
    private let viewModel: PetEggSelectionDialogViewModel

    init(dialog: PetEggSelectionDialogState, staticMode: Bool) {
        self.viewModel = PetEggSelectionDialogViewModel(dialog: dialog)

        let panel = AppDialogPanel(
            contentRect: NSRect(
                x: 0,
                y: 0,
                width: PetEggSelectionDialogView.dialogWidth,
                height: PetEggSelectionDialogView.dialogHeight
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
        panel.setContentSize(NSSize(width: PetEggSelectionDialogView.dialogWidth, height: PetEggSelectionDialogView.dialogHeight))
        panel.minSize = NSSize(width: PetEggSelectionDialogView.dialogWidth, height: PetEggSelectionDialogView.dialogHeight)
        panel.maxSize = NSSize(width: PetEggSelectionDialogView.dialogWidth, height: PetEggSelectionDialogView.dialogHeight)

        super.init(panel: panel)

        let contentView = PetEggSelectionDialogView(
            viewModel: viewModel,
            staticMode: staticMode,
            onCancel: { [weak self] in
                self?.finish(with: .abort)
            },
            onConfirm: { [weak self] in
                guard let self else { return }
                let result = PetEggClaimResult(
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
            width: PetEggSelectionDialogView.dialogWidth,
            height: PetEggSelectionDialogView.dialogHeight
        )
        hostingController.view.autoresizingMask = [.width, .height]
        panel.contentViewController = hostingController
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
