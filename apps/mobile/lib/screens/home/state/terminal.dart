part of '../home_page.dart';

/// Terminal session handling for [_CoduxHomePageState]: output/upload
/// effects, buffer-resync bookkeeping, viewport claim/release, resize, input
/// send, and terminal create/lookup. Split into a part + extension to keep the
/// State class navigable; behaviour is unchanged. Rebuilds route through
/// [_CoduxHomePageState._applyState] (`setState` is `@protected`).
extension _HomePageTerminal on HomeController {
  void _handleTerminalViewportState(RelayEnvelope message) {
    final applied = _terminalViewportController.applyRemoteState(message);
    if (!applied) return;
    final sessionId = message.sessionId?.trim();
    if (sessionId == null || sessionId.isEmpty) return;
    // Recover the viewport if the desktop took it back while we're the active
    // viewer. The host hands ownership to the desktop when our lease lapses (or
    // when the desktop actively repaints/resizes its own copy). Left there, the
    // host renders at the desktop's wide grid (e.g. 111 cols) and our 47-col
    // screen fills with reflowed/garbled bytes that only a manual project switch
    // or re-enter would clear. Re-send our resize: it re-claims, snaps the host
    // PTY back to our grid, and triggers a fresh keyframe -- so the screen
    // self-heals. A bare claim wouldn't resize the host or refetch the screen.
    // Only a local (desktop) owner is recovered; another remote viewer is left
    // alone, and once we own it the next state reports a remote owner so this
    // converges instead of ping-ponging.
    if (sessionId == _sessionId &&
        _terminalViewportInteractive &&
        _terminalViewportClaimable) {
      final owner = _terminalViewportController.owner ?? '';
      if (owner.isNotEmpty && !owner.startsWith('remote:')) {
        final cols = _terminalViewportController.pendingCols;
        final rows = _terminalViewportController.pendingRows;
        if (cols != null && rows != null && cols > 0 && rows > 0) {
          _sendTerminalResize(cols, rows, sessionId: sessionId);
        } else {
          _claimTerminalViewport(sessionId: sessionId);
        }
      }
    }
    final size = _terminalViewportController.reportedSize(sessionId);
    if (size == null || size.cols <= 0 || size.rows <= 0) return;
    // Size the local cell screen to the host's ROW count but the phone's COLUMN
    // count. Rows: the host keeps its own (taller) row count for remote viewers,
    // so a TUI anchors its input box near the host's last row; feeding that into
    // a screen clamped to the phone's shorter viewport collapsed those rows onto
    // each other (overlapping text, a cursor stranded on the status line, the
    // input box clipped off-screen). Adopt the host rows and let the renderer
    // show the bottom window + scroll. Cols: the phone DRIVES the width (the
    // host reflows to it), so keep our measured cols -- adopting the host's
    // transient desktop width (e.g. 111 while the desktop briefly owns the
    // viewport) would clip the grid horizontally. resizeScreen bumps the render
    // generation, so a static (non-generating) session repaints too.
    final cols = _terminalViewportController.pendingCols ?? size.cols;
    _terminalOutputController.resizeScreen(
      sessionId,
      cols: cols,
      rows: size.rows,
    );
    _terminalRepaint.tick();
  }

  void _handleTerminalOutput(RelayEnvelope message) {
    final effects = _terminalOutputController.accept(
      message,
      activeSessionId: _sessionId,
    );
    _applyTerminalOutputEffects(effects);
  }

  void _applyTerminalOutputEffects(List<RemoteTerminalOutputEffect> effects) {
    for (final effect in effects) {
      switch (effect.kind) {
        case RemoteTerminalOutputEffectKind.loading:
          // A `receiving` loading tick means a baseline chunk just assembled --
          // proof the transfer is advancing. Tell the retry coordinator so a
          // slow but live high-latency transfer is not mistaken for a stall and
          // wiped. (The effect carries no session id; it is only emitted for the
          // active session, so the pending id is the active one.)
          if (effect.loading &&
              effect.phase == RemoteTerminalBufferPhase.receiving) {
            _terminalBufferRetry.noteProgress(_sessionId);
          }
          if (mounted) {
            _applyState(
              () => _setTerminalBufferLoading(
                effect.loading,
                progress: effect.progress,
                phase: effect.phase ?? RemoteTerminalBufferPhase.requesting,
              ),
            );
          } else {
            _setTerminalBufferLoading(
              effect.loading,
              progress: effect.progress,
              phase: effect.phase ?? RemoteTerminalBufferPhase.requesting,
            );
          }
        case RemoteTerminalOutputEffectKind.ack:
          final sessionId = effect.sessionId;
          if (sessionId != null) {
            _ackTerminalOutputIfNeeded(
              sessionId,
              effect.outputSeq,
              effect.bufferLength,
            );
          }
        case RemoteTerminalOutputEffectKind.markBufferReceived:
          _markTerminalBufferReceived(effect.sessionId);
        case RemoteTerminalOutputEffectKind.sessionUpdated:
          // The self-drawn renderer reads the Rust cell snapshot directly; tick
          // the shared notifier so it repaints only that subtree (no full-page
          // setState / keyboard-inset / layout recompute per live frame).
          _terminalRepaint.tick();
        case RemoteTerminalOutputEffectKind.requestBaselineResync:
          final sessionId = effect.sessionId;
          if (sessionId != null) {
            _requestTerminalGapResync(sessionId);
          }
      }
    }
  }

