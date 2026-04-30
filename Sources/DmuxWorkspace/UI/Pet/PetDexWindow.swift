import AppKit
import SwiftUI

@MainActor
enum PetDexWindowPresenter {
    private static var controller: NSWindowController?

    static func show(model: AppModel) {
        if let window = controller?.window {
            if let hosting = controller?.contentViewController as? NSHostingController<AnyView> {
                hosting.rootView = AnyView(PetDexWindowView(model: model))
            }
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 860, height: 640),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.identifier = AppWindowIdentifier.petDex
        applyStandardWindowChrome(window, title: petL("pet.dex.window.title", "Pet Dex"))
        window.center()
        window.isReleasedWhenClosed = false
        window.minSize = NSSize(width: 760, height: 540)
        let hosting = NSHostingController(rootView: AnyView(PetDexWindowView(model: model)))
        window.contentViewController = hosting
        let controller = NSWindowController(window: window)
        self.controller = controller
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}

// MARK: - Root View

private struct PetDexWindowView: View {
    let model: AppModel

    @State private var spotlightEntry: PetDexEntry?

    private var petStore: PetStore { model.petStore }
    private var unlockedSpecies: Set<PetSpecies> {
        var unlocked = Set(petStore.legacy.map(\.species))
        if petStore.isClaimed { unlocked.insert(petStore.species) }
        return unlocked
    }
    private var currentRecord: PetLegacyRecord? {
        guard petStore.isClaimed else { return nil }
        return PetLegacyRecord(
            id: UUID(),
            species: petStore.species,
            customName: petStore.customName,
            evoPath: petStore.currentEvoPath(),
            totalXP: petStore.currentExperienceTokens,
            stats: petStore.currentStats,
            retiredAt: petStore.claimedAt ?? Date()
        )
    }

    var body: some View {
        ZStack {
            HStack(spacing: 0) {
                DexSidebar(
                    currentRecord: currentRecord,
                    claimedAt: petStore.claimedAt,
                    legacyCount: petStore.legacy.count,
                    unlockedCount: unlockedSpecies.count * PetDexEntry.catalogStages.count,
                    totalCount: PetDexEntry.allCases.count
                )
                .frame(width: 260)

                Divider()

                ScrollView {
                    VStack(alignment: .leading, spacing: 24) {
                        DexSpeciesGrid(
                            unlockedSpecies: unlockedSpecies,
                            onSelect: { entry in
                                withAnimation(.spring(response: 0.35, dampingFraction: 0.82)) {
                                    spotlightEntry = entry
                                }
                            }
                        )

                        if !petStore.legacy.isEmpty {
                            DexHistorySection(records: petStore.legacy)
                        }
                    }
                    .padding(20)
                }
            }
            .frame(minWidth: 760, minHeight: 540)
            .background(Color(NSColor.windowBackgroundColor))

            // Spotlight overlay
            if let entry = spotlightEntry {
                DexSpotlightOverlay(entry: entry) {
                    withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) {
                        spotlightEntry = nil
                    }
                }
                .transition(.opacity.combined(with: .scale(scale: 0.96)))
            }
        }
        .frame(minWidth: 760, minHeight: 540)
    }
}

// MARK: - Left Sidebar

private struct DexSidebar: View {
    let currentRecord: PetLegacyRecord?
    let claimedAt: Date?
    let legacyCount: Int
    let unlockedCount: Int
    let totalCount: Int

    private var currentIdentity: PetResolvedIdentity? {
        currentRecord.map { $0.resolvedIdentity(for: $0.progressInfo.stage) }
    }

