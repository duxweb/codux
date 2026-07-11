part of '../home_page.dart';

/// Inbound remote-protocol handling for [_CoduxHomePageState]: the transport
/// envelope dispatch switch and the per-message handlers. Split into a part +
/// extension to keep the State class navigable; behaviour is unchanged.
///
/// These methods route mutations through [_CoduxHomePageState._applyState]
/// because `setState` is `@protected` and cannot be called from an extension.
extension _HomePageProtocol on HomeController {
  Future<void> _handleTransportEnvelope(
    RelayEnvelope message,
    StoredDevice target,
    int generation,
    RemoteTransport transport,
    int runtimeEpoch,
  ) async {
    try {
      final seq = message.seq;
      if (!_receiveSequenceGuard.accept(
        type: message.type,
        sessionId: message.sessionId,
        seq: seq,
      )) {
        CoduxLog.debug(
          '[codux-flutter-remote] drop duplicate seq=$seq type=${message.type} session=${message.sessionId ?? ''}',
        );
        return;
      }
      if (generation != _transportGeneration ||
          runtimeEpoch != _remoteRuntimeEpoch ||
          !identical(_activeTransport, transport)) {
        CoduxLog.debug(
          '[codux-flutter-remote] drop stale decoded envelope gen=$generation current=$_transportGeneration epoch=$runtimeEpoch currentEpoch=$_remoteRuntimeEpoch type=${message.type} session=${message.sessionId ?? ''}',
        );
        return;
      }
      _healthTimer?.cancel();
      _healthTimer = null;
      CoduxLog.debug(
        '[codux-flutter-remote] recv type=${message.type} session=${message.sessionId ?? ''}',
      );
      switch (message.type) {
        case final type when type == RemoteMessageType.hello:
          _reconnectAttempt = 0;
          CoduxLog.info('[codux-flutter-remote] hello received');
          if (!_transportConnected) {
            _applyState(() {
              _transportConnected = true;
              _hasShownTerminal = true;
              if (!_backgroundConnect) _status = _t('app.connecting');
            });
          }
          _sendHostInfoRequest(force: true);
          _startHostResponseProbe(reason: 'hello');
        case final type when type == RemoteMessageType.hostOffline:
          final payload = message.payload;
          final messageText = payload is Map
              ? '${payload['message'] ?? _t('connection.macDisconnected')}'
              : _t('connection.macDisconnected');
          _terminalInputBatcher.reset();
          _terminalInputSender.clear();
          _clearLatencyProbe();
          _applyState(() {
            _transportReady = false;
            _remoteSyncController.resetProtocolReady();
            _hostResponsive = false;
            _leaveTerminalUi();
            _resetRemoteSyncState();
            _status = messageText;
            _terminalBufferRetry.reset();
            _cancelTerminalBaselineRearm();
            _terminalOutputController.resetTransient();
            _setTerminalBufferLoading(false);
          });
          _clearConnectionGrace();
          _cancelHostResponseProbe();
          _scheduleReconnect(target);
        case final type when type == RemoteMessageType.transportPong:
          _handleTransportPong(message);
        case final type when type == RemoteMessageType.hostInfo:
          if (!_isCompatibleRemoteProtocol(message.payload)) {
            _failRemoteProtocol(target, message.payload);
            return;
          }
          final hostRuntimeChanged = _recordHostRuntimeInstance(
            message.payload,
          );
          _markHostResponsive(
            'host.info',
            transport: _deviceTransportKind(target),
          );
          _markActiveDeviceResponsive();
          _startLatencyProbe();
          final payload = message.payload;
          if (payload is Map) {
            _terminalBufferCapability = TerminalBufferCapability.fromHostInfo(
              payload,
            );
            _resourceSubscriptionCoordinator.configure(
              RemoteResourceSubscriptionCapability.fromHostInfo(payload),
            );
            if (payload['name'] != null) {
              _updateDevice(
                target.deviceId,
                hostName: payload['name']?.toString(),
              );
            }
          }
          _markRemoteProtocolReady(
            force:
                hostRuntimeChanged ||
                !_projectListLoaded ||
                !_terminalListLoaded,
          );
        case final type when type == RemoteMessageType.projectSelected:
          _handleProjectSelected(message);
        case final type when type == RemoteMessageType.projectList:
          if (_remoteStateVersions.accept(
            'projects',
            version: remoteStateVersionFromPayload(message.payload),
          )) {
            _handleProjectList(message);
          }
        case final type when type == RemoteMessageType.terminalList:
          if (_remoteStateVersions.accept(
            'terminals',
            version: remoteStateVersionFromPayload(message.payload),
          )) {
            _handleTerminalList(message);
          }
        case final type when type == RemoteMessageType.terminalCreated:
          _handleTerminalCreated(message);
        case final type when type == RemoteMessageType.terminalClosed:
          _handleTerminalClosed(message);
        case final type when type == RemoteMessageType.terminalViewportState:
          _handleTerminalViewportState(message);
        case final type when type == RemoteMessageType.worktreeList:
          _handleWorktreeList(message);
        case final type when type == RemoteMessageType.worktreeUpdated:
          _handleWorktreeUpdated(message);
        case final type when type == RemoteMessageType.terminalOutput:
          _handleTerminalOutput(message);
        case final type when type == RemoteMessageType.error:
          _handleRemoteError(message);
        case final type when type == RemoteMessageType.fileList:
          _handleFileList(message);
        case final type when type == RemoteMessageType.projectUpdated:
          _refreshLists();
          _showToast(_t('project.updated'));
        case final type when type == RemoteMessageType.aiStats:
          final payload = message.payload;
          if (payload is Map &&
              _remoteStateVersions.acceptProjectPayload(
                'ai.stats',
                payload,
                currentProjectId: _selectedProjectId,
              )) {
            final statsPayload = Map<String, dynamic>.from(payload);
            _applyState(() {
              _currentAIStats = AIStatsInfo.fromJson(statsPayload);
              _aiStatsLoading = false;
            });
          }
        case final type when type == RemoteMessageType.gitStatus:
          final payload = message.payload;
          final accepted = _remoteStateVersions.acceptProjectPayload(
            'git.status',
            payload,
            currentProjectId: _selectedProjectId,
          );
          if (accepted) {
            final status = remoteGitStatusFromPayload(message.payload);
            if (status != null) {
              final plan = _remoteRuntime.applyGitStatus(status);
              _applyRuntimePlan(plan, reason: 'git-status');
            }
          }
        case final type when type == RemoteMessageType.gitRead:
          _handleGitRead(message.payload);
        case final type when type == RemoteMessageType.aiSessionResult:
          _handleAISessionResult(message.payload);
        case final type when type == RemoteMessageType.sshListResult:
          _handleSshListResult(message.payload);
        case final type when type == RemoteMessageType.fileRead:
          _handleFileRead(message);
        case final type when type == RemoteMessageType.fileWritten:
          _applyState(() => _fileEditorSaving = false);
          _showToast(_t('file.saved'));
        case final type when type == RemoteMessageType.fileRenamed:
          _requestProjectFiles(_projectFilesPath);
          _showToast(_t('file.renamed'));
        case final type when type == RemoteMessageType.fileDeleted:
          _handleFileDeleted(message);
          _requestProjectFiles(_projectFilesPath);
          _showToast(_t('file.deleted'));
        case final type when type == RemoteMessageType.terminalUploaded:
          _handleTerminalUploaded(message);
        case final type when type == RemoteMessageType.terminalInputAck:
          _terminalInputSender.handleAck(message);
      }
    } catch (error) {
      CoduxLog.error('[codux-flutter-remote] receive failed: $error');
    }
  }

