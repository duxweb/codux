part of '../home_page.dart';

/// Workspace actions: terminal/worktree control, project form, file
/// browser + editor, git/ssh/ai-stats requests and view-mode switching.
///
/// Split into a part + extension to keep the State class navigable; behaviour
/// is unchanged. Rebuilds route through [_CoduxHomePageState._applyState]
/// (`setState` is `@protected` and cannot be called from an extension).
extension _HomePageWorkspace on HomeController {
  void _selectTerminal(TerminalInfo terminal) {
    _terminalActions.selectTerminal(terminal);
  }

  void _createCurrentProjectTerminal() {
    _terminalActions.createTerminalForSelectedProject(_createTerminal);
  }

  void _showTerminalWorkspace() {
    if (!mounted) return;
    _applyState(() => _workspaceMode = WorkspaceMode.terminal);
  }

  void _closeCurrentTerminal() {
    _terminalActions.closeCurrentTerminal();
  }

  void _closeTerminal(TerminalInfo terminal) {
    _terminalActions.closeTerminal(terminal);
  }

  Future<void> _openTerminalSwitcher() async {
    await _terminalActions.openTerminalSwitcher(_showTerminalSwitcher);
  }

  void _closeTerminalSwitcher() {
    _pendingWorktreeSwitch = null;
    _terminalActions.closeTerminalSwitcher(_popCupertinoPage);
  }

  void _hideTerminalSwitcher() {
    _showTerminalSwitcher = false;
  }

  void _selectTerminalFromSwitcher(TerminalInfo terminal) {
    _selectTerminal(terminal);
    _closeTerminalSwitcher();
  }

  void _selectWorktree(RemoteWorktreeInfo worktree) {
    _worktreeActions.selectWorktree(worktree);
  }

  Future<void> _createWorktree() async {
    await _worktreeActions.createWorktree();
  }

  Future<void> _mergeWorktree(RemoteWorktreeInfo worktree) async {
    await _worktreeActions.mergeWorktree(worktree);
  }

  Future<void> _deleteWorktree(RemoteWorktreeInfo worktree) async {
    await _worktreeActions.deleteWorktree(worktree);
  }

  Future<bool> _confirmWorktreeAction({
    required String title,
    required String message,
    required bool destructive,
  }) async {
    return await showDialog<bool>(
          context: context,
          builder: (ctx) => WorktreeActionDialog(
            title: title,
            message: message,
            cancelLabel: _t('app.cancel'),
            destructive: destructive,
          ),
        ) ??
        false;
  }

  String _worktreeTitle(RemoteWorktreeInfo worktree) {
    return worktreeTitle(worktree);
  }

  void _requestProjectEdit() {
    final project = _selectedProject;
    if (project == null) {
      _showSnack(_t('project.selectFirst'));
      return;
    }
    final draft = _projectController.editDraft(project);
    _applyState(() {
      _applyProjectFormDraft(draft);
      _showProjectForm = true;
    });
  }

  void _requestProjectAdd() {
    final draft = _projectController.addDraft();
    _applyState(() {
      _applyProjectFormDraft(draft);
      _showProjectForm = true;
    });
  }

  void _chooseProjectFormPath() {
    _filePickerMode = 'projectForm';
    final current = _projectPathController.text.trim();
    _openRemoteFilePicker(current.isEmpty ? null : current);
  }

  void _saveProjectForm() {
    final plan = _projectController.savePlan(
      mode: _projectFormMode,
      path: _projectPathController.text,
      name: _projectNameController.text,
      selectedProject: _selectedProject,
    );
    if (!plan.valid) {
      _showToast(_t('project.selectPathFirst'));
      return;
    }
    _send(plan.envelope!);
    _applyState(() => _showProjectForm = false);
    _showToast(_t('project.saveSubmitted'));
  }

  void _openRemoteFilePicker([String? path]) {
    _filePickerTimeoutTimer?.cancel();
    _applyState(() {
      _showFilePicker = true;
      _filePickerLoading = true;
      _filePickerPath = path ?? _filePickerPath;
    });
    _filePickerTimeoutTimer = Timer(const Duration(seconds: 8), () {
      if (!mounted || !_filePickerLoading) return;
      _applyState(() => _filePickerLoading = false);
      _showToast(_t('remote.dirTimeout'));
    });
    _send(_projectController.filePickerListEnvelope(path));
  }

