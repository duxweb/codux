import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'dart:io';
import 'package:codux_flutter/main.dart';
import 'package:codux_flutter/i18n.dart';
import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/services/log_service.dart';
import 'package:codux_flutter/services/remote_protocol_service.dart';
import 'package:codux_flutter/services/remote_transport.dart';
import 'package:codux_flutter/theme/app_theme.dart';
import 'package:codux_flutter/widgets/device_home_screen.dart';

void main() {
  testWidgets('Codux app boots', (WidgetTester tester) async {
    await tester.pumpWidget(const CoduxFlutterApp());
    await tester.pump();
    expect(find.byType(MaterialApp), findsOneWidget);
  });

  testWidgets('device row shows saved relay endpoint without changing header', (
    WidgetTester tester,
  ) async {
    final device = await _fakeDevice();

    await tester.pumpWidget(
      MaterialApp(
        theme: buildAppTheme(),
        home: Scaffold(
          body: AppPreferences(
            accent: AccentChoices.cyan,
            locale: LocaleChoices.english,
            child: Directionality(
              textDirection: TextDirection.ltr,
              child: DeviceHomeScreen(
                devices: [device],
                activeDeviceId: device.deviceId,
                ready: false,
                status: 'Off',
                latencyMs: null,
                deviceSubtitle: (_) => 'Relay https://relay.example',
                topInset: 0,
                bottomInset: 0,
                onOpen: (_) {},
                onConnect: (_) {},
                onAdd: () {},
                onEdit: (_) {},
                onDelete: (_) {},
                onRefresh: () async {},
                onSettings: () {},
                onLogs: () {},
                onCheckUpdate: () {},
                onAbout: () {},
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pump();

    expect(find.textContaining('https://relay.example'), findsOneWidget);
    expect(
      find.text('Choose a connected computer to enter terminal'),
      findsOneWidget,
    );
  });

  testWidgets('device row shows global network when endpoint is unavailable', (
    WidgetTester tester,
  ) async {
    final device = StoredDevice(
      server: '',
      hostId: 'host-1',
      deviceId: 'device-1',
      token: 'token',
      name: 'Mac',
      hostName: 'Mac',
    );

    await tester.pumpWidget(
      MaterialApp(
        theme: buildAppTheme(),
        home: Scaffold(
          body: AppPreferences(
            accent: AccentChoices.cyan,
            locale: LocaleChoices.zhCN,
            child: Directionality(
              textDirection: TextDirection.ltr,
              child: DeviceHomeScreen(
                devices: [device],
                activeDeviceId: device.deviceId,
                ready: false,
                status: '未连',
                latencyMs: null,
                deviceSubtitle: (_) => tr('device.globalNetwork', 'zh-CN'),
                topInset: 0,
                bottomInset: 0,
                onOpen: (_) {},
                onConnect: (_) {},
                onAdd: () {},
                onEdit: (_) {},
                onDelete: (_) {},
                onRefresh: () async {},
                onSettings: () {},
                onLogs: () {},
                onCheckUpdate: () {},
                onAbout: () {},
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pump();

    expect(find.text('全球网络'), findsOneWidget);
    expect(find.text('Iroh'), findsNothing);
  });

  testWidgets('device row follows current direct endpoint', (
    WidgetTester tester,
  ) async {
    final device = await _fakeDevice();
    final fake = _FakeRemoteTransport(
      device: device,
      initialPath: 'relay',
      onSent: (transport, envelope) {
        if (envelope['type'] == RemoteMessageType.hostInfo) {
          transport.emitEncrypted(
            RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
          );
        }
      },
    );

    await tester.pumpWidget(
      CoduxFlutterApp(initialDevices: [device], transportFactory: (_) => fake),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Mac'));
    await tester.pumpAndSettle(const Duration(milliseconds: 300));

    fake.emitState('connected:path=direct;addr=10.0.0.2:51515');
    await tester.pump();

    expect(find.textContaining('10.0.0.2:51515'), findsOneWidget);
    expect(find.text('Codux'), findsOneWidget);
  });

  testWidgets('device row follows current relay url from Iroh path state', (
    WidgetTester tester,
  ) async {
    final device = await _fakeDevice();
    final fake = _FakeRemoteTransport(
      device: device,
      initialPath: 'relay',
      onSent: (transport, envelope) {
        if (envelope['type'] == RemoteMessageType.hostInfo) {
          transport.emitEncrypted(
            RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
          );
        }
      },
    );

    await tester.pumpWidget(
      CoduxFlutterApp(initialDevices: [device], transportFactory: (_) => fake),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Mac'));
    await tester.pumpAndSettle(const Duration(milliseconds: 300));

    fake.emitState(
      'connected:path=relay;addr=https://relay.example;relayUrl=https://iroh-service.dux.plus',
    );
    await tester.pump();

    expect(
      find.textContaining('https://iroh-service.dux.plus'),
      findsOneWidget,
    );
    expect(find.text('Codux'), findsOneWidget);
  });

  testWidgets(
    'opening terminal after list sync asks host to bind missing project terminal',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final sent = <Map<String, dynamic>>[];
      final device = await _fakeDevice();
      final fake = _FakeRemoteTransport(
        device: device,
        onSent: (transport, envelope) {
          final type = '${envelope['type'] ?? ''}';
          sent.add(envelope);
          if (type == 'host.info' || type == 'terminal.list') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {'terminals': []},
              ),
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
            );
            return;
          }
          if (type == 'project.select') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-1',
                      'title': 'Terminal',
                      'projectId': 'project-1',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
            );
            return;
          }
          if (type == 'terminal.buffer' || _isTerminalSubscribe(envelope)) {
            final sessionId = _isTerminalSubscribe(envelope)
                ? _sessionIdForSubscribe(envelope, {'project-1': 'session-1'})
                : '${envelope['sessionId'] ?? 'session-1'}';
            transport.emitEncrypted(
              RelayEnvelope(
                type: 'terminal.output',
                sessionId: sessionId,
                payload: const {
                  'data': 'ready',
                  'buffer': true,
                  'offset': 0,
                  'bufferLength': 5,
                  'outputSeq': 1,
                },
              ),
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();
      await tester.pump(const Duration(milliseconds: 200));
      await tester.pumpAndSettle();

      expect(CoduxLog.snapshotText(), contains('terminal.list count=0'));
      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 300));

      final log = CoduxLog.snapshotText();
      expect(
        log,
        contains(
          'send project.select reason=missing-terminal project=project-1',
        ),
      );
      expect(log, contains('bind session=session-1 project=project-1'));
      expect(log, isNot(contains('request terminal.buffer session=session-1')));
      final subscribePayload = _lastTerminalBaselineSubscribePayload(
        sent,
        projectId: 'project-1',
      );
      expect(subscribePayload?['resource'], RemoteResourceType.terminals);
      expect(subscribePayload?['projectId'], 'project-1');
      expect(subscribePayload?['baseline'], isTrue);
      expect(subscribePayload?['maxChars'], isA<int>());
      expect(subscribePayload?['chunkChars'], isA<int>());
      expect(_sentTypes(sent), isNot(contains('terminal.viewport.resize')));
      expect(_sentTypes(sent), isNot(contains('terminal.resize')));
    },
  );

  testWidgets(
    'opening terminal binds the host selected project terminal immediately',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final sentTypes = <String>[];
      final sent = <Map<String, dynamic>>[];
      final device = await _fakeDevice();
      final fake = _FakeRemoteTransport(
        device: device,
        onSent: (transport, envelope) {
          final type = '${envelope['type'] ?? ''}';
          sentTypes.add(type);
          sent.add(envelope);
          if (type == 'host.info') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-2',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                    {'id': 'project-2', 'name': 'Project 2', 'path': '/tmp/p2'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-2',
                      'title': 'Terminal',
                      'projectId': 'project-2',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
            );
            return;
          }
          if (type == 'terminal.buffer' || _isTerminalSubscribe(envelope)) {
            final sessionId = _isTerminalSubscribe(envelope)
                ? _sessionIdForSubscribe(envelope, {'project-2': 'session-2'})
                : '${envelope['sessionId'] ?? 'session-2'}';
            transport.emitEncrypted(
              RelayEnvelope(
                type: 'terminal.output',
                sessionId: sessionId,
                payload: const {
                  'data': 'ready',
                  'buffer': true,
                  'offset': 0,
                  'bufferLength': 5,
                  'outputSeq': 1,
                },
              ),
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 300));

      final log = CoduxLog.snapshotText();
      expect(log, contains('project.list count=2 selected=project-2'));
      expect(log, contains('bind session=session-2 project=project-2'));
      expect(log, isNot(contains('request terminal.buffer session=session-2')));
      expect(sentTypes.where((type) => type == 'project.select'), isEmpty);
      expect(
        sentTypes
            .where((type) => type == RemoteMessageType.resourceSubscribe)
            .length,
        1,
      );
      final subscribePayload = _lastTerminalBaselineSubscribePayload(
        sent,
        projectId: 'project-2',
      );
      expect(subscribePayload?['resource'], RemoteResourceType.terminals);
      expect(subscribePayload?['projectId'], 'project-2');
      expect(subscribePayload?['baseline'], isTrue);
      expect(subscribePayload?['maxChars'], isA<int>());
      expect(subscribePayload?['chunkChars'], isA<int>());
      expect(sentTypes, isNot(contains('terminal.viewport.resize')));
      expect(sentTypes, isNot(contains('terminal.resize')));
    },
  );

  testWidgets(
    'switching projects remounts cached terminal from the local pty pool',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final sentTypes = <String>[];
      final sent = <Map<String, dynamic>>[];
      final device = await _fakeDevice();
      final fake = _FakeRemoteTransport(
        device: device,
        onSent: (transport, envelope) {
          final type = '${envelope['type'] ?? ''}';
          sentTypes.add(type);
          sent.add(envelope);
          if (type == 'host.info') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-1',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                    {'id': 'project-2', 'name': 'Project 2', 'path': '/tmp/p2'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-1',
                      'title': 'Terminal 1',
                      'projectId': 'project-1',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
            );
            return;
          }
          if (type == 'project.select') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-2',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                    {'id': 'project-2', 'name': 'Project 2', 'path': '/tmp/p2'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-1',
                      'title': 'Terminal 1',
                      'projectId': 'project-1',
                      'layoutKind': 'split',
                    },
                    {
                      'id': 'session-2',
                      'title': 'Terminal 2',
                      'projectId': 'project-2',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
            );
            return;
          }
          if (type == 'terminal.buffer' ||
              _isTerminalBaselineSubscribe(envelope)) {
            final sessionId = _isTerminalSubscribe(envelope)
                ? _sessionIdForSubscribe(envelope, {
                    'project-1': 'session-1',
                    'project-2': 'session-2',
                  })
                : '${envelope['sessionId'] ?? 'session-2'}';
            final payload = envelope['payload'];
            transport.emitEncrypted(
              RelayEnvelope(
                type: 'terminal.output',
                sessionId: sessionId,
                payload: {
                  'data': 'ready',
                  'buffer': true,
                  'offset': 0,
                  'bufferLength': 5,
                  'outputSeq': 1,
                  if (payload is Map && payload['requestId'] != null)
                    'requestId': payload['requestId'],
                },
              ),
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      fake.emitEncrypted(
        const RelayEnvelope(
          type: 'terminal.output',
          sessionId: 'session-2',
          payload: {
            'data': 'cached-before-switch',
            'buffer': true,
            'offset': 0,
            'bufferLength': 20,
            'outputSeq': 1,
          },
        ),
      );
      await tester.pump(const Duration(milliseconds: 300));
      sent.clear();
      await _tapProjectTab(tester, 'Project 2');
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 300));

      final projectSelectCount = sentTypes
          .where((type) => type == 'project.select')
          .length;
      final log = CoduxLog.snapshotText();
      expect(projectSelectCount, 1);
      expect(
        log,
        contains('send project.select reason=user-select project=project-2'),
      );
      expect(
        log,
        isNot(
          contains(
            'send project.select reason=missing-terminal project=project-2',
          ),
        ),
      );
      expect(log, contains('bind session=session-2 project=project-2'));
      expect(log, isNot(contains('request terminal.buffer session=session-2')));
      final subscribePayload = _lastPayloadOf(
        sent,
        RemoteMessageType.resourceSubscribe,
      );
      expect(subscribePayload?['resource'], RemoteResourceType.terminals);
      expect(subscribePayload?['projectId'], 'project-2');
      expect(subscribePayload?['baseline'], isTrue);
      expect(sentTypes, isNot(contains('terminal.viewport.resize')));
      expect(sentTypes, isNot(contains('terminal.resize')));
    },
  );

  testWidgets(
    'project tab switch sends host select and immediately binds known terminal',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final sent = <Map<String, dynamic>>[];
      final device = await _fakeDevice();
      final fake = _FakeRemoteTransport(
        device: device,
        onSent: (transport, envelope) {
          sent.add(envelope);
          final type = '${envelope['type'] ?? ''}';
          if (type == 'host.info' || type == 'project.list') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-1',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                    {'id': 'project-2', 'name': 'Project 2', 'path': '/tmp/p2'},
                    {'id': 'project-3', 'name': 'Project 3', 'path': '/tmp/p3'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-1',
                      'title': 'One',
                      'projectId': 'project-1',
                      'layoutKind': 'split',
                    },
                    {
                      'id': 'session-2',
                      'title': 'Two',
                      'projectId': 'project-2',
                      'layoutKind': 'split',
                    },
                    {
                      'id': 'session-3',
                      'title': 'Three',
                      'projectId': 'project-3',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
            );
            return;
          }
          if (type == 'terminal.buffer' ||
              _isTerminalBaselineSubscribe(envelope)) {
            final sessionId = _isTerminalSubscribe(envelope)
                ? _sessionIdForSubscribe(envelope, {
                    'project-1': 'session-1',
                    'project-2': 'session-2',
                    'project-3': 'session-3',
                  })
                : '${envelope['sessionId'] ?? 'session-1'}';
            transport.emitEncrypted(
              RelayEnvelope(
                type: 'terminal.output',
                sessionId: sessionId,
                payload: const {
                  'data': 'ready',
                  'buffer': true,
                  'offset': 0,
                  'bufferLength': 5,
                  'outputSeq': 1,
                },
              ),
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      sent.clear();

      await _tapProjectTab(tester, 'Project 2');
      await tester.pump(const Duration(milliseconds: 300));

      final log = CoduxLog.snapshotText();
      expect(log, contains('user select project=project-2 previous=project-1'));
      expect(
        log,
        contains('send project.select reason=user-select project=project-2'),
      );
      expect(log, contains('bind session=session-2 project=project-2'));
      expect(
        sent.where((envelope) => envelope['type'] == 'project.select').length,
        1,
      );
      expect(
        sent.where((envelope) {
          if (envelope['type'] != RemoteMessageType.resourceSubscribe) {
            return false;
          }
          final payload = envelope['payload'];
          return payload is Map &&
              payload['projectId'] == 'project-2' &&
              payload['baseline'] == true;
        }).length,
        1,
      );
    },
  );

  testWidgets(
    'stale project selected ack is ignored during fast project switching',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final sent = <Map<String, dynamic>>[];
      final device = await _fakeDevice();
      final fake = _FakeRemoteTransport(
        device: device,
        onSent: (transport, envelope) {
          sent.add(envelope);
          final type = '${envelope['type'] ?? ''}';
          if (type == 'host.info' || type == 'project.list') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-1',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                    {'id': 'project-2', 'name': 'Project 2', 'path': '/tmp/p2'},
                    {'id': 'project-3', 'name': 'Project 3', 'path': '/tmp/p3'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-1',
                      'title': 'One',
                      'projectId': 'project-1',
                      'layoutKind': 'split',
                    },
                    {
                      'id': 'session-2',
                      'title': 'Two',
                      'projectId': 'project-2',
                      'layoutKind': 'split',
                    },
                    {
                      'id': 'session-3',
                      'title': 'Three',
                      'projectId': 'project-3',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      sent.clear();

      await _tapProjectTab(tester, 'Project 2');
      await tester.pump(const Duration(milliseconds: 80));
      await tester.tap(find.text('Project 3'));
      await tester.pump(const Duration(milliseconds: 80));
      fake.emitEncrypted(
        const RelayEnvelope(
          type: 'project.selected',
          payload: {'projectId': 'project-2'},
        ),
      );
      fake.emitEncrypted(
        const RelayEnvelope(
          type: 'project.list',
          payload: {
            'selectedProjectId': 'project-2',
            'projects': [
              {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
              {'id': 'project-2', 'name': 'Project 2', 'path': '/tmp/p2'},
              {'id': 'project-3', 'name': 'Project 3', 'path': '/tmp/p3'},
            ],
          },
        ),
      );
      await tester.pumpAndSettle(const Duration(milliseconds: 300));

      final log = CoduxLog.snapshotText();
      expect(log, contains('user select project=project-3 previous=project-2'));
      expect(
        log,
        contains(
          'project.selected project=project-2 worktree= current=project-3',
        ),
      );
      expect(log, contains('bind session=session-3 project=project-3'));
      final staleAckOffset = log.indexOf(
        'project.selected project=project-2 worktree= current=project-3',
      );
      expect(staleAckOffset, isNonNegative);
      expect(
        log.substring(staleAckOffset),
        isNot(contains('bind session=session-2 project=project-2')),
      );
      expect(
        sent.where((envelope) => envelope['type'] == 'project.select').length,
        greaterThanOrEqualTo(2),
      );
    },
  );

  testWidgets(
    'accepts out of order encrypted project and terminal list messages',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final device = await _fakeDevice();
      final fake = _FakeRemoteTransport(
        device: device,
        onSent: (transport, envelope) {
          final type = '${envelope['type'] ?? ''}';
          if (type == 'host.info') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-1',
                      'title': 'Terminal',
                      'projectId': 'project-1',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
              seq: 34,
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-1',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                  ],
                },
              ),
              seq: 33,
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
              seq: 35,
            );
            return;
          }
          if (type == 'terminal.buffer' || _isTerminalSubscribe(envelope)) {
            final sessionId = _isTerminalSubscribe(envelope)
                ? _sessionIdForSubscribe(envelope, {'project-1': 'session-1'})
                : '${envelope['sessionId'] ?? 'session-1'}';
            transport.emitEncrypted(
              RelayEnvelope(
                type: 'terminal.output',
                sessionId: sessionId,
                payload: const {
                  'data': 'ready',
                  'buffer': true,
                  'offset': 0,
                  'bufferLength': 5,
                  'outputSeq': 1,
                },
              ),
              seq: 36,
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 300));

      final log = CoduxLog.snapshotText();
      expect(log, contains('project.list count=1 selected=project-1'));
      expect(log, contains('terminal.list count=1'));
      expect(log, contains('bind session=session-1 project=project-1'));
    },
  );

  testWidgets('foreground recovery resumes cached remote pty incrementally', (
    WidgetTester tester,
  ) async {
    CoduxLog.setLevelName('debug');
    CoduxLog.clear();
    final sent = <Map<String, dynamic>>[];
    final device = await _fakeDevice();
    var terminalBufferCharacters = 5;
    void emitLists(_FakeRemoteTransport transport) {
      transport.emitEncrypted(
        const RelayEnvelope(
          type: 'project.list',
          payload: {
            'selectedProjectId': 'project-1',
            'projects': [
              {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
            ],
          },
        ),
      );
      transport.emitEncrypted(
        RelayEnvelope(
          type: 'terminal.list',
          payload: {
            'terminals': [
              {
                'id': 'session-1',
                'title': 'Terminal',
                'projectId': 'project-1',
                'layoutKind': 'split',
                'bufferCharacters': terminalBufferCharacters,
              },
            ],
          },
        ),
      );
    }

    final fake = _FakeRemoteTransport(
      device: device,
      onSent: (transport, envelope) {
        final type = '${envelope['type'] ?? ''}';
        sent.add(envelope);
        if (type == 'host.info') {
          emitLists(transport);
          transport.emitEncrypted(
            RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
          );
          return;
        }
        if (type == 'terminal.list') {
          emitLists(transport);
          return;
        }
        if (type == 'terminal.buffer' || _isTerminalSubscribe(envelope)) {
          final sessionId = _isTerminalSubscribe(envelope)
              ? _sessionIdForSubscribe(envelope, {'project-1': 'session-1'})
              : '${envelope['sessionId'] ?? 'session-1'}';
          transport.emitEncrypted(
            RelayEnvelope(
              type: 'terminal.output',
              sessionId: sessionId,
              payload: {
                'data': 'ready',
                'buffer': true,
                'offset': 0,
                'bufferLength': 5,
                'outputSeq': 1,
                if (envelope['payload'] is Map &&
                    (envelope['payload'] as Map)['requestId'] != null)
                  'requestId': (envelope['payload'] as Map)['requestId'],
              },
            ),
          );
        }
      },
    );

    await tester.pumpWidget(
      CoduxFlutterApp(initialDevices: [device], transportFactory: (_) => fake),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Mac'));
    await tester.pumpAndSettle(const Duration(milliseconds: 300));
    await tester.pump(const Duration(milliseconds: 300));
    if (find.byKey(const ValueKey('remote-terminal-body')).evaluate().isEmpty) {
      await tester.tap(find.text('Mac').first);
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 300));
    }
    sent.clear();

    terminalBufferCharacters = 8;
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pumpAndSettle(const Duration(milliseconds: 300));
    await tester.pump(const Duration(milliseconds: 300));

    expect(_sentTypes(sent), isNot(contains('terminal.buffer')));
    final subscribePayload = _lastTerminalBaselineSubscribePayload(sent);
    expect(subscribePayload?['resource'], RemoteResourceType.terminals);
    expect(subscribePayload?['baseline'], isTrue);
  });

  testWidgets('cached terminal remount does not request a ui buffer', (
    WidgetTester tester,
  ) async {
    CoduxLog.setLevelName('debug');
    CoduxLog.clear();
    final sent = <Map<String, dynamic>>[];
    final device = await _fakeDevice();
    final fake = _FakeRemoteTransport(
      device: device,
      onSent: (transport, envelope) {
        final type = '${envelope['type'] ?? ''}';
        sent.add(envelope);
        if (type == 'host.info') {
          transport.emitEncrypted(
            const RelayEnvelope(
              type: 'project.list',
              payload: {
                'selectedProjectId': 'project-1',
                'projects': [
                  {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                ],
              },
            ),
          );
          transport.emitEncrypted(
            const RelayEnvelope(
              type: 'terminal.list',
              payload: {
                'terminals': [
                  {
                    'id': 'session-1',
                    'title': 'Terminal',
                    'projectId': 'project-1',
                    'layoutKind': 'split',
                    'bufferCharacters': 10,
                  },
                ],
              },
            ),
          );
          transport.emitEncrypted(
            RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
          );
          return;
        }
        if (type == 'terminal.buffer' || _isTerminalSubscribe(envelope)) {
          final sessionId = _isTerminalSubscribe(envelope)
              ? _sessionIdForSubscribe(envelope, {'project-1': 'session-1'})
              : '${envelope['sessionId'] ?? 'session-1'}';
          final payload = envelope['payload'];
          transport.emitEncrypted(
            RelayEnvelope(
              type: 'terminal.output',
              sessionId: sessionId,
              payload: {
                'data': 'cached',
                'buffer': true,
                'offset': 0,
                'bufferLength': 6,
                'outputSeq': 1,
                if (payload is Map && payload['requestId'] != null)
                  'requestId': payload['requestId'],
              },
            ),
          );
        }
      },
    );

    await tester.pumpWidget(
      CoduxFlutterApp(initialDevices: [device], transportFactory: (_) => fake),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Mac'));
    await tester.pumpAndSettle(const Duration(milliseconds: 300));
    await tester.pump(const Duration(milliseconds: 300));
    if (find.byKey(const ValueKey('remote-terminal-body')).evaluate().isEmpty) {
      await tester.tap(find.text('Mac').first);
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 300));
    }
    sent.clear();

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pumpAndSettle(const Duration(milliseconds: 300));
    await tester.pump(const Duration(milliseconds: 300));

    expect(_sentTypes(sent), isNot(contains('terminal.buffer')));
    final subscribePayload = _lastTerminalBaselineSubscribePayload(sent);
    expect(subscribePayload?['resource'], RemoteResourceType.terminals);
    expect(subscribePayload?['baseline'], isTrue);
    expect(
      CoduxLog.snapshotText(),
      isNot(
        contains(
          'request terminal.buffer session=session-1 full=true tail=true',
        ),
      ),
    );
    expect(find.text('terminal.loadingHistory'), findsNothing);
  });

  testWidgets(
    'opening terminal with an empty pool refreshes subscription baseline instead of ui buffer',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final sent = <Map<String, dynamic>>[];
      final device = await _fakeDevice();
      final fake = _FakeRemoteTransport(
        device: device,
        onSent: (transport, envelope) {
          final type = '${envelope['type'] ?? ''}';
          sent.add(envelope);
          if (type == 'host.info') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-1',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-1',
                      'title': 'Terminal',
                      'projectId': 'project-1',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();
      sent.clear();

      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 300));

      final sentTypes = _sentTypes(sent);
      expect(sentTypes, contains(RemoteMessageType.resourceSubscribe));
      expect(sentTypes, isNot(contains(RemoteMessageType.terminalBuffer)));
      final subscribePayload = _lastPayloadOf(
        sent,
        RemoteMessageType.resourceSubscribe,
      );
      expect(subscribePayload?['resource'], RemoteResourceType.terminals);
      expect(subscribePayload?['sessionId'], isNull);
      expect(subscribePayload?['projectId'], 'project-1');
      expect(subscribePayload?['baseline'], isTrue);
      expect(
        CoduxLog.snapshotText(),
        contains('bind session=session-1 project=project-1 cached=false'),
      );
    },
  );

  testWidgets(
    'transport health events degrade direct to relay without clearing runtime',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final device = await _fakeDevice();
      final sent = <Map<String, dynamic>>[];
      final fake = _FakeRemoteTransport(
        device: device,
        initialPath: 'direct',
        onSent: (transport, envelope) {
          sent.add(envelope);
          final type = '${envelope['type'] ?? ''}';
          if (type == 'host.info') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-1',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-1',
                      'title': 'Terminal',
                      'projectId': 'project-1',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));

      final baselineSubscribesBeforeTimeout = sent
          .where(_isTerminalBaselineSubscribe)
          .length;
      final projectListRequestsBeforeTimeout = sent
          .where(
            (envelope) => envelope['type'] == RemoteMessageType.projectList,
          )
          .length;
      final terminalListRequestsBeforeTimeout = sent
          .where(
            (envelope) => envelope['type'] == RemoteMessageType.terminalList,
          )
          .length;
      fake.emitState('latency:timeout=1;path=direct');
      fake.emitState('connected:path=relay');
      await tester.pump();

      expect(
        sent.where(_isTerminalBaselineSubscribe).length,
        baselineSubscribesBeforeTimeout,
      );
      expect(
        sent
            .where(
              (envelope) => envelope['type'] == RemoteMessageType.projectList,
            )
            .length,
        projectListRequestsBeforeTimeout,
      );
      expect(
        sent
            .where(
              (envelope) => envelope['type'] == RemoteMessageType.terminalList,
            )
            .length,
        terminalListRequestsBeforeTimeout,
      );
      final log = CoduxLog.snapshotText();
      expect(log, contains('latency timeout=1;path=direct'));
      expect(log, contains('state=connected detail=path=relay'));
      expect(
        log,
        isNot(contains('reset runtime reason=transport_ping_timeout')),
      );
    },
  );

  testWidgets('latency timeout and background keep last visible rtt', (
    WidgetTester tester,
  ) async {
    CoduxLog.setLevelName('debug');
    CoduxLog.clear();
    final device = await _fakeDevice();
    String? latencyPingId;
    final fake = _FakeRemoteTransport(
      device: device,
      initialPath: 'direct',
      onSent: (transport, envelope) {
        final type = '${envelope['type'] ?? ''}';
        if (type == RemoteMessageType.transportPing) {
          final payload = envelope['payload'];
          if (payload is Map) latencyPingId = '${payload['id'] ?? ''}';
          return;
        }
        if (type == 'host.info') {
          transport.emitEncrypted(
            const RelayEnvelope(
              type: 'project.list',
              payload: {
                'selectedProjectId': 'project-1',
                'projects': [
                  {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                ],
              },
            ),
          );
          transport.emitEncrypted(
            const RelayEnvelope(
              type: 'terminal.list',
              payload: {
                'terminals': [
                  {
                    'id': 'session-1',
                    'title': 'Terminal',
                    'projectId': 'project-1',
                    'layoutKind': 'split',
                  },
                ],
              },
            ),
          );
          transport.emitEncrypted(
            RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
          );
        }
      },
    );

    await tester.pumpWidget(
      CoduxFlutterApp(initialDevices: [device], transportFactory: (_) => fake),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Mac'));
    await tester.pumpAndSettle(const Duration(milliseconds: 300));

    expect(latencyPingId, isNotNull);
    await tester.pump(const Duration(milliseconds: 17));
    fake.emitEncrypted(
      RelayEnvelope(
        type: RemoteMessageType.transportPong,
        payload: {'id': latencyPingId},
      ),
    );
    await tester.pumpAndSettle();
    final latencyTextFinder = find.byWidgetPredicate((widget) {
      return widget is Text &&
          widget.data != null &&
          RegExp(r'^\d+ms$').hasMatch(widget.data!);
    });
    expect(latencyTextFinder, findsWidgets);
    final visibleLatency = tester
        .widgetList<Text>(latencyTextFinder)
        .map((widget) => widget.data)
        .firstWhere((value) => value != null)!;

    fake.emitState('latency:timeout=1;path=direct');
    await tester.pump();
    expect(find.text(visibleLatency), findsWidgets);

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    await tester.pump();
    expect(find.text(visibleLatency), findsWidgets);
  });

  testWidgets(
    'pending project select is not resent after direct path degrades',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final sent = <Map<String, dynamic>>[];
      final device = await _fakeDevice();
      var selectedProjectId = 'project-1';
      final fake = _FakeRemoteTransport(
        device: device,
        initialPath: 'direct',
        onSent: (transport, envelope) {
          sent.add(envelope);
          final type = '${envelope['type'] ?? ''}';
          if (type == 'host.info' || type == 'project.list') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-1',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                    {'id': 'project-2', 'name': 'Project 2', 'path': '/tmp/p2'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
            );
            return;
          }
          if (type == 'terminal.list') {
            transport.emitEncrypted(
              RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': _terminalListForProject(selectedProjectId),
                },
              ),
            );
            return;
          }
          if (type == 'project.select') {
            selectedProjectId = 'project-2';
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.selected',
                payload: {'projectId': 'project-2'},
              ),
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-1',
                      'title': 'One',
                      'projectId': 'project-1',
                      'layoutKind': 'split',
                    },
                    {
                      'id': 'session-2',
                      'title': 'Two',
                      'projectId': 'project-2',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      sent.clear();

      await _tapProjectTab(tester, 'Project 2');
      await tester.pump(const Duration(milliseconds: 300));
      fake.emitState('latency:timeout=1;path=direct');
      fake.emitState('connected:path=relay');
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 300));

      expect(
        sent.where((envelope) => envelope['type'] == 'project.select').length,
        1,
      );
      final log = CoduxLog.snapshotText();
      expect(
        log,
        contains('send project.select reason=user-select project=project-2'),
      );
      expect(
        log,
        isNot(
          contains(
            'send project.select reason=path-direct-relay project=project-2',
          ),
        ),
      );
      expect(log, contains('bind session=session-2 project=project-2'));
    },
  );

  testWidgets('transport failure leaves terminal page for device list', (
    WidgetTester tester,
  ) async {
    CoduxLog.setLevelName('debug');
    CoduxLog.clear();
    final device = await _fakeDevice();
    final fake = _FakeRemoteTransport(
      device: device,
      onSent: (transport, envelope) {
        final type = '${envelope['type'] ?? ''}';
        if (type == 'host.info') {
          transport.emitEncrypted(
            const RelayEnvelope(
              type: 'project.list',
              payload: {
                'selectedProjectId': 'project-1',
                'projects': [
                  {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                ],
              },
            ),
          );
          transport.emitEncrypted(
            const RelayEnvelope(
              type: 'terminal.list',
              payload: {
                'terminals': [
                  {
                    'id': 'session-1',
                    'title': 'Terminal',
                    'projectId': 'project-1',
                    'layoutKind': 'split',
                  },
                ],
              },
            ),
          );
          transport.emitEncrypted(
            RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
          );
        }
      },
    );

    await tester.pumpWidget(
      CoduxFlutterApp(initialDevices: [device], transportFactory: (_) => fake),
    );
    await tester.pumpAndSettle();
    for (var attempt = 0; attempt < 2; attempt += 1) {
      if (find
          .byKey(const ValueKey('remote-terminal-body'))
          .evaluate()
          .isNotEmpty) {
        break;
      }
      await tester.tap(find.text('Mac').first);
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
    }

    expect(find.byKey(const ValueKey('remote-terminal-body')), findsOneWidget);

    fake.emitState('failed:network');
    await tester.pumpAndSettle(const Duration(milliseconds: 300));

    expect(find.byKey(const ValueKey('remote-terminal-body')), findsNothing);
    expect(find.text('Mac'), findsWidgets);
  });

  testWidgets('none transport path leaves terminal page for device list', (
    WidgetTester tester,
  ) async {
    final device = await _fakeDevice();
    final fake = _FakeRemoteTransport(
      device: device,
      onSent: (transport, envelope) {
        final type = '${envelope['type'] ?? ''}';
        if (type == 'host.info') {
          transport.emitEncrypted(
            const RelayEnvelope(
              type: 'project.list',
              payload: {
                'selectedProjectId': 'project-1',
                'projects': [
                  {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                ],
              },
            ),
          );
          transport.emitEncrypted(
            const RelayEnvelope(
              type: 'terminal.list',
              payload: {
                'terminals': [
                  {
                    'id': 'session-1',
                    'title': 'Terminal',
                    'projectId': 'project-1',
                    'layoutKind': 'split',
                  },
                ],
              },
            ),
          );
          transport.emitEncrypted(
            RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
          );
        }
      },
    );

    await tester.pumpWidget(
      CoduxFlutterApp(initialDevices: [device], transportFactory: (_) => fake),
    );
    await tester.pumpAndSettle();
    for (var attempt = 0; attempt < 2; attempt += 1) {
      if (find
          .byKey(const ValueKey('remote-terminal-body'))
          .evaluate()
          .isNotEmpty) {
        break;
      }
      await tester.tap(find.text('Mac').first);
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
    }

    expect(find.byKey(const ValueKey('remote-terminal-body')), findsOneWidget);

    fake.emitState('path:path=none');
    await tester.pumpAndSettle(const Duration(milliseconds: 300));

    expect(find.byKey(const ValueKey('remote-terminal-body')), findsNothing);
    expect(find.text('Mac'), findsWidgets);
  });

  testWidgets(
    'pending project select ack timeout retries project select and refreshes terminal list',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final sent = <Map<String, dynamic>>[];
      final device = await _fakeDevice();
      var selectedProjectId = 'project-1';
      final fake = _FakeRemoteTransport(
        device: device,
        onSent: (transport, envelope) {
          sent.add(envelope);
          final type = '${envelope['type'] ?? ''}';
          if (type == 'host.info' || type == 'project.list') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-1',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                    {'id': 'project-2', 'name': 'Project 2', 'path': '/tmp/p2'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
            );
            return;
          }
          if (type == 'terminal.list') {
            transport.emitEncrypted(
              RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': _terminalListForProject(selectedProjectId),
                },
              ),
            );
            return;
          }
          if (type == 'project.select') {
            return;
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      sent.clear();

      await _tapProjectTab(tester, 'Project 2');
      await tester.pump(const Duration(seconds: 4));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));

      expect(
        sent.where((envelope) => envelope['type'] == 'project.select').length,
        2,
      );
      final log = CoduxLog.snapshotText();
      expect(log, contains('project.select ack timeout project=project-2'));
      expect(
        log,
        contains('send project.select reason=ack-timeout project=project-2'),
      );
      expect(
        sent.where((envelope) => envelope['type'] == 'terminal.list').length,
        greaterThan(0),
      );
    },
  );

  testWidgets(
    'rejected project select closes bad transport and keeps pending selection',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final sent = <Map<String, dynamic>>[];
      final device = await _fakeDevice();
      var selectedProjectId = 'project-1';
      var rejectProjectSelect = true;
      var rejectedProjectSelectDelivery = false;
      final fake = _FakeRemoteTransport(
        device: device,
        onBeforeSend: (envelope) {
          if (envelope['type'] == 'project.select' && rejectProjectSelect) {
            rejectProjectSelect = false;
            rejectedProjectSelectDelivery = true;
            return false;
          }
          return true;
        },
        onSent: (transport, envelope) {
          sent.add(envelope);
          final type = '${envelope['type'] ?? ''}';
          if (type == 'host.info' || type == 'project.list') {
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.list',
                payload: {
                  'selectedProjectId': 'project-1',
                  'projects': [
                    {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
                    {'id': 'project-2', 'name': 'Project 2', 'path': '/tmp/p2'},
                  ],
                },
              ),
            );
            transport.emitEncrypted(
              RelayEnvelope(type: 'host.info', payload: _hostInfoPayload()),
            );
            return;
          }
          if (type == 'terminal.list') {
            transport.emitEncrypted(
              RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': _terminalListForProject(selectedProjectId),
                },
              ),
            );
            return;
          }
          if (type == 'project.select') {
            if (rejectedProjectSelectDelivery) {
              rejectedProjectSelectDelivery = false;
              return;
            }
            selectedProjectId = 'project-2';
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'project.selected',
                payload: {'projectId': 'project-2'},
              ),
            );
            transport.emitEncrypted(
              const RelayEnvelope(
                type: 'terminal.list',
                payload: {
                  'terminals': [
                    {
                      'id': 'session-1',
                      'title': 'One',
                      'projectId': 'project-1',
                      'layoutKind': 'split',
                    },
                    {
                      'id': 'session-2',
                      'title': 'Two',
                      'projectId': 'project-2',
                      'layoutKind': 'split',
                    },
                  ],
                },
              ),
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      sent.clear();

      await _tapProjectTab(tester, 'Project 2');
      await tester.pump(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 900));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));

      expect(
        sent.where((envelope) => envelope['type'] == 'project.select').length,
        2,
      );
      final log = CoduxLog.snapshotText();
      expect(log, contains('send result type=project.select'));
      expect(log, contains('result=rejected'));
      expect(
        log,
        contains(
          'project.select delivery failed reason=user-select project=project-2 result=rejected',
        ),
      );
      expect(
        log,
        contains(
          'host unavailable reason=send_rejected:project.select host=host-1 device=device-1',
        ),
      );
      expect(log, contains('send result type=project.select'));
      expect(log, contains('result=delivered'));
      expect(log, contains('bind session=session-2 project=project-2'));
    },
  );

  testWidgets(
    'host runtime instance change clears stale terminal cache and resyncs',
    (WidgetTester tester) async {
      CoduxLog.setLevelName('debug');
      CoduxLog.clear();
      final sentTypes = <String>[];
      final device = await _fakeDevice();
      var hostInfoCount = 0;
      var runtimeId = 'runtime-1';
      void emitCurrentLists(_FakeRemoteTransport transport, int seqBase) {
        final suffix = runtimeId == 'runtime-1' ? 'old' : 'new';
        transport.emitEncrypted(
          const RelayEnvelope(
            type: 'project.list',
            payload: {
              'selectedProjectId': 'project-1',
              'projects': [
                {'id': 'project-1', 'name': 'Project 1', 'path': '/tmp/p1'},
              ],
            },
          ),
          seq: seqBase,
        );
        transport.emitEncrypted(
          RelayEnvelope(
            type: 'terminal.list',
            payload: {
              'terminals': [
                {
                  'id': 'session-$suffix',
                  'title': 'Terminal',
                  'projectId': 'project-1',
                  'layoutKind': 'split',
                },
              ],
            },
          ),
          seq: seqBase + 1,
        );
      }

      final fake = _FakeRemoteTransport(
        device: device,
        onSent: (transport, envelope) {
          final type = '${envelope['type'] ?? ''}';
          sentTypes.add(type);
          if (type == 'host.info') {
            hostInfoCount += 1;
            runtimeId = hostInfoCount < 3 ? 'runtime-1' : 'runtime-2';
            transport.emitEncrypted(
              RelayEnvelope(
                type: 'host.info',
                payload: _hostInfoPayload(runtimeInstanceId: runtimeId),
              ),
              seq: 10 + hostInfoCount,
            );
            if (hostInfoCount == 1 || hostInfoCount == 3) {
              emitCurrentLists(transport, 20 + hostInfoCount);
            }
            return;
          }
          if (type == 'project.list') {
            emitCurrentLists(transport, 60 + sentTypes.length * 2);
            return;
          }
          if (type == 'terminal.list') {
            emitCurrentLists(transport, 80 + sentTypes.length * 2);
            return;
          }
          if (type == 'terminal.buffer' || _isTerminalSubscribe(envelope)) {
            final sessionId = _isTerminalSubscribe(envelope)
                ? (runtimeId == 'runtime-1' ? 'session-old' : 'session-new')
                : '${envelope['sessionId'] ?? ''}';
            transport.emitEncrypted(
              RelayEnvelope(
                type: 'terminal.output',
                sessionId: sessionId,
                payload: {
                  'data': sessionId == 'session-new' ? 'new' : 'old',
                  'buffer': true,
                  'offset': 0,
                  'bufferLength': 3,
                  'outputSeq': 1,
                },
              ),
              seq: 40 + sentTypes.length,
            );
          }
        },
      );

      await tester.pumpWidget(
        CoduxFlutterApp(
          initialDevices: [device],
          transportFactory: (_) => fake,
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('Mac'));
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      runtimeId = 'runtime-2';
      fake.emitEncrypted(
        RelayEnvelope(
          type: 'host.info',
          payload: _hostInfoPayload(runtimeInstanceId: 'runtime-2'),
        ),
        seq: 100,
      );
      await tester.pumpAndSettle(const Duration(milliseconds: 300));
      await tester.pump(const Duration(milliseconds: 300));

      final log = CoduxLog.snapshotText();
      expect(
        log,
        contains(
          'reset runtime reason=host-runtime-instance-changed:runtime-1->runtime-2',
        ),
      );
      expect(log, contains('bind session=session-new project=project-1'));
      expect(
        log,
        isNot(contains('request terminal.buffer session=session-new')),
      );
      expect(
        log,
        isNot(
          contains('bind session=session-old project=project-1 cached=true'),
        ),
      );
    },
  );

  testWidgets('unauthorized error stops reconnect loop and prompts repair', (
    WidgetTester tester,
  ) async {
    CoduxLog.setLevelName('debug');
    CoduxLog.clear();
    final sent = <Map<String, dynamic>>[];
    final device = await _fakeDevice();
    late final _FakeRemoteTransport fake;
    fake = _FakeRemoteTransport(
      device: device,
      onSent: (_, envelope) => sent.add(envelope),
    );

    await tester.pumpWidget(
      CoduxFlutterApp(initialDevices: [device], transportFactory: (_) => fake),
    );
    await tester.pump(const Duration(milliseconds: 200));
    await tester.pumpAndSettle();
    expect(CoduxLog.snapshotText(), contains('request host.info'));

    fake.emit(
      const RelayEnvelope(
        type: 'error',
        payload: {'code': 'device_unauthorized'},
      ),
    );
    await tester.pump(const Duration(milliseconds: 300));
    await tester.pump(const Duration(seconds: 20));

    final log = CoduxLog.snapshotText();
    expect(log, contains('authorization failed code=device_unauthorized'));
    expect(log, isNot(contains('reconnect scheduled')));
    expect(
      log,
      isNot(contains('host unavailable reason=host_response_timeout')),
    );
  });

  testWidgets('startup auto connect waits until after first frame', (
    WidgetTester tester,
  ) async {
    CoduxLog.setLevelName('debug');
    CoduxLog.clear();
    final device = await _fakeDevice();
    var connectCount = 0;
    final fake = _FakeRemoteTransport(
      device: device,
      onSent: (transport, envelope) {},
      onConnect: (_) => connectCount += 1,
    );

    await tester.pumpWidget(
      CoduxFlutterApp(initialDevices: [device], transportFactory: (_) => fake),
    );
    await tester.pump();

    expect(connectCount, 0);
    expect(CoduxLog.snapshotText(), isNot(contains('connect start')));

    await tester.pump(const Duration(milliseconds: 200));

    expect(connectCount, 1);
    expect(CoduxLog.snapshotText(), contains('connect start'));
  });

  test('mobile languages match Mac language count', () {
    expect(LocaleChoices.all.length, 11);
    expect(LocaleChoices.byId('zh-CN').id, 'simplifiedChinese');
    expect(LocaleChoices.byId('en-US').id, 'english');
    expect(tr('settings.title', 'traditionalChinese'), '設定');
    expect(tr('settings.title', 'japanese'), '設定');
  });

  test('all mobile locales cover the English string catalog', () {
    final keys = _englishI18nKeys();
    expect(keys.length, greaterThan(250));
    for (final locale in LocaleChoices.all.where(
      (item) => item.id != 'system',
    )) {
      for (final key in keys) {
        expect(
          tr(key, locale.id),
          isNot(key),
          reason: 'missing $key for ${locale.id}',
        );
      }
    }
  });

  test('visible strings resolve through i18n fallback', () {
    const keys = [
      'app.connected',
      'app.notConnected',
      'app.about',
      'app.removeDevice',
      'toolbar.upload',
      'toolbar.enter',
      'toolbar.keyboard',
      'project.edit',
      'project.add',
      'project.rebuildTerminal',
      'terminal.loadingHistory',
      'device.homeHint',
      'device.globalNetwork',
      'pair.confirmTitle',
      'update.checking',
      'stats.aiTitle',
      'remote.qrInvalid',
    ];

    for (final locale in LocaleChoices.all.where(
      (item) => item.id != 'system',
    )) {
      for (final key in keys) {
        expect(tr(key, locale.id), isNot(key));
      }
    }
  });
}

