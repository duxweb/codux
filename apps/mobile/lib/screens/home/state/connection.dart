part of '../home_page.dart';

/// Transport connection lifecycle: route refresh, grace/probe timers,
/// host-info + protocol-ready gating, host-responsive tracking, latency probe,
/// runtime reset, connect/reconnect, initial requests and envelope send.
///
/// Split into a part + extension to keep the State class navigable; behaviour
/// is unchanged. Rebuilds route through [_CoduxHomePageState._applyState]
/// (`setState` is `@protected` and cannot be called from an extension).
extension _HomePageConnection on HomeController {
  void _startNetworkRouteRefresh() {
    _networkRouteRefreshController.start();
  }

  void _refreshTransportRoute({required String reason}) {
    final device = _activeDevice;
    if (device == null || !_shouldReconnect) return;
    final now = DateTime.now();
    final lastRefresh = _lastTransportRefreshAt;
    if (lastRefresh != null &&
        now.difference(lastRefresh) < const Duration(seconds: 8)) {
      CoduxLog.info('[codux-flutter-remote] refresh skipped reason=$reason');
      return;
    }
    _lastTransportRefreshAt = now;
    CoduxLog.info('[codux-flutter-remote] refresh route reason=$reason');
    final transport = _activeTransport;
    if (!_transportConnected || transport == null) {
      _connect(device, true);
      return;
    }
    _sendHostInfoRequest(force: true);
  }

  void _clearConnectionGrace() {
    _connectionGraceTimer?.cancel();
    _connectionGraceTimer = null;
    _connectionGraceUntil = null;
  }

  void _startConnectionGrace({
    required String reason,
    Duration duration = const Duration(seconds: 8),
  }) {
    if (!_shouldReconnect || !_appInForeground) return;
    _connectionGraceTimer?.cancel();
    _connectionGraceUntil = DateTime.now().add(duration);
    CoduxLog.info(
      '[codux-flutter-remote] grace reason=$reason until=${_connectionGraceUntil!.toIso8601String()} transport=$_lastTransportState lastConnectedAt=${_lastConnectedAt?.toIso8601String() ?? 'null'}',
    );
    _connectionGraceTimer = Timer(duration, () {
      if (!mounted || _disposing) return;
      if (_connectionGraceUntil == null) return;
      if (DateTime.now().isBefore(_connectionGraceUntil!)) return;
      _applyState(() {
        _connectionGraceUntil = null;
      });
      CoduxLog.info('[codux-flutter-remote] grace expired reason=$reason');
    });
  }

  void _markTransportConnected(String transport) {
    _lastTransportState = transport;
    _lastConnectedAt = DateTime.now();
    _clearConnectionGrace();
  }

  void _cancelHostResponseProbe() {
    _hostResponseTimer?.cancel();
    _hostResponseTimer = null;
  }

  void _startHostResponseProbe({
    required String reason,
    Duration duration = _remoteStartupProbeTimeout,
    bool restart = true,
  }) {
    final device = _activeDevice;
    final generation = _transportGeneration;
    final startedAtSerial = _hostResponseSerial;
    if (!_transportConnected || device == null) return;
    if (!restart && _hostResponseTimer != null) return;
    _cancelHostResponseProbe();
    CoduxLog.info(
      '[codux-flutter-remote] host probe start reason=$reason timeoutMs=${duration.inMilliseconds}',
    );
    _hostResponseTimer = Timer(duration, () {
      if (!mounted || _disposing || !_appInForeground) return;
      if (_transportGeneration != generation ||
          !_transportConnected ||
          _hostResponseSerial != startedAtSerial) {
        return;
      }
      _failHostConnection(device, 'host_response_timeout:$reason');
    });
  }

  void _markTransportOpen({String? path}) {
    _reconnectAttempt = 0;
    _applyState(() {
      _transportConnected = true;
      _hasShownTerminal = true;
      if (path != null) _connectionPath = path;
      if (!_backgroundConnect && !_transportReady) {
        _status = _t('app.connecting');
      }
    });
  }

