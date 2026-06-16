import 'dart:typed_data';

import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/services/remote_transport.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('factory creates unified Rust controller transport', () {
    final transport = createRemoteTransport(
      const StoredDevice(
        server: 'https://relay.example',
        hostId: 'host-1',
        deviceId: 'device-1',
        token: 'token-1',
        name: 'Phone',
        transports: [
          RemoteTransportCandidate(
            kind: RemoteTransportKind.iroh,
            url: 'https://relay.example',
            nodeId: 'node-1',
            relayUrl: 'https://relay.example',
          ),
        ],
      ),
    );

    expect(transport, isA<RustControllerTransport>());
    expect(transport.kind, RemoteTransportKind.iroh);
  });

  test('Rust controller transport uses iroh as the only protocol', () async {
    final seenConfigs = <Map<String, dynamic>>[];
    final transport = RustControllerTransport(
      handleFactory: (config) {
        seenConfigs.add(config);
        return _FakeControllerHandle([
          {'kind': 'state', 'state': 'connected:path=relay'},
        ]);
      },
    );

    await transport.connect(
      const StoredDevice(
        server: 'https://relay.example',
        hostId: 'host-1',
        deviceId: 'device-1',
        token: 'token-1',
        name: 'Phone',
        transports: [
          RemoteTransportCandidate(
            kind: RemoteTransportKind.iroh,
            url: 'https://relay.example',
            nodeId: 'node-1',
            relayUrl: 'https://relay.example',
            relayAuthentication: 'relay-token',
          ),
        ],
      ),
    );

    expect(transport.kind, RemoteTransportKind.iroh);
    expect(seenConfigs.single['transports'], [
      {
        'kind': RemoteTransportKind.iroh,
        'url': 'https://relay.example',
        'nodeId': 'node-1',
        'relayUrl': 'https://relay.example',
        'relayAuthentication': 'relay-token',
      },
    ]);
    expect(seenConfigs.single.containsKey('stunUrls'), isFalse);
    await transport.close();
  });

  test('connect waits for native connected state before returning', () async {
    late _FakeControllerHandle handle;
    final transport = RustControllerTransport(
      handleFactory: (_) {
        handle = _FakeControllerHandle([]);
        return handle;
      },
    );
    final states = <String>[];
    transport.onState = states.add;

    final connected = transport.connect(_storedDevice());
    await Future<void>.delayed(Duration.zero);

    expect(states, ['connecting']);
    expect(handle.pollCount, greaterThanOrEqualTo(1));

    handle.addEvent({'kind': 'state', 'state': 'connected:path=relay'});
    await connected;

    expect(states, ['connecting', 'connected:path=relay']);
    await transport.close();
  });

  test('connect fails when native transport reports failure', () async {
    final transport = RustControllerTransport(
      handleFactory: (_) => _FakeControllerHandle([
        {'kind': 'state', 'state': 'failed:iroh controller connect failed'},
      ]),
    );

    await expectLater(transport.connect(_storedDevice()), throwsStateError);
    await transport.close();
  });

  test('drain stops when state callback closes the active handle', () async {
    late _FakeControllerHandle handle;
    final transport = RustControllerTransport(
      handleFactory: (_) {
        handle = _FakeControllerHandle([
          {'kind': 'state', 'state': 'connected:path=relay'},
          {'kind': 'state', 'state': 'closed'},
          {'kind': 'state', 'state': 'connected:path=direct'},
        ]);
        return handle;
      },
    );
    final states = <String>[];
    transport.onState = (state) {
      states.add(state);
      if (state == 'closed') {
        transport.close();
      }
    };

    await transport.connect(
      const StoredDevice(
        server: 'https://relay.example',
        hostId: 'host-1',
        deviceId: 'device-1',
        token: 'token-1',
        name: 'Phone',
        transports: [
          RemoteTransportCandidate(
            kind: RemoteTransportKind.iroh,
            url: 'https://relay.example',
            nodeId: 'node-1',
            relayUrl: 'https://relay.example',
          ),
        ],
      ),
    );

    expect(states, ['connecting', 'connected:path=relay', 'closed']);
    expect(handle.pollCount, 2);
  });

  test('transport path updates do not masquerade as latency samples', () {
    final event = RemoteTransportStateEvent.parse(
      'latency:rtt=470;path=direct;addr=10.0.0.2:51515',
    );

    expect(event.isPathUpdate, isTrue);
    expect(event.path, 'direct');
    expect(event.addr, '10.0.0.2:51515');
    expect(event.detail, 'rtt=470;path=direct;addr=10.0.0.2:51515');
  });

  test('none transport path is parsed as an explicit path update', () {
    final event = RemoteTransportStateEvent.parse('path:path=none');

    expect(event.isPathUpdate, isTrue);
    expect(event.path, 'none');
  });

  test('relay url is parsed from transport state detail', () {
    final event = RemoteTransportStateEvent.parse(
      'connected:path=relay;addr=relay.example;relayUrl=https://iroh-service.dux.plus',
    );

    expect(event.path, 'relay');
    expect(event.relayUrl, 'https://iroh-service.dux.plus');
  });

  test('terminal envelopes use native terminal stream', () async {
    late _FakeControllerHandle handle;
    final transport = RustControllerTransport(
      handleFactory: (_) {
        handle = _FakeControllerHandle([
          {'kind': 'state', 'state': 'connected:path=relay'},
        ]);
        return handle;
      },
    );

    await transport.connect(_storedDevice());
    final sent = await transport.sendTerminal({
      'type': 'terminal.input',
      'sessionId': 'session-1',
    });

    expect(sent, isTrue);
    expect(handle.terminalMessages, [
      {'type': 'terminal.input', 'sessionId': 'session-1'},
    ]);
    await transport.close();
  });

  test('terminal uploads use iroh blobs', () async {
    late _FakeControllerHandle handle;
    final transport = RustControllerTransport(
      handleFactory: (_) {
        handle = _FakeControllerHandle([
          {'kind': 'state', 'state': 'connected:path=relay'},
        ]);
        return handle;
      },
    );

    await transport.connect(_storedDevice());
    final sent = await transport.sendTerminalUpload(
      deviceId: 'device-1',
      sessionId: 'session-1',
      name: 'photo.png',
      mime: 'image/png',
      kind: 'image',
      bytes: Uint8List.fromList([1, 2, 3]),
    );

    expect(sent, isTrue);
    expect(handle.uploads, hasLength(1));
    expect(handle.uploads.single['sessionId'], 'session-1');
    expect(handle.uploads.single['name'], 'photo.png');
    expect(handle.uploads.single['bytes'], [1, 2, 3]);
    await transport.close();
  });
}