Set<String> _englishI18nKeys() {
  final file = File('lib/i18n.dart');
  final source = file.readAsStringSync();
  final match = RegExp(
    r"const Map<String, String> _en = \{([\s\S]*?)\n\};",
  ).firstMatch(source);
  expect(match, isNotNull);
  return RegExp(
    r"^\s+'([^']+)':",
    multiLine: true,
  ).allMatches(match!.group(1)!).map((item) => item.group(1)!).toSet();
}

typedef _FakeEnvelopeHandler =
    void Function(
      _FakeRemoteTransport transport,
      Map<String, dynamic> envelope,
    );
typedef _FakeSendDecision = bool Function(Map<String, dynamic> envelope);
typedef _FakeConnectHandler = void Function(StoredDevice device);

Future<StoredDevice> _fakeDevice() async {
  return const StoredDevice(
    server: 'https://relay.example',
    hostId: 'host-1',
    deviceId: 'device-1',
    token: 'token-1',
    name: 'Mac',
    transports: [
      RemoteTransportCandidate(
        kind: RemoteTransportKind.iroh,
        url: 'https://relay.example',
        nodeId: 'node-1',
        relayUrl: 'https://relay.example',
      ),
    ],
  );
}

final class _FakeRemoteTransport implements RemoteTransport {
  _FakeRemoteTransport({
    required this.device,
    required this.onSent,
    this.initialPath = 'relay',
    this.onBeforeSend,
    this.onConnect,
  });