  void _markTransportPathDetail(
    String path, {
    String? endpoint,
    String? relayUrl,
  }) {
    final connected = path != 'none';
    _applyState(() {
      _transportConnected = connected;
      if (!connected) {
        _transportReady = false;
        _connectionEndpoint = '';
        _connectionRelayUrl = '';
      }
      if (connected) {
        _hasShownTerminal = true;
        if (!_backgroundConnect && !_transportReady) {
          _status = _t('app.connecting');
        }
        final cleanedEndpoint = cleanRemoteTransportEndpoint(endpoint ?? '');
        if (cleanedEndpoint.isNotEmpty) {
          _connectionEndpoint = cleanedEndpoint;
        } else if (path != _connectionPath) {
          _connectionEndpoint = '';
        }
        final cleanedRelayUrl = cleanRemoteTransportEndpoint(relayUrl ?? '');
        if (cleanedRelayUrl.isNotEmpty) {
          _connectionRelayUrl = cleanedRelayUrl;
        } else if (path != 'relay' && path != _connectionPath) {
          _connectionRelayUrl = '';
        }
      }
      _connectionPath = path;
    });
  }

  bool _isCompatibleRemoteProtocol(Object? payload) {
    if (payload is! Map) return false;
    return payload['protocolVersion'] == _remoteProtocolVersion;
  }

  void _markRemoteProtocolReady({bool force = false}) {
    if (!_remoteSyncController.markProtocolReady(force: force)) return;
    CoduxLog.info('[codux-flutter-remote] protocol ready force=$force');
    _sendInitialTransportRequests(force: force);
    _ensureTerminalForSelectedProject();
    _bindActiveTerminalAfterProtocolReady(reason: 'protocol-ready');
    _drivePendingProjectSelect(reason: 'protocol-ready');
    _resubscribeVisibleTerminal(reason: 'protocol-ready');
  }

  void _failRemoteProtocol(StoredDevice target, Object? payload) {
    final version = payload is Map ? '${payload['protocolVersion'] ?? ''}' : '';
    CoduxLog.warn(
      '[codux-flutter-remote] incompatible protocol expected=$_remoteProtocolVersion received=$version host=${target.hostId} device=${target.deviceId}',
    );
    _shouldReconnect = false;
    final shouldPrompt = _protocolBlockedHostIds.add(target.hostId);
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    _cancelHostResponseProbe();
    _clearConnectionGrace();
    _clearLatencyProbe();
    _transportConnected = false;
    unawaited(_closeActiveTransport());
    _terminalInputBatcher.reset();
    _terminalInputSender.clear();
    _terminalBindingCoordinator.reset();
    final message = _t('connection.upgradeRequired');
    _applyState(() {
      _transportReady = false;
      _remoteSyncController.resetProtocolReady();
      _hostResponsive = false;
      _backgroundConnect = false;
      _showTerminal = false;
      _workspaceMode = WorkspaceMode.terminal;
      _resetRemoteSyncState();
      _showTerminalSwitcher = false;
      _status = message;
      _terminalBufferRetry.reset();
      _terminalOutputController.resetTransient();
      _setTerminalBufferLoading(false);
    });
    if (shouldPrompt) {
      _showToast(message);
    }
  }

  void _stopRemoteConnectionForAuthChange() {
    _shouldReconnect = false;
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    _connectInFlight = false;
    _connectInFlightKey = null;
    _cancelHostResponseProbe();
    _cancelRemoteSyncTimers();
    _clearConnectionGrace();
    _clearLatencyProbe();
    _transportConnected = false;
    unawaited(_closeActiveTransport());
    _terminalInputBatcher.reset();
    _terminalInputSender.clear();
    _terminalBindingCoordinator.reset();
    _terminalBufferRetry.reset();
  }

