import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

const String _libName = 'codux_protocol_ffi';

final DynamicLibrary _dylib = _loadLibrary();

DynamicLibrary _loadLibrary() {
  if (Platform.isMacOS || Platform.isIOS) {
    if (!Platform.isIOS) {
      final localPath = _localDevelopmentLibraryPath();
      if (localPath != null) return DynamicLibrary.open(localPath);
    }
    final process = DynamicLibrary.process();
    if (_hasRequiredSymbols(process)) return process;
    return process;
  }
  if (Platform.isAndroid || Platform.isLinux) {
    return DynamicLibrary.open('lib$_libName.so');
  }
  if (Platform.isWindows) {
    return DynamicLibrary.open('$_libName.dll');
  }
  throw UnsupportedError('Unsupported platform: ${Platform.operatingSystem}');
}

bool _hasRequiredSymbols(DynamicLibrary library) {
  try {
    library.lookup<NativeFunction<Pointer<Utf8> Function()>>(
      'codux_protocol_version',
    );
    library.lookup<NativeFunction<Pointer<Utf8> Function(Pointer<Utf8>)>>(
      'codux_terminal_text_input_json',
    );
    library.lookup<NativeFunction<Pointer<Utf8> Function(Pointer<Utf8>)>>(
      'codux_terminal_insert_input_json',
    );
    library.lookup<
      NativeFunction<
        Pointer<Utf8> Function(
          Pointer<Utf8>,
          Pointer<Utf8>,
          Bool,
          Bool,
          Bool,
          Bool,
          Bool,
        )
      >
    >('codux_terminal_key_input_json');
    library.lookup<
      NativeFunction<
        Pointer<Utf8> Function(
          Pointer<Utf8>,
          Pointer<Utf8>,
          Int64,
          Int64,
          Bool,
          Bool,
          Bool,
          Bool,
          Bool,
          Bool,
          Bool,
          Bool,
        )
      >
    >('codux_terminal_mouse_input_json');
    library.lookup<NativeFunction<Pointer<Utf8> Function(Pointer<Utf8>)>>(
      'codux_protocol_transport_kind',
    );
    library.lookup<
      NativeFunction<Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>)>
    >('codux_transport_relay_url_for_preset');
    library.lookup<NativeFunction<Pointer<Utf8> Function()>>(
      'codux_transport_relay_presets_json',
    );
    library.lookup<NativeFunction<Pointer<Utf8> Function(Pointer<Utf8>)>>(
      'codux_controller_transport_config_summary_json',
    );
    library.lookup<NativeFunction<Pointer<Void> Function(Pointer<Utf8>)>>(
      'codux_controller_transport_connect_json',
    );
    library.lookup<NativeFunction<Bool Function(Pointer<Void>, Pointer<Utf8>)>>(
      'codux_controller_transport_send_terminal_json',
    );
    library.lookup<
      NativeFunction<
        Bool Function(
          Pointer<Void>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Uint8>,
          IntPtr,
        )
      >
    >('codux_controller_transport_send_terminal_upload');
    library.lookup<NativeFunction<Pointer<Utf8> Function()>>(
      'codux_protocol_last_error',
    );
    library.lookup<NativeFunction<Pointer<Void> Function(Int64)>>(
      'codux_remote_sequence_guard_new',
    );
    library.lookup<NativeFunction<Pointer<Void> Function()>>(
      'codux_remote_runtime_model_new',
    );
    library.lookup<
      NativeFunction<
        Pointer<Utf8> Function(
          Pointer<Void>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Bool,
          Bool,
        )
      >
    >('codux_remote_runtime_model_apply_project_list_json');
    library.lookup<
      NativeFunction<
        Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)
      >
    >('codux_remote_runtime_model_project_selected_json');
    library.lookup<
      NativeFunction<
        Pointer<Utf8> Function(
          Pointer<Void>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Bool,
          Bool,
        )
      >
    >('codux_remote_runtime_model_worktree_selected_json');
    library.lookup<
      NativeFunction<
        Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Bool, Bool, Bool)
      >
    >('codux_remote_runtime_model_apply_worktree_state_json');
    library.lookup<
      NativeFunction<Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)>
    >('codux_remote_runtime_model_terminal_scope_for_project_json');
    library.lookup<
      NativeFunction<
        Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)
      >
    >('codux_remote_runtime_model_terminal_scope_for_session_json');
    library.lookup<NativeFunction<Void Function(Pointer<Void>, Pointer<Utf8>)>>(
      'codux_remote_runtime_model_begin_terminal_create_json',
    );
    return true;
  } catch (_) {
    return false;
  }
}

String? _localDevelopmentLibraryPath() {
  final candidates = [
    '../../target/debug/lib$_libName.dylib',
    '../../target/release/lib$_libName.dylib',
    '../target/debug/lib$_libName.dylib',
    '../target/release/lib$_libName.dylib',
    'target/debug/lib$_libName.dylib',
    'target/release/lib$_libName.dylib',
  ];
  for (final candidate in candidates) {
    final file = File(candidate);
    if (file.existsSync()) return file.absolute.path;
  }
  return null;
}

final _version = _dylib
    .lookupFunction<Pointer<Utf8> Function(), Pointer<Utf8> Function()>(
      'codux_protocol_version',
    );
final _messageType = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>)
    >('codux_protocol_message_type');
final _resourceType = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>)
    >('codux_protocol_resource_type');
final _transportKind = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>)
    >('codux_protocol_transport_kind');
final _relayBlocks = _dylib
    .lookupFunction<Bool Function(Pointer<Utf8>), bool Function(Pointer<Utf8>)>(
      'codux_protocol_relay_blocks_message',
    );
final _isTerminalStreamMessage = _dylib
    .lookupFunction<Bool Function(Pointer<Utf8>), bool Function(Pointer<Utf8>)>(
      'codux_protocol_is_terminal_stream_message',
    );
final _transportRelayUrl = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>)
    >('codux_transport_relay_url');
final _transportRelayUrlForPreset = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>)
    >('codux_transport_relay_url_for_preset');
final _transportRelayPresetsJson = _dylib
    .lookupFunction<Pointer<Utf8> Function(), Pointer<Utf8> Function()>(
      'codux_transport_relay_presets_json',
    );
final _transportPreferredKind = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>, Bool),
      Pointer<Utf8> Function(Pointer<Utf8>, bool)
    >('codux_transport_preferred_kind');
final _parsePairingPayload = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>)
    >('codux_parse_pairing_payload');
final _controllerTransportConfigSummaryJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>)
    >('codux_controller_transport_config_summary_json');
final _controllerTransportConnectJson = _dylib
    .lookupFunction<
      Pointer<Void> Function(Pointer<Utf8>),
      Pointer<Void> Function(Pointer<Utf8>)
    >('codux_controller_transport_connect_json');
