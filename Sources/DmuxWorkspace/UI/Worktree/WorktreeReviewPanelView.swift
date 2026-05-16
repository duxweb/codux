import SwiftUI

private enum WorktreeReviewTypography {
    static let body: CGFloat = 14
    static let caption: CGFloat = 12
    static let code: CGFloat = 12
}

struct WorktreeReviewPanelView: View {
    let model: AppModel
    @State private var unsavedReviewResultFileID: String?

    private var worktree: ProjectWorktree? {
        guard let id = model.selectedWorktreeReviewID else { return nil }
        return model.worktrees.first(where: { $0.id == id })
    }

    private var task: WorktreeTask? {
        guard let id = model.selectedWorktreeReviewID else { return nil }
        return model.worktreeTask(id)
    }

    private var snapshot: WorktreeReviewSnapshot? {
        model.worktreeReviewSnapshot
    }

    private var isAuditMode: Bool {
        snapshot?.mode == .workingTreeAudit || worktree?.isDefault == true
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if let worktree {
                VStack(spacing: 0) {
                    reviewChecks
                    Divider()
                    HStack(spacing: 0) {
                        fileList
                            .frame(width: 224)
                        Divider()
                        comparisonArea
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    if shouldShowFooter {
                        Divider()
                        footer(worktree)
                    }
                }
            } else {
                WorktreeReviewEmptyView()
            }
        }
        .background(AppTheme.panel)
        .onAppear {
            model.refreshWorktreeReview()
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: isAuditMode ? "doc.text.magnifyingglass" : "arrow.triangle.merge")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(isAuditMode ? AppTheme.focus : AppTheme.success)
            VStack(alignment: .leading, spacing: 2) {
                Text(reviewTitle)
                    .font(.system(size: 14, weight: .bold, design: .rounded))
                    .foregroundStyle(AppTheme.textPrimary)
                Text(reviewSubtitle)
                    .font(.system(size: WorktreeReviewTypography.caption, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if model.isLoadingWorktreeReview {
                ProgressView()
                    .controlSize(.small)
            }
            Button {
                model.refreshWorktreeReview()
            } label: {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 12, weight: .semibold))
                    .frame(width: 28, height: 28)
            }
            .buttonStyle(.plain)
            .appCursor(.pointingHand)
            .help(String(localized: "git.status.refresh", defaultValue: "Refresh Git Status", bundle: .module))
        }
        .padding(.horizontal, 16)
        .frame(height: 58)
    }

    private var reviewTitle: String {
        if isAuditMode {
            return snapshot?.title ?? String(localized: "worktree.review.audit_title", defaultValue: "Uncommitted Audit", bundle: .module)
        }
        return String(localized: "worktree.review.title", defaultValue: "Worktree Review", bundle: .module)
    }

    private var reviewSubtitle: String {
        if isAuditMode {
            return model.selectedProject?.name ?? worktree?.name ?? ""
        }
        return task?.title ?? worktree?.name ?? ""
    }