  void _requireRepairPairing(Object? payload) {
    final code = payload is Map ? '${payload['code'] ?? ''}' : '';
    CoduxLog.warn('[codux-flutter-remote] authorization failed code=$code');
    _stopRemoteConnectionForAuthChange();
    _applyState(() {
      _transportReady = false;
      _remoteSyncController.resetProtocolReady();
      _hostResponsive = false;
      _backgroundConnect = false;
      _leaveTerminalUi();
      _status = _t('pair.repairRequired');
      _terminalOutputController.resetTransient();
      _setTerminalBufferLoading(false);
    });
  }

  void _markHostResponsive(String source, {String? transport}) {
    final wasResponsive = _hostResponsive;
    if (mounted && !_disposing) {
      _applyState(() {
        _transportConnected = true;
        _transportReady = true;
        _hostResponsive = true;
        _connectInFlight = false;
        _connectInFlightKey = null;
        if (!_backgroundConnect) _status = _t('app.connected');
      });
    } else {
      _transportConnected = true;
      _transportReady = true;
      _hostResponsive = true;
      _connectInFlight = false;
      _connectInFlightKey = null;
    }
    _hostResponseSerial += 1;
    _cancelHostResponseProbe();
    _markTransportConnected(transport ?? _deviceTransportKind(_activeDevice));
    if (!wasResponsive) {
      CoduxLog.info('[codux-flutter-remote] host responsive source=$source');
    }
  }

  String _deviceTransportKind(StoredDevice? device) {
    if (device == null) return RemoteTransportKind.iroh;
    final kind = remotePreferredTransportKind(
      device.transports,
      pairing: false,
    );
    return kind.isEmpty ? RemoteTransportKind.iroh : kind;
  }

  bool _hasConnectableTransport(StoredDevice device) {
    final kind = remotePreferredTransportKind(
      device.transports,
      pairing: false,
    );
    return kind.isNotEmpty && device.transportByKind(kind) != null;
  }

  void _clearLatencyProbe() {
    _latencyProbeTimer?.cancel();
    _latencyProbeTimer = null;
    _latencyProbeSentAt.clear();
    _latencyProbeCounter = 0;
    if (_latencyMs == null) return;
    _latencyMs = null;
  }

  void _pauseLatencyProbe() {
    // App lifecycle pauses the ping loop but does not mean the route is gone.
    // Keep the last measured RTT visible until the transport explicitly closes
    // or a new measurement replaces it.
    _latencyProbeTimer?.cancel();
    _latencyProbeTimer = null;
  }

  void _startLatencyProbe() {
    if (_latencyProbeTimer != null || !_transportConnected) return;
    _sendLatencyProbe();
    _latencyProbeTimer = Timer.periodic(
      _remoteLatencyProbeInterval,
      (_) => _sendLatencyProbe(),
    );
  }

  void _sendLatencyProbe() {
    final transport = _activeTransport;
    final device = _activeDevice;
    if (transport == null || device == null || !_transportConnected) return;
    final now = DateTime.now();
    _latencyProbeSentAt.removeWhere(
      (_, sentAt) => now.difference(sentAt) > _remoteLatencyProbeTimeout,
    );
    final id = '${now.microsecondsSinceEpoch}-${++_latencyProbeCounter}';
    _latencyProbeSentAt[id] = now;
    unawaited(
      transport.send(
        RelayEnvelope(
          type: RemoteMessageType.transportPing,
          deviceId: device.deviceId,
          payload: {'id': id},
        ).toJson(),
      ),
    );
  }

  void _handleTransportPong(RelayEnvelope message) {
    final payload = message.payload;
    final id = payload is Map ? '${payload['id'] ?? ''}' : '';
    if (id.isEmpty) return;
    final sentAt = _latencyProbeSentAt.remove(id);
    if (sentAt == null) return;
    final rtt = DateTime.now().difference(sentAt).inMilliseconds;
    CoduxLog.debug(
      '[codux-flutter-remote] app latency rtt=${rtt}ms path=$_connectionPath',
    );
    if (!mounted || _disposing || _latencyMs == rtt) return;
    _applyState(() => _latencyMs = rtt);
  }