final _lastError = _dylib
    .lookupFunction<Pointer<Utf8> Function(), Pointer<Utf8> Function()>(
      'codux_protocol_last_error',
    );
final _controllerTransportSendJson = _dylib
    .lookupFunction<
      Bool Function(Pointer<Void>, Pointer<Utf8>),
      bool Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_controller_transport_send_json');
final _controllerTransportSendTerminalJson = _dylib
    .lookupFunction<
      Bool Function(Pointer<Void>, Pointer<Utf8>),
      bool Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_controller_transport_send_terminal_json');
final _controllerTransportSendTerminalUpload = _dylib
    .lookupFunction<
      Bool Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Uint8>,
        IntPtr,
      ),
      bool Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Uint8>,
        int,
      )
    >('codux_controller_transport_send_terminal_upload');
final _controllerTransportPollEventJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>),
      Pointer<Utf8> Function(Pointer<Void>)
    >('codux_controller_transport_poll_event_json');
final _controllerTransportClose = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_controller_transport_close',
    );
final _resourceSubscribeJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Bool,
        Int32,
        Int32,
      ),
      Pointer<Utf8> Function(
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        bool,
        int,
        int,
      )
    >('codux_protocol_resource_subscribe_json');
final _resourceUnsubscribeJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
    >('codux_protocol_resource_unsubscribe_json');
final _terminalTextInputJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>)
    >('codux_terminal_text_input_json');
final _terminalInsertInputJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>)
    >('codux_terminal_insert_input_json');
final _terminalKeyInputJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(
        Pointer<Utf8>,
        Pointer<Utf8>,
        Bool,
        Bool,
        Bool,
        Bool,
        Bool,
      ),
      Pointer<Utf8> Function(
        Pointer<Utf8>,
        Pointer<Utf8>,
        bool,
        bool,
        bool,
        bool,
        bool,
      )
    >('codux_terminal_key_input_json');
final _terminalMouseInputJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(
        Pointer<Utf8>,
        Pointer<Utf8>,
        Int64,
        Int64,
        Bool,
        Bool,
        Bool,
        Bool,
        Bool,
        Bool,
        Bool,
        Bool,
      ),
      Pointer<Utf8> Function(
        Pointer<Utf8>,
        Pointer<Utf8>,
        int,
        int,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
        bool,
      )
    >('codux_terminal_mouse_input_json');
final _remoteSequenceGuardNew = _dylib
    .lookupFunction<Pointer<Void> Function(Int64), Pointer<Void> Function(int)>(
      'codux_remote_sequence_guard_new',
    );
final _remoteSequenceGuardFree = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_remote_sequence_guard_free',
    );
final _remoteSequenceGuardAccept = _dylib
    .lookupFunction<
      Bool Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Int64),
      bool Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, int)
    >('codux_remote_sequence_guard_accept');
final _remoteSequenceGuardReset = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_remote_sequence_guard_reset',
    );
final _remoteRuntimeModelNew = _dylib
    .lookupFunction<Pointer<Void> Function(), Pointer<Void> Function()>(
      'codux_remote_runtime_model_new',
    );
final _remoteRuntimeModelFree = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_remote_runtime_model_free',
    );
final _remoteRuntimeModelSnapshotJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>),
      Pointer<Utf8> Function(Pointer<Void>)
    >('codux_remote_runtime_model_snapshot_json');
final _remoteRuntimeModelReset = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Bool),
      void Function(Pointer<Void>, bool)
    >('codux_remote_runtime_model_reset');
final _remoteRuntimeModelRestoreCachedProjectsJson = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>),
      void Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_restore_cached_projects_json');
final _remoteRuntimeModelApplyProjectListJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Bool,
        Bool,
      ),
      Pointer<Utf8> Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        bool,
        bool,
      )
    >('codux_remote_runtime_model_apply_project_list_json');
final _remoteRuntimeModelApplyTerminalListJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Bool, Bool),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, bool, bool)
    >('codux_remote_runtime_model_apply_terminal_list_json');
final _remoteRuntimeModelUserSelectProjectJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Bool),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, bool)
    >('codux_remote_runtime_model_user_select_project_json');
final _remoteRuntimeModelProjectSelectedJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)
    >('codux_remote_runtime_model_project_selected_json');
final _remoteRuntimeModelWorktreeSelectedJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Bool,
        Bool,
      ),
      Pointer<Utf8> Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        bool,
        bool,
      )
    >('codux_remote_runtime_model_worktree_selected_json');
final _remoteRuntimeModelApplyWorktreeStateJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Bool, Bool, Bool),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, bool, bool, bool)
    >('codux_remote_runtime_model_apply_worktree_state_json');
final _remoteRuntimeModelEnsureTerminalJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Bool, Bool),
      Pointer<Utf8> Function(Pointer<Void>, bool, bool)
    >('codux_remote_runtime_model_ensure_terminal_json');
final _remoteRuntimeModelSelectTerminalJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_select_terminal_json');
final _remoteRuntimeModelRemoveTerminalJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_remove_terminal_json');
final _remoteRuntimeModelApplyGitStatusJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_apply_git_status_json');
final _remoteRuntimeModelBeginTerminalCreateJson = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>),
      void Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_begin_terminal_create_json');
final _remoteRuntimeModelTerminalCreatedJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_terminal_created_json');
final _remoteRuntimeModelMarkProjectSelectSent = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>),
      void Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_mark_project_select_sent');
final _remoteRuntimeModelClearProjectSelectSent = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>),
      void Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_clear_project_select_sent');
final _remoteRuntimeModelPendingProjectSelect = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Bool),
      Pointer<Utf8> Function(Pointer<Void>, bool)
    >('codux_remote_runtime_model_pending_project_select');
final _remoteRuntimeModelCurrentProjectTerminalsJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>),
      Pointer<Utf8> Function(Pointer<Void>)
    >('codux_remote_runtime_model_current_project_terminals_json');
final _remoteRuntimeModelTerminalScopeForProjectJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_terminal_scope_for_project_json');
final _remoteRuntimeModelTerminalScopeForSessionJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)
    >('codux_remote_runtime_model_terminal_scope_for_session_json');
final _stringFree = _dylib
    .lookupFunction<Void Function(Pointer<Utf8>), void Function(Pointer<Utf8>)>(
      'codux_protocol_string_free',
    );

String protocolVersion() => _takeString(_version());

String messageType(String name) {
  final pointer = name.toNativeUtf8();
  try {
    return _takeString(_messageType(pointer));
  } finally {
    malloc.free(pointer);
  }
}

String resourceType(String name) {
  final pointer = name.toNativeUtf8();
  try {
    return _takeString(_resourceType(pointer));
  } finally {
    malloc.free(pointer);
  }
}