    private var widestStatText: String {
        currentRecord?.stats.widestCompactValueText ?? "0"
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {

                // Header
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        Image(systemName: "books.vertical.fill")
                            .font(.system(size: 15, weight: .bold))
                            .foregroundStyle(.secondary)
                        Text(petL("pet.dex.title", "Pet Dex"))
                            .font(.system(size: 17, weight: .bold, design: .rounded))
                    }
                    Text(petL("pet.dex.subtitle", "A record of every coding companion you've raised"))
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                .padding(.top, 4)

                Divider()

                // Overview stats
                VStack(spacing: 8) {
                    DexStat(label: petL("pet.dex.current_companion", "Current Companion"),
                            value: currentIdentity?.title ?? petL("pet.dex.unclaimed", "Not Claimed"),
                            sub: currentRecord.map {
                                let identity = $0.resolvedIdentity(for: $0.progressInfo.stage)
                                let prefix = identity.subtitle.map { "\($0) · " } ?? ""
                                return "\(prefix)Lv.\($0.progressInfo.level)"
                            } ?? "—")
                    DexStat(label: petL("pet.dex.inherited", "Inherited"),
                            value: "\(legacyCount)",
                            sub: legacyCount == 0 ? petL("pet.dex.inherited.none", "No inheritance records yet") : petL("pet.dex.inherited.history", "Past companions"))
                    DexStat(label: petL("pet.dex.collection", "Dex Collection"),
                            value: "\(unlockedCount)/\(totalCount)",
                            sub: unlockedCount == totalCount ? petL("pet.dex.collection.complete", "All stages unlocked") : petL("pet.dex.collection.continue", "Keep exploring"))
                }

                Divider()

                // Current pet detail
                if let record = currentRecord {
                    let identity = record.resolvedIdentity(for: record.progressInfo.stage)
                    VStack(alignment: .leading, spacing: 10) {
                        Text(petL("pet.dex.current_pet", "Current Pet"))
                            .font(.system(size: 12, weight: .semibold))
                            .foregroundStyle(.secondary)

                        // Sprite + name
                        VStack(spacing: 8) {
                            PetSpriteView(
                                species: record.species,
                                stage: record.progressInfo.stage,
                                staticMode: true,
                                displaySize: 80
                            )
                            .frame(width: 80, height: 80)

                            Text(identity.title)
                                .font(.system(size: 14, weight: .bold, design: .rounded))
                            if let subtitle = identity.subtitle {
                                Text("\(subtitle) · \(record.progressInfo.stage.displayName)")
                                    .font(.system(size: 12))
                                    .foregroundStyle(.secondary)
                            } else {
                                Text(record.progressInfo.stage.displayName)
                                    .font(.system(size: 12))
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .frame(maxWidth: .infinity)

                        // Attribute bars
                        let accent = record.species.dexAccent
                        let maxV = PetStats.traitDisplayMaxValue
                        Group {
                            DexAttrBar(emoji: "🧠", name: petL("pet.attribute.wisdom", "Wisdom"), value: record.stats.wisdom, maxValue: maxV, color: accent, widestValueText: widestStatText)
                            DexAttrBar(emoji: "🔥", name: petL("pet.attribute.chaos", "Chaos"), value: record.stats.chaos, maxValue: maxV, color: Color(hex: 0xFF6030), widestValueText: widestStatText)
                            DexAttrBar(emoji: "🌙", name: petL("pet.attribute.night", "Night"), value: record.stats.night, maxValue: maxV, color: Color(hex: 0x6060CC), widestValueText: widestStatText)
                            DexAttrBar(emoji: "💪", name: petL("pet.attribute.stamina", "Stamina"), value: record.stats.stamina, maxValue: maxV, color: Color(hex: 0x20A060), widestValueText: widestStatText)
                            DexAttrBar(emoji: "🩹", name: petL("pet.attribute.empathy", "Empathy"), value: record.stats.empathy, maxValue: maxV, color: Color(hex: 0xE060A0), widestValueText: widestStatText)
                        }

                        // Persona + time
                        HStack(spacing: 6) {
                            Text(record.stats.personaTag)
                                .font(.system(size: 12, weight: .medium, design: .rounded))
                                .foregroundStyle(accent)
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .background(Capsule().fill(accent.opacity(0.12)))

                            Spacer()

                            if let at = claimedAt {
                                Text(at, format: .dateTime.month().day())
                                    .font(.system(size: 12))
                                    .foregroundStyle(.tertiary)
                            }
                        }

                        // XP
                        HStack {
                            Text(petL("pet.total_xp", "Total XP"))
                                .font(.system(size: 12))
                                .foregroundStyle(.secondary)
                            Spacer()
                            Text(petFormatCompactNumber(record.totalXP))
                                .font(.system(size: 12, weight: .semibold, design: .rounded))
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 8)
                        .background(
                            RoundedRectangle(cornerRadius: 10, style: .continuous)
                                .fill(Color.primary.opacity(0.04))
                        )
                    }
                } else {
                    VStack(spacing: 8) {
                        Image(systemName: "pawprint")
                            .font(.system(size: 28, weight: .semibold))
                            .foregroundStyle(.secondary)
                        Text(petL("pet.dex.no_current_pet", "No active pet yet"))
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, minHeight: 120)
                }

                Spacer(minLength: 20)
            }
            .padding(16)
        }
        .background(Color.primary.opacity(0.02))
    }
}

private struct DexStat: View {
    let label: String
    let value: String
    let sub: String