  void _sendHostInfoRequest({bool force = false}) {
    if (!_remoteSyncController.shouldSendHostInfo(
      transportReady: _transportReady,
      transportConnected: _transportConnected,
      force: force,
    )) {
      return;
    }
    CoduxLog.info('[codux-flutter-remote] request host.info');
    _send(
      RelayEnvelope(type: RemoteMessageType.hostInfo),
      onResult: (_, result) {
        if (result == RemoteEnvelopeSendResult.delivered) {
          _remoteSyncController.markHostInfoSent();
        }
      },
    );
  }

  void _failHostConnection(StoredDevice target, String reason) {
    CoduxLog.warn(
      '[codux-flutter-remote] host unavailable reason=$reason host=${target.hostId} device=${target.deviceId}',
    );
    _remoteRuntimeEpoch += 1;
    _disconnectTransport(
      status: _t('connection.failedRetry'),
      closeTerminal: true,
      notifyHost: false,
    );
    if (_appSuspended || !_appInForeground) {
      CoduxLog.info(
        '[codux-flutter-remote] reconnect deferred reason=$reason appSuspended=$_appSuspended',
      );
      return;
    }
    _scheduleReconnect(target);
  }

  void _resetRemoteSyncState() {
    _remoteRuntimeEpoch += 1;
    _cancelRemoteSyncTimers();
    _remoteSyncController.resetSyncForCurrentGeneration();
    _remoteRuntime.reset();
    _terminalBindingCoordinator.reset();
    _terminalViewportController.resetScroll();
    _terminalViewportInteractive = false;
    _syncRuntimeViewState();
  }

  void _resetRemoteRuntime({bool keepProjects = false}) {
    _remoteRuntimeEpoch += 1;
    _remoteRuntime.reset(keepProjects: keepProjects);
    _terminalBindingCoordinator.reset();
    _terminalViewportController.resetScroll();
    _terminalViewportInteractive = false;
    _syncRuntimeViewState();
  }

  void _resetRemoteRuntimeAfterHostRestart(String reason) {
    CoduxLog.info('[codux-flutter-remote] reset runtime reason=$reason');
    _remoteRuntimeEpoch += 1;
    _cancelRemoteSyncTimers();
    _remoteSyncController.resetSyncForCurrentGeneration();
    _remoteSyncController.resetProtocolReady();
    _terminalBindingCoordinator.reset();
    _terminalInputBatcher.reset();
    _terminalInputSender.clear();
    _terminalBufferRetry.reset();
    _terminalOutputController.resetAll();
    _terminalRepaint.tick();
    _terminalViewportController.resetScroll();
    _terminalViewportInteractive = false;
    _receiveSequenceGuard.reset();
    _receiveChain = Future<void>.value();
    _hostResponsive = false;
    _remoteRuntime.reset(keepProjects: true);
    _syncRuntimeViewState();
    _setTerminalBufferLoading(false);
    _clearTerminal();
  }

  bool _recordHostRuntimeInstance(Object? payload) {
    if (payload is! Map) return false;
    final next = payload['runtimeInstanceId']?.toString().trim();
    if (next == null || next.isEmpty) return false;
    final previous = _hostRuntimeInstanceId;
    _hostRuntimeInstanceId = next;
    if (previous == null || previous == next) return false;
    _resetRemoteRuntimeAfterHostRestart(
      'host-runtime-instance-changed:$previous->$next',
    );
    return true;
  }

  void _cancelRemoteSyncTimers() {
    _projectListRetryTimer?.cancel();
    _projectListRetryTimer = null;
    _terminalListRetryTimer?.cancel();
    _terminalListRetryTimer = null;
    for (final timer in _projectSelectAckTimers.values) {
      timer.cancel();
    }
    _projectSelectAckTimers.clear();
  }

  void _leaveTerminalUi() {
    _showTerminal = false;
    _workspaceMode = WorkspaceMode.terminal;
    _showTerminalSwitcher = false;
    _keyboardRequested = false;
    _keyboardRequestSerial += 1;
    _keyboardShownSinceRequest = false;
    _keyboardVisible = false;
  }