String transportKind(String name) {
  final pointer = name.toNativeUtf8();
  try {
    return _takeString(_transportKind(pointer));
  } finally {
    malloc.free(pointer);
  }
}

bool relayBlocksMessage(String kind) {
  final pointer = kind.toNativeUtf8();
  try {
    return _relayBlocks(pointer);
  } finally {
    malloc.free(pointer);
  }
}

/// True for terminal messages that travel on the dedicated terminal-stream lane
/// (PTY I/O), mirroring the host. Backed by `codux-protocol` so the controller
/// never keeps its own copy of the lane classification.
bool isTerminalStreamMessage(String kind) {
  final pointer = kind.toNativeUtf8();
  try {
    return _isTerminalStreamMessage(pointer);
  } finally {
    malloc.free(pointer);
  }
}

String transportRelayUrl(String base) {
  final basePtr = base.toNativeUtf8();
  try {
    return _takeString(_transportRelayUrl(basePtr));
  } finally {
    malloc.free(basePtr);
  }
}

String transportRelayUrlForPreset({
  required String preset,
  String customUrl = '',
}) {
  final presetPtr = preset.toNativeUtf8();
  final customPtr = customUrl.toNativeUtf8();
  try {
    return _takeString(_transportRelayUrlForPreset(presetPtr, customPtr));
  } finally {
    malloc.free(presetPtr);
    malloc.free(customPtr);
  }
}

List<Map<String, dynamic>> transportRelayPresets() {
  final decoded = jsonDecode(_takeString(_transportRelayPresetsJson()));
  if (decoded is! List) return const [];
  return decoded
      .whereType<Map>()
      .map((item) => Map<String, dynamic>.from(item))
      .toList(growable: false);
}

String preferredTransportKind(
  List<Map<String, dynamic>> transports, {
  required bool pairing,
}) {
  final transportsPtr = jsonEncode(transports).toNativeUtf8();
  try {
    return _takeString(_transportPreferredKind(transportsPtr, pairing));
  } finally {
    malloc.free(transportsPtr);
  }
}

/// Validate a DECODED pairing-payload object through the SHARED Rust parser
/// (codux_protocol) — the same format the desktop and agent hosts emit, so the
/// client no longer re-implements it in Dart. The caller does the stable
/// base64url/URL decode and passes the JSON object. Returns either
/// `{'ok': {server, code, secret, pairingId, hostId?, hostName?, transports}}`
/// or `{'missingFields': [...]}`.
Map<String, dynamic> parsePairingPayload(Map<String, dynamic> payload) {
  final payloadPtr = jsonEncode(payload).toNativeUtf8();
  try {
    final decoded = _takeString(_parsePairingPayload(payloadPtr));
    return Map<String, dynamic>.from(jsonDecode(decoded) as Map);
  } finally {
    malloc.free(payloadPtr);
  }
}

Map<String, dynamic> controllerTransportConfigSummary(
  Map<String, dynamic> config,
) {
  final configPtr = jsonEncode(config).toNativeUtf8();
  try {
    return _decodeEnvelope(_controllerTransportConfigSummaryJson(configPtr));
  } finally {
    malloc.free(configPtr);
  }
}

class ControllerTransportHandle {
  ControllerTransportHandle._(this._handle);

  Pointer<Void> _handle;

  bool get isClosed => _handle == nullptr;

  static ControllerTransportHandle? connect(Map<String, dynamic> config) {
    final configPtr = jsonEncode(config).toNativeUtf8();
    try {
      final handle = _controllerTransportConnectJson(configPtr);
      if (handle == nullptr) return null;
      return ControllerTransportHandle._(handle);
    } finally {
      malloc.free(configPtr);
    }
  }

  bool send(Map<String, dynamic> envelope) {
    final handle = _liveHandle();
    final envelopePtr = jsonEncode(envelope).toNativeUtf8();
    try {
      return _controllerTransportSendJson(handle, envelopePtr);
    } finally {
      malloc.free(envelopePtr);
    }
  }

  bool sendTerminal(Map<String, dynamic> envelope) {
    final handle = _liveHandle();
    final envelopePtr = jsonEncode(envelope).toNativeUtf8();
    try {
      return _controllerTransportSendTerminalJson(handle, envelopePtr);
    } finally {
      malloc.free(envelopePtr);
    }
  }

  bool sendTerminalUpload({
    required String deviceId,
    required String sessionId,
    required String name,
    required String mime,
    required String kind,
    required Uint8List bytes,
  }) {
    if (bytes.isEmpty) return false;
    final handle = _liveHandle();
    final devicePtr = deviceId.toNativeUtf8();
    final sessionPtr = sessionId.toNativeUtf8();
    final namePtr = name.toNativeUtf8();
    final mimePtr = mime.toNativeUtf8();
    final kindPtr = kind.toNativeUtf8();
    final bytesPtr = malloc<Uint8>(bytes.length);
    try {
      bytesPtr.asTypedList(bytes.length).setAll(0, bytes);
      return _controllerTransportSendTerminalUpload(
        handle,
        devicePtr,
        sessionPtr,
        namePtr,
        mimePtr,
        kindPtr,
        bytesPtr,
        bytes.length,
      );
    } finally {
      malloc.free(devicePtr);
      malloc.free(sessionPtr);
      malloc.free(namePtr);
      malloc.free(mimePtr);
      malloc.free(kindPtr);
      malloc.free(bytesPtr);
    }
  }

  Map<String, dynamic>? pollEvent() {
    final pointer = _controllerTransportPollEventJson(_liveHandle());
    if (pointer == nullptr) return null;
    final decoded = _decodeEnvelope(pointer);
    return decoded.isEmpty ? null : decoded;
  }

  void close() {
    final handle = _handle;
    if (handle == nullptr) return;
    _handle = nullptr;
    _controllerTransportClose(handle);
  }

  Pointer<Void> _liveHandle() {
    final handle = _handle;
    if (handle == nullptr) {
      throw StateError('Controller transport has been closed');
    }
    return handle;
  }
}

String lastError() => _takeString(_lastError());

Map<String, dynamic> resourceSubscribeEnvelope({
  required String resource,
  String? projectId,
  String? sessionId,
  bool baseline = true,
  int? maxChars,
  int? chunkChars,
}) {
  final resourcePtr = resource.toNativeUtf8();
  final projectPtr = (projectId ?? '').toNativeUtf8();
  final sessionPtr = (sessionId ?? '').toNativeUtf8();
  try {
    return _decodeEnvelope(
      _resourceSubscribeJson(
        resourcePtr,
        projectPtr,
        sessionPtr,
        baseline,
        maxChars ?? 0,
        chunkChars ?? 0,
      ),
    );
  } finally {
    malloc.free(resourcePtr);
    malloc.free(projectPtr);
    malloc.free(sessionPtr);
  }
}