  void _handleProjectSelected(RelayEnvelope message) {
    _markHostResponsive('project.selected');
    _markActiveDeviceResponsive();
    final payload = message.payload;
    final projectId = payload is Map ? payload['projectId']?.toString() : null;
    final worktreeId = payload is Map
        ? payload['worktreeId']?.toString()
        : null;
    CoduxLog.info(
      '[codux-flutter-projects] project.selected project=${projectId ?? ''} worktree=${worktreeId ?? ''} current=${_selectedProjectId ?? ''}',
    );
    if (projectId != null && projectId.isNotEmpty) {
      _clearProjectSelectAck(projectId);
    }
    // Stale-response guard. Under high latency, switching A->B->A can land on B:
    // B's late `project.selected` arrives after the user re-selected A and would
    // yank the view back. Ignore a confirmation that no longer matches the
    // user's current selection. The initial host->client sync (no selection
    // yet) and any matching confirmation still apply.
    final current = _selectedProjectId;
    if (current != null &&
        current.isNotEmpty &&
        projectId != null &&
        projectId.isNotEmpty &&
        projectId != current) {
      CoduxLog.info(
        '[codux-flutter-projects] ignore stale project.selected '
        'project=$projectId current=$current',
      );
      return;
    }
    final plan = _remoteRuntime.projectSelected(
      projectId: projectId,
      worktreeId: worktreeId,
    );
    _applyRuntimePlan(plan, reason: 'project-selected');
  }