    var body: some View {
        HStack(alignment: .center) {
            VStack(alignment: .leading, spacing: 1) {
                Text(label)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.secondary)
                Text(sub)
                    .font(.system(size: 12))
                    .foregroundStyle(.tertiary)
            }
            Spacer()
            Text(value)
                .font(.system(size: 14, weight: .bold, design: .rounded))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color.primary.opacity(0.04))
        )
    }
}

private struct DexAttrBar: View {
    let emoji: String
    let name: String
    let value: Int
    let maxValue: Int
    let color: Color
    let widestValueText: String

    var body: some View {
        HStack(spacing: 6) {
            Text(emoji).font(.system(size: 12)).frame(width: 16)
            Text(name).font(.system(size: 12, weight: .medium)).foregroundStyle(.secondary).frame(width: 30, alignment: .leading)
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 2).fill(color.opacity(0.12)).frame(height: 4)
                    RoundedRectangle(cornerRadius: 2).fill(color.opacity(0.75))
                        .frame(width: geo.size.width * min(1, max(0, CGFloat(value) / CGFloat(max(1, maxValue)))), height: 4)
                }
            }
            .frame(height: 4)
            ZStack(alignment: .trailing) {
                Text(widestValueText)
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .hidden()
                Text(petFormatCompactNumber(value))
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .foregroundStyle(.secondary)
            }
        }
    }
}

// MARK: - Species Grid

private struct DexSpeciesGrid: View {
    let unlockedSpecies: Set<PetSpecies>
    let onSelect: (PetDexEntry) -> Void

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 12), count: 4)

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(petL("pet.dex.title", "Pet Dex"))
                    .font(.system(size: 15, weight: .bold, design: .rounded))
                Spacer()
                Text(String(format: petL("pet.dex.unlocked_count", "%@/%@ unlocked"), "\(unlockedSpecies.count * PetDexEntry.catalogStages.count)", "\(PetDexEntry.allCases.count)"))
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.secondary)
            }

            LazyVGrid(columns: columns, spacing: 12) {
                ForEach(PetDexEntry.allCases) { entry in
                    DexSpeciesCard(
                        entry: entry,
                        unlocked: unlockedSpecies.contains(entry.species)
                    )
                    .onTapGesture {
                        guard unlockedSpecies.contains(entry.species) else { return }
                        onSelect(entry)
                    }
                }
            }
        }
    }
}

private struct DexSpeciesCard: View {
    let entry: PetDexEntry
    let unlocked: Bool

    var body: some View {
        VStack(spacing: 10) {
            ZStack {
                Circle()
                    .fill(unlocked ? entry.species.dexAccent.opacity(0.12) : Color.primary.opacity(0.05))
                    .frame(width: 56, height: 56)

                if unlocked {
                    PetSpriteView(
                        species: entry.species,
                        stage: entry.stage,
                        staticMode: true,
                        displaySize: 44
                    )
                    .frame(width: 44, height: 44)
                } else {
                    Image(systemName: "questionmark")
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(Color.secondary.opacity(0.5))
                }
            }

            VStack(spacing: 3) {
                Text(unlocked ? entry.stage.speciesName(for: entry.species, evoPath: entry.evoPath) : petL("pet.dex.unknown", "???"))
                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                    .multilineTextAlignment(.center)
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                Text(unlocked ? entry.stage.displayName : petL("pet.dex.locked", "Locked"))
                    .font(.system(size: 12))
                    .foregroundStyle(unlocked ? entry.species.dexAccent : .secondary)
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity)
        .frame(minHeight: 136)
        .padding(.horizontal, 10)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(Color.primary.opacity(0.03))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(unlocked ? entry.species.dexAccent.opacity(0.25) : Color.primary.opacity(0.08), lineWidth: 1)
        )
        .opacity(unlocked ? 1 : 0.82)
    }
}

// MARK: - History Section

private struct DexHistorySection: View {
    let records: [PetLegacyRecord]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(petL("pet.inherit.history", "Inheritance History"))
                .font(.system(size: 15, weight: .bold, design: .rounded))

            VStack(spacing: 6) {
                ForEach(records) { record in
                    DexHistoryRow(record: record)
                }
            }
        }
    }
}

private struct DexHistoryRow: View {
    let record: PetLegacyRecord

    private var info: PetProgressInfo { record.progressInfo }

