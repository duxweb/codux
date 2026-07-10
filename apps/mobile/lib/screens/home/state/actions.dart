part of '../home_page.dart';

/// UI-triggered actions: device management, project selection, clipboard
/// + upload + voice, update/about, log viewer and edge-back navigation.
///
/// Split into a part + extension to keep the State class navigable; behaviour
/// is unchanged. Rebuilds route through [_CoduxHomePageState._applyState]
/// (`setState` is `@protected` and cannot be called from an extension).
extension _HomePageActions on HomeController {
  Future<void> _refreshDeviceList() async {
    final device = _activeDevice;
    if (device == null) return;
    if (!_isConnected) {
      _connect(device);
      await Future<void>.delayed(const Duration(milliseconds: 350));
      return;
    }
    _refreshTransportRoute(reason: 'manual-refresh');
    _sendHostInfoRequest(force: true);
    _requestProjectList(resetRetry: true);
    _requestTerminalList(resetRetry: true);
    await Future<void>.delayed(const Duration(milliseconds: 350));
  }

  Future<void> _removeDevice(StoredDevice device) async {
    final result = _deviceController.remove(
      devices: _devices,
      activeDevice: _activeDevice,
      device: device,
    );
    if (result.removedActive) {
      _shouldReconnect = false;
      _transportConnected = false;
      unawaited(_closeActiveTransport());
      _clearLatencyProbe();
      if (_remoteRuntime.cancelTerminalCreate()) {
        _syncRuntimeViewState();
      }
    }
    await _saveDevices(result.state.devices);
    if (result.state.devices.isEmpty) {
      _applyState(() => _showTerminal = false);
    }
  }

  void _openDeviceTerminal(StoredDevice device) {
    if (device.deviceId != _activeDevice?.deviceId || !_isConnected) return;
    unawaited(
      _pushCupertinoPage(() {
        _showTerminal = true;
        _workspaceMode = WorkspaceMode.terminal;
        _terminalReady = false;
        _setTerminalBufferLoading(false);
      }).then((_) {
        if (!mounted) return;
        _ensureTerminalForSelectedProject();
      }),
    );
    if (!_projectListLoaded) {
      _requestProjectList(resetRetry: true);
    }
    if (!_terminalListLoaded) {
      _requestTerminalList(resetRetry: true);
    }
    _ensureTerminalForSelectedProject();
    _mountVisibleTerminal(reason: 'open');
    _focusTerminalViewSoon();
  }

  Future<void> _editDevice(StoredDevice device) async {
    final next = await showDialog<StoredDevice>(
      context: context,
      builder: (ctx) => DeviceEditDialog(
        device: device,
        title: _t('device.editTitle'),
        nameLabel: _t('device.nameLabel'),
        cancelLabel: _t('app.cancel'),
        saveLabel: _t('common.save'),
      ),
    );
    if (next == null) return;
    final nextState = _deviceController.replace(
      devices: _devices,
      device: next,
      activeDevice: _activeDevice,
    );
    await _saveDevices(nextState.devices);
    if (_activeDevice?.deviceId == next.deviceId) {
      _connect(next, true);
    }
  }