  final StoredDevice device;
  final _FakeEnvelopeHandler onSent;
  final String initialPath;
  final _FakeSendDecision? onBeforeSend;
  final _FakeConnectHandler? onConnect;
  RemoteTransportStateHandler? _onState;
  RemoteTransportEnvelopeHandler? _onEnvelope;

  @override
  String get kind => RemoteTransportKind.iroh;

  @override
  set onState(RemoteTransportStateHandler? handler) => _onState = handler;

  @override
  set onEnvelope(RemoteTransportEnvelopeHandler? handler) =>
      _onEnvelope = handler;

  @override
  Future<void> connect(StoredDevice device) async {
    onConnect?.call(device);
    _onState?.call('connecting');
    _onState?.call('connected:path=$initialPath');
    emit(const RelayEnvelope(type: 'hello'));
  }

  @override
  Future<bool> send(Map<String, dynamic> envelope) async {
    final accepted = onBeforeSend?.call(envelope) ?? true;
    onSent(this, envelope);
    if (!accepted) return false;
    return true;
  }

  @override
  Future<bool> sendTerminal(Map<String, dynamic> envelope) async {
    final accepted = onBeforeSend?.call(envelope) ?? true;
    onSent(this, envelope);
    if (!accepted) return false;
    return true;
  }