  /// A live-output sequence gap was detected for [sessionId]: lost frames can
  /// only be repaired by re-requesting the baseline. Inactive sessions stay
  /// marked in the output controller and resync when they are next bound.
  void _requestTerminalGapResync(String sessionId) {
    if (!mounted || _disposing) return;
    if (sessionId != _sessionId) return;
    if (_terminalOutputController.hasActiveBufferRequest(sessionId)) return;
    CoduxLog.warn(
      '[codux-flutter-terminal] sequence gap resync session=$sessionId',
    );
    final requested = _terminalBindingCoordinator.subscribeSessionBaseline(
      sessionId: sessionId,
      reason: 'sequence-gap',
      capability: _terminalBufferCapability,
      replaceActive: true,
    );
    if (requested) {
      _trackTerminalBaselineRequest(sessionId);
    }
  }

  void _trackTerminalBaselineRequest(String sessionId) {
    _terminalBufferRetry.trackWhilePending(
      sessionId,
      send: _retryTerminalBaseline,
      hasPendingRequest: _terminalOutputController.hasActiveBufferRequest,
    );
  }

  void _handleTerminalUploaded(RelayEnvelope message) {
    final payload = message.payload;
    if (payload is Map && payload['path'] != null) {
      final completion = _terminalUploadCompletion;
      if (completion != null && !completion.isCompleted) {
        completion.complete();
      }
      _terminalUploadCompletion = null;
      final inserted = payload['inserted'] == true;
      final mode = payload['mode']?.toString();
      final tool = payload['tool']?.toString();
      final kind = payload['kind']?.toString();
      if (!inserted) {
        final path = '${payload['path']}';
        _insertTerminalText('$path ');
      }
      _applyState(() {
        _terminalUploadLoading = false;
        _terminalUploadStatus = '';
        _status = kind == 'file'
            ? _t('upload.fileSentPath')
            : mode == 'clipboard'
            ? _t(
                'upload.imageSentTool',
                params: {'tool': tool ?? _t('upload.aiTool')},
              )
            : _t('upload.imageSentPath');
      });
    }
  }

  StoredDevice? _updateDevice(String deviceId, {String? hostName}) {
    final result = _deviceController.updateHostName(
      devices: _devices,
      activeDevice: _activeDevice,
      deviceId: deviceId,
      hostName: hostName,
    );
    final updated = result.updatedDevice;
    if (updated != null) {
      _applyState(() {
        _devices = result.state.devices;
        _activeDevice = result.state.activeDevice;
      });
      unawaited(_storage.saveDevices(result.state.devices));
    }
    return updated;
  }

  bool _retryTerminalBaseline(String sessionId) {
    if (!mounted || _sessionId != sessionId) return false;
    CoduxLog.info('[codux-flutter-terminal] baseline retry session=$sessionId');
    return _terminalBindingCoordinator.subscribeSessionBaseline(
      sessionId: sessionId,
      reason: 'baseline-retry',
      capability: _terminalBufferCapability,
      replaceActive: true,
    );
  }