Map<String, dynamic> resourceUnsubscribeEnvelope({
  required String resource,
  String? projectId,
  String? sessionId,
}) {
  final resourcePtr = resource.toNativeUtf8();
  final projectPtr = (projectId ?? '').toNativeUtf8();
  final sessionPtr = (sessionId ?? '').toNativeUtf8();
  try {
    return _decodeEnvelope(
      _resourceUnsubscribeJson(resourcePtr, projectPtr, sessionPtr),
    );
  } finally {
    malloc.free(resourcePtr);
    malloc.free(projectPtr);
    malloc.free(sessionPtr);
  }
}

String terminalTextInput(String text) {
  final textPtr = text.toNativeUtf8();
  try {
    return _terminalInputFromJson(_terminalTextInputJson(textPtr));
  } finally {
    malloc.free(textPtr);
  }
}

String terminalInsertInput(String text) {
  final textPtr = text.toNativeUtf8();
  try {
    return _terminalInputFromJson(_terminalInsertInputJson(textPtr));
  } finally {
    malloc.free(textPtr);
  }
}

String terminalKeyInput({
  required String key,
  String keyChar = '',
  bool shift = false,
  bool alt = false,
  bool control = false,
  bool platform = false,
  bool applicationCursor = false,
}) {
  final keyPtr = key.toNativeUtf8();
  final keyCharPtr = keyChar.toNativeUtf8();
  try {
    return _terminalInputFromJson(
      _terminalKeyInputJson(
        keyPtr,
        keyCharPtr,
        shift,
        alt,
        control,
        platform,
        applicationCursor,
      ),
    );
  } finally {
    malloc.free(keyPtr);
    malloc.free(keyCharPtr);
  }
}

String terminalMouseInput({
  required String action,
  String button = '',
  required int row,
  required int col,
  bool shift = false,
  bool alt = false,
  bool control = false,
  bool platform = false,
  bool mouseMotion = false,
  bool mouseDrag = false,
  bool sgrMouse = false,
  bool utf8Mouse = false,
}) {
  final actionPtr = action.toNativeUtf8();
  final buttonPtr = button.toNativeUtf8();
  try {
    return _terminalInputFromJson(
      _terminalMouseInputJson(
        actionPtr,
        buttonPtr,
        row,
        col,
        shift,
        alt,
        control,
        platform,
        mouseMotion,
        mouseDrag,
        sgrMouse,
        utf8Mouse,
      ),
    );
  } finally {
    malloc.free(actionPtr);
    malloc.free(buttonPtr);
  }
}

String _terminalInputFromJson(Pointer<Utf8> pointer) {
  final decoded = _decodeEnvelope(pointer);
  return '${decoded['input'] ?? ''}';
}

class TerminalScreenSnapshot {
  const TerminalScreenSnapshot({
    required this.data,
    required this.cols,
    required this.rows,
    required this.totalLines,
    required this.displayOffset,
    this.marginRows = 0,
    this.marginRowsBelow = 0,
    required this.scrollPixelOffset,
    required this.applicationCursor,
    required this.cells,
    required this.cursor,
    this.inputMode = const TerminalScreenInputMode(),
  });

  final String data;
  final int cols;
  final int rows;
  final int totalLines;
  final int displayOffset;

  /// Rows at the top of the grid that are pre-rendered overscan context
  /// above the visible viewport (host-served scrolling).
  final int marginRows;

  /// Rows at the bottom of the grid that are pre-rendered overscan context
  /// below the visible viewport (host-served scrolling).
  final int marginRowsBelow;
  final double scrollPixelOffset;
  final bool applicationCursor;
  final List<TerminalScreenCell> cells;
  final TerminalScreenCursor cursor;

  /// Active terminal modes (mouse tracking, alternate screen/scroll, ...),
  /// used to decide whether a scroll gesture scrolls the local scrollback or
  /// is forwarded to the app as wheel / arrow input (TUIs like Claude Code).
  final TerminalScreenInputMode inputMode;

  factory TerminalScreenSnapshot.fromJson(Map<String, dynamic> json) {
    final cells = json['cells'];
    return TerminalScreenSnapshot(
      data: '${json['data'] ?? ''}',
      cols: _jsonInt(json['cols']),
      rows: _jsonInt(json['rows']),
      totalLines: _jsonInt(json['totalLines']),
      displayOffset: _jsonInt(json['displayOffset']),
      marginRows: _jsonInt(json['marginRows']),
      marginRowsBelow: _jsonInt(json['marginRowsBelow']),
      scrollPixelOffset: _jsonDouble(json['scrollPixelOffset']),
      applicationCursor: json['applicationCursor'] == true,
      cells: [
        if (cells is List)
          for (final cell in cells)
            if (cell is Map)
              TerminalScreenCell.fromJson(Map<String, dynamic>.from(cell)),
      ],
      cursor: TerminalScreenCursor.fromJson(
        json['cursor'] is Map
            ? Map<String, dynamic>.from(json['cursor'] as Map)
            : const {},
      ),
      inputMode: json['inputMode'] is Map
          ? TerminalScreenInputMode.fromJson(
              Map<String, dynamic>.from(json['inputMode'] as Map),
            )
          : const TerminalScreenInputMode(),
    );
  }
}

class TerminalScreenInputMode {
  const TerminalScreenInputMode({
    this.applicationCursor = false,
    this.alternateScreen = false,
    this.alternateScroll = false,
    this.mouseTracking = false,
    this.mouseMotion = false,
    this.mouseDrag = false,
    this.sgrMouse = false,
    this.utf8Mouse = false,
  });

  final bool applicationCursor;
  final bool alternateScreen;
  final bool alternateScroll;
  final bool mouseTracking;
  final bool mouseMotion;
  final bool mouseDrag;
  final bool sgrMouse;
  final bool utf8Mouse;

  factory TerminalScreenInputMode.fromJson(Map<String, dynamic> json) {
    return TerminalScreenInputMode(
      applicationCursor: json['applicationCursor'] == true,
      alternateScreen: json['alternateScreen'] == true,
      alternateScroll: json['alternateScroll'] == true,
      mouseTracking: json['mouseTracking'] == true,
      mouseMotion: json['mouseMotion'] == true,
      mouseDrag: json['mouseDrag'] == true,
      sgrMouse: json['sgrMouse'] == true,
      utf8Mouse: json['utf8Mouse'] == true,
    );
  }
}

class TerminalScreenCursor {
  const TerminalScreenCursor({
    required this.row,
    required this.col,
    required this.visible,
    required this.shape,
  });

  final int row;
  final int col;
  final bool visible;
  final TerminalScreenCursorShape shape;