  void _disconnectTransport({
    required String status,
    bool closeTerminal = false,
    bool notifyHost = true,
    bool resetRuntime = false,
  }) {
    if (notifyHost && _transportConnected) {
      _notifyHostBeforeTransportClose();
    }
    _cancelHostResponseProbe();
    _clearConnectionGrace();
    _lastConnectedAt = null;
    _healthTimer?.cancel();
    _healthTimer = null;
    _clearLatencyProbe();
    _transportConnected = false;
    unawaited(_closeActiveTransport());
    _terminalInputBatcher.reset();
    _terminalInputSender.clear();
    if (resetRuntime) {
      _terminalOutputController.resetAll();
      _terminalRepaint.tick();
      _terminalBindingCoordinator.reset();
    }
    _applyState(() {
      _transportReady = false;
      if (resetRuntime) {
        _remoteSyncController.resetProtocolReady();
      }
      _hostResponsive = false;
      _backgroundConnect = false;
      if (closeTerminal) {
        _leaveTerminalUi();
      }
      _status = status;
      _terminalBufferRetry.reset();
      _setTerminalBufferLoading(false);
    });
    if (resetRuntime || closeTerminal) {
      _clearTerminal();
    }
  }

  void _recoverForegroundState() {
    if (!_transportReady) {
      final device = _activeDevice;
      if (device != null) _connect(device, true);
      return;
    }
    _backgroundConnect = false;
    _requestProjectList(resetRetry: true);
    _requestTerminalList(resetRetry: true);
    _sendHostInfoRequest();
    _mountVisibleTerminal(reason: 'foreground');
    _terminalInputBatcher.flush();
  }