  /// Slow heartbeat after the fast retry burst gave up. Re-requests the baseline
  /// for the still-visible session; if it lands the retry coordinator stops, if
  /// it gives up again this re-arms, so a merely-slow (still connected) link
  /// heals on its own without a manual project switch / reconnect.
  void _scheduleTerminalBaselineRearm(String sessionId) {
    _terminalBaselineRearmTimer?.cancel();
    _terminalBaselineRearmTimer = Timer(const Duration(seconds: 10), () {
      _terminalBaselineRearmTimer = null;
      if (!mounted || _disposing) return;
      if (_sessionId != sessionId || !_transportConnected) return;
      final requested = _terminalBindingCoordinator.subscribeSessionBaseline(
        sessionId: sessionId,
        reason: 'baseline-rearm',
        capability: _terminalBufferCapability,
        replaceActive: true,
      );
      if (requested) {
        _trackTerminalBaselineRequest(sessionId);
      } else {
        // Transport busy right now -- check back on the next heartbeat.
        _scheduleTerminalBaselineRearm(sessionId);
      }
    });
  }

  void _cancelTerminalBaselineRearm() {
    _terminalBaselineRearmTimer?.cancel();
    _terminalBaselineRearmTimer = null;
  }

  String _nextTerminalBufferRequestId(String sessionId) {
    _terminalBufferRequestCounter += 1;
    return '${DateTime.now().microsecondsSinceEpoch}-$_terminalBufferRequestCounter-$sessionId';
  }

  void _markTerminalBufferReceived(String? sessionId) {
    _terminalBufferRetry.markReceived(
      sessionId: sessionId,
      activeSessionId: _sessionId,
    );
    // A complete baseline landed: stop the slow heal heartbeat for it.
    if (sessionId == null || sessionId == _sessionId) {
      _cancelTerminalBaselineRearm();
    }
    if (_terminalBufferLoading && mounted) {
      _applyState(() => _setTerminalBufferLoading(false));
    }
    CoduxLog.info(
      '[codux-flutter-terminal] terminal.buffer received session=${sessionId ?? ''}',
    );
    if (sessionId != null) {
      _closeTerminalSwitcherAfterPendingWorktreeBuffer(sessionId);
    }
  }

  void _closeTerminalSwitcherAfterPendingWorktreeBuffer(String sessionId) {
    if (sessionId != _sessionId) return;
    _closeTerminalSwitcherIfPendingWorktreeReady();
  }

  void _closeTerminalSwitcherIfPendingWorktreeReady() {
    if (!_showTerminalSwitcher || !_pendingWorktreeSwitchHasActiveTerminal()) {
      return;
    }
    _pendingWorktreeSwitch = null;
    _closeTerminalSwitcher();
  }

  bool _pendingWorktreeSwitchHasActiveTerminal() {
    final pending = _pendingWorktreeSwitch;
    if (pending == null) return false;
    if (_selectedProjectId != pending.projectId ||
        _selectedWorktreeId != pending.worktreeId) {
      return false;
    }
    final active = _remoteRuntime.activeTerminal();
    if (active == null || active.projectId != pending.projectId) {
      return false;
    }
    return _terminalWorktreeId(active) == pending.worktreeId;
  }

  String _terminalWorktreeId(TerminalInfo terminal) {
    final worktreeId = terminal.worktreeId?.trim();
    if (worktreeId != null && worktreeId.isNotEmpty) return worktreeId;
    return terminal.projectId;
  }

  void _clearTerminal() {
    _terminalSelectedText = null;
    if (mounted) _applyState(() {});
  }

  void _sendTerminalResize(int cols, int rows, {String? sessionId}) {
    // Cache the measured grid up front -- the very first measurement arrives
    // before any session exists (the pane is laid out, then create is issued),
    // so recording it here lets `_createTerminal` seed the host PTY with the
    // phone's width and avoid the connect-time duplicate prompt line.
    _terminalViewportController.recordMeasured(cols, rows);
    final id = sessionId ?? _sessionId;
    if (id == null) return;
    final resize = _terminalViewportController.resize(
      sessionId: id,
      cols: cols,
      rows: rows,
      keyboardVisible: _keyboardVisible,
    );
    // The self-drawn terminal renders the host's grid, so the host PTY must
    // match the mobile viewport. Claim the viewport when the terminal is the
    // active view (rather than waiting for explicit input) so this resize
    // actually reaches the host; otherwise a repaint/TUI app keeps painting at
    // the host's old row count and leaves the bottom of the screen blank.
    if (!_terminalViewportInteractive && _terminalViewportClaimable) {
      _claimTerminalViewport(sessionId: id);
    }
    if (!_terminalViewportClaimable || !_terminalViewportInteractive) return;
    final terminal = _terminalById(id);
    if (!_canResizeTerminal(terminal)) return;
    if (resize == null) {
      CoduxLog.debug(
        '[codux-flutter-terminal] resize skip duplicate measured=${cols}x$rows keyboard=$_keyboardVisible session=$id',
      );
      return;
    }
    CoduxLog.info(
      '[codux-flutter-terminal] send viewport.resize size=${resize.cols}x${resize.rows} measured=${cols}x$rows keyboard=$_keyboardVisible session=$id',
    );
    _sendTerminalEnvelope(
      RelayEnvelope(
        type: RemoteMessageType.terminalViewportResize,
        sessionId: id,
        payload: {'cols': resize.cols, 'rows': resize.rows},
      ),
      terminal: terminal,
    );
    _terminalViewportController.markSent(id, resize);
  }

