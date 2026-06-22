part of '../home_page.dart';

/// Session sync: project/terminal/worktree list requests + retries,
/// project-select handshake, runtime-view sync, terminal mount/restore and
/// runtime-plan application.
///
/// Split into a part + extension to keep the State class navigable; behaviour
/// is unchanged. Rebuilds route through [_CoduxHomePageState._applyState]
/// (`setState` is `@protected` and cannot be called from an extension).
extension _HomePageSync on HomeController {
  void _mountVisibleTerminal({required String reason}) {
    final sessionId = _sessionId;
    if (sessionId == null || _workspaceMode != WorkspaceMode.terminal) return;
    if (!_terminalViewportClaimable) return;
    final restored = _restoreTerminalSessionFromCache(sessionId);
    CoduxLog.debug(
      '[codux-flutter-terminal] mount session=$sessionId reason=$reason cached=$restored',
    );
    _focusTerminalViewSoon();
    if (restored) {
      // Re-request a baseline on foreground resume (output may have been
      // missed while backgrounded) or when live frames were actually dropped.
      // A cached, in-sync session being switched back to must NOT be reloaded:
      // replaying the trimmed raw history would clobber the live screen -- for
      // a TUI it loses the alt-screen / mouse-tracking modes set long ago (now
      // trimmed off the front), so the input border vanishes and scrolling
      // stops. The viewport re-claim/resize on mount triggers a fresh repaint,
      // so a gap-free switch stays current without a reload.
      final needsBaseline =
          reason == 'foreground' ||
          _terminalOutputController.hasSequenceGap(sessionId);
      if (_transportConnected && _remoteProtocolReady && needsBaseline) {
        final requested = _terminalBindingCoordinator.subscribeSessionBaseline(
          sessionId: sessionId,
          reason: 'mount-$reason',
          capability: _terminalBufferCapability,
          replaceActive: true,
        );
        if (requested) {
          _trackTerminalBaselineRequest(sessionId);
        }
      }
      return;
    }
    final projectId = _selectedProjectId;
    if (projectId == null) return;
    _terminalBindingCoordinator.replaceProjectSubscription(
      projectId: projectId,
      reason: 'mount-$reason',
      capability: _terminalBufferCapability,
      activeSessionId: _sessionId,
    );
  }

  bool _restoreTerminalSessionFromCache(String sessionId) {
    // The self-drawn renderer reads the cell snapshot straight from the output
    // controller, so a cached session just needs a repaint. Report whether any
    // local content exists so the caller can decide whether to show it.
    if (!_terminalOutputController.hasCachedOutput(sessionId)) return false;
    _terminalRepaint.tick();
    if (mounted) _applyState(() {});
    return true;
  }

  List<RemoteWorktreeInfo> _worktreesForProject(String projectId) {
    return _worktrees
        .where((worktree) => worktree.projectId == projectId)
        .toList(growable: false);
  }

  void _requestProjectList({bool resetRetry = false}) {
    if (!_remoteProtocolReady) return;
    if (resetRetry) {
      _projectListRetryTimer?.cancel();
      _projectListRetryTimer = null;
      _remoteSync.resetProjectListRetry();
    }
    if (!_remoteSync.shouldRequestProjectList(force: resetRetry)) return;
    _send(
      RelayEnvelope(type: RemoteMessageType.projectList),
      onResult: (_, result) {
        if (result != RemoteEnvelopeSendResult.delivered ||
            _projectListLoaded) {
          return;
        }
        _remoteSync.markProjectListRequested();
        CoduxLog.info(
          '[codux-flutter-projects] request project.list attempt=$_projectListRetryAttempt',
        );
        _scheduleProjectListRetry();
      },
    );
  }

  void _scheduleProjectListRetry() {
    if (!_transportReady || _projectListLoaded) return;
    _projectListRetryTimer?.cancel();
    if (!_remoteSync.canRetryProjectList(6)) return;
    final delay = Duration(
      milliseconds: (800 * (1 << _projectListRetryAttempt)).clamp(800, 5000),
    );
    _projectListRetryTimer = Timer(delay, () {
      if (!mounted || !_transportReady || _projectListLoaded) return;
      final attempt = _remoteSync.nextProjectListRetryAttempt();
      CoduxLog.info(
        '[codux-flutter-projects] retry project.list attempt=$attempt',
      );
      _requestProjectList();
    });
  }

  void _markProjectListReceived() {
    _remoteSync.markProjectListReceived();
    _projectListRetryTimer?.cancel();
    _projectListRetryTimer = null;
    CoduxLog.debug('[codux-flutter-projects] project.list received');
  }