  factory TerminalScreenCursor.fromJson(Map<String, dynamic> json) {
    return TerminalScreenCursor(
      row: _jsonInt(json['row']),
      col: _jsonInt(json['col']),
      visible: json['visible'] == true,
      shape: TerminalScreenCursorShape.fromJson(json['shape']),
    );
  }
}

enum TerminalScreenCursorShape {
  block,
  beam,
  underline,
  hollowBlock;

  factory TerminalScreenCursorShape.fromJson(Object? value) {
    switch ('$value') {
      case 'beam':
        return TerminalScreenCursorShape.beam;
      case 'underline':
        return TerminalScreenCursorShape.underline;
      case 'hollowBlock':
        return TerminalScreenCursorShape.hollowBlock;
      case 'block':
      default:
        return TerminalScreenCursorShape.block;
    }
  }
}

class TerminalScreenCell {
  const TerminalScreenCell({
    required this.row,
    required this.col,
    required this.text,
    required this.width,
    required this.fg,
    required this.bg,
    required this.bold,
    required this.dim,
    required this.italic,
    required this.underline,
    required this.inverse,
    required this.hidden,
    required this.strikeout,
  });

  final int row;
  final int col;
  final String text;
  final int width;
  final Map<String, dynamic> fg;
  final Map<String, dynamic> bg;
  final bool bold;
  final bool dim;
  final bool italic;
  final bool underline;
  final bool inverse;
  final bool hidden;
  final bool strikeout;

  factory TerminalScreenCell.fromJson(Map<String, dynamic> json) {
    return TerminalScreenCell(
      row: _jsonInt(json['row']),
      col: _jsonInt(json['col']),
      text: '${json['text'] ?? ''}',
      width: _jsonInt(json['width']),
      fg: json['fg'] is Map
          ? Map<String, dynamic>.from(json['fg'] as Map)
          : const {},
      bg: json['bg'] is Map
          ? Map<String, dynamic>.from(json['bg'] as Map)
          : const {},
      bold: json['bold'] == true,
      dim: json['dim'] == true,
      italic: json['italic'] == true,
      underline: json['underline'] == true,
      inverse: json['inverse'] == true,
      hidden: json['hidden'] == true,
      strikeout: json['strikeout'] == true,
    );
  }
}

class RemoteSequenceGuardCore {
  RemoteSequenceGuardCore({int maxEntriesPerChannel = 128})
    : _handle = _remoteSequenceGuardNew(maxEntriesPerChannel);

  Pointer<Void> _handle;

  bool get isDisposed => _handle == nullptr;

  bool accept({
    required String type,
    required String? sessionId,
    required int? seq,
  }) {
    if (seq == null) return true;
    final typePtr = type.toNativeUtf8();
    final sessionPtr = (sessionId ?? '').toNativeUtf8();
    try {
      return _remoteSequenceGuardAccept(
        _liveHandle(),
        typePtr,
        sessionPtr,
        seq,
      );
    } finally {
      malloc.free(typePtr);
      malloc.free(sessionPtr);
    }
  }

  void reset() {
    _remoteSequenceGuardReset(_liveHandle());
  }

  void dispose() {
    final handle = _handle;
    if (handle == nullptr) return;
    _handle = nullptr;
    _remoteSequenceGuardFree(handle);
  }

  Pointer<Void> _liveHandle() {
    final handle = _handle;
    if (handle == nullptr) {
      throw StateError('RemoteSequenceGuardCore is disposed');
    }
    return handle;
  }
}

class RemoteRuntimeCoreState {
  const RemoteRuntimeCoreState({
    required this.projects,
    required this.terminals,
    required this.worktrees,
    required this.lastTerminalIdByProject,
    required this.baseBranchesByProject,
    required this.defaultBaseBranchByProject,
    required this.gitStatusByProject,
    this.selectedProjectId,
    this.activeSessionId,
    this.selectedWorktreeId,
    this.pendingProjectSelectId,
    this.pendingProjectSelectSent = false,
    this.projectSelectAcknowledgedId,
    this.creatingTerminalProjectId,
  });

  final List<Map<String, dynamic>> projects;
  final List<Map<String, dynamic>> terminals;
  final List<Map<String, dynamic>> worktrees;
  final String? selectedProjectId;
  final String? activeSessionId;
  final String? selectedWorktreeId;
  final String? pendingProjectSelectId;
  final bool pendingProjectSelectSent;
  final String? projectSelectAcknowledgedId;
  final String? creatingTerminalProjectId;
  final Map<String, String> lastTerminalIdByProject;
  final Map<String, List<String>> baseBranchesByProject;
  final Map<String, String> defaultBaseBranchByProject;
  final Map<String, Map<String, dynamic>> gitStatusByProject;

  factory RemoteRuntimeCoreState.fromJson(Map<String, dynamic> json) {
    final projects = json['projects'];
    final terminals = json['terminals'];
    final worktrees = json['worktrees'];
    final last = json['lastTerminalIdByProject'];
    final baseBranches = json['baseBranchesByProject'];
    final defaultBaseBranches = json['defaultBaseBranchByProject'];
    final gitStatus = json['gitStatusByProject'];
    return RemoteRuntimeCoreState(
      projects: [
        if (projects is List)
          for (final item in projects)
            if (item is Map) Map<String, dynamic>.from(item),
      ],
      terminals: [
        if (terminals is List)
          for (final item in terminals)
            if (item is Map) Map<String, dynamic>.from(item),
      ],
      worktrees: [
        if (worktrees is List)
          for (final item in worktrees)
            if (item is Map) Map<String, dynamic>.from(item),
      ],
      selectedProjectId: _nullableString(json['selectedProjectId']),
      activeSessionId: _nullableString(json['activeSessionId']),
      selectedWorktreeId: _nullableString(json['selectedWorktreeId']),
      pendingProjectSelectId: _nullableString(json['pendingProjectSelectId']),
      pendingProjectSelectSent: json['pendingProjectSelectSent'] == true,
      projectSelectAcknowledgedId: _nullableString(
        json['projectSelectAcknowledgedId'],
      ),
      creatingTerminalProjectId: _nullableString(
        json['creatingTerminalProjectId'],
      ),
      lastTerminalIdByProject: {
        if (last is Map)
          for (final entry in last.entries) '${entry.key}': '${entry.value}',
      },
      baseBranchesByProject: {
        if (baseBranches is Map)
          for (final entry in baseBranches.entries)
            '${entry.key}': [
              if (entry.value is List)
                for (final item in entry.value as List)
                  if ('$item'.trim().isNotEmpty) '$item'.trim(),
            ],
      },
      defaultBaseBranchByProject: {
        if (defaultBaseBranches is Map)
          for (final entry in defaultBaseBranches.entries)
            '${entry.key}': '${entry.value}',
      },
      gitStatusByProject: {
        if (gitStatus is Map)
          for (final entry in gitStatus.entries)
            if (entry.value is Map)
              '${entry.key}': Map<String, dynamic>.from(entry.value as Map),
      },
    );
  }
}