  void _flushPendingTerminalResize({bool force = false, String? sessionId}) {
    final id = sessionId ?? _sessionId;
    if (id == null) return;
    if (!_terminalViewportClaimable) return;
    if (!_terminalViewportInteractive) return;
    final terminal = _terminalById(id);
    if (!_canResizeTerminal(terminal)) return;
    final resize = _terminalViewportController.flushPending(
      sessionId: id,
      force: force,
    );
    if (resize == null) return;
    CoduxLog.info(
      '[codux-flutter-terminal] flush viewport.resize size=${resize.cols}x${resize.rows} force=$force session=$id',
    );
    _sendTerminalEnvelope(
      RelayEnvelope(
        type: RemoteMessageType.terminalViewportResize,
        sessionId: id,
        payload: {'cols': resize.cols, 'rows': resize.rows},
      ),
      terminal: terminal,
    );
    _terminalViewportController.markSent(id, resize);
  }

  void _claimTerminalViewport({String? sessionId, bool throttled = false}) {
    final id = sessionId ?? _sessionId;
    if (id == null || id.trim().isEmpty) return;
    if (!_terminalViewportClaimable) return;
    final terminal = _terminalById(id);
    if (terminal == null || !_canResizeTerminal(terminal)) return;
    // High-frequency input/scroll claims are throttled: we already own the
    // viewport interactively, so re-asserting it on every keystroke / fling tick
    // just floods the wire. Skip the network send (but keep the interactive
    // flag) when we claimed this same session very recently. Non-throttled
    // callers (keyboard keepalive, reactive reclaim after a desktop steal,
    // resize) always send.
    if (throttled &&
        _terminalViewportInteractive &&
        _lastViewportClaimSession == id &&
        _lastViewportClaimAt != null &&
        DateTime.now().difference(_lastViewportClaimAt!) <
            _viewportClaimThrottle) {
      return;
    }
    _terminalViewportInteractive = true;
    _lastViewportClaimAt = DateTime.now();
    _lastViewportClaimSession = id;
    _sendTerminalEnvelope(
      RelayEnvelope(
        type: RemoteMessageType.terminalViewportClaim,
        sessionId: id,
      ),
      terminal: terminal,
    );
  }

  void _releaseTerminalViewport({String? sessionId}) {
    final id = sessionId ?? _sessionId;
    if (id == null || id.trim().isEmpty) return;
    final terminal = _terminalById(id);
    if (terminal == null || !_canResizeTerminal(terminal)) return;
    _terminalViewportInteractive = false;
    _sendTerminalEnvelope(
      RelayEnvelope(
        type: RemoteMessageType.terminalViewportRelease,
        sessionId: id,
      ),
      terminal: terminal,
    );
  }

  void _queueTerminalTyping(String data) {
    if (data.isEmpty) return;
    _terminalInputBatcher.add(data);
  }

  void _sendTerminalKey(String data) {
    if (data.isEmpty) return;
    _terminalInputBatcher.flush();
    _sendInputNow(data, source: 'key');
  }

  void _insertTerminalText(String text) {
    if (text.isEmpty) return;
    _terminalInputBatcher.flush();
    _sendInputNow(
      codux_terminal_core.terminalInsertInput(text),
      source: 'insert',
    );
  }

  void _sendInputNow(String data, {required String source}) {
    if (data.isEmpty) return;
    var id = _sessionId;
    if (id == null) {
      CoduxLog.debug(
        '[codux-flutter-input] no session, ensure terminal before input',
      );
      _ensureTerminalForSelectedProject();
      id = _sessionId;
    }
    if (id == null) {
      _applyState(() => _status = _t('terminal.createOrSelectFirst'));
      return;
    }
    _claimTerminalViewport(sessionId: id, throttled: true);
    _flushPendingTerminalResize(force: true, sessionId: id);
    _terminalInputSender.send(
      sessionId: id,
      data: data,
      source: source,
      retry: data != '\u0003',
    );
  }