  void _handleProjectList(RelayEnvelope message) {
    _markHostResponsive('project.list');
    _markActiveDeviceResponsive();
    _markProjectListReceived();
    final payload = message.payload;
    final next = remoteProjectsFromPayload(payload);
    final worktrees = remoteWorktreesFromPayload(payload);
    final remoteSelectedProjectId = remoteSelectedProjectIdFromPayload(payload);
    final remoteSelectedWorktreeId = remoteSelectedWorktreeIdFromPayload(
      payload,
    );
    CoduxLog.info(
      '[codux-flutter-projects] recv project.list count=${next.length} remoteSelected=${remoteSelectedProjectId ?? ''} remoteWorktree=${remoteSelectedWorktreeId ?? ''} current=${_selectedProjectId ?? ''}',
    );
    final plan = _remoteRuntime.applyProjectList(
      projects: next,
      remoteSelectedProjectId: remoteSelectedProjectId,
      remoteSelectedWorktreeId: remoteSelectedWorktreeId,
      terminalVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
    );
    _applyRuntimePlan(plan, reason: 'missing-terminal');
    if (worktrees.isNotEmpty) {
      final worktreePlan = _remoteRuntime.applyWorktreeState(
        worktrees: worktrees,
        projectId: null,
        selectedWorktreeId: remoteSelectedWorktreeId,
        baseBranches: const [],
        defaultBaseBranch: null,
        allowRuntimeSelection: false,
        terminalVisible: _terminalDataVisible,
        terminalListLoaded: _terminalListLoaded,
      );
      if (mounted) _applyState(_syncRuntimeViewState);
      if (worktreePlan.hasRuntimeAction) {
        _applyRuntimePlan(worktreePlan, reason: 'project-list-worktrees');
      }
    }
    CoduxLog.debug(
      '[codux-flutter-projects] project.list count=${next.length} selected=${_selectedProjectId ?? ''}',
    );
    // Load AI history for the resolved selection. This is the auto-restore path
    // (background reconnect) where no manual project tap fires; guarded so it
    // only requests once per selected project.
    _requestAISessions();
    _refreshAIStats();
    _requestWorktreeList();
    unawaited(_cacheProjects(next));
  }

  void _handleTerminalList(RelayEnvelope message) {
    _markHostResponsive('terminal.list');
    _markActiveDeviceResponsive();
    _markTerminalListReceived();
    final next = remoteTerminalsFromPayload(message.payload);
    CoduxLog.debug(
      '[codux-flutter-terminal] recv terminal.list count=${next.length} selected=${_selectedProjectId ?? ''} worktree=${_selectedWorktreeId ?? ''} active=${_sessionId ?? ''} projects=${next.map((item) => item.projectId).toSet().join(',')}',
    );
    CoduxLog.debug(
      '[codux-flutter-terminal] terminal.list items=${next.map((item) => '${item.projectId}/${item.worktreeId ?? '-'}:${item.id}:${item.layoutOrder ?? -1}').join('|')}',
    );
    final plan = _remoteRuntime.applyTerminalList(
      terminals: next,
      terminalVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
    );
    if (plan.bindSessionId != null && _selectedProjectId != null) {
      _clearProjectSelectAck(_selectedProjectId!);
    }
    _applyRuntimePlan(plan, reason: 'missing-terminal');
  }

