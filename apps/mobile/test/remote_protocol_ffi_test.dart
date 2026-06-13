import 'package:codux_flutter/services/remote_protocol_service.dart';
import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_protocol_ffi/codux_protocol_ffi.dart'
    as codux_protocol_ffi;
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Rust FFI protocol names match Dart compile-time constants', () {
    expect(codux_protocol_ffi.protocolVersion(), remoteProtocolVersion);
    expect(
      codux_protocol_ffi.messageType('resourceSubscribe'),
      RemoteMessageType.resourceSubscribe,
    );
    expect(
      codux_protocol_ffi.messageType('resourceUnsubscribe'),
      RemoteMessageType.resourceUnsubscribe,
    );
    expect(
      codux_protocol_ffi.resourceType('terminals'),
      RemoteResourceType.terminals,
    );
    expect(
      codux_protocol_ffi.transportKind('websocketRelay'),
      RemoteTransportKind.websocketRelay,
    );
    expect(
      codux_protocol_ffi.transportKind('webRtc'),
      RemoteTransportKind.webRtc,
    );
    expect(
      codux_protocol_ffi.messageType('terminalBuffer'),
      RemoteMessageType.terminalBuffer,
    );
    expect(
      codux_protocol_ffi.messageType('gitStatus'),
      RemoteMessageType.gitStatus,
    );
  });

  test('Rust FFI builds terminal resource subscribe envelope', () {
    final envelope = codux_protocol_ffi.resourceSubscribeEnvelope(
      resource: RemoteResourceType.terminals,
      projectId: 'project-1',
      baseline: true,
      maxChars: 65536,
      chunkChars: 16384,
    );

    expect(envelope['type'], RemoteMessageType.resourceSubscribe);
    expect(envelope['sessionId'], isNull);
    final payload = envelope['payload'] as Map;
    expect(payload['resource'], RemoteResourceType.terminals);
    expect(payload['projectId'], 'project-1');
    expect(payload['baseline'], isTrue);
    expect(payload['maxChars'], 65536);
    expect(payload['chunkChars'], 16384);
  });

  test('Rust FFI owns controller transport URL and selection rules', () {
    expect(
      codux_protocol_ffi.transportServerUrl('https://relay.example'),
      'https://relay.example/v3',
    );
    expect(
      codux_protocol_ffi.transportPairingTicketUrl(
        base: 'https://relay.example',
        ticket: 'ticket-1',
      ),
      'https://relay.example/v3/api/tickets/ticket-1',
    );
    expect(
      codux_protocol_ffi.transportPairingCodeUrl(
        base: 'https://relay.example',
        code: '123456',
      ),
      'https://relay.example/v3/api/pairings/code/123456',
    );
    expect(
      codux_protocol_ffi.transportRelayUrlForPreset(preset: 'china'),
      'https://codux-service.dux.plus',
    );
    expect(
      codux_protocol_ffi.transportPairingWebSocketUrl(
        base: 'https://relay.example',
        hostId: 'host-1',
        devicePublicKey: 'device-key',
      ),
      'wss://relay.example/v3/ws/client?hostId=host-1&deviceId=device-key',
    );
    expect(
      codux_protocol_ffi.transportClientWebSocketUrl(
        base: 'https://relay.example',
        hostId: 'host-1',
        deviceId: 'device-1',
        token: 'token-1',
      ),
      'wss://relay.example/v3/ws/client?hostId=host-1&deviceId=device-1&token=token-1',
    );

    final transports = [
      {
        'kind': RemoteTransportKind.websocketRelay,
        'url': 'https://relay.example/v3',
      },
      {'kind': RemoteTransportKind.webRtc, 'url': 'https://relay.example/v3'},
    ];
    expect(
      codux_protocol_ffi.preferredTransportKind(transports, pairing: true),
      RemoteTransportKind.websocketRelay,
    );
    expect(
      codux_protocol_ffi.preferredTransportKind(transports, pairing: false),
      RemoteTransportKind.webRtc,
    );
    expect(
      codux_protocol_ffi.preferredTransportKind([
        {'kind': RemoteTransportKind.webRtc, 'url': 'https://relay.example/v3'},
      ], pairing: false),
      '',
    );
    expect(
      codux_protocol_ffi.transportDefaultIceServers().first['urls'],
      contains('stun:stun.miwifi.com:3478'),
    );
  });

  test('Rust FFI summarizes controller transport config', () {
    final summary = codux_protocol_ffi.controllerTransportConfigSummary({
      'serverUrl': 'https://relay.example',
      'hostId': 'host-1',
      'deviceId': 'device-1',
      'deviceToken': 'token-1',
      'transports': [
        {
          'kind': RemoteTransportKind.websocketRelay,
          'url': 'https://relay.example/v3',
        },
        {'kind': RemoteTransportKind.webRtc, 'url': 'https://relay.example/v3'},
      ],
      'stunUrls': ['stun:example.test:3478'],
    });

    expect(summary['serverUrl'], 'https://relay.example/v3');
    expect(summary['hostId'], 'host-1');
    expect(summary['deviceId'], 'device-1');
    expect(summary['transportKind'], RemoteTransportKind.webRtc);
    expect(summary['transportCount'], 2);
    expect(summary['stunCount'], 1);
  });

  test('Rust FFI terminal input normalizes IME committed text', () {
    expect(codux_protocol_ffi.terminalTextInput('abc'), 'abc');
    expect(codux_protocol_ffi.terminalTextInput('你好かな한글'), '你好かな한글');
    expect(codux_protocol_ffi.terminalTextInput('\u0008'), '\u007f');
    expect(codux_protocol_ffi.terminalTextInput('\n'), '\r');
    expect(codux_protocol_ffi.terminalTextInput('a\u{f700}b'), 'ab');
    expect(codux_protocol_ffi.terminalInsertInput('\u007f'), '\u007f');
    expect(
      codux_protocol_ffi.terminalInsertInput('paste\ntext'),
      '\u001b[200~paste\ntext\u001b[201~',
    );
  });

  test('Rust FFI terminal input maps special keys and app cursor mode', () {
    expect(codux_protocol_ffi.terminalKeyInput(key: 'backspace'), '\u007f');
    expect(codux_protocol_ffi.terminalKeyInput(key: 'enter'), '\r');
    expect(codux_protocol_ffi.terminalKeyInput(key: 'up'), '\u001b[A');
    expect(
      codux_protocol_ffi.terminalKeyInput(key: 'up', applicationCursor: true),
      '\u001bOA',
    );
    expect(
      codux_protocol_ffi.terminalKeyInputBytes(key: 'space', control: true),
      [0],
    );
    expect(
      codux_protocol_ffi.terminalSelectorInput(selector: 'deleteBackward:'),
      '\u007f',
    );
    expect(
      codux_protocol_ffi.terminalSelectorInput(selector: 'moveLeft:'),
      '\u001b[D',
    );
    expect(
      codux_protocol_ffi.terminalSelectorInput(
        selector: 'moveLeft:',
        applicationCursor: true,
      ),
      '\u001bOD',
    );
  });
}
