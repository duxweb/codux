part of '../home_page.dart';

/// Transport state-machine handling for [_CoduxHomePageState]: connection
/// state transitions, latency-state updates, queued/closed envelopes and
/// transport teardown. Split into a part + extension to keep the State class
/// navigable; behaviour is unchanged. Rebuilds route through
/// [_CoduxHomePageState._applyState] (`setState` is `@protected`).
extension _HomePageTransport on HomeController {
  void _handleTransportState(String rawState) {
    final event = RemoteTransportStateEvent.parse(rawState);
    final state = event.state;
    final detail = event.detail;
    if (state == 'latency') {
      if (!mounted || _disposing) return;
      _handleTransportLatencyState(detail);
      return;
    }
    CoduxLog.info(
      detail.isEmpty
          ? '[codux-flutter-remote] state=$state'
          : '[codux-flutter-remote] state=$state detail=$detail',
    );
    if (!mounted || _disposing) return;
    if (event.isPathUpdate) {
      final path = event.path;
      if (path != null) {
        if (path == 'none') {
          _handleTransportClosed('path:none');
          return;
        }
        final previousPath = _connectionPath;
        final changed = path != _connectionPath;
        _markTransportPathDetail(
          path,
          endpoint: event.addr,
          relayUrl: event.relayUrl,
        );
        if (path != 'none') {
          _sendHostInfoRequest(
            force:
                changed ||
                !_remoteProtocolReady ||
                !_projectListLoaded ||
                !_terminalListLoaded,
          );
          if (!_projectListLoaded || !_terminalListLoaded) {
            _sendInitialTransportRequests();
          }
          // Repeated path reports for an unchanged route must not
          // re-subscribe the visible terminal: the baseline re-request
          // holds live output for a full round-trip (visible stall).
          if (changed) {
            _resubscribeVisibleTerminal(reason: 'path-$previousPath-$path');
          }
        }
        return;
      }
    }
    if (event.isConnected) {
      _markTransportOpen();
      _sendHostInfoRequest(
        force:
            !_remoteProtocolReady ||
            !_projectListLoaded ||
            !_terminalListLoaded,
      );
      _startHostResponseProbe(reason: 'transport');
      return;
    }
    if (event.isClosed) {
      _handleTransportClosed(state);
    }
  }

  void _handleTransportLatencyState(String detail) {
    final event = RemoteTransportStateEvent.parse('latency:$detail');
    final parts = detail.split(';');
    String? rttValue;
    String? timeoutValue;
    String? pathValue;
    for (final part in parts) {
      final trimmed = part.trim();
      if (trimmed.startsWith('rtt=')) {
        rttValue = trimmed.substring(4);
      } else if (trimmed.startsWith('timeout=')) {
        timeoutValue = trimmed.substring(8);
      } else if (trimmed.startsWith('path=')) {
        pathValue = trimmed.substring(5);
      }
    }
    if (pathValue != null && pathValue.isNotEmpty) {
      _markTransportPathDetail(
        pathValue,
        endpoint: event.addr,
        relayUrl: event.relayUrl,
      );
    }
    if (rttValue != null) {
      final nextLatency = int.tryParse(rttValue);
      if (nextLatency == null) return;
      CoduxLog.debug(
        '[codux-flutter-remote] route rtt=${nextLatency}ms path=$_connectionPath',
      );
      if (_latencyMs != nextLatency) {
        _applyState(() => _latencyMs = nextLatency);
      }
      return;
    }
    if (timeoutValue != null || detail == 'lost') {
      CoduxLog.warn(
        '[codux-flutter-remote] latency ${detail.isEmpty ? 'timeout' : detail}',
      );
      if (detail == 'lost') {
        if (_latencyMs != null) _applyState(() => _latencyMs = null);
        final target = _activeDevice;
        if (target != null) {
          _failHostConnection(target, 'latency_lost');
        }
      }
    }
  }

  void _handleTransportEnvelopeQueued(
    RelayEnvelope message, {
    required int generation,
    required RemoteTransport transport,
  }) {
    CoduxLog.debug(
      '[codux-flutter-remote] envelope type=${message.type} session=${message.sessionId ?? ''}',
    );
    final target = _activeDevice;
    if (target == null) return;
    final runtimeEpoch = _remoteRuntimeEpoch;
    final previous = _receiveChain.catchError((_) {});
    final task = previous
        .then((_) {
          if (generation != _transportGeneration ||
              !identical(_activeTransport, transport)) {
            CoduxLog.debug(
              '[codux-flutter-remote] drop stale queued envelope gen=$generation current=$_transportGeneration type=${message.type} session=${message.sessionId ?? ''}',
            );
            return Future<void>.value();
          }
          if (runtimeEpoch != _remoteRuntimeEpoch) {
            CoduxLog.debug(
              '[codux-flutter-remote] drop stale envelope epoch=$runtimeEpoch current=$_remoteRuntimeEpoch type=${message.type} session=${message.sessionId ?? ''}',
            );
            return Future<void>.value();
          }
          return _handleTransportEnvelope(
            message,
            target,
            generation,
            transport,
            runtimeEpoch,
          );
        })
        .catchError((Object error) {
          CoduxLog.error('[codux-flutter-remote] receive queue failed: $error');
        });
    _receiveChain = task;
  }

  void _handleTransportClosed(String reason) {
    _remoteRuntimeEpoch += 1;
    _transportConnected = false;
    _connectInFlight = false;
    _connectInFlightKey = null;
    _cancelHostResponseProbe();
    _clearLatencyProbe();
    final pendingProjectSelect = _remoteRuntime.pendingProjectSelect(
      includeSent: true,
    );
    if (pendingProjectSelect != null) {
      _clearProjectSelectAck(pendingProjectSelect);
    }
    _terminalInputBatcher.reset();
    _terminalInputSender.clear();
    _applyState(() {
      _transportReady = false;
      _hostResponsive = false;
      _status = _t('app.reconnecting');
      _leaveTerminalUi();
      _terminalBufferRetry.reset();
      _setTerminalBufferLoading(false);
    });
    if (_lastConnectedAt == null) {
      _clearConnectionGrace();
    } else {
      _startConnectionGrace(reason: reason);
    }
    final target = _activeDevice;
    if (target != null && _appInForeground && !_appSuspended) {
      _scheduleReconnect(target);
    }
  }

  void _notifyHostBeforeTransportClose() {
    _releaseTerminalViewport();
    _send(const RelayEnvelope(type: 'device.disconnected'));
  }

  Future<void> _closeActiveTransport() async {
    final transport = _activeTransport;
    _activeTransport = null;
    await transport?.close();
  }

}