  void _handleTerminalCreated(RelayEnvelope message) {
    final terminal = remoteTerminalFromPayload(message.payload);
    if (terminal == null) return;
    CoduxLog.info(
      '[codux-flutter-terminal] created session=${terminal.id} project=${terminal.projectId} worktree=${terminal.worktreeId ?? ''} order=${terminal.layoutOrder ?? -1}',
    );
    final plan = _remoteRuntime.terminalCreated(terminal);
    _applyRuntimePlan(plan, reason: 'terminal-created');
  }

  void _handleTerminalClosed(RelayEnvelope message) {
    final closedSessionId = message.sessionId;
    if (closedSessionId == null) return;
    final plan = _remoteRuntime.removeTerminal(closedSessionId);
    _applyRuntimePlan(plan, reason: 'terminal-closed');
  }

  void _handleWorktreeList(RelayEnvelope message) {
    _markHostResponsive('worktree.list');
    final payload = message.payload;
    if (!_remoteStateVersions.acceptProjectPayload(
      'worktrees',
      payload,
      currentProjectId: _selectedProjectId,
    )) {
      return;
    }
    _applyWorktreeState(message, allowRuntimeSelection: false);
  }

  void _handleWorktreeUpdated(RelayEnvelope message) {
    if (!_remoteStateVersions.acceptProjectPayload(
      'worktrees',
      message.payload,
      currentProjectId: _selectedProjectId,
    )) {
      return;
    }
    _applyWorktreeState(message, allowRuntimeSelection: true);
  }

  void _applyWorktreeState(
    RelayEnvelope message, {
    required bool allowRuntimeSelection,
  }) {
    // Captured before _applyState resets it: a create reply (worktree.updated)
    // needs an explicit select afterwards so the host serves the new worktree's
    // terminal (otherwise the terminal view is blank until a manual tap).
    final wasCreating = _creatingWorktree;
    final worktreeState = _worktreeController.stateFromPayload(message.payload);
    if (worktreeState == null) return;
    final scopedProjectId = worktreeState.projectId;
    final currentProjectId =
        _selectedProjectId ?? _remoteRuntime.selectedProjectId;
    final effectiveProjectId = scopedProjectId ?? currentProjectId;
    final affectsCurrentProject =
        effectiveProjectId == null ||
        currentProjectId == null ||
        effectiveProjectId == currentProjectId;
    final canApplyRuntimeSelection =
        allowRuntimeSelection && affectsCurrentProject;
    final scopedWorktrees = effectiveProjectId == null
        ? worktreeState.worktrees
        : worktreeState.worktrees
              .where((worktree) => worktree.projectId == effectiveProjectId)
              .toList(growable: false);
    final confirmedWorktreeId = worktreeState.selectedWorktreeId;
    final pendingSwitch = _pendingWorktreeSwitch;
    final pendingWorktreeId = pendingSwitch?.worktreeId;
    final pendingCurrentProject =
        pendingSwitch != null &&
        canApplyRuntimeSelection &&
        (effectiveProjectId == null ||
            effectiveProjectId == pendingSwitch.projectId);
    if (allowRuntimeSelection && pendingWorktreeId != null) {
      CoduxLog.info(
        '[codux-flutter-worktree] apply type=${message.type} project=${effectiveProjectId ?? ''} current=${currentProjectId ?? ''} confirmed=${confirmedWorktreeId ?? ''} pendingProject=${pendingSwitch?.projectId ?? ''} pendingWorktree=$pendingWorktreeId currentProject=$pendingCurrentProject worktrees=${scopedWorktrees.map((item) => '${item.projectId}:${item.id}').join('|')}',
      );
    }
    final plan = _remoteRuntime.applyWorktreeState(
      worktrees: scopedWorktrees,
      projectId: effectiveProjectId,
      selectedWorktreeId: worktreeState.selectedWorktreeId,
      baseBranches: worktreeState.baseBranches,
      defaultBaseBranch: worktreeState.defaultBaseBranch,
      allowRuntimeSelection: canApplyRuntimeSelection,
      terminalVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
    );
    _applyState(() {
      _syncRuntimeViewState();
      _worktreeListLoading =
          pendingCurrentProject && !_pendingWorktreeSwitchHasActiveTerminal();
      _creatingWorktree = false;
    });
    if (plan.hasRuntimeAction) {
      _applyRuntimePlan(plan, reason: 'worktree-updated');
    }
    _closeTerminalSwitcherIfPendingWorktreeReady();

    // After creating a worktree the host auto-selects it but never received a
    // `select`, so its terminal isn't served. Re-run the bind for the now-
    // selected worktree, exactly like a manual tap.
    if (wasCreating && allowRuntimeSelection && affectsCurrentProject) {
      final selectedId = _selectedWorktreeId;
      if (selectedId != null) {
        final created = scopedWorktrees
            .where((worktree) => worktree.id == selectedId)
            .cast<RemoteWorktreeInfo?>()
            .firstWhere((worktree) => worktree != null, orElse: () => null);
        if (created != null) {
          _worktreeActions.selectWorktree(created, force: true);
        }
      }
    }
  }