  void _onProjectSelected(ProjectInfo project) {
    final projectChanged = _selectedProjectId != project.id;
    final resetTerminal = projectChanged && _workspaceMode == WorkspaceMode.terminal;
    if (resetTerminal) {
      _releaseTerminalViewport();
    }
    CoduxLog.info(
      '[codux-flutter-projects] user select project=${project.id} previous=${_selectedProjectId ?? ''} changed=$projectChanged mode=${_workspaceMode.name} terminalVisible=$resetTerminal currentSession=${_sessionId ?? ''}',
    );
    _applyState(() {
      _currentAIStats = null;
      _projectFileEntries = [];
      _projectFilesPath = project.path ?? '';
      _projectFilesParent = null;
      if (projectChanged) {
        _projectFileController.forget(project.id);
        _pendingWorktreeSwitch = null;
        // Drop the previous project's review/diff selection so the review panel
        // doesn't show stale files after switching projects.
        _gitDiffPath = null;
        _gitDiff = null;
      }
    });
    final plan = _remoteRuntime.userSelectProject(
      project: project,
      terminalVisible: resetTerminal,
    );
    _applyRuntimePlan(plan, reason: 'user-select');
    _ensureSelectedProjectWorktrees(loading: _showTerminalSwitcher);
    _requestAISessions(force: true);
    if (_workspaceMode == WorkspaceMode.stats) {
      _requestAIStats();
      return;
    }
    _refreshAIStats();
    if (_workspaceMode == WorkspaceMode.files) {
      _requestProjectFiles(project.path);
      return;
    }
    // Review and git panels read git.status; refresh it for the new project so
    // they don't stay stale until the view is toggled away and back.
    if (_workspaceMode == WorkspaceMode.review || _workspaceMode == WorkspaceMode.git) {
      _requestGitStatus();
      return;
    }
    if (resetTerminal) {
      return;
    }
    final current = _terminals.any(
      (item) =>
          item.id == _sessionId &&
          item.projectId == project.id &&
          _isAccessibleTerminal(item),
    );
    if (!current) {
      _ensureTerminalForSelectedProject();
    }
  }