  @override
  Future<bool> sendTerminalUpload({
    required String deviceId,
    required String sessionId,
    required String name,
    required String mime,
    required String kind,
    required Uint8List bytes,
  }) async {
    return true;
  }

  @override
  Future<void> close() async {}

  void emit(RelayEnvelope envelope) {
    _onEnvelope?.call(envelope.toJson());
  }

  void emitState(String state) {
    _onState?.call(state);
  }

  void emitEncrypted(RelayEnvelope envelope, {int? seq}) {
    emit(envelope.copyWith(seq: seq ?? DateTime.now().microsecondsSinceEpoch));
  }
}

Map? _lastPayloadOf(List<Map<String, dynamic>> sent, String type) {
  for (final envelope in sent.reversed) {
    if (envelope['type'] == type) return envelope['payload'] as Map?;
  }
  return null;
}

Map? _lastTerminalBaselineSubscribePayload(
  List<Map<String, dynamic>> sent, {
  String? projectId,
  String? sessionId,
}) {
  for (final envelope in sent.reversed) {
    if (!_isTerminalBaselineSubscribe(envelope)) continue;
    final payload = envelope['payload'];
    if (payload is! Map) continue;
    if (projectId != null && payload['projectId'] != projectId) continue;
    if (sessionId != null && payload['sessionId'] != sessionId) continue;
    return payload;
  }
  return null;
}