StoredDevice _storedDevice() => const StoredDevice(
  server: 'https://relay.example',
  hostId: 'host-1',
  deviceId: 'device-1',
  token: 'token-1',
  name: 'Phone',
  transports: [
    RemoteTransportCandidate(
      kind: RemoteTransportKind.iroh,
      url: 'https://relay.example',
      nodeId: 'node-1',
      relayUrl: 'https://relay.example',
    ),
  ],
);

final class _FakeControllerHandle implements ControllerTransportEventHandle {
  _FakeControllerHandle(this._events);

  final List<Map<String, dynamic>> _events;
  final terminalMessages = <Map<String, dynamic>>[];
  final uploads = <Map<String, Object>>[];
  var _closed = false;
  var pollCount = 0;

  @override
  bool get isClosed => _closed;

  @override
  void close() {
    _closed = true;
  }

  @override
  Map<String, dynamic>? pollEvent() {
    if (_closed) {
      throw StateError('Controller transport has been closed');
    }
    pollCount += 1;
    if (_events.isEmpty) return null;
    return _events.removeAt(0);
  }

  @override
  bool send(Map<String, dynamic> envelope) => true;

  @override
  bool sendTerminal(Map<String, dynamic> envelope) {
    terminalMessages.add(envelope);
    return true;
  }

  @override
  bool sendTerminalUpload({
    required String deviceId,
    required String sessionId,
    required String name,
    required String mime,
    required String kind,
    required Uint8List bytes,
  }) {
    uploads.add({
      'deviceId': deviceId,
      'sessionId': sessionId,
      'name': name,
      'mime': mime,
      'kind': kind,
      'bytes': bytes.toList(),
    });
    return true;
  }

  void addEvent(Map<String, dynamic> event) {
    _events.add(event);
  }
}
