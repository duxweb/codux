import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

const String _libName = 'codux_protocol_ffi';

final DynamicLibrary _dylib = _loadLibrary();

DynamicLibrary _loadLibrary() {
  if (Platform.isMacOS || Platform.isIOS) {
    final process = DynamicLibrary.process();
    if (_hasRequiredSymbols(process)) return process;
    if (!Platform.isIOS) {
      final localPath = _localDevelopmentLibraryPath();
      if (localPath != null) return DynamicLibrary.open(localPath);
    }
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
    library
        .lookup<NativeFunction<Pointer<Void> Function(Pointer<Utf8>, Int64)>>(
          'codux_terminal_session_new',
        );
    library.lookup<
      NativeFunction<
        Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Int64, Int64)
      >
    >('codux_terminal_session_replace_from_baseline_json');
    library.lookup<
      NativeFunction<
        Pointer<Utf8> Function(
          Pointer<Void>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Int64,
          Int64,
        )
      >
    >('codux_terminal_session_replace_from_baseline_screen_json');
    library.lookup<
      NativeFunction<
        Void Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Int64, Int64)
      >
    >('codux_terminal_session_append_live_screen');
    library.lookup<NativeFunction<Pointer<Utf8> Function(Pointer<Void>)>>(
      'codux_terminal_session_screen_snapshot_json',
    );
    library.lookup<NativeFunction<Void Function(Pointer<Void>, Int64)>>(
      'codux_terminal_session_scroll_screen_lines',
    );
    library.lookup<NativeFunction<Pointer<Utf8> Function(Pointer<Utf8>)>>(
      'codux_protocol_transport_kind',
    );
    library.lookup<
      NativeFunction<Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>)>
    >('codux_transport_pairing_ticket_url');
    library.lookup<
      NativeFunction<Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>)>
    >('codux_transport_pairing_code_url');
    library.lookup<
      NativeFunction<Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>)>
    >('codux_transport_relay_url_for_preset');
    library.lookup<NativeFunction<Pointer<Utf8> Function(Pointer<Utf8>)>>(
      'codux_controller_transport_config_summary_json',
    );
    library.lookup<NativeFunction<Pointer<Void> Function(Pointer<Utf8>)>>(
      'codux_controller_transport_connect_json',
    );
    library.lookup<NativeFunction<Bool Function(Pointer<Void>, Pointer<Utf8>)>>(
      'codux_controller_transport_report_ping_timeout',
    );
    library.lookup<NativeFunction<Bool Function(Pointer<Void>)>>(
      'codux_controller_transport_probe_preferred_route',
    );
    library.lookup<NativeFunction<Pointer<Utf8> Function()>>(
      'codux_protocol_last_error',
    );
    library.lookup<NativeFunction<Pointer<Void> Function()>>(
      'codux_terminal_output_sequencer_new',
    );
    library.lookup<NativeFunction<Pointer<Void> Function(Int64)>>(
      'codux_terminal_buffer_assembler_new',
    );
    library.lookup<NativeFunction<Pointer<Void> Function(Int64)>>(
      'codux_remote_sequence_guard_new',
    );
    library.lookup<NativeFunction<Pointer<Void> Function()>>(
      'codux_remote_runtime_model_new',
    );
    library.lookup<NativeFunction<Pointer<Void> Function(Int64, Int64, Int64)>>(
      'codux_terminal_screen_new',
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
final _transportServerUrl = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>)
    >('codux_transport_server_url');
final _transportRelayUrlForPreset = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>)
    >('codux_transport_relay_url_for_preset');
final _transportPairingTicketUrl = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>)
    >('codux_transport_pairing_ticket_url');
final _transportPairingCodeUrl = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>)
    >('codux_transport_pairing_code_url');
final _transportPairingWebSocketUrl = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Utf8>, Pointer<Utf8>, Pointer<Utf8>)
    >('codux_transport_pairing_websocket_url');
final _transportClientWebSocketUrl = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
      ),
      Pointer<Utf8> Function(
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Pointer<Utf8>,
      )
    >('codux_transport_client_websocket_url');
final _transportDefaultIceServersJson = _dylib
    .lookupFunction<Pointer<Utf8> Function(), Pointer<Utf8> Function()>(
      'codux_transport_default_ice_servers_json',
    );