  void _requestTerminalList({bool resetRetry = false}) {
    if (!_remoteProtocolReady) return;
    if (resetRetry) {
      _terminalListRetryTimer?.cancel();
      _terminalListRetryTimer = null;
      _remoteSync.resetTerminalListRetry();
    }
    if (!_remoteSync.shouldRequestTerminalList(force: resetRetry)) return;
    _send(
      RelayEnvelope(type: RemoteMessageType.terminalList),
      onResult: (_, result) {
        if (result != RemoteEnvelopeSendResult.delivered ||
            _terminalListLoaded) {
          return;
        }
        _remoteSync.markTerminalListRequested();
        CoduxLog.info(
          '[codux-flutter-terminal] request terminal.list attempt=$_terminalListRetryAttempt',
        );
        _scheduleTerminalListRetry();
      },
    );
  }

  void _requestWorktreeList({bool loading = false}) {
    final project = _selectedProject;
    if (!_remoteProtocolReady || project == null) return;
    if (loading) {
      _applyState(() {
        _worktreeListLoading = true;
      });
    }
    _send(_worktreeController.listEnvelope(project));
  }

  void _ensureSelectedProjectWorktrees({bool loading = false}) {
    final projectId = _selectedProjectId;
    if (projectId == null || _remoteRuntime.hasWorktreesForProject(projectId)) {
      return;
    }
    _requestWorktreeList(loading: loading);
  }

  void _scheduleTerminalListRetry() {
    if (!_transportReady || _terminalListLoaded) return;
    _terminalListRetryTimer?.cancel();
    if (!_remoteSync.canRetryTerminalList(6)) return;
    final delay = Duration(
      milliseconds: (800 * (1 << _terminalListRetryAttempt)).clamp(800, 5000),
    );
    _terminalListRetryTimer = Timer(delay, () {
      if (!mounted || !_transportReady || _terminalListLoaded) return;
      final attempt = _remoteSync.nextTerminalListRetryAttempt();
      CoduxLog.info(
        '[codux-flutter-terminal] retry terminal.list attempt=$attempt',
      );
      _requestTerminalList();
    });
  }

  void _markTerminalListReceived() {
    _remoteSync.markTerminalListReceived();
    _terminalListRetryTimer?.cancel();
    _terminalListRetryTimer = null;
    CoduxLog.debug('[codux-flutter-terminal] terminal.list received');
  }

  void _markActiveDeviceResponsive() {
    final device = _activeDevice;
    if (device != null) _rememberActiveDevice(device);
  }

  bool _sendProjectSelect(String projectId, {required String reason}) {
    final scope = _remoteRuntime.terminalScopeForProject(projectId);
    final payload = <String, Object>{
      'projectId': projectId,
      if (scope?.worktreeId != null && scope!.worktreeId!.trim().isNotEmpty)
        'worktreeId': scope.worktreeId!,
      if (scope?.projectPath != null && scope!.projectPath!.trim().isNotEmpty)
        'projectPath': scope.projectPath!,
    };
    CoduxLog.info(
      '[codux-flutter-projects] send project.select reason=$reason project=$projectId worktree=${payload['worktreeId'] ?? ''}',
    );
    final sent = _send(
      RelayEnvelope(type: RemoteMessageType.projectSelect, payload: payload),
      onResult: (message, result) {
        if (result == RemoteEnvelopeSendResult.delivered) {
          _scheduleProjectSelectAckTimeout(projectId);
          return;
        }
        _remoteRuntime.clearPendingProjectSelectSent(projectId);
        if (!mounted || _disposing) return;
        CoduxLog.warn(
          '[codux-flutter-projects] project.select delivery failed reason=$reason project=$projectId result=${result.name}',
        );
      },
    );
    if (sent) {
      _remoteRuntime.markProjectSelectSent(projectId);
    } else {
      CoduxLog.warn(
        '[codux-flutter-projects] project.select not sent reason=$reason project=$projectId connected=$_transportConnected ready=$_transportReady',
      );
    }
    return sent;
  }

  void _scheduleProjectSelectAckTimeout(String projectId) {
    _projectSelectAckTimers.remove(projectId)?.cancel();
    _projectSelectAckTimers[projectId] = Timer(const Duration(seconds: 3), () {
      _projectSelectAckTimers.remove(projectId);
      if (!mounted || !_transportReady || !_remoteProtocolReady) return;
      if (_remoteRuntime.pendingProjectSelect(includeSent: true) != projectId) {
        return;
      }
      CoduxLog.warn(
        '[codux-flutter-projects] project.select ack timeout project=$projectId',
      );
      _remoteRuntime.clearPendingProjectSelectSent(projectId);
      _drivePendingProjectSelect(reason: 'ack-timeout');
      _requestTerminalList(resetRetry: true);
    });
  }

  void _clearProjectSelectAck(String projectId) {
    if (_remoteRuntime.pendingProjectSelect(includeSent: true) != projectId) {
      return;
    }
    _projectSelectAckTimers.remove(projectId)?.cancel();
  }

  void _drivePendingProjectSelect({required String reason}) {
    final projectId = _remoteRuntime.pendingProjectSelect();
    if (projectId == null) return;
    _sendProjectSelect(projectId, reason: reason);
  }