  Future<void> _pasteToTerminal() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (data?.text?.isNotEmpty == true) {
      _insertTerminalText(data!.text!);
    }
  }

  Future<void> _copyTerminalSelection() async {
    final prefs = AppPreferences.of(context);
    final text = _terminalSelectedText?.trim().isNotEmpty == true
        ? _terminalSelectedText!
        : _visibleTerminalText();
    final copied = text.trim().isNotEmpty;
    if (copied) {
      await Clipboard.setData(ClipboardData(text: text));
    }
    _showSnack(
      copied ? prefs.t('toolbar.copyDone') : prefs.t('toolbar.copyEmpty'),
    );
  }

  String _visibleTerminalText() {
    final sessionId = _sessionId;
    if (sessionId == null) return '';
    return _terminalOutputController.cachedOutput(sessionId)?.trimRight() ?? '';
  }

  Future<void> _startVoiceInput() async {
    if (_showVoiceOverlay) return;
    _applyState(() => _showVoiceOverlay = true);
  }

  Future<void> _chooseUploadForTerminal() async {
    if (_terminalUploadLoading) return;
    CoduxLog.info(
      '[codux-flutter-upload] choose start connected=$_isConnected path=$_connectionPath session=$_sessionId',
    );
    if (!_canUploadOverCurrentPath) {
      _showSnack(_t('upload.directRequired'));
      _applyState(() => _status = _t('upload.directRequired'));
      return;
    }
    final prefs = AppPreferences.of(context);
    final source = await showModalBottomSheet<TerminalUploadSource>(
      context: context,
      backgroundColor: AppColors.bgElevated,
      barrierColor: AppColors.backdrop,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(AppRadius.lg)),
      ),
      builder: (context) => TerminalUploadSourceSheet(
        fileLabel: prefs.t('upload.chooseFile'),
        imageLabel: prefs.t('upload.chooseImage'),
      ),
    );
    CoduxLog.info('[codux-flutter-upload] source selected source=$source');
    if (source == null || !mounted) return;
    await _uploadPickedFileToTerminal(source);
  }

  Future<void> _uploadPickedFileToTerminal(TerminalUploadSource source) async {
    if (_terminalUploadLoading) return;
    if (!_canUploadOverCurrentPath) {
      _showSnack(_t('upload.directRequired'));
      _applyState(() => _status = _t('upload.directRequired'));
      return;
    }
    final id = _sessionId;
    if (id == null) {
      _applyState(() => _status = _t('terminal.createOrSelectFirst'));
      return;
    }
    final result = await FilePicker.pickFiles(
      type: source == TerminalUploadSource.image
          ? FileType.image
          : FileType.any,
      allowMultiple: false,
      withData: true,
    );
    final files = result?.files;
    final picked = files == null || files.isEmpty ? null : files.single;
    CoduxLog.info(
      '[codux-flutter-upload] picker result selected=${picked != null} source=$source',
    );
    if (picked == null) return;
    if (picked.size > 20 * 1024 * 1024) {
      _showSnack(_t('upload.fileTooLarge'));
      return;
    }
    final bytes =
        picked.bytes ??
        (picked.path == null ? null : await File(picked.path!).readAsBytes());
    if (bytes == null) {
      _showSnack(_t('upload.fileReadFailed'));
      return;
    }
    if (bytes.isEmpty) {
      CoduxLog.warn('[codux-flutter-upload] picked file is empty');
      return;
    }
    if (!_canUploadOverCurrentPath) {
      _showSnack(_t('upload.directRequired'));
      _applyState(() => _status = _t('upload.directRequired'));
      return;
    }
    _terminalUploadCompletion?.completeError(
      StateError('Terminal upload superseded'),
    );
    final uploadCompletion = Completer<void>();
    _terminalUploadCompletion = uploadCompletion;
    final uploadingMessage = _t(terminalUploadUploadingKey(source));
    _applyState(() {
      _terminalUploadLoading = true;
      _terminalUploadStatus = uploadingMessage;
      _status = _terminalUploadStatus;
    });
    try {
      final sent = await _activeTransport?.sendTerminalUpload(
        deviceId: _activeDevice?.deviceId ?? '',
        sessionId: id,
        name: picked.name,
        mime: terminalUploadMime(
          picked.name,
          image: source == TerminalUploadSource.image,
        ),
        bytes: bytes,
        kind: terminalUploadKind(source),
      );
      CoduxLog.info(
        '[codux-flutter-upload] blob enqueue result=$sent session=$id name=${picked.name} bytes=${bytes.length}',
      );
      if (sent != true) {
        throw StateError('Upload transport is not connected');
      }
      CoduxLog.info(
        '[codux-flutter-upload] blob sent session=$id name=${picked.name} bytes=${bytes.length}',
      );
      if (!mounted) return;
      final insertingMessage = _t(terminalUploadInsertingKey(source));
      _applyState(() {
        _terminalUploadStatus = insertingMessage;
        _status = insertingMessage;
      });
      await uploadCompletion.future.timeout(const Duration(seconds: 30));
    } catch (error) {
      CoduxLog.warn('[codux-flutter-upload] upload failed: $error');
      if (!mounted) return;
      if (_terminalUploadCompletion == uploadCompletion) {
        _terminalUploadCompletion = null;
      }
      _applyState(() {
        _terminalUploadLoading = false;
        _terminalUploadStatus = '';
        _status = '${_t('remote.error')}: $error';
      });
    }
  }

  Future<void> _checkUpdate() async {
    _applyState(() {
      _status = _t('update.checking');
      _blockingLoadingMessage = _t('update.loading');
    });
    try {
      final result = await _updateCheckService.check();
      if (!result.available) {
        final toastKey = result.toastKey;
        if (toastKey != null && toastKey.isNotEmpty) {
          _showToast(_t(toastKey, params: result.toastParams));
        }
        return;
      }
      if (!mounted) return;
      showDialog<void>(
        // ignore: use_build_context_synchronously  (mounted == view.mounted)
        context: context,
        builder: (ctx) => UpdateAvailableDialog(
          title: _t(
            'update.foundTitle',
            params: {'version': result.version ?? ''},
          ),
          body: _t(
            result.isIos ? 'update.foundBodyAppStore' : 'update.foundBody',
            params: {'version': result.currentVersion},
          ),
          laterLabel: _t('common.later'),
          actionLabel: result.isIos
              ? _t('common.openAppStore')
              : _t('common.openGithub'),
          onOpen: () {
            if (result.url.isNotEmpty) _openUrl(result.url);
          },
        ),
      );
    } catch (error) {
      _showToast(_t('update.failed', params: {'reason': '$error'}));
    } finally {
      if (mounted) _applyState(() => _blockingLoadingMessage = null);
    }
  }

  Future<void> _showAboutDialogNow() async {
    final info = await PackageInfo.fromPlatform();
    if (!mounted) return;
    showDialog<void>(
      // ignore: use_build_context_synchronously  (mounted == view.mounted)
      context: context,
      builder: (ctx) => CoduxAboutDialog(
        title: _t('app.about'),
        body: _t('app.aboutText'),
        versionText: 'v${info.version}+${info.buildNumber}',
        closeLabel: _t('app.close'),
        onOpenGithub: () => _openUrl('https://github.com/duxweb/codux-flutter'),
      ),
    );
  }

  Future<void> _openUrl(String value) async {
    final uri = Uri.parse(value);
    if (!await launchUrl(uri, mode: LaunchMode.externalApplication)) {
      await launchUrl(uri);
    }
  }

  void _showLogViewer() {
    showDialog<void>(
      context: context,
      builder: (ctx) => DebugLogDialog(
        title: _t('app.debugLogs'),
        emptyLabel: _t('logs.empty'),
        clearLabel: _t('logs.clear'),
        copyLabel: _t('logs.copy'),
        exportLabel: _t('logs.export'),
        closeLabel: _t('app.close'),
        onCopy: (text) async {
          await Clipboard.setData(ClipboardData(text: text));
          if (mounted) _showToast(_t('logs.copied'));
        },
        onExport: _exportLogs,
      ),
    );
  }

  Future<void> _exportLogs(String text) async {
    try {
      await _logExportService.export(text, shareText: _t('logs.shareText'));
      if (mounted) _showToast(_t('logs.exported'));
    } catch (error) {
      if (mounted) _showToast('${_t('logs.exportFailed')}: $error');
    }
  }

  void _confirmRemoveDevice(StoredDevice device) {
    showDialog<bool>(
      context: context,
      builder: (ctx) => DeviceRemoveDialog(
        title: _t('app.removeDevice'),
        message: _t(
          'app.removeDeviceConfirm',
          params: {'name': device.hostName ?? device.name},
        ),
        cancelLabel: _t('app.cancel'),
        removeLabel: _t('app.remove'),
      ),
    ).then((confirmed) {
      if (confirmed == true) _removeDevice(device);
    });
  }

  void _handleBackNavigation() {
    if (_editingFilePath != null) {
      _applyState(() {
        _editingFilePath = null;
        _fileEditorLoading = false;
        _fileEditorSaving = false;
        _fileEditorEditing = false;
      });
      return;
    }
    if (_showScanner) {
      _applyState(() => _showScanner = false);
      return;
    }
    if (_showFilePicker) {
      _filePickerTimeoutTimer?.cancel();
      _applyState(() => _showFilePicker = false);
      return;
    }
    if (_showProjectForm) {
      _applyState(() => _showProjectForm = false);
      return;
    }
    if (_showSettings) {
      _popCupertinoPage(() {
        _showSettings = false;
      });
      return;
    }
    if (_showTerminalSwitcher) {
      _popCupertinoPage(() {
        _showTerminalSwitcher = false;
      }).then((_) {
        if (mounted) _mountVisibleTerminal(reason: 'switcher-back');
      });
      return;
    }
    if (_pendingPairing != null) {
      _cancelPairing();
      return;
    }
    if (_showTerminal) {
      _releaseTerminalViewport();
      _popCupertinoPage(() {
        _showTerminal = false;
        _workspaceMode = WorkspaceMode.terminal;
      });
      return;
    }
    _disconnectTransport(status: _t('app.disconnected'), closeTerminal: true);
    SystemNavigator.pop();
  }

  void _handleWorkspaceEdgeDragStart(DragStartDetails details) {
    if (!Platform.isIOS ||
        (!_showTerminal && !_showSettings && !_showTerminalSwitcher)) {
      return;
    }
    final edgeWidth = MediaQuery.viewPaddingOf(context).left + 24.0;
    final startX = details.localPosition.dx;
    if (startX > edgeWidth) {
      _edgeBackDragStartX = null;
      return;
    }
    _edgeBackDragStartX = startX;
    _edgeBackDragDeltaX = 0;
    _edgeBackDragDeltaY = 0;
    _edgeBackController.stop();
  }

  void _handleWorkspaceEdgeDragUpdate(DragUpdateDetails details) {
    if (_edgeBackDragStartX == null) return;
    _edgeBackDragDeltaX += details.delta.dx;
    _edgeBackDragDeltaY += details.delta.dy;
    final width = MediaQuery.sizeOf(context).width;
    if (width <= 0) return;
    _edgeBackController.value = (_edgeBackDragDeltaX / width).clamp(0.0, 1.0);
  }

  void _handleWorkspaceEdgeDragEnd(DragEndDetails details) {
    if (_edgeBackDragStartX == null) return;
    final dragX = _edgeBackDragDeltaX;
    final dragY = _edgeBackDragDeltaY.abs();
    final velocityX = details.velocity.pixelsPerSecond.dx;
    _edgeBackDragStartX = null;
    _edgeBackDragDeltaX = 0;
    _edgeBackDragDeltaY = 0;
    final width = MediaQuery.sizeOf(context).width;
    final progress = width <= 0 ? 0.0 : (dragX / width).clamp(0.0, 1.0);
    final shouldComplete =
        dragX > 72 &&
        dragX > dragY * 1.4 &&
        (velocityX > 260 || progress > 0.34);
    if (shouldComplete) {
      unawaited(_completeCupertinoPageBack());
    } else {
      unawaited(
        _edgeBackController.animateBack(
          0,
          duration: const Duration(milliseconds: 180),
          curve: Curves.easeOutCubic,
        ),
      );
    }
  }

  Future<void> _completeCupertinoPageBack() async {
    await _edgeBackController.animateTo(
      1,
      duration: const Duration(milliseconds: 180),
      curve: Curves.easeOutCubic,
    );
    if (!mounted) return;
    final closingTerminal = !_showSettings && !_showTerminalSwitcher;
    if (closingTerminal) {
      _releaseTerminalViewport();
    }
    final closingSwitcher = _showTerminalSwitcher;
    _applyState(() {
      if (_showSettings) {
        _showSettings = false;
      } else if (_showTerminalSwitcher) {
        _showTerminalSwitcher = false;
      } else {
        _showTerminal = false;
      }
      _workspaceMode = WorkspaceMode.terminal;
    });
    _edgeBackController.value = 0;
    if (closingSwitcher) {
      _mountVisibleTerminal(reason: 'switcher-edge-back');
    }
  }

  Future<void> _pushCupertinoPage(VoidCallback updateState) async {
    _edgeBackController.value = 1;
    _applyState(updateState);
    await _edgeBackController.animateBack(
      0,
      duration: const Duration(milliseconds: 260),
      curve: Curves.easeOutCubic,
    );
  }

  Future<void> _popCupertinoPage(VoidCallback updateState) async {
    if (!Platform.isIOS) {
      _applyState(updateState);
      return;
    }
    await _edgeBackController.animateTo(
      1,
      duration: const Duration(milliseconds: 220),
      curve: Curves.easeOutCubic,
    );
    if (!mounted) return;
    _applyState(updateState);
    _edgeBackController.value = 0;
  }

  void _cancelWorkspaceEdgeBack() {
    _edgeBackDragStartX = null;
    _edgeBackDragDeltaX = 0;
    _edgeBackDragDeltaY = 0;
    unawaited(
      _edgeBackController.animateBack(
        0,
        duration: const Duration(milliseconds: 180),
        curve: Curves.easeOutCubic,
      ),
    );
  }

}