    private var reviewChecks: some View {
        VStack(alignment: .leading, spacing: 10) {
            if isAuditMode {
                HStack(spacing: 12) {
                    WorktreeReviewInfoRow(
                        title: String(localized: "worktree.review.audit_scope", defaultValue: "Scope", bundle: .module),
                        value: String(localized: "worktree.review.audit_working_tree", defaultValue: "Working Tree", bundle: .module)
                    )
                    WorktreeReviewInfoRow(
                        title: String(localized: "worktree.review.audit_compare", defaultValue: "Compare", bundle: .module),
                        value: "HEAD"
                    )
                }
            } else {
                HStack(spacing: 12) {
                    WorktreeReviewInfoRow(
                        title: String(localized: "worktree.task.base_branch", defaultValue: "Base Branch", bundle: .module),
                        value: task?.baseBranch ?? ""
                    )
                    WorktreeReviewInfoRow(
                        title: String(localized: "worktree.task.branch", defaultValue: "Task Branch", bundle: .module),
                        value: worktree?.branch ?? ""
                    )
                }
            }

            if let checks = snapshot?.checks, !checks.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(checks) { check in
                            WorktreeReviewCheckPill(check: check)
                        }
                    }
                }
            }

        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
    }

    private var fileList: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Text(String(localized: "worktree.review.changed_files", defaultValue: "Changed Files", bundle: .module))
                    .font(.system(size: WorktreeReviewTypography.body, weight: .bold))
                    .foregroundStyle(AppTheme.textPrimary)
                Spacer(minLength: 0)
                Text("\(snapshot?.files.count ?? 0)")
                    .font(.system(size: WorktreeReviewTypography.caption, weight: .semibold, design: .monospaced))
                    .foregroundStyle(AppTheme.textMuted)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)

            Divider()

            if let files = snapshot?.files, !files.isEmpty {
                ScrollView {
                    LazyVStack(spacing: 4) {
                        ForEach(files) { file in
                            WorktreeReviewFileRow(
                                file: file,
                                isSelected: file.id == snapshot?.selectedFileID
                            ) {
                                selectReviewFile(file)
                            }
                        }
                    }
                    .padding(8)
                }
            } else {
                VStack(spacing: 8) {
                    Image(systemName: "doc.text.magnifyingglass")
                        .font(.system(size: 20, weight: .semibold))
                        .foregroundStyle(AppTheme.textMuted)
                    Text(noChangesText)
                        .font(.system(size: WorktreeReviewTypography.body, weight: .medium))
                        .foregroundStyle(AppTheme.textMuted)
                        .multilineTextAlignment(.center)
                }
                .padding(18)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.34))
    }

    private var noChangesText: String {
        isAuditMode
            ? String(localized: "worktree.review.audit_no_changes", defaultValue: "No uncommitted changes.", bundle: .module)
            : String(localized: "worktree.review.no_changes", defaultValue: "No changes relative to the base branch.", bundle: .module)
    }

    private var comparisonArea: some View {
        VStack(spacing: 0) {
            if let comparison = snapshot?.selectedFileComparison {
                WorktreeReviewComparisonView(model: model, comparison: comparison) { fileID, isDirty in
                    updateUnsavedReviewResult(fileID: fileID, isDirty: isDirty)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if model.isLoadingWorktreeReview {
                VStack(spacing: 10) {
                    ProgressView()
                        .controlSize(.small)
                    Text(String(localized: "worktree.review.loading", defaultValue: "Loading review.", bundle: .module))
                        .font(.system(size: WorktreeReviewTypography.body, weight: .medium))
                        .foregroundStyle(AppTheme.textMuted)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                WorktreeReviewEmptyComparisonView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder
    private func footer(_ worktree: ProjectWorktree) -> some View {
        if isAuditMode {
            auditFooter
        } else {
            mergeFooter(worktree)
        }
    }

    private var auditFooter: some View {
        HStack(spacing: 10) {
            if hasUnsavedReviewResult {
                Text(String(localized: "worktree.review.result.save_or_discard", defaultValue: "Save or discard the current result.", bundle: .module))
                    .font(.system(size: WorktreeReviewTypography.caption, weight: .semibold))
                    .foregroundStyle(AppTheme.warning)
                    .lineLimit(1)
            }

            Spacer(minLength: 0)

            if hasReviewChanges {
                Button {
                    model.discardSelectedReviewFile()
                } label: {
                    Label(String(localized: "git.files.discard_changes", defaultValue: "Discard Changes", bundle: .module), systemImage: "arrow.uturn.backward")
                }
                .buttonStyle(WorktreeReviewSecondaryActionStyle(tint: AppTheme.warning))
                .disabled(snapshot?.selectedFileID == nil || hasUnsavedReviewResult)

                Button {
                    model.discardAllReviewChanges()
                } label: {
                    Label(String(localized: "git.files.discard_all", defaultValue: "Discard All", bundle: .module), systemImage: "trash")
                }
                .buttonStyle(WorktreeReviewSecondaryActionStyle(tint: AppTheme.warning))
                .disabled(hasUnsavedReviewResult)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    private func mergeFooter(_ worktree: ProjectWorktree) -> some View {
        HStack(spacing: 10) {
            if hasUnsavedReviewResult {
                Text(String(localized: "worktree.review.result.save_before_merge", defaultValue: "Save or discard the current result before merging.", bundle: .module))
                    .font(.system(size: WorktreeReviewTypography.caption, weight: .semibold))
                    .foregroundStyle(AppTheme.warning)
                    .lineLimit(1)
            }

            Spacer(minLength: 0)

            if hasReviewChanges {
                Button {
                    model.discardSelectedReviewFile()
                } label: {
                    Label(String(localized: "git.files.discard_changes", defaultValue: "Discard Changes", bundle: .module), systemImage: "arrow.uturn.backward")
                }
                .buttonStyle(WorktreeReviewSecondaryActionStyle(tint: AppTheme.warning))
                .disabled(snapshot?.selectedFileID == nil || hasUnsavedReviewResult)
            }

            Button {
                model.mergeReviewedWorktree(removeAfterMerge: true)
            } label: {
                Label(String(localized: "worktree.review.merge_cleanup", defaultValue: "Merge & Remove", bundle: .module), systemImage: "arrow.triangle.merge")
            }
            .buttonStyle(AppDialogPrimaryButtonStyle(tint: AppTheme.success))
            .disabled(!canMerge(worktree))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    private var shouldShowFooter: Bool {
        !isAuditMode || hasUnsavedReviewResult || hasReviewChanges
    }

    private var hasReviewChanges: Bool {
        snapshot?.files.isEmpty == false
    }

    private func selectReviewFile(_ file: WorktreeReviewFileChange) {
        if let unsavedReviewResultFileID,
           unsavedReviewResultFileID != file.id {
            model.statusMessage = String(localized: "worktree.review.result.save_before_switch", defaultValue: "Save or discard the current result before switching files.", bundle: .module)
            return
        }
        model.selectWorktreeReviewFile(file.id)
    }

    private func updateUnsavedReviewResult(fileID: String, isDirty: Bool) {
        if isDirty {
            unsavedReviewResultFileID = fileID
        } else if unsavedReviewResultFileID == fileID {
            unsavedReviewResultFileID = nil
        }
    }

    private var hasUnsavedReviewResult: Bool {
        unsavedReviewResultFileID != nil
    }

    private func canMerge(_ worktree: ProjectWorktree) -> Bool {
        !worktree.isDefault
            && snapshot?.mode != .workingTreeAudit
            && !model.isLoadingWorktreeReview
            && snapshot != nil
            && model.isWorktreeMergeCandidateStatus(model.effectiveWorktreeTaskStatus(for: worktree))
            && !hasBlockingCheck
            && !hasUnsavedReviewResult
    }

    private var hasBlockingCheck: Bool {
        snapshot?.checks.contains(where: { $0.severity == .blocking }) ?? false
    }
}

private struct WorktreeReviewSecondaryActionStyle: ButtonStyle {
    let tint: Color

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: WorktreeReviewTypography.caption, weight: .bold))
            .foregroundStyle(tint)
            .padding(.horizontal, 11)
            .frame(height: 28)
            .background(
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .fill(tint.opacity(configuration.isPressed ? 0.18 : 0.10))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .stroke(tint.opacity(0.20), lineWidth: 0.5)
            )
            .appCursor(.pointingHand)
    }
}

private struct WorktreeReviewInfoRow: View {
    let title: String
    let value: String

    var body: some View {
        HStack(spacing: 7) {
            Text(title)
                .font(.system(size: WorktreeReviewTypography.caption, weight: .bold))
                .foregroundStyle(AppTheme.textMuted)
            Text(value.isEmpty ? "-" : value)
                .font(.system(size: WorktreeReviewTypography.code, weight: .semibold, design: .monospaced))
                .foregroundStyle(AppTheme.textSecondary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }
}

private struct WorktreeReviewCheckPill: View {
    let check: WorktreeReviewCheck

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: iconName)
                .font(.system(size: WorktreeReviewTypography.caption, weight: .bold))
                .foregroundStyle(color)
            VStack(alignment: .leading, spacing: 1) {
                Text(check.title)
                    .font(.system(size: WorktreeReviewTypography.caption, weight: .bold))
                    .foregroundStyle(AppTheme.textSecondary)
                Text(check.detail)
                    .font(.system(size: WorktreeReviewTypography.caption, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 7)
        .background(color.opacity(0.10))
        .overlay(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .stroke(color.opacity(0.24), lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }

    private var iconName: String {
        switch check.severity {
        case .ok:
            return "checkmark.circle.fill"
        case .warning:
            return "exclamationmark.triangle.fill"
        case .blocking:
            return "xmark.octagon.fill"
        }
    }

    private var color: Color {
        switch check.severity {
        case .ok:
            return AppTheme.success
        case .warning:
            return AppTheme.warning
        case .blocking:
            return Color.red
        }
    }
}

private struct WorktreeReviewFileRow: View {
    let file: WorktreeReviewFileChange
    let isSelected: Bool
    let action: () -> Void
    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Text(file.status.displayName)
                        .font(.system(size: WorktreeReviewTypography.caption, weight: .bold))
                        .foregroundStyle(statusColor)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 2)
                        .background(statusColor.opacity(0.12))
                        .clipShape(RoundedRectangle(cornerRadius: 5, style: .continuous))
                    Spacer(minLength: 0)
                    if let additions = file.additions, let deletions = file.deletions {
                        Text("+\(additions) -\(deletions)")
                            .font(.system(size: WorktreeReviewTypography.caption, weight: .semibold, design: .monospaced))
                            .foregroundStyle(AppTheme.textMuted)
                    }
                }
                Text(file.path)
                    .font(.system(size: WorktreeReviewTypography.code, weight: .semibold, design: .monospaced))
                    .foregroundStyle(isSelected ? AppTheme.textPrimary : AppTheme.textSecondary)
                    .lineLimit(2)
                    .truncationMode(.middle)
                if let oldPath = file.oldPath, oldPath != file.path {
                    Text(oldPath)
                        .font(.system(size: WorktreeReviewTypography.code, weight: .medium, design: .monospaced))
                        .foregroundStyle(AppTheme.textMuted)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            .padding(.horizontal, 9)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(rowBackground)
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(isSelected ? AppTheme.focus.opacity(0.34) : Color.clear, lineWidth: 1)
            )
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }

    private var rowBackground: Color {
        if isSelected {
            return AppTheme.focus.opacity(0.14)
        }
        if isHovered {
            return Color(nsColor: .controlAccentColor).opacity(0.08)
        }
        return Color.clear
    }

    private var statusColor: Color {
        switch file.status {
        case .added:
            return AppTheme.success
        case .modified, .typeChanged, .unknown:
            return AppTheme.focus
        case .deleted:
            return Color.red
        case .renamed, .copied:
            return AppTheme.warning
        }
    }
}

private struct WorktreeReviewComparisonView: View {
    let model: AppModel
    let comparison: WorktreeReviewFileComparison
    let onDirtyChanged: (String, Bool) -> Void
    @State private var resultText: String
    @State private var savedResultText: String
    @State private var resultDeletesFile: Bool
    @State private var savedResultDeletesFile: Bool
    @State private var focusToken = 0
    @State private var copyToken = 0
    @State private var pasteToken = 0
    @State private var undoToken = 0
    @State private var redoToken = 0
    @State private var findToken = 0
    @State private var snapshotToken = 0
    @State private var markSavedToken = 0
    @State private var pendingSave = false

    init(model: AppModel, comparison: WorktreeReviewFileComparison, onDirtyChanged: @escaping (String, Bool) -> Void) {
        self.model = model
        self.comparison = comparison
        self.onDirtyChanged = onDirtyChanged
        _resultText = State(initialValue: comparison.resultText)
        _savedResultText = State(initialValue: comparison.resultText)
        _resultDeletesFile = State(initialValue: comparison.resultDeletesFile)
        _savedResultDeletesFile = State(initialValue: comparison.resultDeletesFile)
    }

    private var editorTheme: ProjectFileEditorTheme {
        ProjectFileEditorTheme(
            appearance: model.terminalAppearance,
            fontSize: max(Int(WorktreeReviewTypography.code), model.appSettings.terminalFontSize)
        )
    }

    private var isDirty: Bool {
        resultText != savedResultText || resultDeletesFile != savedResultDeletesFile
    }

    private var deleteMessage: String {
        if model.worktreeReviewSnapshot?.mode == .workingTreeAudit {
            return String(localized: "worktree.review.audit_result_deletes_file", defaultValue: "Audit result deletes this file.", bundle: .module)
        }
        return String(localized: "worktree.review.result_deletes_file", defaultValue: "Merge result deletes this file.", bundle: .module)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(comparison.file.path)
                        .font(.system(size: WorktreeReviewTypography.body, weight: .bold, design: .monospaced))
                        .foregroundStyle(AppTheme.textPrimary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if let message = comparison.message, !message.isEmpty {
                        Text(message)
                            .font(.system(size: WorktreeReviewTypography.caption, weight: .medium))
                            .foregroundStyle(AppTheme.warning)
                            .lineLimit(2)
                    }
                }
                Spacer(minLength: 0)
                if isDirty {
                    Text(String(localized: "worktree.review.result.unsaved", defaultValue: "Unsaved result", bundle: .module))
                        .font(.system(size: WorktreeReviewTypography.caption, weight: .semibold))
                        .foregroundStyle(AppTheme.warning)
                }
                Text(comparison.file.status.displayName)
                    .font(.system(size: WorktreeReviewTypography.caption, weight: .bold))
                    .foregroundStyle(AppTheme.textMuted)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)

            Divider()

            GeometryReader { geometry in
                let dividerWidth: CGFloat = 1
                let minimumColumnWidth: CGFloat = 240
                let availableWidth = max(1, geometry.size.width)
                let columnWidth = max(minimumColumnWidth, (availableWidth - dividerWidth * 2) / 3)
                let contentWidth = columnWidth * 3 + dividerWidth * 2

                ScrollView(.horizontal) {
                    HStack(spacing: 0) {
                        WorktreeReviewTextColumn(
                            title: comparison.baseTitle,
                            text: comparison.baseText,
                            isDeleted: comparison.baseDeletesFile,
                            emptyText: String(localized: "worktree.review.empty_base", defaultValue: "No base content.", bundle: .module)
                        )
                        .frame(width: columnWidth)
                        .frame(maxHeight: .infinity)
                        Divider()
                            .frame(width: dividerWidth)
                        WorktreeReviewTextColumn(
                            title: comparison.worktreeTitle,
                            text: comparison.worktreeText,
                            isDeleted: comparison.worktreeDeletesFile,
                            emptyText: String(localized: "worktree.review.empty_worktree", defaultValue: "No worktree content.", bundle: .module)
                        )
                        .frame(width: columnWidth)
                        .frame(maxHeight: .infinity)
                        Divider()
                            .frame(width: dividerWidth)
                        WorktreeReviewResultColumn(
                            title: comparison.resultTitle,
                            text: $resultText,
                            deletesFile: $resultDeletesFile,
                            editorTheme: editorTheme,
                            deleteMessage: deleteMessage,
                            fileURL: URL(fileURLWithPath: comparison.file.path),
                            focusToken: focusToken,
                            copyToken: copyToken,
                            pasteToken: pasteToken,
                            undoToken: undoToken,
                            redoToken: redoToken,
                            findToken: findToken,
                            snapshotToken: snapshotToken,
                            markSavedToken: markSavedToken,
                            acceptBase: acceptBase,
                            acceptWorktree: acceptWorktree,
                            deleteResult: deleteResult,
                            save: saveResult,
                            onSnapshot: handleSnapshot
                        )
                        .frame(width: columnWidth)
                        .frame(maxHeight: .infinity)
                    }
                    .frame(width: contentWidth, height: geometry.size.height, alignment: .topLeading)
                    .clipped()
                }
                .clipped()
            }
            .clipped()
        }
        .id(comparison.file.id)
        .onAppear {
            focusToken &+= 1
            notifyDirtyState()
        }
        .onDisappear {
            onDirtyChanged(comparison.file.id, false)
        }
        .onChange(of: resultText) { _, _ in
            notifyDirtyState()
        }
        .onChange(of: resultDeletesFile) { _, _ in
            notifyDirtyState()
        }
    }

    private func acceptBase() {
        resultText = comparison.baseText
        resultDeletesFile = comparison.baseDeletesFile
        focusToken &+= 1
    }

    private func acceptWorktree() {
        resultText = comparison.worktreeText
        resultDeletesFile = comparison.worktreeDeletesFile
        focusToken &+= 1
    }

    private func deleteResult() {
        resultDeletesFile = true
    }

    private func saveResult() {
        if resultDeletesFile {
            pendingSave = true
            handleSnapshot("")
            return
        }
        pendingSave = true
        snapshotToken &+= 1
    }

    private func handleSnapshot(_ content: String) {
        guard pendingSave else {
            return
        }
        pendingSave = false
        let textToSave = resultDeletesFile ? "" : content
        model.saveSelectedWorktreeReviewResult(text: textToSave, deletesFile: resultDeletesFile)
        resultText = textToSave
        savedResultText = textToSave
        savedResultDeletesFile = resultDeletesFile
        markSavedToken &+= 1
        notifyDirtyState()
    }

    private func notifyDirtyState() {
        onDirtyChanged(comparison.file.id, isDirty)
    }
}

private struct WorktreeReviewTextColumn: View {
    let title: String
    let text: String
    let isDeleted: Bool
    let emptyText: String

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(.system(size: WorktreeReviewTypography.caption, weight: .bold))
                .foregroundStyle(AppTheme.textSecondary)
                .lineLimit(1)
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
            Divider()
            ScrollView {
                if isDeleted {
                    Text(String(localized: "worktree.review.file_deleted", defaultValue: "File deleted in this side.", bundle: .module))
                        .font(.system(size: WorktreeReviewTypography.body, weight: .medium))
                        .foregroundStyle(AppTheme.textMuted)
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                } else if text.isEmpty {
                    Text(emptyText)
                        .font(.system(size: WorktreeReviewTypography.body, weight: .medium))
                        .foregroundStyle(AppTheme.textMuted)
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                } else {
                    Text(numberedText)
                        .font(.system(size: WorktreeReviewTypography.code, weight: .regular, design: .monospaced))
                        .foregroundStyle(AppTheme.textSecondary)
                        .textSelection(.enabled)
                        .lineSpacing(2)
                        .padding(10)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .background(Color(nsColor: .textBackgroundColor).opacity(0.38))
        .clipped()
    }

    private var numberedText: String {
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        let width = max(2, String(lines.count).count)
        return lines.enumerated().map { index, line in
            let number = String(index + 1).leftPadded(to: width)
            return "\(number)  \(line)"
        }.joined(separator: "\n")
    }
}

private struct WorktreeReviewResultColumn: View {
    let title: String
    @Binding var text: String
    @Binding var deletesFile: Bool
    let editorTheme: ProjectFileEditorTheme
    let deleteMessage: String
    let fileURL: URL
    let focusToken: Int
    let copyToken: Int
    let pasteToken: Int
    let undoToken: Int
    let redoToken: Int
    let findToken: Int
    let snapshotToken: Int
    let markSavedToken: Int
    let acceptBase: () -> Void
    let acceptWorktree: () -> Void
    let deleteResult: () -> Void
    let save: () -> Void
    let onSnapshot: (String) -> Void
    @State private var editorReportedDirty = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Text(title)
                    .font(.system(size: WorktreeReviewTypography.caption, weight: .bold))
                    .foregroundStyle(AppTheme.textSecondary)
                    .lineLimit(1)
                Spacer(minLength: 0)
                resultButton(
                    title: String(localized: "worktree.review.accept_base", defaultValue: "Base", bundle: .module),
                    symbol: "arrow.left.to.line",
                    action: acceptBase
                )
                resultButton(
                    title: String(localized: "worktree.review.accept_worktree", defaultValue: "Worktree", bundle: .module),
                    symbol: "arrow.right.to.line",
                    action: acceptWorktree
                )
                resultButton(
                    title: String(localized: "worktree.review.delete_result", defaultValue: "Delete", bundle: .module),
                    symbol: "trash",
                    action: deleteResult
                )
                resultButton(
                    title: String(localized: "files.preview.save", defaultValue: "Save", bundle: .module),
                    symbol: "checkmark.circle",
                    action: save
                )
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            Divider()

            if deletesFile {
                VStack(spacing: 8) {
                    Image(systemName: "trash")
                        .font(.system(size: 24, weight: .semibold))
                        .foregroundStyle(AppTheme.textMuted)
                    Text(deleteMessage)
                        .font(.system(size: WorktreeReviewTypography.body, weight: .medium))
                        .foregroundStyle(AppTheme.textMuted)
                    Button {
                        deletesFile = false
                    } label: {
                        Label(String(localized: "worktree.review.keep_file", defaultValue: "Keep File", bundle: .module), systemImage: "doc")
                    }
                    .buttonStyle(.borderless)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                NativeSourceEditorTextPreview(
                    text: $text,
                    editorTheme: editorTheme,
                    focusToken: focusToken,
                    renderToken: 0,
                    copyToken: copyToken,
                    pasteToken: pasteToken,
                    undoToken: undoToken,
                    redoToken: redoToken,
                    findToken: findToken,
                    snapshotToken: snapshotToken,
                    markSavedToken: markSavedToken,
                    fileURL: fileURL,
                    isLargeFileMode: false,
                    onFocused: {},
                    onDirtyChanged: { editorReportedDirty = $0 },
                    onTextSnapshot: onSnapshot,
                    onSaveRequested: save
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .clipped()
            }
        }
        .background(Color(nsColor: .textBackgroundColor).opacity(0.38))
    }

    private func resultButton(title: String, symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: WorktreeReviewTypography.caption, weight: .semibold))
                .frame(width: 22, height: 22)
        }
        .buttonStyle(.plain)
        .help(title)
    }
}

private struct WorktreeReviewEmptyComparisonView: View {
    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: "rectangle.split.3x1")
                .font(.system(size: 26, weight: .semibold))
                .foregroundStyle(AppTheme.textMuted)
            Text(String(localized: "worktree.review.select_file", defaultValue: "Select a changed file to compare.", bundle: .module))
                .font(.system(size: WorktreeReviewTypography.body, weight: .medium))
                .foregroundStyle(AppTheme.textMuted)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct WorktreeReviewEmptyView: View {
    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: "arrow.triangle.merge")
                .font(.system(size: 26, weight: .semibold))
                .foregroundStyle(AppTheme.textMuted)
            Text(String(localized: "worktree.review.empty", defaultValue: "Select a worktree to review.", bundle: .module))
                .font(.system(size: WorktreeReviewTypography.body, weight: .medium))
                .foregroundStyle(AppTheme.textMuted)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private extension String {
    func leftPadded(to width: Int) -> String {
        guard count < width else {
            return self
        }
        return String(repeating: " ", count: width - count) + self
    }
}