    var body: some View {
        HStack(spacing: 12) {
            PetSpriteView(
                species: record.species,
                stage: info.stage,
                staticMode: true,
                displaySize: 36
            )
            .frame(width: 36, height: 36)

            VStack(alignment: .leading, spacing: 2) {
                let identity = record.resolvedIdentity(for: info.stage)
                HStack(spacing: 6) {
                    Text(identity.title)
                        .font(.system(size: 13, weight: .semibold, design: .rounded))
                    Text(info.stage.displayName)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(record.species.dexAccent)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Capsule().fill(record.species.dexAccent.opacity(0.12)))
                }
                Text(historySubtitle(identity: identity, info: info))
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Text(record.retiredAt, format: .dateTime.year().month().day())
                .font(.system(size: 12))
                .foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color.primary.opacity(0.03))
        )
    }
}

// MARK: - Spotlight Overlay

private struct DexSpotlightOverlay: View {
    let entry: PetDexEntry
    let onDismiss: () -> Void

    @State private var spriteScale: CGFloat = 0.6
    @State private var spriteOpacity: Double = 0

    var body: some View {
        ZStack {
            // Dimmed backdrop
            Color.black.opacity(0.72)
                .ignoresSafeArea()

            // Card
            VStack(spacing: 20) {
                // Glow circle
                ZStack {
                    Circle()
                        .fill(entry.species.dexAccent.opacity(0.18))
                        .frame(width: 200, height: 200)
                        .blur(radius: 30)

                    Circle()
                        .fill(entry.species.dexAccent.opacity(0.10))
                        .frame(width: 160, height: 160)

                    PetSpriteView(
                        species: entry.species,
                        stage: entry.stage,
                        staticMode: false,
                        displaySize: 140
                    )
                    .frame(width: 140, height: 140)
                }
                .scaleEffect(spriteScale)
                .opacity(spriteOpacity)

                // Name & stage
                VStack(spacing: 6) {
                    Text(entry.stage.speciesName(for: entry.species, evoPath: entry.evoPath))
                        .font(.system(size: 22, weight: .black, design: .rounded))
                        .foregroundStyle(.white)

                    Text(entry.stage.displayName)
                        .font(.system(size: 13, weight: .semibold, design: .rounded))
                        .foregroundStyle(entry.species.dexAccent)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 4)
                        .background(Capsule().fill(entry.species.dexAccent.opacity(0.18)))

                }
                .opacity(spriteOpacity)

                // Dismiss hint
                Text(petL("common.click_anywhere_to_close", "Click anywhere to close"))
                    .font(.system(size: 11))
                    .foregroundStyle(.white.opacity(0.25))
                    .opacity(spriteOpacity)
            }
            .padding(40)
            .allowsHitTesting(false)
        }
        .contentShape(Rectangle())
        .onTapGesture { onDismiss() }
        .onAppear {
            withAnimation(.spring(response: 0.45, dampingFraction: 0.72)) {
                spriteScale = 1.0
                spriteOpacity = 1.0
            }
        }
    }
}

// MARK: - Shared helpers

private extension PetLegacyRecord {
    var progressInfo: PetProgressInfo {
        PetProgressInfo(totalXP: totalXP, hatchTokens: PetProgressInfo.hatchThreshold, evoPath: evoPath)
    }
}

struct PetDexEntry: CaseIterable, Identifiable {
    let species: PetSpecies
    let stage: PetStage
    let evoPath: PetEvoPath

    var id: String { "\(species.rawValue)-\(stage.rawValue)-\(evoPath.rawValue)" }

    static let catalogStages: [(PetStage, PetEvoPath)] = [
        (.infant, .pathA),
        (.child, .pathA),
        (.adult, .pathA),
        (.evoA, .pathA),
        (.evoB, .pathB),
        (.megaA, .pathA),
        (.megaB, .pathB),
    ]

    static let allCases: [PetDexEntry] = PetSpecies.allCases.flatMap { species in
        catalogStages.map { stage, evoPath in
            PetDexEntry(species: species, stage: stage, evoPath: evoPath)
        }
    }
}

private extension PetSpecies {
    var dexAccent: Color {
        switch self {
        case .voidcat:    return Color(hex: 0x6A5CFF)
        case .rusthound:  return Color(hex: 0xFF8A3D)
        case .goose:      return Color(hex: 0x3E86F6)
        case .chaossprite: return Color(hex: 0xFF4FA3)
        }
    }
}

private func historySubtitle(identity: PetResolvedIdentity, info: PetProgressInfo) -> String {
    let parts = [
        identity.subtitle,
        "\(petFormatCompactNumber(info.totalXP)) XP",
        "Lv.\(info.level)",
    ].compactMap { $0 }
    return parts.joined(separator: " · ")
}