  void _handleRemoteError(RelayEnvelope message) {
    final payload = message.payload;
    final code = payload is Map ? '${payload['code'] ?? ''}' : '';
    if (code == 'device_unauthorized') {
      _requireRepairPairing(payload);
      return;
    }
    final errorMessage =
        message.error ??
        (payload is Map
            ? '${payload['message'] ?? _t('remote.error')}'
            : _t('remote.error'));
    CoduxLog.warn(
      '[codux-flutter-remote] error type=${message.type} session=${message.sessionId ?? ''} message=$errorMessage',
    );
    final isActiveTerminalError =
        message.sessionId != null && message.sessionId == _sessionId;
    if (isActiveTerminalError) {
      _terminalBufferRetry.reset();
    }
    final terminalCreateCancelled =
        message.sessionId != null &&
        _remoteRuntime.cancelTerminalCreate(message.sessionId);
    _applyState(() {
      if (terminalCreateCancelled) _syncRuntimeViewState();
      _aiStatsLoading = false;
      _filePickerLoading = false;
      _worktreeListLoading = false;
      _creatingWorktree = false;
      _pendingWorktreeSwitch = null;
      _blockingLoadingMessage = null;
      if (isActiveTerminalError) {
        _terminalOutputController.resetSessionTransient(message.sessionId!);
        _setTerminalBufferLoading(false);
      }
      _status = errorMessage;
    });
  }

  void _handleFileList(RelayEnvelope message) {
    final listState = _projectFileController.listStateFromPayload(
      message.payload,
    );
    if (listState != null) {
      _applyFileListState(listState);
    }
  }

  void _handleFileRead(RelayEnvelope message) {
    final fileState = _projectFileController.readStateFromPayload(
      message.payload,
    );
    if (fileState == null) return;
    _applyState(() {
      _applyFileEditorState(fileState);
    });
    if (!fileState.editable) {
      _showToast(_t('file.readOnlyLarge'));
    }
  }

  void _handleFileDeleted(RelayEnvelope message) {
    final deletedPath = _projectFileController.deletedPathFromPayload(
      message.payload,
    );
    if (_projectFileController.shouldCloseEditorAfterDelete(
      deletedPath: deletedPath,
      editingPath: _editingFilePath,
    )) {
      _applyState(() => _editingFilePath = null);
    }
  }
}