class RemoteRuntimeCorePlan {
  const RemoteRuntimeCorePlan({
    this.stateChanged = false,
    this.clearTerminal = false,
    this.resetTerminalInput = false,
    this.resetTerminalBuffer = false,
    this.requestTerminalList = false,
    this.requestProjectSelectId,
    this.bindSessionId,
    this.bindFullBuffer = false,
    this.flushTerminalInput = false,
    this.removedSessionId,
  });

  final bool stateChanged;
  final bool clearTerminal;
  final bool resetTerminalInput;
  final bool resetTerminalBuffer;
  final bool requestTerminalList;
  final String? requestProjectSelectId;
  final String? bindSessionId;
  final bool bindFullBuffer;
  final bool flushTerminalInput;
  final String? removedSessionId;

  factory RemoteRuntimeCorePlan.fromJson(Map<String, dynamic> json) {
    return RemoteRuntimeCorePlan(
      stateChanged: json['stateChanged'] == true,
      clearTerminal: json['clearTerminal'] == true,
      resetTerminalInput: json['resetTerminalInput'] == true,
      resetTerminalBuffer: json['resetTerminalBuffer'] == true,
      requestTerminalList: json['requestTerminalList'] == true,
      requestProjectSelectId: _nullableString(json['requestProjectSelectId']),
      bindSessionId: _nullableString(json['bindSessionId']),
      bindFullBuffer: json['bindFullBuffer'] == true,
      flushTerminalInput: json['flushTerminalInput'] == true,
      removedSessionId: _nullableString(json['removedSessionId']),
    );
  }
}

class RemoteRuntimeCore {
  RemoteRuntimeCore() : _handle = _newRemoteRuntimeModel();

  Pointer<Void> _handle;

  bool get isDisposed => _handle == nullptr;

  RemoteRuntimeCoreState snapshot() {
    return RemoteRuntimeCoreState.fromJson(
      _decodeEnvelope(_remoteRuntimeModelSnapshotJson(_liveHandle())),
    );
  }

  void reset({bool keepProjects = false}) {
    _remoteRuntimeModelReset(_liveHandle(), keepProjects);
  }

  void restoreCachedProjects(List<Map<String, dynamic>> projects) {
    final projectsPtr = jsonEncode(projects).toNativeUtf8();
    try {
      _remoteRuntimeModelRestoreCachedProjectsJson(_liveHandle(), projectsPtr);
    } finally {
      malloc.free(projectsPtr);
    }
  }

  RemoteRuntimeCorePlan applyProjectList({
    required List<Map<String, dynamic>> projects,
    required String? remoteSelectedProjectId,
    required String? remoteSelectedWorktreeId,
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    final projectsPtr = jsonEncode(projects).toNativeUtf8();
    final selectedPtr = (remoteSelectedProjectId ?? '').toNativeUtf8();
    final selectedWorktreePtr = (remoteSelectedWorktreeId ?? '').toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelApplyProjectListJson(
            _liveHandle(),
            projectsPtr,
            selectedPtr,
            selectedWorktreePtr,
            terminalVisible,
            terminalListLoaded,
          ),
        ),
      );
    } finally {
      malloc.free(projectsPtr);
      malloc.free(selectedPtr);
      malloc.free(selectedWorktreePtr);
    }
  }

  RemoteRuntimeCorePlan applyTerminalList({
    required List<Map<String, dynamic>> terminals,
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    final terminalsPtr = jsonEncode(terminals).toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelApplyTerminalListJson(
            _liveHandle(),
            terminalsPtr,
            terminalVisible,
            terminalListLoaded,
          ),
        ),
      );
    } finally {
      malloc.free(terminalsPtr);
    }
  }

  RemoteRuntimeCorePlan userSelectProject({
    required Map<String, dynamic> project,
    required bool terminalVisible,
  }) {
    final projectPtr = jsonEncode(project).toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelUserSelectProjectJson(
            _liveHandle(),
            projectPtr,
            terminalVisible,
          ),
        ),
      );
    } finally {
      malloc.free(projectPtr);
    }
  }

  RemoteRuntimeCorePlan projectSelected({
    required String? projectId,
    required String? worktreeId,
  }) {
    final projectPtr = (projectId ?? '').toNativeUtf8();
    final worktreePtr = (worktreeId ?? '').toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelProjectSelectedJson(
            _liveHandle(),
            projectPtr,
            worktreePtr,
          ),
        ),
      );
    } finally {
      malloc.free(projectPtr);
      malloc.free(worktreePtr);
    }
  }

  RemoteRuntimeCorePlan worktreeSelected({
    required String? projectId,
    required String? worktreeId,
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    final projectPtr = (projectId ?? '').toNativeUtf8();
    final worktreePtr = (worktreeId ?? '').toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelWorktreeSelectedJson(
            _liveHandle(),
            projectPtr,
            worktreePtr,
            terminalVisible,
            terminalListLoaded,
          ),
        ),
      );
    } finally {
      malloc.free(projectPtr);
      malloc.free(worktreePtr);
    }
  }

  RemoteRuntimeCorePlan applyWorktreeState({
    required Map<String, dynamic> state,
    required bool allowRuntimeSelection,
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    final statePtr = jsonEncode(state).toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelApplyWorktreeStateJson(
            _liveHandle(),
            statePtr,
            allowRuntimeSelection,
            terminalVisible,
            terminalListLoaded,
          ),
        ),
      );
    } finally {
      malloc.free(statePtr);
    }
  }

  RemoteRuntimeCorePlan ensureTerminalForSelectedProject({
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    return RemoteRuntimeCorePlan.fromJson(
      _decodeEnvelope(
        _remoteRuntimeModelEnsureTerminalJson(
          _liveHandle(),
          terminalVisible,
          terminalListLoaded,
        ),
      ),
    );
  }

  RemoteRuntimeCorePlan selectTerminal(Map<String, dynamic> terminal) {
    final terminalPtr = jsonEncode(terminal).toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelSelectTerminalJson(_liveHandle(), terminalPtr),
        ),
      );
    } finally {
      malloc.free(terminalPtr);
    }
  }

  RemoteRuntimeCorePlan removeTerminal(String terminalId) {
    final terminalPtr = terminalId.toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelRemoveTerminalJson(_liveHandle(), terminalPtr),
        ),
      );
    } finally {
      malloc.free(terminalPtr);
    }
  }

  RemoteRuntimeCorePlan applyGitStatus(Map<String, dynamic> status) {
    final statusPtr = jsonEncode(status).toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelApplyGitStatusJson(_liveHandle(), statusPtr),
        ),
      );
    } finally {
      malloc.free(statusPtr);
    }
  }

  void beginTerminalCreate(Map<String, dynamic> request) {
    final requestPtr = jsonEncode(request).toNativeUtf8();
    try {
      _remoteRuntimeModelBeginTerminalCreateJson(_liveHandle(), requestPtr);
    } finally {
      malloc.free(requestPtr);
    }
  }

  RemoteRuntimeCorePlan terminalCreated(Map<String, dynamic> terminal) {
    final terminalPtr = jsonEncode(terminal).toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelTerminalCreatedJson(_liveHandle(), terminalPtr),
        ),
      );
    } finally {
      malloc.free(terminalPtr);
    }
  }

  void markProjectSelectSent(String projectId) {
    final projectPtr = projectId.toNativeUtf8();
    try {
      _remoteRuntimeModelMarkProjectSelectSent(_liveHandle(), projectPtr);
    } finally {
      malloc.free(projectPtr);
    }
  }

  void clearProjectSelectSent(String projectId) {
    final projectPtr = projectId.toNativeUtf8();
    try {
      _remoteRuntimeModelClearProjectSelectSent(_liveHandle(), projectPtr);
    } finally {
      malloc.free(projectPtr);
    }
  }

  String? pendingProjectSelect({bool includeSent = false}) {
    final value = _takeString(
      _remoteRuntimeModelPendingProjectSelect(_liveHandle(), includeSent),
    );
    return value.isEmpty ? null : value;
  }

  List<Map<String, dynamic>> currentProjectTerminals() {
    final decoded = _decodeJson(
      _remoteRuntimeModelCurrentProjectTerminalsJson(_liveHandle()),
    );
    if (decoded is! List) return const [];
    return [
      for (final item in decoded)
        if (item is Map) Map<String, dynamic>.from(item),
    ];
  }

  Map<String, dynamic>? terminalScopeForProject(String projectId) {
    final projectPtr = projectId.toNativeUtf8();
    try {
      final decoded = _decodeJson(
        _remoteRuntimeModelTerminalScopeForProjectJson(
          _liveHandle(),
          projectPtr,
        ),
      );
      return decoded is Map ? Map<String, dynamic>.from(decoded) : null;
    } finally {
      malloc.free(projectPtr);
    }
  }

  Map<String, dynamic>? terminalScopeForSession({
    required String sessionId,
    Map<String, dynamic>? terminal,
  }) {
    final sessionPtr = sessionId.toNativeUtf8();
    final terminalPtr = jsonEncode(terminal ?? const {}).toNativeUtf8();
    try {
      final decoded = _decodeJson(
        _remoteRuntimeModelTerminalScopeForSessionJson(
          _liveHandle(),
          sessionPtr,
          terminalPtr,
        ),
      );
      return decoded is Map ? Map<String, dynamic>.from(decoded) : null;
    } finally {
      malloc.free(sessionPtr);
      malloc.free(terminalPtr);
    }
  }

  void dispose() {
    final handle = _handle;
    if (handle == nullptr) return;
    _handle = nullptr;
    _remoteRuntimeModelFree(handle);
  }

  Pointer<Void> _liveHandle() {
    final handle = _handle;
    if (handle == nullptr) {
      throw StateError('RemoteRuntimeCore is disposed');
    }
    return handle;
  }
}