final _transportPreferredKind = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Utf8>, Bool),
      Pointer<Utf8> Function(Pointer<Utf8>, bool)
    >('codux_transport_preferred_kind');
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
final _controllerTransportReportPingTimeout = _dylib
    .lookupFunction<
      Bool Function(Pointer<Void>, Pointer<Utf8>),
      bool Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_controller_transport_report_ping_timeout');
final _controllerTransportProbePreferredRoute = _dylib
    .lookupFunction<Bool Function(Pointer<Void>), bool Function(Pointer<Void>)>(
      'codux_controller_transport_probe_preferred_route',
    );
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
final _terminalSessionNew = _dylib
    .lookupFunction<
      Pointer<Void> Function(Pointer<Utf8>, Int64),
      Pointer<Void> Function(Pointer<Utf8>, int)
    >('codux_terminal_session_new');
final _terminalSessionFree = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_terminal_session_free',
    );
final _terminalSessionSnapshotJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>),
      Pointer<Utf8> Function(Pointer<Void>)
    >('codux_terminal_session_snapshot_json');
final _terminalSessionScreenSnapshotJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>),
      Pointer<Utf8> Function(Pointer<Void>)
    >('codux_terminal_session_screen_snapshot_json');
final _terminalSessionResizeScreen = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Int64, Int64),
      void Function(Pointer<Void>, int, int)
    >('codux_terminal_session_resize_screen');
final _terminalSessionScrollScreenLines = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Int64),
      void Function(Pointer<Void>, int)
    >('codux_terminal_session_scroll_screen_lines');
final _terminalSessionScrollScreenToBottom = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_terminal_session_scroll_screen_to_bottom',
    );
final _terminalSessionContent = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>),
      Pointer<Utf8> Function(Pointer<Void>)
    >('codux_terminal_session_content');
final _terminalSessionBufferLength = _dylib
    .lookupFunction<Int64 Function(Pointer<Void>), int Function(Pointer<Void>)>(
      'codux_terminal_session_buffer_length',
    );
final _terminalSessionSequence = _dylib
    .lookupFunction<Int64 Function(Pointer<Void>), int Function(Pointer<Void>)>(
      'codux_terminal_session_sequence',
    );
final _terminalSessionIsRestoringBaseline = _dylib
    .lookupFunction<Bool Function(Pointer<Void>), bool Function(Pointer<Void>)>(
      'codux_terminal_session_is_restoring_baseline',
    );
final _terminalSessionRequireBaseline = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_terminal_session_require_baseline',
    );
final _terminalSessionResetTransient = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Bool),
      void Function(Pointer<Void>, bool)
    >('codux_terminal_session_reset_transient');
final _terminalSessionSetSequence = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Int64),
      void Function(Pointer<Void>, int)
    >('codux_terminal_session_set_sequence');
final _terminalSessionHoldLiveToken = _dylib
    .lookupFunction<
      Bool Function(Pointer<Void>, Int64, Int64),
      bool Function(Pointer<Void>, int, int)
    >('codux_terminal_session_hold_live_token');
final _terminalSessionAcceptBaselinePageJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Int64, Int64, Bool),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, int, int, bool)
    >('codux_terminal_session_accept_baseline_page_json');
final _terminalSessionReplaceFromBaselineScreenJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        Int64,
        Int64,
      ),
      Pointer<Utf8> Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Pointer<Utf8>,
        int,
        int,
      )
    >('codux_terminal_session_replace_from_baseline_screen_json');
final _terminalSessionAppendLive = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>, Int64, Int64),
      void Function(Pointer<Void>, Pointer<Utf8>, int, int)
    >('codux_terminal_session_append_live');
final _terminalSessionAppendLiveScreen = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, Int64, Int64),
      void Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>, int, int)
    >('codux_terminal_session_append_live_screen');
final _terminalSessionClear = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_terminal_session_clear',
    );
final _terminalScreenNew = _dylib
    .lookupFunction<
      Pointer<Void> Function(Int64, Int64, Int64),
      Pointer<Void> Function(int, int, int)
    >('codux_terminal_screen_new');
final _terminalScreenFree = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_terminal_screen_free',
    );
final _terminalScreenProcess = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>),
      void Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_terminal_screen_process');