  void _connect([StoredDevice? device, bool background = false]) {
    final target = device ?? _activeDevice;
    if (target == null) {
      _applyState(() => _showScanner = true);
      return;
    }
    final connectKey = '${target.hostId}:${target.deviceId}';
    if (background &&
        _activeDevice?.hostId == target.hostId &&
        _activeDevice?.deviceId == target.deviceId &&
        _transportConnected &&
        _remoteProtocolReady) {
      CoduxLog.info(
        '[codux-flutter-remote] connect skipped reason=already-ready host=${target.hostId} device=${target.deviceId}',
      );
      return;
    }
    if (_connectInFlight &&
        _connectInFlightKey == connectKey &&
        _transportConnected &&
        !_remoteProtocolReady) {
      CoduxLog.info(
        '[codux-flutter-remote] connect skipped reason=in-flight host=${target.hostId} device=${target.deviceId}',
      );
      return;
    }
    if (_protocolBlockedHostIds.contains(target.hostId)) {
      if (!background) {
        _applyState(() => _status = _t('connection.upgradeRequired'));
      }
      return;
    }
    _shouldReconnect = true;
    _backgroundConnect = background;
    _connectInFlight = true;
    _connectInFlightKey = connectKey;
    final previousDevice = _activeDevice;
    final switchingDevice =
        previousDevice == null ||
        previousDevice.hostId != target.hostId ||
        previousDevice.deviceId != target.deviceId;
    _cancelRemoteSyncTimers();
    final generation = _remoteSyncController.beginConnectionGeneration();
    if (switchingDevice) {
      _hostRuntimeInstanceId = null;
      _resetRemoteRuntime(keepProjects: false);
      _terminalOutputController.resetAll();
      _terminalRepaint.tick();
    }
    CoduxLog.info(
      '[codux-flutter-remote] connect start gen=$generation background=$background host=${target.hostId} device=${target.deviceId} transport=${_deviceTransportKind(target)} relay=${_savedDeviceRelayEndpoint(target)}',
    );
    _cancelHostResponseProbe();
    _reconnectTimer?.cancel();
    _transportCloseTimer?.cancel();
    _healthTimer?.cancel();
    _clearLatencyProbe();
    unawaited(_closeActiveTransport());
    _transportConnected = false;
    _sendQueue.reset(seed: DateTime.now().microsecondsSinceEpoch);
    _receiveSequenceGuard.reset();
    _receiveChain = Future<void>.value();
    if (background && _lastConnectedAt != null) {
      _startConnectionGrace(reason: 'background_connect');
    }
    if (!background) _clearTerminal();
    if (!background) _terminalInputBatcher.reset();
    _applyState(() {
      _transportReady = false;
      _remoteSyncController.resetProtocolReady();
      _hostResponsive = false;
      _connectionPath = 'unknown';
      _connectionEndpoint = '';
      _connectionRelayUrl = '';
      _latencyMs = null;
      if (!background) {
        _status = _t('app.connecting');
        _showTerminalSwitcher = false;
        _terminalBufferRetry.reset();
        _terminalOutputController.resetTransient();
        _setTerminalBufferLoading(false);
      }
      _activeDevice = target;
    });
    unawaited(_restoreCachedProjects(target));
    if (!_hasConnectableTransport(target)) {
      _applyState(() => _status = _t('pair.repairRequired'));
      return;
    }
    final transport = (widget.transportFactory ?? createRemoteTransport)(
      target,
    );
    transport
      ..onState = (rawState) {
        if (generation != _transportGeneration ||
            !identical(_activeTransport, transport)) {
          CoduxLog.debug(
            '[codux-flutter-remote] drop stale transport state gen=$generation current=$_transportGeneration state=$rawState',
          );
          return;
        }
        _handleTransportState(rawState);
      }
      ..onEnvelope = (envelope) {
        if (generation != _transportGeneration ||
            !identical(_activeTransport, transport)) {
          CoduxLog.debug(
            '[codux-flutter-remote] drop stale transport envelope gen=$generation current=$_transportGeneration type=${envelope['type'] ?? ''}',
          );
          return;
        }
        _handleTransportEnvelopeQueued(
          RelayEnvelope.fromJson(envelope),
          generation: generation,
          transport: transport,
        );
      };
    _activeTransport = transport;
    transport.connect(target).catchError((Object error) {
      CoduxLog.warn(
        '[codux-flutter-remote] connect failed gen=$generation error=$error',
      );
      if (generation != _transportGeneration) return;
      _connectInFlight = false;
      _connectInFlightKey = null;
      if (!_backgroundConnect && mounted) {
        _applyState(() => _status = _t('connection.failedRetry'));
      }
      _handleTransportClosed('connect_failed');
    });
    _healthTimer = Timer(const Duration(seconds: 16), () {
      if (generation != _transportGeneration) return;
      if (!_transportConnected) {
        CoduxLog.warn('[codux-flutter-remote] connect timeout gen=$generation');
        _connectInFlight = false;
        _connectInFlightKey = null;
        if (!_backgroundConnect && mounted) {
          _applyState(() => _status = _t('connection.failedRetry'));
        }
        _handleTransportClosed('hello_timeout');
      }
    });
  }

  void _scheduleReconnect(StoredDevice target) {
    if (!_shouldReconnect) return;
    _reconnectTimer?.cancel();
    _reconnectAttempt += 1;
    final delay = Duration(
      milliseconds: (800 * (1 << (_reconnectAttempt - 1).clamp(0, 5))).clamp(
        800,
        30000,
      ),
    );
    CoduxLog.info(
      '[codux-flutter-remote] reconnect scheduled host=${target.hostId} device=${target.deviceId} attempt=$_reconnectAttempt delayMs=${delay.inMilliseconds}',
    );
    _reconnectTimer = Timer(delay, () => _connect(target, true));
  }

  void _sendInitialTransportRequests({bool force = false}) {
    final plan = _remoteSyncController.initialSyncPlan(
      transportReady: _transportReady,
      transportConnected: _transportConnected,
      force: force,
    );
    if (!plan.hasWork) {
      return;
    }
    if (plan.resetTerminalBufferRetry) {
      _terminalBufferRetry.reset();
    }
    CoduxLog.info('[codux-flutter-remote] request initial sync force=$force');
    if (plan.sendDeviceInfo) {
      _sendDeviceInfo(force: force);
    }
    if (plan.requestProjectList) {
      _requestProjectList(resetRetry: force);
    }
    if (plan.requestTerminalList) {
      _requestTerminalList(resetRetry: force);
    }
  }