  void _sendTerminalOutputAck(
    String sessionId,
    int outputSeq,
    int? bufferLength,
  ) {
    final payload = <String, Object>{'outputSeq': outputSeq};
    if (bufferLength != null) {
      payload['bufferLength'] = bufferLength;
    }
    _sendTerminalEnvelope(
      RelayEnvelope(
        type: RemoteMessageType.terminalOutputAck,
        sessionId: sessionId,
        payload: payload,
      ),
    );
  }

  void _ackTerminalOutputIfNeeded(
    String sessionId,
    int? outputSeq,
    int? bufferLength,
  ) {
    if (outputSeq == null) return;
    _sendTerminalOutputAck(sessionId, outputSeq, bufferLength);
  }

  void _createTerminal([String? projectId, String layoutKind = 'split']) {
    final target =
        projectId ??
        _selectedProjectId ??
        (_projects.isNotEmpty ? _projects.first.id : null);
    if (target == null) {
      _applyState(() => _status = _t('project.noAvailable'));
      return;
    }
    if (_creatingTerminalProjectId == target) return;
    final normalizedLayoutKind = layoutKind.trim().toLowerCase() == 'tab'
        ? 'tab'
        : 'split';
    final scope = _remoteRuntime.terminalScopeForProject(target);
    _remoteRuntime.beginTerminalCreate(
      projectId: target,
      worktreeId: scope?.worktreeId,
      layoutKind: normalizedLayoutKind,
    );
    _creatingTerminalLayoutKind = normalizedLayoutKind;
    _applyState(_syncRuntimeViewState);
    // Spawn the host PTY at the phone's measured grid (when known) so the shell
    // draws its prompt once at the final width. Without this the host spawns at
    // its 100x32 default, prints the prompt, then the phone's first
    // viewport.resize triggers a SIGWINCH redraw -> a duplicate/ghost first
    // prompt line. The follow-up resize then matches the spawn size and the
    // host short-circuits it (no redraw).
    final spawnCols = _terminalViewportController.pendingCols;
    final spawnRows = _terminalViewportController.pendingRows;
    _send(
      RelayEnvelope(
        type: RemoteMessageType.terminalCreate,
        payload: {
          'projectId': target,
          if (scope?.worktreeId != null && scope!.worktreeId!.trim().isNotEmpty)
            'worktreeId': scope.worktreeId!,
          if (scope?.projectPath != null &&
              scope!.projectPath!.trim().isNotEmpty)
            'projectPath': scope.projectPath!,
          'command': '',
          'layoutKind': layoutKind,
          if (spawnCols != null && spawnRows != null &&
              spawnCols > 0 && spawnRows > 0) ...{
            'cols': spawnCols,
            'rows': spawnRows,
          },
        },
      ),
    );
  }

  bool _isAccessibleTerminal(TerminalInfo terminal) {
    return RemoteRuntimeStore.isAccessibleTerminal(terminal);
  }

  TerminalInfo? _currentTerminal() {
    return _remoteRuntime.activeTerminal();
  }

  TerminalInfo? _terminalById(String sessionId) {
    for (final terminal in _terminals) {
      if (terminal.id == sessionId) return terminal;
    }
    return null;
  }

  RemoteTerminalScope? _terminalScopeForSession(
    String sessionId, {
    TerminalInfo? terminal,
  }) {
    return _remoteRuntime.terminalScopeForSession(
      sessionId,
      terminal: terminal,
    );
  }

  RelayEnvelope? _scopeTerminalEnvelope(
    RelayEnvelope message, {
    TerminalInfo? terminal,
  }) {
    final sessionId = message.sessionId?.trim();
    if (sessionId == null || sessionId.isEmpty) return message;
    final scope = _terminalScopeForSession(sessionId, terminal: terminal);
    if (scope == null) {
      CoduxLog.warn(
        '[codux-flutter-terminal] drop ${message.type} reason=missing-scope session=$sessionId',
      );
      return null;
    }
    return scopedTerminalEnvelope(message, scope);
  }

  bool _canResizeTerminal(TerminalInfo? terminal) {
    return terminal != null && _isAccessibleTerminal(terminal);
  }

  List<TerminalInfo> _currentProjectTerminals() {
    return _remoteRuntime.currentProjectTerminals();
  }

}