final _terminalScreenResize = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Int64, Int64),
      void Function(Pointer<Void>, int, int)
    >('codux_terminal_screen_resize');
final _terminalScreenClear = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_terminal_screen_clear',
    );
final _terminalScreenSnapshotJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>),
      Pointer<Utf8> Function(Pointer<Void>)
    >('codux_terminal_screen_snapshot_json');
final _terminalOutputSequencerNew = _dylib
    .lookupFunction<Pointer<Void> Function(), Pointer<Void> Function()>(
      'codux_terminal_output_sequencer_new',
    );
final _terminalOutputSequencerFree = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_terminal_output_sequencer_free',
    );
final _terminalOutputSequencerSequenceFor = _dylib
    .lookupFunction<
      Int64 Function(Pointer<Void>, Pointer<Utf8>),
      int Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_terminal_output_sequencer_sequence_for');
final _terminalOutputSequencerObserveJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(
        Pointer<Void>,
        Pointer<Utf8>,
        Bool,
        Int64,
        Int64,
        Bool,
      ),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, bool, int, int, bool)
    >('codux_terminal_output_sequencer_observe_json');
final _terminalOutputSequencerRemove = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>),
      void Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_terminal_output_sequencer_remove');
final _terminalOutputSequencerReset = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_terminal_output_sequencer_reset',
    );
final _terminalBufferAssemblerNew = _dylib
    .lookupFunction<Pointer<Void> Function(Int64), Pointer<Void> Function(int)>(
      'codux_terminal_buffer_assembler_new',
    );
final _terminalBufferAssemblerFree = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_terminal_buffer_assembler_free',
    );
final _terminalBufferAssemblerAcceptJson = _dylib
    .lookupFunction<
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>, Pointer<Utf8>)
    >('codux_terminal_buffer_assembler_accept_json');
final _terminalBufferAssemblerRemove = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>),
      void Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_terminal_buffer_assembler_remove');
final _terminalBufferAssemblerReset = _dylib
    .lookupFunction<Void Function(Pointer<Void>), void Function(Pointer<Void>)>(
      'codux_terminal_buffer_assembler_reset',
    );
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
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>),
      Pointer<Utf8> Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_project_selected_json');
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
final _remoteRuntimeModelSetTerminalCreatingProject = _dylib
    .lookupFunction<
      Void Function(Pointer<Void>, Pointer<Utf8>),
      void Function(Pointer<Void>, Pointer<Utf8>)
    >('codux_remote_runtime_model_set_terminal_creating_project');
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