  void _resubscribeVisibleTerminal({required String reason}) {
    if (!_terminalViewportClaimable) return;
    _terminalBindingCoordinator.resubscribeVisibleTerminal(
      transportConnected: _transportConnected,
      protocolReady: _remoteProtocolReady,
      activeSessionId: _sessionId,
      selectedProjectId: _selectedProjectId,
      capability: _terminalBufferCapability,
      reason: reason,
      ensureBoundBaseline: (sessionId, baselineRequested) {
        if (baselineRequested) {
          _trackTerminalBaselineRequest(sessionId);
        }
        _terminalBindingCoordinator.ensureBoundTerminalHasBaseline(
          sessionId: sessionId,
          baselineRequested: baselineRequested,
          reason: reason,
          capability: _terminalBufferCapability,
        );
      },
    );
  }

  void _syncRuntimeViewState() {
    _projects = _remoteRuntime.projects;
    _terminals = _remoteRuntime.terminals;
    _selectedProjectId = _remoteRuntime.selectedProjectId;
    _sessionId = _remoteRuntime.activeSessionId;
    _creatingTerminalProjectId = _remoteRuntime.creatingTerminalProjectId;
    if (_creatingTerminalProjectId == null) {
      _creatingTerminalLayoutKind = null;
    }
  }

  void _setTerminalBufferLoading(
    bool loading, {
    double? progress,
    RemoteTerminalBufferPhase phase = RemoteTerminalBufferPhase.requesting,
  }) {
    _terminalBufferPhase = loading ? phase : RemoteTerminalBufferPhase.idle;
    _terminalBufferProgress = loading ? progress : null;
  }

  String _terminalHistoryLoadingText() {
    if (_terminalBufferPhase == RemoteTerminalBufferPhase.rendering) {
      return _t('terminal.renderingHistory');
    }
    final progress = _terminalBufferProgress;
    if (progress == null) return _t('terminal.loadingHistory');
    final percent = (progress.clamp(0.0, 1.0) * 100).round();
    return _t(
      'terminal.loadingHistoryProgress',
      params: {'percent': '$percent'},
    );
  }

  void _applyRuntimePlan(RemoteRuntimePlan plan, {String reason = ''}) {
    _runtimeCoordinator.applyRuntimePlan(plan, reason: reason);
  }

  void _bindActiveTerminalAfterProtocolReady({required String reason}) {
    _runtimeCoordinator.bindActiveTerminalAfterProtocolReady(reason: reason);
  }

  Future<void> _cacheProjects(List<ProjectInfo> projects) async {
    final device = _activeDevice;
    if (device == null) return;
    try {
      await _storage.saveCachedProjects(device, projects);
    } catch (error) {
      CoduxLog.warn('[codux-flutter-projects] cache save failed: $error');
    }
  }

  void _refreshLists() {
    _refreshTransportRoute(reason: 'manual-refresh');
    _sendHostInfoRequest(force: true);
    _requestProjectList(resetRetry: true);
    _requestTerminalList(resetRetry: true);
    _requestGitStatus();
    _refreshAIStats();
    _requestAISessions(force: true);
    _requestSshProfiles();
  }

  void _rebuildCurrentTerminal() {
    final projectId = _selectedProjectId;
    if (projectId == null) {
      _showToast(_t('project.selectFirst'));
      return;
    }
    String? closingSessionId;
    TerminalInfo? closingTerminal;
    final current = _currentTerminal();
    final projectTerminals = _terminals
        .where(
          (terminal) =>
              terminal.projectId == projectId &&
              _isAccessibleTerminal(terminal),
        )
        .toList();
    if (current != null &&
        current.projectId == projectId &&
        _isAccessibleTerminal(current)) {
      closingSessionId = current.id;
      closingTerminal = current;
    } else if (projectTerminals.isNotEmpty) {
      closingTerminal = projectTerminals.first;
      closingSessionId = closingTerminal.id;
    }
    final shouldCreateReplacement = projectTerminals.length > 1;
    final canCloseCurrent = projectTerminals.length > 1;
    if (closingSessionId != null && canCloseCurrent) {
      final plan = _remoteRuntime.removeTerminal(closingSessionId);
      _applyRuntimePlan(plan, reason: 'rebuild-terminal');
      _sendTerminalEnvelope(
        RelayEnvelope(
          type: RemoteMessageType.terminalClose,
          sessionId: closingSessionId,
        ),
        terminal: closingTerminal,
      );
    } else {
      _clearTerminal();
    }
    if (shouldCreateReplacement) {
      _createTerminal(projectId);
    }
    _showToast(_t('terminal.rebuilding'));
  }

  void _ensureTerminalForSelectedProject() {
    final plan = _remoteRuntime.ensureTerminalForSelectedProject(
      terminalVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
    );
    _applyRuntimePlan(plan, reason: 'missing-terminal');
  }

}
