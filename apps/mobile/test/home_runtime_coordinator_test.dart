import 'dart:async';

import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/screens/home/home_runtime_coordinator.dart';
import 'package:codux_flutter/services/remote_capabilities.dart';
import 'package:codux_flutter/services/remote_protocol.dart';
import 'package:codux_flutter/services/remote_runtime_store.dart';
import 'package:codux_flutter/services/remote_terminal_binding_coordinator.dart';
import 'package:codux_flutter/services/remote_terminal_output_controller.dart';
import 'package:codux_flutter/services/terminal_buffer_retry.dart';
import 'package:codux_flutter/services/terminal_input_batcher.dart';
import 'package:codux_flutter/services/terminal_input_reliable_sender.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('switching visible session cancels the previous baseline retry', () {
    final timers = <_FakeTimer>[];
    final sentRetryRequests = <String>[];
    final sentEnvelopes = <RelayEnvelope>[];
    final removedSessionStates = <String>[];
    var focusCount = 0;
    var sessionId = 'term-a';
    String? pendingSessionId;
    var projectId = 'project-1';
    final output = RemoteTerminalOutputController();
    final retry = TerminalBufferRetryCoordinator(
      timerFactory: (delay, callback) {
        final timer = _FakeTimer(callback);
        timers.add(timer);
        return timer;
      },
    );
    final binding = RemoteTerminalBindingCoordinator(
      outputController: output,
      send: (envelope) {
        sentEnvelopes.add(envelope);
        return true;
      },
      terminalById: (id) =>
          TerminalInfo(id: id, title: id, projectId: projectId),
      nextRequestId: (scope) => 'req-$scope',
    );
    final inputSender = TerminalInputReliableSender(send: (_) => true);
    final coordinator = HomeRuntimeCoordinator(
      remoteProtocolReady: true,
      selectedProjectId: projectId,
      terminalBufferCapability: TerminalBufferCapability.fallback,
      outputController: output,
      terminalInputSender: inputSender,
      terminalInputBatcher: TerminalInputBatcher(send: (_) {}),
      terminalBufferRetry: retry,
      terminalBindingCoordinator: binding,
      captureSnapshot: () => HomeRuntimeSnapshot(
        selectedProjectId: projectId,
        selectedWorktreeId: null,
        sessionId: sessionId,
      ),
      syncRuntimeViewState: () {
        if (pendingSessionId != null) {
          sessionId = pendingSessionId!;
          pendingSessionId = null;
        }
      },
      setTerminalBufferLoading: (_) {},
      restoreTerminalSessionFromCache: (_) => false,
      closeTerminalSwitcherAfterPendingWorktreeBuffer: (_) {},
      trackTerminalBaselineRequest: (id) {
        retry.trackWhilePending(
          id,
          send: (sessionId) {
            sentRetryRequests.add(sessionId);
            return true;
          },
          hasPendingRequest: (_) => true,
        );
      },
      removeTerminalSessionState: removedSessionStates.add,
      releaseTerminalViewport: ({String? sessionId}) {},
      clearTerminal: () {},
      requestTerminalList: () {},
      sendProjectSelect: (_, {required reason}) {},
      focusTerminalViewSoon: () => focusCount += 1,
      onSessionStateChanged: (_, _) {},
    );

    coordinator.applyRuntimePlan(
      const RemoteRuntimePlan(bindSessionId: 'term-a', bindFullBuffer: true),
      reason: 'initial',
    );
    expect(retry.pendingSessionId, 'term-a');
    expect(timers.single.isActive, isTrue);
    expect(focusCount, 1);

    output.accept(
      RelayEnvelope(
        type: RemoteMessageType.terminalBuffer,
        sessionId: 'term-b',
        payload: const {
          'buffer': true,
          'data': 'cached',
          'offset': 0,
          'bufferLength': 6,
          'truncated': false,
          'tail': true,
          'outputSeq': 1,
        },
      ),
      activeSessionId: 'term-b',
    );
    pendingSessionId = 'term-b';
    coordinator.applyRuntimePlan(
      const RemoteRuntimePlan(bindSessionId: 'term-b'),
      reason: 'switch',
    );

    expect(retry.pendingSessionId, isNull);
    expect(timers.single.isActive, isFalse);
    expect(focusCount, 2);
    timers.single.fire();
    expect(sentRetryRequests, isEmpty);
    expect(
      sentEnvelopes
          .where(
            (message) => message.type == RemoteMessageType.resourceSubscribe,
          )
          .length,
      greaterThanOrEqualTo(1),
    );

    coordinator.applyRuntimePlan(
      const RemoteRuntimePlan(removedSessionIds: ['term-a', 'term-b']),
      reason: 'removed',
    );
    expect(removedSessionStates, ['term-a', 'term-b']);
  });
}

final class _FakeTimer implements Timer {
  _FakeTimer(this._callback);

  final void Function() _callback;
  var _active = true;

  void fire() {
    if (!_active) return;
    _active = false;
    _callback();
  }

  @override
  void cancel() {
    _active = false;
  }

  @override
  bool get isActive => _active;

  @override
  int get tick => 0;
}