  void _selectRemoteProjectFolder(RemoteFileEntry entry) {
    if (_filePickerMode == 'projectForm') {
      final selection = _projectController.selectFolder(
        entry: entry,
        currentName: _projectNameController.text,
      );
      _applyState(() {
        _projectPathController.text = selection.path;
        _projectNameController.text = selection.name;
        _showFilePicker = false;
      });
      return;
    }
    _applyState(() => _showFilePicker = false);
  }

  void _applyProjectFormDraft(ProjectFormDraft draft) {
    _projectFormMode = draft.mode;
    _projectNameController.text = draft.name;
    _projectPathController.text = draft.path;
  }

  void _requestProjectRemove() {
    final project = _selectedProject;
    if (project == null) {
      _showSnack(_t('project.selectFirst'));
      return;
    }
    _send(_projectController.removeEnvelope(project));
    _showToast(_t('project.removeRequested'));
  }

  void _requestAIStats() {
    _workspaceModeActions.requestAIStats(_t('project.selectFirst'));
  }

  void _refreshAIStats() {
    _workspaceModeActions.refreshAIStats();
  }

  void _requestGitStatus() {
    _workspaceModeActions.requestGitStatus();
  }

  void _gitAction(String op, {Map<String, dynamic> args = const {}}) {
    _workspaceModeActions.gitInvoke(op, args: args);
  }

  void _requestAISessions({bool force = false}) {
    final project = _selectedProject;
    if (project == null) return;
    if (!force && _aiSessionsProjectId == project.id) return;
    _aiSessionsProjectId = project.id;
    _send(_projectController.aiSessionListEnvelope(project));
  }

  void _handleAISessionResult(Object? payload) {
    if (payload is! Map) return;
    final op = '${payload['op'] ?? ''}';
    final result = payload['result'];
    switch (op) {
      case 'list':
        if (result is! List) return;
        if (!mounted) return;
        _applyState(() {
          _aiSessions = result
              .whereType<Map>()
              .map(
                (item) =>
                    AISessionRecord.fromJson(Map<String, dynamic>.from(item)),
              )
              .toList();
        });
      case 'rename':
      case 'remove':
        // The host applied the change; pull a fresh list so the row updates.
        _requestAISessions(force: true);
      case 'restore':
        _applyRestoredSession(result);
    }
  }

  /// Write the host-built resume command (e.g. `claude --resume <id>`) into the
  /// active terminal and surface it, mirroring the desktop "open" action.
  void _applyRestoredSession(Object? result) {
    if (result is! Map) return;
    final command = '${result['command'] ?? ''}'.trim();
    if (command.isEmpty) return;
    _showTerminalMode();
    _insertTerminalText(command);
    _sendTerminalKey('\r');
  }

  Future<void> _openAISession(AISessionRecord session) async {
    final project = _selectedProject;
    if (project == null) return;
    _send(_projectController.aiSessionRestoreEnvelope(project, session.id));
  }

  Future<void> _renameAISession(AISessionRecord session) async {
    final project = _selectedProject;
    if (project == null) return;
    final prefs = AppPreferences.of(context);
    final nextTitle = await showDialog<String>(
      context: context,
      builder: (ctx) => FileRenameDialog(
        title: prefs.t('session.renameTitle'),
        label: prefs.t('session.renameLabel'),
        cancelLabel: prefs.t('file.cancel'),
        saveLabel: prefs.t('file.save'),
        initialName: session.title,
      ),
    );
    final trimmed = nextTitle?.trim();
    if (trimmed == null || trimmed.isEmpty || trimmed == session.title) return;
    _send(
      _projectController.aiSessionRenameEnvelope(project, session.id, trimmed),
    );
  }