List<String> _sentTypes(List<Map<String, dynamic>> sent) =>
    sent.map((item) => '${item['type'] ?? ''}').toList();

List<Map<String, Object?>> _terminalListForProject(String projectId) {
  final terminals = <Map<String, Object?>>[
    {
      'id': 'session-1',
      'title': 'One',
      'projectId': 'project-1',
      'layoutKind': 'split',
    },
  ];
  if (projectId == 'project-2') {
    terminals.add({
      'id': 'session-2',
      'title': 'Two',
      'projectId': 'project-2',
      'layoutKind': 'split',
    });
  }
  if (projectId == 'project-3') {
    terminals.add({
      'id': 'session-3',
      'title': 'Three',
      'projectId': 'project-3',
      'layoutKind': 'split',
    });
  }
  return terminals;
}

String _sessionIdForSubscribe(
  Map<String, dynamic> envelope,
  Map<String, String> sessionIdByProject,
) {
  final payload = envelope['payload'];
  if (payload is Map) {
    final sessionId = '${payload['sessionId'] ?? ''}';
    if (sessionId.isNotEmpty) return sessionId;
  }
  final projectId = payload is Map ? '${payload['projectId'] ?? ''}' : '';
  return sessionIdByProject[projectId] ?? sessionIdByProject.values.first;
}