  void _sendDeviceInfo({bool force = false}) {
    if (!_remoteSyncController.shouldSendDeviceInfo(force: force)) return;
    final target = _activeDevice;
    _send(
      RelayEnvelope(
        type: 'device.info',
        payload: {
          'name': _settings.localName.isNotEmpty
              ? _settings.localName
              : (target?.name ?? _detectedDeviceName),
        },
      ),
      onResult: (_, result) {
        if (result == RemoteEnvelopeSendResult.delivered) {
          _remoteSyncController.markDeviceInfoSent();
        }
      },
    );
  }

  bool _send(
    RelayEnvelope message, {
    bool sendTerminal = false,
    RemoteSendResultHandler? onResult,
  }) {
    if (!_transportConnected) {
      _applyState(() => _status = _t('app.remoteNotConnected'));
      CoduxLog.warn(
        '[codux-flutter-remote] drop type=${message.type} reason=not_ready',
      );
      return false;
    }
    final transport = _activeTransport;
    if (transport == null) {
      CoduxLog.warn(
        '[codux-flutter-remote] drop type=${message.type} reason=no_transport',
      );
      return false;
    }
    CoduxLog.debug(
      '[codux-flutter-remote] send type=${message.type} session=${message.sessionId ?? ''}',
    );
    unawaited(
      _sendQueue.send(
        message: message,
        transport: transport,
        connected: () => _transportConnected,
        activeDevice: _activeDevice,
        terminalStream: sendTerminal,
        onResult: (sentMessage, result) {
          if (sentMessage.type == RemoteMessageType.hostInfo ||
              sentMessage.type == RemoteMessageType.projectSelect ||
              result != RemoteEnvelopeSendResult.delivered) {
            CoduxLog.info(
              '[codux-flutter-remote] send result type=${sentMessage.type} session=${sentMessage.sessionId ?? ''} result=${result.name} connected=$_transportConnected ready=$_transportReady path=$_connectionPath',
            );
          }
          if (result == RemoteEnvelopeSendResult.rejected) {
            _handleRejectedTransportSend(sentMessage);
          }
          onResult?.call(sentMessage, result);
        },
        onError: (error) {
          CoduxLog.error('[codux-flutter-remote] send failed: $error');
        },
      ),
    );
    return true;
  }

  void _handleRejectedTransportSend(RelayEnvelope message) {
    if (!mounted || _disposing || !_transportConnected) return;
    if (message.type == 'device.disconnected') return;
    final target = _activeDevice;
    if (target == null) {
      _handleTransportClosed('send_rejected:${message.type}');
      return;
    }
    _failHostConnection(target, 'send_rejected:${message.type}');
  }

  bool _sendTerminalEnvelope(RelayEnvelope message, {TerminalInfo? terminal}) {
    final scoped = _scopeTerminalEnvelope(message, terminal: terminal);
    if (scoped == null) return false;
    if (_isTerminalStreamEnvelope(scoped)) {
      return _send(scoped, sendTerminal: true);
    }
    return _send(scoped);
  }

  bool _isTerminalStreamEnvelope(RelayEnvelope message) {
    return message.type == RemoteMessageType.terminalInput ||
        message.type == RemoteMessageType.terminalInputAck ||
        message.type == RemoteMessageType.terminalOutput ||
        message.type == RemoteMessageType.terminalOutputAck ||
        message.type == RemoteMessageType.terminalSignal ||
        message.type == RemoteMessageType.terminalBuffer;
  }

  /// `setState` is `@protected`, so the protocol handlers in the
  /// `home_page_protocol.part.dart` extension route their rebuilds through this
  /// in-class shim instead of calling `setState` directly.
}