  Future<void> _deleteAISession(AISessionRecord session) async {
    final project = _selectedProject;
    if (project == null) return;
    final prefs = AppPreferences.of(context);
    final name = session.title.trim().isNotEmpty
        ? session.title.trim()
        : session.id;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => FileDeleteDialog(
        title: prefs.t('session.deleteTitle'),
        message: prefs.t('session.deleteConfirm', params: {'name': name}),
        cancelLabel: prefs.t('file.cancel'),
        deleteLabel: prefs.t('file.menuDelete'),
      ),
    );
    if (confirmed != true) return;
    _send(_projectController.aiSessionRemoveEnvelope(project, session.id));
  }

  void _requestSshProfiles() {
    _send(_projectController.sshListEnvelope());
  }

  /// Add/update a saved SSH profile on the host; the host replies with a fresh
  /// ssh.list which refreshes the panel.
  void _sshUpsert(Map<String, dynamic> fields) {
    _send(_projectController.sshUpsertEnvelope(fields));
  }

  void _sshRemove(String id) {
    _send(_projectController.sshRemoveEnvelope(id));
  }

  void _handleSshListResult(Object? payload) {
    if (payload is! Map) return;
    final profiles = payload['profiles'];
    if (profiles is! List) return;
    if (!mounted) return;
    _applyState(() {
      _sshProfiles = profiles
          .whereType<Map>()
          .map(
            (item) =>
                RemoteSshProfile.fromJson(Map<String, dynamic>.from(item)),
          )
          .toList();
    });
  }

  void _requestGitDiff(String path) {
    _applyState(() {
      _gitDiffPath = path;
      _gitDiff = null;
    });
    _workspaceModeActions.requestGitDiff(path);
  }

  /// Open a changed file's diff in the center review view. Used by both the
  /// review list and the git panel (which switches over to review on tap).
  void _openReviewFile(String path) {
    if (_workspaceMode != WorkspaceMode.review) {
      _applyState(() => _workspaceMode = WorkspaceMode.review);
    }
    _requestGitDiff(path);
  }

  void _handleGitRead(Object? payload) {
    if (payload is! Map) return;
    final op = '${payload['op'] ?? ''}';
    final result = payload['result'];
    if (op == 'diff' && result is Map && mounted) {
      _applyState(() {
        _gitDiff = RemoteGitDiff.fromResult(Map<String, dynamic>.from(result));
      });
    }
  }

  void _showTerminalMode() {
    // Keep any open file in state so switching back to "文件" shows it again;
    // the center returns to the terminal because primaryWorkspaceMode follows
    // workspaceMode, not editingFilePath.
    _workspaceModeActions.showTerminalMode();
  }

  void _showFilesMode() {
    _workspaceModeActions.showFilesMode(
      _t('project.selectFirst'),
      _t('project.currentNoDir'),
    );
  }

  void _showReviewMode() {
    _applyState(() => _workspaceMode = WorkspaceMode.review);
    _requestGitStatus();
  }

  void _showSshMode() {
    _applyState(() => _workspaceMode = WorkspaceMode.ssh);
    _requestSshProfiles();
  }

  void _showGitMode() {
    _applyState(() => _workspaceMode = WorkspaceMode.git);
    _requestGitStatus();
  }

  /// Tapping a right-column tool button again collapses the column back to the
  /// terminal; otherwise it opens that tool. Used by the pad header actions.
  void _toggleWorkspaceTool(WorkspaceMode target, VoidCallback open) {
    if (_workspaceMode == target) {
      _showTerminalMode();
    } else {
      open();
    }
  }

  void _requestProjectFiles([String? path]) {
    _workspaceModeActions.requestProjectFiles(
      _t('project.currentNoDir'),
      path: path,
    );
  }

  Future<void> _copyProjectFilePath(RemoteFileEntry entry) async {
    final message = AppPreferences.of(context).t('file.pathCopied');
    await Clipboard.setData(ClipboardData(text: entry.path));
    _showToast(message);
  }

  Future<void> _renameProjectFile(RemoteFileEntry entry) async {
    final prefs = AppPreferences.of(context);
    final nextName = await showDialog<String>(
      context: context,
      builder: (ctx) => FileRenameDialog(
        title: prefs.t('file.renameTitle'),
        label: prefs.t('file.renameLabel'),
        cancelLabel: prefs.t('file.cancel'),
        saveLabel: prefs.t('file.save'),
        initialName: entry.name,
      ),
    );
    if (nextName == null) return;
    final plan = _projectFileController.renamePlan(entry, nextName);
    if (plan == null) return;
    if (!plan.valid) {
      _showToast(prefs.t('file.nameInvalid'));
      return;
    }
    _send(plan.envelope!);
  }

  Future<void> _deleteProjectFile(RemoteFileEntry entry) async {
    final prefs = AppPreferences.of(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => FileDeleteDialog(
        title: prefs.t('file.deleteTitle'),
        message: prefs.t('file.deleteConfirm', params: {'name': entry.name}),
        cancelLabel: prefs.t('file.cancel'),
        deleteLabel: prefs.t('file.menuDelete'),
      ),
    );
    if (confirmed != true) return;
    _send(_projectFileController.deleteEnvelope(entry));
  }

  void _openFileLocation(String path) {
    if (_showFilePicker) {
      _openRemoteFilePicker(path);
      return;
    }
    _requestProjectFiles(path);
  }

  void _requestFileRead(RemoteFileEntry entry) {
    if (entry.isDirectory) return;
    final fileState = _projectFileController.beginReadState(entry);
    _applyState(() {
      _workspaceMode = WorkspaceMode.files;
      _applyFileEditorState(fileState);
    });
    _send(_projectFileController.readEnvelope(entry));
  }

  void _applyFileListState(RemoteFileListState state) {
    if (state.isProjectFiles) {
      _applyState(() {
        _projectFilesPath = state.path;
        _projectFilesParent = state.parent;
        _projectFileEntries = state.entries;
        _projectFilesLoading = false;
        final projectId = _selectedProjectId;
        if (projectId != null && state.path.isNotEmpty) {
          _projectFileController.remember(
            projectId: projectId,
            path: state.path,
          );
        }
      });
      return;
    }
    _applyState(() {
      _filePickerPath = state.path;
      _filePickerParent = state.parent;
      _filePickerEntries = state.entries;
      _filePickerLoading = false;
      _filePickerTimeoutTimer?.cancel();
      _showFilePicker = true;
    });
  }

  void _applyFileEditorState(RemoteFileEditorState state) {
    _editingFilePath = state.path;
    _fileEditorController.text = state.content;
    _fileEditorController.highlightEnabled = state.highlightEnabled;
    _fileEditorLoading = state.loading;
    _fileEditorSaving = state.saving;
    _fileEditorEditing = state.editing;
    _fileEditorEditable = state.editable;
  }

  void _saveEditingFile() {
    final path = _editingFilePath;
    if (path == null || _fileEditorSaving || !_fileEditorEditing) return;
    _applyState(() => _fileEditorSaving = true);
    _send(
      _projectFileController.writeEnvelope(
        path: path,
        content: _fileEditorController.text,
      ),
    );
  }

  void _beginEditingFile() {
    _applyState(() {
      _fileEditorOriginal = _fileEditorController.text;
      _fileEditorEditing = true;
    });
  }

  /// Discard unsaved edits and return to read-only view.
  void _cancelEditingFile() {
    if (_fileEditorSaving) return;
    _applyState(() {
      _fileEditorController.text = _fileEditorOriginal;
      _fileEditorEditing = false;
    });
  }

  void _focusTerminalSoon() {
    Future<void>.delayed(const Duration(milliseconds: 80), () {
      if (!mounted) return;
      _applyState(() {
        _keyboardRequested = true;
        _keyboardRequestSerial += 1;
        _keyboardShownSinceRequest = false;
      });
    });
  }

  /// Tapping the terminal body brings up the keyboard / focuses the input so
  /// typing flows directly (the toolbar key button still toggles it off).
  void _requestTerminalKeyboard() {
    if (_keyboardRequested) return;
    _applyState(() {
      _keyboardRequested = true;
      _keyboardRequestSerial += 1;
      _keyboardShownSinceRequest = false;
    });
  }

  void _toggleTerminalKeyboard() {
    if (_keyboardRequested || _keyboardVisible) {
      _applyState(() {
        _keyboardRequested = false;
        _keyboardRequestSerial += 1;
        _keyboardShownSinceRequest = false;
      });
      return;
    }
    _focusTerminalSoon();
  }

  void _focusTerminalViewSoon() {
    Future<void>.delayed(const Duration(milliseconds: 80), () {
      if (!mounted) return;
      if (_workspaceMode != WorkspaceMode.terminal || !_hasShownTerminal) {
        return;
      }
      _claimTerminalViewport();
      _flushPendingTerminalResize(force: true);
    });
  }
}