bool _isTerminalSubscribe(Map<String, dynamic> envelope) {
  if (envelope['type'] != RemoteMessageType.resourceSubscribe) return false;
  final payload = envelope['payload'];
  return payload is Map && payload['resource'] == RemoteResourceType.terminals;
}

bool _isTerminalBaselineSubscribe(Map<String, dynamic> envelope) {
  if (!_isTerminalSubscribe(envelope)) return false;
  final payload = envelope['payload'];
  return payload is Map && payload['baseline'] == true;
}

Future<void> _tapProjectTab(WidgetTester tester, String label) async {
  await tester.pumpAndSettle(const Duration(milliseconds: 300));
  if (find.text(label).evaluate().isEmpty &&
      find.text('Mac').evaluate().isNotEmpty) {
    await tester.tap(find.text('Mac').first);
    await tester.pumpAndSettle(const Duration(milliseconds: 300));
  }
  final finder = find.text(label);
  if (finder.evaluate().isEmpty) {
    await tester.pumpAndSettle(const Duration(milliseconds: 300));
  }
  if (finder.evaluate().isEmpty) {
    try {
      await tester.scrollUntilVisible(
        finder,
        120,
        scrollable: find.byType(Scrollable).first,
        duration: const Duration(milliseconds: 16),
      );
    } catch (_) {
      // Fall through to the final tap so the test failure keeps the standard
      // finder diagnostics.
    }
  }
  await tester.tap(finder);
}

Map<String, Object?> _hostInfoPayload({
  String runtimeInstanceId = 'runtime-1',
}) => {
  'protocolVersion': remoteProtocolVersion,
  'runtimeInstanceId': runtimeInstanceId,
  'capabilities': {
    'terminalBuffer': {
      'chunking': true,
      'maxChars': 200000,
      'chunkChars': 16384,
      'requestId': true,
    },
    'terminalViewport': {'ownership': true, 'scroll': true},
  },
};