String transportServerUrl(String base) {
  final basePtr = base.toNativeUtf8();
  try {
    return _takeString(_transportServerUrl(basePtr));
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

String transportPairingTicketUrl({
  required String base,
  required String ticket,
}) {
  final basePtr = base.toNativeUtf8();
  final ticketPtr = ticket.toNativeUtf8();
  try {
    return _takeString(_transportPairingTicketUrl(basePtr, ticketPtr));
  } finally {
    malloc.free(basePtr);
    malloc.free(ticketPtr);
  }
}

String transportPairingCodeUrl({required String base, required String code}) {
  final basePtr = base.toNativeUtf8();
  final codePtr = code.toNativeUtf8();
  try {
    return _takeString(_transportPairingCodeUrl(basePtr, codePtr));
  } finally {
    malloc.free(basePtr);
    malloc.free(codePtr);
  }
}

String transportPairingWebSocketUrl({
  required String base,
  required String hostId,
  required String devicePublicKey,
}) {
  final basePtr = base.toNativeUtf8();
  final hostPtr = hostId.toNativeUtf8();
  final devicePtr = devicePublicKey.toNativeUtf8();
  try {
    return _takeString(
      _transportPairingWebSocketUrl(basePtr, hostPtr, devicePtr),
    );
  } finally {
    malloc.free(basePtr);
    malloc.free(hostPtr);
    malloc.free(devicePtr);
  }
}

String transportClientWebSocketUrl({
  required String base,
  required String hostId,
  required String deviceId,
  String token = '',
}) {
  final basePtr = base.toNativeUtf8();
  final hostPtr = hostId.toNativeUtf8();
  final devicePtr = deviceId.toNativeUtf8();
  final tokenPtr = token.toNativeUtf8();
  try {
    return _takeString(
      _transportClientWebSocketUrl(basePtr, hostPtr, devicePtr, tokenPtr),
    );
  } finally {
    malloc.free(basePtr);
    malloc.free(hostPtr);
    malloc.free(devicePtr);
    malloc.free(tokenPtr);
  }
}

List<Map<String, dynamic>> transportDefaultIceServers() {
  final decoded = _decodeJson(_transportDefaultIceServersJson());
  if (decoded is! List) return const [];
  return [
    for (final item in decoded)
      if (item is Map) Map<String, dynamic>.from(item),
  ];
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

  bool reportPingTimeout({required String path}) {
    final pathPtr = path.toNativeUtf8();
    try {
      return _controllerTransportReportPingTimeout(_liveHandle(), pathPtr);
    } finally {
      malloc.free(pathPtr);
    }
  }

  bool probePreferredRoute() {
    return _controllerTransportProbePreferredRoute(_liveHandle());
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

class TerminalSessionSnapshot {
  const TerminalSessionSnapshot({
    required this.sessionId,
    required this.content,
    required this.bufferLength,
    required this.sequence,
  });

  final String sessionId;
  final String content;
  final int bufferLength;
  final int sequence;

  factory TerminalSessionSnapshot.fromJson(Map<String, dynamic> json) {
    return TerminalSessionSnapshot(
      sessionId: '${json['sessionId'] ?? ''}',
      content: '${json['content'] ?? ''}',
      bufferLength: _jsonInt(json['bufferLength']),
      sequence: _jsonInt(json['sequence']),
    );
  }
}

class TerminalBaselinePageResult {
  const TerminalBaselinePageResult({
    required this.accepted,
    required this.duplicate,
    required this.ready,
    required this.data,
    required this.nextOffset,
    required this.progress,
  });

  final bool accepted;
  final bool duplicate;
  final bool ready;
  final String data;
  final int nextOffset;
  final double? progress;

  factory TerminalBaselinePageResult.fromJson(Map<String, dynamic> json) {
    final progress = json['progress'];
    return TerminalBaselinePageResult(
      accepted: json['accepted'] == true,
      duplicate: json['duplicate'] == true,
      ready: json['ready'] == true,
      data: '${json['data'] ?? ''}',
      nextOffset: _jsonInt(json['nextOffset']),
      progress: progress is num ? progress.toDouble() : null,
    );
  }
}

class TerminalCoreSession {
  TerminalCoreSession({required String sessionId, required int maxCachedChars})
    : _handle = _newSession(sessionId, maxCachedChars);

  Pointer<Void> _handle;

  bool get isDisposed => _handle == nullptr;
  String get content => _takeString(_terminalSessionContent(_liveHandle()));
  int get bufferLength => _terminalSessionBufferLength(_liveHandle());
  int get sequence => _terminalSessionSequence(_liveHandle());
  bool get isRestoringBaseline =>
      _terminalSessionIsRestoringBaseline(_liveHandle());

  TerminalSessionSnapshot snapshot() {
    return TerminalSessionSnapshot.fromJson(
      _decodeEnvelope(_terminalSessionSnapshotJson(_liveHandle())),
    );
  }

  TerminalScreenSnapshot screenSnapshot() {
    return TerminalScreenSnapshot.fromJson(
      _decodeEnvelope(_terminalSessionScreenSnapshotJson(_liveHandle())),
    );
  }

  void resizeScreen({required int cols, required int rows}) {
    _terminalSessionResizeScreen(_liveHandle(), cols, rows);
  }

  void scrollScreenLines(int lines) {
    _terminalSessionScrollScreenLines(_liveHandle(), lines);
  }

  void scrollScreenToBottom() {
    _terminalSessionScrollScreenToBottom(_liveHandle());
  }

  void requireBaseline() {
    _terminalSessionRequireBaseline(_liveHandle());
  }

  void resetTransient({bool resetSequence = false}) {
    _terminalSessionResetTransient(_liveHandle(), resetSequence);
  }

  void setSequence(int sequence) {
    _terminalSessionSetSequence(_liveHandle(), sequence);
  }

  bool holdLiveToken({required int? sequence, required int token}) {
    return _terminalSessionHoldLiveToken(_liveHandle(), sequence ?? -1, token);
  }

  TerminalBaselinePageResult acceptBaselinePage({
    required String data,
    required int offset,
    required int? bufferLength,
    required bool truncated,
  }) {
    final dataPtr = data.toNativeUtf8();
    try {
      return TerminalBaselinePageResult.fromJson(
        _decodeEnvelope(
          _terminalSessionAcceptBaselinePageJson(
            _liveHandle(),
            dataPtr,
            offset,
            bufferLength ?? -1,
            truncated,
          ),
        ),
      );
    } finally {
      malloc.free(dataPtr);
    }
  }

  List<int> replaceFromBaseline({
    required String content,
    String? screenData,
    required int? bufferLength,
    required int? sequence,
  }) {
    final contentPtr = content.toNativeUtf8();
    final screenDataPtr = (screenData ?? '').toNativeUtf8();
    try {
      final decoded = _decodeEnvelope(
        _terminalSessionReplaceFromBaselineScreenJson(
          _liveHandle(),
          contentPtr,
          screenDataPtr,
          bufferLength ?? -1,
          sequence ?? -1,
        ),
      );
      final tokens = decoded['replayTokens'];
      if (tokens is! List) {
        throw const FormatException(
          'Terminal core FFI did not return replay tokens',
        );
      }
      return [
        for (final token in tokens)
          if (token is num) token.toInt(),
      ];
    } finally {
      malloc.free(contentPtr);
      malloc.free(screenDataPtr);
    }
  }

  void appendLive({
    required String data,
    String? screenData,
    required int? bufferLength,
    required int? sequence,
  }) {
    final dataPtr = data.toNativeUtf8();
    final screenDataPtr = (screenData ?? '').toNativeUtf8();
    try {
      _terminalSessionAppendLiveScreen(
        _liveHandle(),
        dataPtr,
        screenDataPtr,
        bufferLength ?? -1,
        sequence ?? -1,
      );
    } finally {
      malloc.free(dataPtr);
      malloc.free(screenDataPtr);
    }
  }

  void clear() {
    _terminalSessionClear(_liveHandle());
  }

  void dispose() {
    final handle = _handle;
    if (handle == nullptr) return;
    _handle = nullptr;
    _terminalSessionFree(handle);
  }

  Pointer<Void> _liveHandle() {
    final handle = _handle;
    if (handle == nullptr) {
      throw StateError('TerminalCoreSession is disposed');
    }
    return handle;
  }
}

class TerminalScreenSnapshot {
  const TerminalScreenSnapshot({
    required this.data,
    required this.cols,
    required this.rows,
    required this.displayOffset,
    required this.cells,
    required this.cursor,
  });

  final String data;
  final int cols;
  final int rows;
  final int displayOffset;
  final List<TerminalScreenCell> cells;
  final TerminalScreenCursor cursor;

  factory TerminalScreenSnapshot.fromJson(Map<String, dynamic> json) {
    final cells = json['cells'];
    return TerminalScreenSnapshot(
      data: '${json['data'] ?? ''}',
      cols: _jsonInt(json['cols']),
      rows: _jsonInt(json['rows']),
      displayOffset: _jsonInt(json['displayOffset']),
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
    );
  }
}

class TerminalScreenCursor {
  const TerminalScreenCursor({
    required this.row,
    required this.col,
    required this.visible,
  });

  final int row;
  final int col;
  final bool visible;

  factory TerminalScreenCursor.fromJson(Map<String, dynamic> json) {
    return TerminalScreenCursor(
      row: _jsonInt(json['row']),
      col: _jsonInt(json['col']),
      visible: json['visible'] == true,
    );
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

class TerminalScreenCore {
  TerminalScreenCore({
    required int cols,
    required int rows,
    int scrollback = 2000,
  }) : _handle = _newTerminalScreen(cols, rows, scrollback);

  Pointer<Void> _handle;

  bool get isDisposed => _handle == nullptr;

  void process(String data) {
    final dataPtr = data.toNativeUtf8();
    try {
      _terminalScreenProcess(_liveHandle(), dataPtr);
    } finally {
      malloc.free(dataPtr);
    }
  }

  void resize({required int cols, required int rows}) {
    _terminalScreenResize(_liveHandle(), cols, rows);
  }

  void clear() {
    _terminalScreenClear(_liveHandle());
  }

  TerminalScreenSnapshot snapshot() {
    return TerminalScreenSnapshot.fromJson(
      _decodeEnvelope(_terminalScreenSnapshotJson(_liveHandle())),
    );
  }

  void dispose() {
    final handle = _handle;
    if (handle == nullptr) return;
    _handle = nullptr;
    _terminalScreenFree(handle);
  }

  Pointer<Void> _liveHandle() {
    final handle = _handle;
    if (handle == nullptr) {
      throw StateError('TerminalScreenCore is disposed');
    }
    return handle;
  }
}

class TerminalBufferAssemblyResult {
  const TerminalBufferAssemblyResult({
    required this.ready,
    required this.progress,
    this.payload,
  });

  final bool ready;
  final double? progress;
  final Map<dynamic, dynamic>? payload;

  factory TerminalBufferAssemblyResult.fromJson(Map<String, dynamic> json) {
    final progress = json['progress'];
    final payload = json['payload'];
    return TerminalBufferAssemblyResult(
      ready: json['ready'] == true,
      progress: progress is num ? progress.toDouble() : null,
      payload: payload is Map ? Map<dynamic, dynamic>.from(payload) : null,
    );
  }
}

class TerminalBufferAssemblerCore {
  TerminalBufferAssemblerCore({int maxChars = 200000})
    : _handle = _terminalBufferAssemblerNew(maxChars);

  Pointer<Void> _handle;

  bool get isDisposed => _handle == nullptr;

  TerminalBufferAssemblyResult accept({
    required String sessionId,
    required Map<dynamic, dynamic> payload,
  }) {
    final sessionPtr = sessionId.toNativeUtf8();
    final payloadPtr = jsonEncode(payload).toNativeUtf8();
    try {
      return TerminalBufferAssemblyResult.fromJson(
        _decodeEnvelope(
          _terminalBufferAssemblerAcceptJson(
            _liveHandle(),
            sessionPtr,
            payloadPtr,
          ),
        ),
      );
    } finally {
      malloc.free(sessionPtr);
      malloc.free(payloadPtr);
    }
  }

  void remove(String sessionId) {
    final sessionPtr = sessionId.toNativeUtf8();
    try {
      _terminalBufferAssemblerRemove(_liveHandle(), sessionPtr);
    } finally {
      malloc.free(sessionPtr);
    }
  }

  void reset() {
    _terminalBufferAssemblerReset(_liveHandle());
  }

  void dispose() {
    final handle = _handle;
    if (handle == nullptr) return;
    _handle = nullptr;
    _terminalBufferAssemblerFree(handle);
  }

  Pointer<Void> _liveHandle() {
    final handle = _handle;
    if (handle == nullptr) {
      throw StateError('TerminalBufferAssemblerCore is disposed');
    }
    return handle;
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

class TerminalOutputSequenceObservation {
  const TerminalOutputSequenceObservation({
    required this.action,
    required this.previousSeq,
    required this.shouldRender,
  });

  final String action;
  final int previousSeq;
  final bool shouldRender;

  factory TerminalOutputSequenceObservation.fromJson(
    Map<String, dynamic> json,
  ) {
    return TerminalOutputSequenceObservation(
      action: '${json['action'] ?? ''}',
      previousSeq: _jsonInt(json['previousSeq']),
      shouldRender: json['shouldRender'] == true,
    );
  }
}

class TerminalOutputSequencerCore {
  TerminalOutputSequencerCore() : _handle = _newOutputSequencer();

  Pointer<Void> _handle;

  bool get isDisposed => _handle == nullptr;

  int sequenceFor(String sessionId) {
    final sessionPtr = sessionId.toNativeUtf8();
    try {
      return _terminalOutputSequencerSequenceFor(_liveHandle(), sessionPtr);
    } finally {
      malloc.free(sessionPtr);
    }
  }

  TerminalOutputSequenceObservation observe({
    required String sessionId,
    required bool isBuffer,
    required int? outputSeq,
    required int? offset,
    required bool resetsSequence,
  }) {
    final sessionPtr = sessionId.toNativeUtf8();
    try {
      return TerminalOutputSequenceObservation.fromJson(
        _decodeEnvelope(
          _terminalOutputSequencerObserveJson(
            _liveHandle(),
            sessionPtr,
            isBuffer,
            outputSeq ?? -1,
            offset ?? -1,
            resetsSequence,
          ),
        ),
      );
    } finally {
      malloc.free(sessionPtr);
    }
  }

  void remove(String sessionId) {
    final sessionPtr = sessionId.toNativeUtf8();
    try {
      _terminalOutputSequencerRemove(_liveHandle(), sessionPtr);
    } finally {
      malloc.free(sessionPtr);
    }
  }

  void reset() {
    _terminalOutputSequencerReset(_liveHandle());
  }

  void dispose() {
    final handle = _handle;
    if (handle == nullptr) return;
    _handle = nullptr;
    _terminalOutputSequencerFree(handle);
  }

  Pointer<Void> _liveHandle() {
    final handle = _handle;
    if (handle == nullptr) {
      throw StateError('TerminalOutputSequencerCore is disposed');
    }
    return handle;
  }
}

class RemoteRuntimeCoreState {
  const RemoteRuntimeCoreState({
    required this.projects,
    required this.terminals,
    required this.lastTerminalIdByProject,
    this.selectedProjectId,
    this.activeSessionId,
    this.pendingProjectSelectId,
    this.pendingProjectSelectSent = false,
    this.projectSelectAcknowledgedId,
    this.creatingTerminalProjectId,
  });

  final List<Map<String, dynamic>> projects;
  final List<Map<String, dynamic>> terminals;
  final String? selectedProjectId;
  final String? activeSessionId;
  final String? pendingProjectSelectId;
  final bool pendingProjectSelectSent;
  final String? projectSelectAcknowledgedId;
  final String? creatingTerminalProjectId;
  final Map<String, String> lastTerminalIdByProject;

  factory RemoteRuntimeCoreState.fromJson(Map<String, dynamic> json) {
    final projects = json['projects'];
    final terminals = json['terminals'];
    final last = json['lastTerminalIdByProject'];
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
      selectedProjectId: _nullableString(json['selectedProjectId']),
      activeSessionId: _nullableString(json['activeSessionId']),
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
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    final projectsPtr = jsonEncode(projects).toNativeUtf8();
    final selectedPtr = (remoteSelectedProjectId ?? '').toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelApplyProjectListJson(
            _liveHandle(),
            projectsPtr,
            selectedPtr,
            terminalVisible,
            terminalListLoaded,
          ),
        ),
      );
    } finally {
      malloc.free(projectsPtr);
      malloc.free(selectedPtr);
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

  RemoteRuntimeCorePlan projectSelected(String? projectId) {
    final projectPtr = (projectId ?? '').toNativeUtf8();
    try {
      return RemoteRuntimeCorePlan.fromJson(
        _decodeEnvelope(
          _remoteRuntimeModelProjectSelectedJson(_liveHandle(), projectPtr),
        ),
      );
    } finally {
      malloc.free(projectPtr);
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

  void setTerminalCreatingProject(String? projectId) {
    final projectPtr = (projectId ?? '').toNativeUtf8();
    try {
      _remoteRuntimeModelSetTerminalCreatingProject(_liveHandle(), projectPtr);
    } finally {
      malloc.free(projectPtr);
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

Pointer<Void> _newSession(String sessionId, int maxCachedChars) {
  final sessionPtr = sessionId.toNativeUtf8();
  try {
    final handle = _terminalSessionNew(sessionPtr, maxCachedChars);
    if (handle == nullptr) {
      throw StateError('Failed to create terminal core session');
    }
    return handle;
  } finally {
    malloc.free(sessionPtr);
  }
}

Pointer<Void> _newRemoteRuntimeModel() {
  final handle = _remoteRuntimeModelNew();
  if (handle == nullptr) {
    throw StateError('Failed to create remote runtime model');
  }
  return handle;
}

Pointer<Void> _newOutputSequencer() {
  final handle = _terminalOutputSequencerNew();
  if (handle == nullptr) {
    throw StateError('Failed to create terminal output sequencer');
  }
  return handle;
}

Pointer<Void> _newTerminalScreen(int cols, int rows, int scrollback) {
  final handle = _terminalScreenNew(cols, rows, scrollback);
  if (handle == nullptr) {
    throw StateError('Failed to create terminal screen core');
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

String? _nullableString(Object? value) {
  if (value == null) return null;
  final text = '$value';
  return text.isEmpty ? null : text;
}