Pointer<Void> _newRemoteRuntimeModel() {
  final handle = _remoteRuntimeModelNew();
  if (handle == nullptr) {
    throw StateError('Failed to create remote runtime model');
  }
  return handle;
}

Map<String, dynamic> _decodeEnvelope(Pointer<Utf8> pointer) {
  final decoded = _decodeJson(pointer);
  if (decoded is Map<String, dynamic>) return decoded;
  if (decoded is Map) return Map<String, dynamic>.from(decoded);
  throw const FormatException('Protocol FFI did not return a JSON object');
}

Object? _decodeJson(Pointer<Utf8> pointer) {
  final text = _takeString(pointer);
  return jsonDecode(text);
}

String _takeString(Pointer<Utf8> pointer) {
  if (pointer == nullptr) return '';
  try {
    return pointer.toDartString();
  } finally {
    _stringFree(pointer);
  }
}

int _jsonInt(Object? value) {
  if (value is int) return value;
  if (value is num) return value.toInt();
  return int.tryParse('${value ?? ''}') ?? 0;
}

double _jsonDouble(Object? value) {
  if (value is double) return value;
  if (value is num) return value.toDouble();
  return double.tryParse('${value ?? ''}') ?? 0.0;
}

String? _nullableString(Object? value) {
  if (value == null) return null;
  final text = '$value';
  return text.isEmpty ? null : text;
}

String? _takeStringOrNull(Pointer<Utf8> pointer) {
  if (pointer == nullptr) return null;
  try {
    return pointer.toDartString();
  } finally {
    _stringFree(pointer);
  }
}

// ---- RemoteTerminalOutputRouter FFI -------------------------------------

final _outputRouterNew = _dylib
    .lookupFunction<
      Pointer<Void> Function(Int64, Int64),
      Pointer<Void> Function(int, int)
    >('codux_output_router_new');
final _outputRouterFree = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_output_router_free',
    );
final _outputRouterAccept = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)
    >('codux_output_router_accept');
final _outputRouterBindSession = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>, Bool),
      void Function(Pointer<Void>, Pointer<Utf8>, bool)
    >('codux_output_router_bind_session');
final _outputRouterRemoveSession = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>),
      void Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_output_router_remove_session');
final _outputRouterStartBufferRequest = _dylib
    .lookupFunction<
      Bool Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Bool,
        Bool,
        Bool,
      ),
      bool Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        bool,
        bool,
        bool,
      )
    >('codux_output_router_start_buffer_request');
final _outputRouterEvictInactive = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Int64),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, int)
    >('codux_output_router_evict_inactive');
final _outputRouterResetTransient = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_output_router_reset_transient',
    );
final _outputRouterResetSessionTransient = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>, Bool),
      void Function(Pointer<Void>, Pointer<Utf8>, bool)
    >('codux_output_router_reset_session_transient');
final _outputRouterResetAll = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_output_router_reset_all',
    );
final _outputRouterContent = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_output_router_content');
final _outputRouterHasCachedOutput = _dylib
    .lookupFunction<
      Bool Function(Pointer<Void>, Pointer<Utf8>),
      bool Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_output_router_has_cached_output');
final _outputRouterBufferOffset = _dylib
    .lookupFunction<
      Int64 Function(Pointer<Void>, Pointer<Utf8>),
      int Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_output_router_buffer_offset');
final _outputRouterHasSequenceGap = _dylib
    .lookupFunction<
      Bool Function(Pointer<Void>, Pointer<Utf8>),
      bool Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_output_router_has_sequence_gap');
