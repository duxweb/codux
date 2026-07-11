import 'package:codux_flutter/services/remote_capabilities.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('mobile default terminal buffer window matches host default', () {
    expect(TerminalBufferCapability.mobileMaxChars, 200000);
    expect(TerminalBufferCapability.fallback.maxChars, 200000);
  });

  test('parses terminal buffer capability from host info', () {
    final capability = TerminalBufferCapability.fromHostInfo({
      'protocolVersion': 'v3.2',
      'capabilities': {
        'terminalBuffer': {
          'chunking': true,
          'maxChars': 180000,
          'chunkChars': 32768,
          'screenData': true,
          'baselineFailed': true,
        },
        'terminalOutput': {'staleOutput': true},
        'terminalViewport': {'keyframe': true},
      },
    }, clientMaxChars: 200000);

    expect(capability.chunking, isTrue);
    expect(capability.maxChars, 180000);
    expect(capability.chunkChars, 32768);
    expect(capability.requestId, isFalse);
    expect(capability.screenData, isTrue);
    expect(capability.baselineFailed, isTrue);
    expect(capability.staleOutput, isTrue);
    expect(capability.viewportKeyframe, isTrue);
  });

  test('parses request id capability', () {
    final capability = TerminalBufferCapability.fromHostInfo({
      'protocolVersion': 'v3.2',
      'capabilities': {
        'terminalBuffer': {
          'chunking': true,
          'maxChars': 65536,
          'chunkChars': 16384,
          'requestId': true,
        },
      },
    });

    expect(capability.requestId, isTrue);
  });

  test('limits terminal buffer capability to mobile default', () {
    final capability = TerminalBufferCapability.fromHostInfo({
      'protocolVersion': 'v3.2',
      'capabilities': {
        'terminalBuffer': {
          'chunking': true,
          'maxChars': 250000,
          'chunkChars': 32768,
        },
      },
    });

    expect(capability.chunking, isTrue);
    expect(capability.maxChars, TerminalBufferCapability.mobileMaxChars);
    expect(capability.chunkChars, 32768);
  });

  test('clamps terminal buffer capability to mobile limits', () {
    final capability = TerminalBufferCapability.fromHostInfo({
      'capabilities': {
        'terminalBuffer': {
          'chunking': true,
          'maxChars': 999999,
          'chunkChars': 999999,
        },
      },
    });

    expect(capability.maxChars, TerminalBufferCapability.mobileMaxChars);
    expect(capability.chunkChars, 65536);
  });

  test('falls back when host info has no terminal capability', () {
    final capability = TerminalBufferCapability.fromHostInfo({
      'protocolVersion': 'v3.0',
    });

    expect(capability.chunking, isFalse);
    expect(capability.maxChars, TerminalBufferCapability.mobileMaxChars);
    expect(capability.chunkChars, 16384);
  });

  test('resource subscription capability reads advertised resources', () {
    final capability = RemoteResourceSubscriptionCapability.fromHostInfo({
      'capabilities': {
        'resourceSubscriptions': ['projects', 'git.status'],
      },
    });

    expect(capability.supports('projects'), isTrue);
    expect(capability.supports('git.status'), isTrue);
    expect(capability.supports('worktrees'), isFalse);
  });
}