final _outputRouterActiveBufferRequestId = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_output_router_active_buffer_request_id');
final _outputRouterHasActiveBufferRequest = _dylib
    .lookupFunction<
      Bool Function(Pointer<Void>, Pointer<Utf8>),
      bool Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_output_router_has_active_buffer_request');
final _outputRouterRenderGeneration = _dylib
    .lookupFunction<
      Int64 Function(Pointer<Void>, Pointer<Utf8>),
      int Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_output_router_render_generation');
final _outputRouterScreenSnapshotJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_output_router_screen_snapshot_json');
final _outputRouterResizeScreen = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>, Int64, Int64),
      void Function(Pointer<Void>, Pointer<Utf8>, int, int)
    >('codux_output_router_resize_screen');
final _outputRouterScrollScreenPixels = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>, Double, Double),
      void Function(Pointer<Void>, Pointer<Utf8>, double, double)
    >('codux_output_router_scroll_screen_pixels');
final _outputRouterSettleScreenPixelScroll = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>),
      void Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_output_router_settle_screen_pixel_scroll');

/// Dart handle for the Rust `RemoteTerminalOutputRouter`: terminal output
/// orchestration + render-path screen ops, all owned by the shared core.
class RemoteOutputRouter {
  RemoteOutputRouter({required int maxBufferChars, required int maxCachedChars})
    : _handle = _outputRouterNew(maxBufferChars, maxCachedChars);

  Pointer<Void> _handle;

  bool get isDisposed => _handle == nullptr;

  Pointer<Void> _liveHandle() {
    if (_handle == nullptr) {
      throw StateError('RemoteOutputRouter has been disposed');
    }
    return _handle;
  }

  void dispose() {
    if (_handle == nullptr) return;
    _outputRouterFree(_handle);
    _handle = nullptr;
  }

  T _withSession<T>(String sessionId, T Function(Pointer<Utf8>) body) {
    final pointer = sessionId.toNativeUtf8();
    try {
      return body(pointer);
    } finally {
      malloc.free(pointer);
    }
  }

  List<dynamic> accept(String messageJson, String? activeSessionId) {
    final messagePtr = messageJson.toNativeUtf8();
    final activePtr = (activeSessionId ?? '').toNativeUtf8();
    try {
      final result = _takeString(
        _outputRouterAccept(_liveHandle(), messagePtr, activePtr),
      );
      if (result.isEmpty) return const [];
      final decoded = jsonDecode(result);
      return decoded is List ? decoded : const [];
    } finally {
      malloc.free(messagePtr);
      malloc.free(activePtr);
    }
  }

  void bindSession(String sessionId, {required bool requireBaseline}) {
    _withSession(
      sessionId,
      (ptr) => _outputRouterBindSession(_liveHandle(), ptr, requireBaseline),
    );
  }

  void removeSession(String sessionId) {
    _withSession(
      sessionId,
      (ptr) => _outputRouterRemoveSession(_liveHandle(), ptr),
    );
  }

  bool startBufferRequest(
    String sessionId,
    String requestId, {
    bool requireBaseline = false,
    bool resetAssembler = true,
    bool replaceActive = false,
  }) {
    final sessionPtr = sessionId.toNativeUtf8();
    final requestPtr = requestId.toNativeUtf8();
    try {
      return _outputRouterStartBufferRequest(
        _liveHandle(),
        sessionPtr,
        requestPtr,
        requireBaseline,
        resetAssembler,
        replaceActive,
      );
    } finally {
      malloc.free(sessionPtr);
      malloc.free(requestPtr);
    }
  }

  List<String> evictInactive(String activeSessionId, {int maxSessions = 8}) {
    return _withSession(activeSessionId, (ptr) {
      final result = _takeString(
        _outputRouterEvictInactive(_liveHandle(), ptr, maxSessions),
      );
      if (result.isEmpty) return const <String>[];
      final decoded = jsonDecode(result);
      return decoded is List
          ? decoded.map((value) => '$value').toList()
          : const <String>[];
    });
  }

  void resetTransient() => _outputRouterResetTransient(_liveHandle());

  void resetSessionTransient(String sessionId, {bool resetSequence = false}) {
    _withSession(
      sessionId,
      (ptr) =>
          _outputRouterResetSessionTransient(_liveHandle(), ptr, resetSequence),
    );
  }

  void resetAll() => _outputRouterResetAll(_liveHandle());

  String? content(String sessionId) => _withSession(
    sessionId,
    (ptr) => _takeStringOrNull(_outputRouterContent(_liveHandle(), ptr)),
  );

  bool hasCachedOutput(String sessionId) => _withSession(
    sessionId,
    (ptr) => _outputRouterHasCachedOutput(_liveHandle(), ptr),
  );

  int bufferOffset(String sessionId) => _withSession(
    sessionId,
    (ptr) => _outputRouterBufferOffset(_liveHandle(), ptr),
  );

  bool hasSequenceGap(String sessionId) => _withSession(
    sessionId,
    (ptr) => _outputRouterHasSequenceGap(_liveHandle(), ptr),
  );

  String? activeBufferRequestId(String sessionId) => _withSession(
    sessionId,
    (ptr) => _takeStringOrNull(
      _outputRouterActiveBufferRequestId(_liveHandle(), ptr),
    ),
  );

  bool hasActiveBufferRequest(String sessionId) => _withSession(
    sessionId,
    (ptr) => _outputRouterHasActiveBufferRequest(_liveHandle(), ptr),
  );

  int renderGeneration(String sessionId) => _withSession(
    sessionId,
    (ptr) => _outputRouterRenderGeneration(_liveHandle(), ptr),
  );

  TerminalScreenSnapshot? screenSnapshot(String sessionId) {
    final json = _withSession(
      sessionId,
      (ptr) => _takeStringOrNull(
        _outputRouterScreenSnapshotJson(_liveHandle(), ptr),
      ),
    );
    if (json == null) return null;
    final decoded = jsonDecode(json);
    if (decoded is! Map) return null;
    return TerminalScreenSnapshot.fromJson(Map<String, dynamic>.from(decoded));
  }

  void resizeScreen(String sessionId, {required int cols, required int rows}) {
    _withSession(
      sessionId,
      (ptr) => _outputRouterResizeScreen(_liveHandle(), ptr, cols, rows),
    );
  }

  void scrollScreenPixels(
    String sessionId, {
    required double pixels,
    required double cellHeight,
  }) {
    _withSession(
      sessionId,
      (ptr) => _outputRouterScrollScreenPixels(
        _liveHandle(),
        ptr,
        pixels,
        cellHeight,
      ),
    );
  }

  void settleScreenPixelScroll(String sessionId) {
    _withSession(
      sessionId,
      (ptr) => _outputRouterSettleScreenPixelScroll(_liveHandle(), ptr),
    );
  }
}
