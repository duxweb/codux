import 'dart:async';
import 'dart:io';

import 'package:device_info_plus/device_info_plus.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:codux_protocol_ffi/codux_protocol_ffi.dart'
    as codux_terminal_core;
import 'package:package_info_plus/package_info_plus.dart';
import 'package:url_launcher/url_launcher.dart';
import '../i18n.dart';
import '../models/remote_models.dart';
import '../screens/settings_screen.dart';
import '../services/log_service.dart';
import '../services/log_export_service.dart';
import '../services/local_voice_recognition_service.dart';
import '../services/mobile_settings_controller.dart';
import '../services/connection_status_presenter.dart';
import '../services/device_selection_service.dart';
import '../services/remote_device_controller.dart';
import '../services/remote_envelope_send_queue.dart';
import '../services/remote_capabilities.dart';
import '../services/remote_connection_sync_controller.dart';
import '../services/terminal_repaint_signal.dart';
import '../services/remote_project_controller.dart';
import '../services/remote_network_route_refresh_controller.dart';
import '../services/remote_protocol_service.dart';
import '../services/remote_runtime_payloads.dart';
import '../services/remote_runtime_store.dart';
import '../services/remote_sequence_guard.dart';
import '../services/remote_sync_state.dart';
import '../services/remote_project_file_controller.dart';
import '../services/remote_path_utils.dart';
import '../services/remote_terminal_binding_coordinator.dart';
import '../services/remote_terminal_output_controller.dart';
import '../services/remote_terminal_scope.dart';
import '../services/remote_transport.dart';
import '../services/storage_service.dart';
import '../services/terminal_buffer_retry.dart';
import '../services/terminal_input_batcher.dart';
import '../services/terminal_input_reliable_sender.dart';
import '../services/terminal_upload_metadata.dart';
import '../services/update_check_service.dart';
import '../services/terminal_viewport_controller.dart';
import '../theme/app_theme.dart';
import '../services/worktree_utils.dart';
import '../widgets/codux_home_shell.dart';
import '../widgets/device_home_screen.dart';
import '../widgets/project_files_panel.dart';
import '../widgets/remote_terminal_pane.dart';
import '../widgets/remote_workspace_view.dart';
import '../widgets/terminal_switcher_screen.dart';
import '../widgets/worktree_create_dialog.dart';
import '../widgets/worktree_action_dialog.dart';
import '../widgets/terminal_upload_source_sheet.dart';
import '../widgets/update_available_dialog.dart';
import '../widgets/codux_about_dialog.dart';
import '../widgets/debug_log_dialog.dart';
import '../widgets/device_action_dialogs.dart';
import '../widgets/file_action_dialogs.dart';

final String _remoteProtocolVersion = remoteProtocolVersion;
const Duration _remoteStartupProbeTimeout = Duration(seconds: 15);
const Duration _remoteLatencyProbeInterval = Duration(seconds: 3);
const Duration _remoteLatencyProbeTimeout = Duration(seconds: 8);

class _PendingWorktreeSwitch {
  const _PendingWorktreeSwitch({
    required this.projectId,
    required this.worktreeId,
  });

  final String projectId;
  final String worktreeId;
}

class CoduxHomePage extends StatefulWidget {
  const CoduxHomePage({
    super.key,
    required this.onChangeAccent,
    required this.onChangeLocale,
    this.initialDevices,
    this.transportFactory,
  });

  final ValueChanged<AccentOption> onChangeAccent;
  final ValueChanged<LocaleOption> onChangeLocale;
  final List<StoredDevice>? initialDevices;
  final RemoteTransportFactory? transportFactory;

  @override
  State<CoduxHomePage> createState() => _CoduxHomePageState();
}

class _CoduxHomePageState extends State<CoduxHomePage>
    with TickerProviderStateMixin, WidgetsBindingObserver {
  static const int _terminalBufferMaxChars =
      TerminalBufferCapability.mobileMaxChars;

  final _storage = StorageService();
  final _deviceSelection = const DeviceSelectionService();
  final _connectionStatusPresenter = const ConnectionStatusPresenter();
  final _updateCheckService = const UpdateCheckService();
  final _logExportService = const LogExportService();
  final _mobileSettingsController = const MobileSettingsController();
  final _deviceController = const RemoteDeviceController();
  final _sendQueue = RemoteEnvelopeSendQueue();
  final _projectController = const RemoteProjectController();
  final _projectFileController = RemoteProjectFileController();
  final _worktreeController = const RemoteWorktreeController();
  final _remoteSyncController = RemoteConnectionSyncController();
  final _remoteRuntime = RemoteRuntimeStore();
  final _terminalRepaint = TerminalRepaintSignal();
  final _settingsNameController = TextEditingController();
  final _fileEditorController = CodeEditingController();
  final _projectNameController = TextEditingController();
  final _projectPathController = TextEditingController();

  late final AnimationController _edgeBackController;
  late final TerminalBufferRetryCoordinator _terminalBufferRetry;
  late final TerminalInputBatcher _terminalInputBatcher;
  late final TerminalInputReliableSender _terminalInputSender;
  late final RemoteNetworkRouteRefreshController _networkRouteRefreshController;
  late final LocalVoiceRecognitionService _voiceService;
  RemoteTransport? _activeTransport;
  Completer<void>? _terminalUploadCompletion;
  final TerminalViewportController _terminalViewportController =
      TerminalViewportController();
  final RemoteTerminalOutputController _terminalOutputController =
      RemoteTerminalOutputController(maxBufferChars: _terminalBufferMaxChars);
  late final RemoteTerminalBindingCoordinator _terminalBindingCoordinator;
  final Set<String> _protocolBlockedHostIds = {};
  int _terminalBufferRequestCounter = 0;
  bool _keyboardRequested = false;
  int _keyboardRequestSerial = 0;
  bool _keyboardShownSinceRequest = false;
  bool _keyboardVisible = false;

  List<StoredDevice> _devices = [];
  List<ProjectInfo> _projects = [];
  List<TerminalInfo> _terminals = [];
  StoredDevice? _activeDevice;
  Timer? _viewportLeaseKeepalive;
  TerminalBufferCapability _terminalBufferCapability =
      TerminalBufferCapability.fallback;
  String? _hostRuntimeInstanceId;
  MobileSettings _settings = const MobileSettings(localName: '');
  String _detectedDeviceName = 'Codux Mobile';
  String _status = '';
  String? _selectedProjectId;
  String? _sessionId;
  String? _creatingTerminalProjectId;
  String? _creatingTerminalLayoutKind;
  String? _terminalSelectedText;
  bool _showSettings = false;
  bool _showScanner = false;
  PairingPayload? _pendingPairing;
  bool _pairingInFlight = false;
  bool _pairingCancelled = false;
  String? _pairingError;
  bool _showTerminal = false;
  bool _showTerminalSwitcher = false;
  bool _terminalReady = false;
  bool _terminalViewportInteractive = false;
  RemoteTerminalBufferPhase _terminalBufferPhase =
      RemoteTerminalBufferPhase.idle;
  double? _terminalBufferProgress;
  bool _terminalUploadLoading = false;
  String _terminalUploadStatus = '';
  RemoteSyncState get _remoteSync => _remoteSyncController.syncState;
  bool get _terminalListLoaded => _remoteSync.terminalListLoaded;
  bool get _terminalViewportClaimable =>
      _showTerminal &&
      !_showTerminalSwitcher &&
      !_showSettings &&
      !_showScanner &&
      !_showProjectForm &&
      !_showFilePicker &&
      _workspaceMode == 'terminal';

  bool get _terminalDataVisible =>
      _showTerminal && _workspaceMode == 'terminal';

  bool get _projectListLoaded => _remoteSync.projectListLoaded;
  bool _backgroundConnect = false;
  bool _shouldReconnect = true;
  bool _transportReady = false;
  bool get _remoteProtocolReady => _remoteSyncController.protocolReady;
  bool _hostResponsive = false;
  int _hostResponseSerial = 0;
  bool _appSuspended = false;
  bool _disposing = false;
  bool _hasShownTerminal = false;
  bool _aiStatsLoading = false;
  bool _showProjectForm = false;
  bool _showFilePicker = false;
  bool _showVoiceOverlay = false;
  bool _filePickerLoading = false;
  ProjectFormMode _projectFormMode = ProjectFormMode.add;
  String _filePickerMode = 'projectForm';
  String _filePickerPath = '';
  String? _filePickerParent;
  List<RemoteFileEntry> _filePickerEntries = [];
  List<RemoteFileEntry> _projectFileEntries = [];
  AIStatsInfo? _currentAIStats;
  String _workspaceMode = 'terminal';
  String _projectFilesPath = '';
  String? _projectFilesParent;
  String? _editingFilePath;
  String? _toastMessage;
  String? _blockingLoadingMessage;
  bool _projectFilesLoading = false;
  bool _worktreeListLoading = false;
  bool _creatingWorktree = false;
  bool _fileEditorLoading = false;
  bool _fileEditorSaving = false;
  bool _fileEditorEditing = false;
  bool _fileEditorEditable = true;
  int _reconnectAttempt = 0;
  bool _appInForeground = true;

  bool _transportConnected = false;
  bool _connectInFlight = false;
  String? _connectInFlightKey;
  int get _transportGeneration => _remoteSyncController.generation;
  int _remoteRuntimeEpoch = 0;
  final _receiveSequenceGuard = RemoteSequenceGuard();
  Future<void> _receiveChain = Future<void>.value();
  Timer? _reconnectTimer;
  Timer? _healthTimer;
  Timer? _toastTimer;
  Timer? _filePickerTimeoutTimer;
  Timer? _projectListRetryTimer;
  Timer? _terminalListRetryTimer;
  final Map<String, Timer> _projectSelectAckTimers = {};
  Timer? _hostResponseTimer;
  Timer? _transportCloseTimer;
  int get _projectListRetryAttempt => _remoteSync.projectListRetryAttempt;
  int get _terminalListRetryAttempt => _remoteSync.terminalListRetryAttempt;
  double? _edgeBackDragStartX;
  double _edgeBackDragDeltaX = 0;
  double _edgeBackDragDeltaY = 0;
  String _lastTransportState = RemoteTransportKind.iroh;
  String _connectionPath = 'unknown';
  String _connectionEndpoint = '';
  String _connectionRelayUrl = '';
  DateTime? _lastConnectedAt;
  DateTime? _connectionGraceUntil;
  DateTime? _lastTransportRefreshAt;
  _PendingWorktreeSwitch? _pendingWorktreeSwitch;
  int? _latencyMs;
  Timer? _latencyProbeTimer;
  int _latencyProbeCounter = 0;
  final Map<String, DateTime> _latencyProbeSentAt = {};
  Timer? _connectionGraceTimer;

  bool get _isConnected => _transportConnected && _transportReady;
  bool get _isHostReady =>
      _isConnected &&
      _hostResponsive &&
      _connectionPath != 'unknown' &&
      _connectionPath != 'none';
  bool get _isRecoveringConnection {
    final graceUntil = _connectionGraceUntil;
    return _appInForeground &&
        _activeDevice != null &&
        _shouldReconnect &&
        graceUntil != null &&
        DateTime.now().isBefore(graceUntil);
  }

  bool get _isDeviceListConnected => _isHostReady;

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

  String _t(String key, {Map<String, String>? params}) =>
      AppPreferences.of(context).t(key, params: params);

  ConnectionStatusSnapshot get _connectionStatusSnapshot =>
      ConnectionStatusSnapshot(
        connected: _isConnected,
        hostResponsive: _hostResponsive,
        connectionPath: _connectionPath,
        projectListLoaded: _projectListLoaded,
        hasProjects: _projects.isNotEmpty,
        recovering: _isRecoveringConnection,
        hasActiveDevice: _activeDevice != null,
        backgroundConnect: _backgroundConnect,
        status: _status,
        connectedText: _t('app.connected'),
      );

  String get _connectionStatusText {
    final key = _connectionStatusPresenter.connectionStatusKey(
      _connectionStatusSnapshot,
    );
    return key.isEmpty ? _status : _t(key);
  }

  String get _deviceListStatusText {
    if (_isConnected && _hostResponsive) {
      if (!_projectListLoaded && _projects.isEmpty) return _t('status.sync');
      return switch (_connectionPath) {
        'direct' => _t('status.direct'),
        'mixed' => _t('status.relay'),
        'relay' => _t('status.relay'),
        _ => _t('status.connecting'),
      };
    }
    if (_isRecoveringConnection) return _t('status.retry');
    if (_transportConnected || _backgroundConnect) {
      return _t('status.connecting');
    }
    if (_status == _t('pair.repairRequired') ||
        _status == _t('pair.rejected')) {
      return _t('status.rejected');
    }
    if (_status == _t('connection.failedRetry') ||
        _status == _t('app.remoteNotConnected') ||
        _status == _t('connection.macDisconnected')) {
      return _t('status.failed');
    }
    if (_status == _t('app.reconnecting')) return _t('status.offline');
    return _t('status.offline');
  }

  String _deviceSubtitle(StoredDevice device) {
    final isActive = device.deviceId == _activeDevice?.deviceId;
    return _deviceEndpointText(
      device: device,
      path: isActive && _isDeviceListConnected ? _connectionPath : 'unknown',
      endpoint: isActive && _isDeviceListConnected ? _connectionEndpoint : '',
      relayUrl: isActive && _isDeviceListConnected ? _connectionRelayUrl : '',
    );
  }

  String _deviceEndpointText({
    required StoredDevice device,
    required String path,
    required String endpoint,
    required String relayUrl,
  }) {
    final cleanedEndpoint = cleanRemoteTransportEndpoint(endpoint);
    final cleanedRelayUrl = cleanRemoteTransportEndpoint(relayUrl);
    if (path == 'relay' && cleanedRelayUrl.isNotEmpty) {
      return remoteRelayDisplayName(cleanedRelayUrl);
    }
    if (cleanedEndpoint.isNotEmpty) return cleanedEndpoint;
    final nodeId = _savedDeviceNodeId(device);
    if (path == 'direct' && nodeId.isNotEmpty) {
      return nodeId;
    }
    final savedRelay = _savedDeviceRelayEndpoint(device);
    if (savedRelay.isNotEmpty) return remoteRelayDisplayName(savedRelay);
    return _t('device.globalNetwork');
  }

  String _savedDeviceRelayEndpoint(StoredDevice device) {
    for (final candidate in device.transports) {
      if (candidate.relayUrl.trim().isNotEmpty) {
        return cleanRemoteTransportEndpoint(candidate.relayUrl);
      }
    }
    for (final candidate in device.transports) {
      if (candidate.url.trim().isNotEmpty) {
        return cleanRemoteTransportEndpoint(candidate.url);
      }
    }
    return cleanRemoteTransportEndpoint(device.server);
  }

  String _savedDeviceNodeId(StoredDevice device) {
    for (final candidate in device.transports) {
      final nodeId = candidate.nodeId.trim();
      if (nodeId.isNotEmpty) return nodeId;
    }
    return '';
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
      setState(() {
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
    setState(() {
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
    setState(() {
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
    setState(() {
      _transportReady = false;
      _remoteSyncController.resetProtocolReady();
      _hostResponsive = false;
      _backgroundConnect = false;
      _showTerminal = false;
      _workspaceMode = 'terminal';
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
    setState(() {
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
      setState(() {
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
    setState(() => _latencyMs = rtt);
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
    _workspaceMode = 'terminal';
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
    setState(() {
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

  void _mountVisibleTerminal({required String reason}) {
    final sessionId = _sessionId;
    if (sessionId == null || _workspaceMode != 'terminal') return;
    if (!_terminalViewportClaimable) return;
    final restored = _restoreTerminalSessionFromCache(sessionId);
    CoduxLog.debug(
      '[codux-flutter-terminal] mount session=$sessionId reason=$reason cached=$restored',
    );
    _focusTerminalViewSoon();
    if (restored) {
      // Re-request a baseline on foreground resume (output may have been
      // missed while backgrounded) or when live frames were actually dropped.
      // A cached, in-sync session being switched back to must NOT be reloaded:
      // replaying the trimmed raw history would clobber the live screen -- for
      // a TUI it loses the alt-screen / mouse-tracking modes set long ago (now
      // trimmed off the front), so the input border vanishes and scrolling
      // stops. The viewport re-claim/resize on mount triggers a fresh repaint,
      // so a gap-free switch stays current without a reload.
      final needsBaseline =
          reason == 'foreground' ||
          _terminalOutputController.hasSequenceGap(sessionId);
      if (_transportConnected && _remoteProtocolReady && needsBaseline) {
        final requested = _terminalBindingCoordinator.subscribeSessionBaseline(
          sessionId: sessionId,
          reason: 'mount-$reason',
          capability: _terminalBufferCapability,
          replaceActive: true,
        );
        if (requested) {
          _trackTerminalBaselineRequest(sessionId);
        }
      }
      return;
    }
    final projectId = _selectedProjectId;
    if (projectId == null) return;
    _terminalBindingCoordinator.replaceProjectSubscription(
      projectId: projectId,
      reason: 'mount-$reason',
      capability: _terminalBufferCapability,
      activeSessionId: _sessionId,
    );
  }

  bool _restoreTerminalSessionFromCache(String sessionId) {
    // The self-drawn renderer reads the cell snapshot straight from the output
    // controller, so a cached session just needs a repaint. Report whether any
    // local content exists so the caller can decide whether to show it.
    if (!_terminalOutputController.hasCachedOutput(sessionId)) return false;
    _terminalRepaint.tick();
    if (mounted) setState(() {});
    return true;
  }

  ProjectInfo? get _selectedProject {
    return _remoteRuntime.selectedProject();
  }

  List<RemoteWorktreeInfo> get _worktrees => _remoteRuntime.worktrees;

  String? get _selectedWorktreeId => _remoteRuntime.selectedWorktreeId;

  List<String> get _worktreeBaseBranches {
    final projectId = _selectedProjectId ?? _remoteRuntime.selectedProjectId;
    return projectId == null
        ? const []
        : _remoteRuntime.baseBranchesForProject(projectId);
  }

  String? get _defaultWorktreeBaseBranch {
    final projectId = _selectedProjectId ?? _remoteRuntime.selectedProjectId;
    return projectId == null
        ? null
        : _remoteRuntime.defaultBaseBranchForProject(projectId);
  }

  List<RemoteWorktreeInfo> _worktreesForProject(String projectId) {
    return _worktrees
        .where((worktree) => worktree.projectId == projectId)
        .toList(growable: false);
  }

  @override
  void initState() {
    super.initState();
    // The host viewport lease (20s TTL) is renewed by input and output
    // acks; an idle session emits neither, so keep the lease alive while
    // the terminal is actually on screen. Claims for the current owner are
    // idempotent renewals on the host.
    _viewportLeaseKeepalive = Timer.periodic(const Duration(seconds: 8), (_) {
      if (!mounted || !_appInForeground) return;
      if (_workspaceMode != 'terminal' || !_hasShownTerminal) return;
      if (_sessionId == null) return;
      // Renew only: a phone left idle on the terminal screen must not
      // steal the viewport back from an actively-used desktop. Explicit
      // interaction (scroll, input) reclaims instead.
      if (!_terminalViewportInteractive) return;
      _claimTerminalViewport();
    });
    WidgetsBinding.instance.addObserver(this);
    _edgeBackController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 220),
    );
    _terminalBufferRetry = TerminalBufferRetryCoordinator(
      // Chunked baselines over a slow relay can take seconds; retrying
      // mid-transfer would wipe the assembler and restart the download.
      retryDelay: const Duration(milliseconds: 2500),
      onRetryExhausted: (sessionId) {
        if (!mounted || _sessionId != sessionId) return;
        _terminalOutputController.resetSessionTransient(
          sessionId,
          resetSequence: true,
        );
        _terminalBufferRetry.resetLastBuffered();
        setState(() => _setTerminalBufferLoading(false));
      },
    );
    _terminalInputBatcher = TerminalInputBatcher(
      send: (data) => _sendInputNow(data, source: 'typed-batch'),
    );
    _terminalInputSender = TerminalInputReliableSender(
      send: _sendTerminalEnvelope,
      activeSessionId: () => _sessionId,
    );
    _terminalBindingCoordinator = RemoteTerminalBindingCoordinator(
      outputController: _terminalOutputController,
      send: _send,
      terminalById: _terminalById,
      nextRequestId: _nextTerminalBufferRequestId,
      maxCharsLimit: _terminalBufferMaxChars,
    );
    _networkRouteRefreshController = RemoteNetworkRouteRefreshController(
      onPauseLatency: _pauseLatencyProbe,
      onRefreshRoute: (reason) {
        if (!mounted || _disposing || !_appInForeground) return;
        _refreshTransportRoute(reason: reason);
      },
      onInitialSignature: (signature) {
        CoduxLog.info('[codux-flutter-network] initial state=$signature');
      },
      onSignatureChanged: (previous, next) {
        CoduxLog.info(
          '[codux-flutter-network] changed from=$previous to=$next path=$_connectionPath',
        );
      },
      onInitialCheckFailed: (error) {
        CoduxLog.warn('[codux-flutter-network] initial check failed $error');
      },
      onListenError: (error) {
        CoduxLog.warn('[codux-flutter-network] listen failed $error');
      },
    );
    _voiceService = LocalVoiceRecognitionService(
      onLog: (message) => CoduxLog.info('[codux-flutter-voice] $message'),
    );
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _startNetworkRouteRefresh();
      unawaited(_bootstrap());
    });
  }

  @override
  void dispose() {
    final wasConnected = _transportConnected;
    if (wasConnected) {
      _notifyHostBeforeTransportClose();
    }
    _viewportLeaseKeepalive?.cancel();
    WidgetsBinding.instance.removeObserver(this);
    _disposing = true;
    _shouldReconnect = false;
    _reconnectTimer?.cancel();
    _healthTimer?.cancel();
    _clearLatencyProbe();
    _connectionGraceTimer?.cancel();
    _transportCloseTimer?.cancel();
    _networkRouteRefreshController.dispose();
    _toastTimer?.cancel();
    _filePickerTimeoutTimer?.cancel();
    _projectListRetryTimer?.cancel();
    _terminalListRetryTimer?.cancel();
    for (final timer in _projectSelectAckTimers.values) {
      timer.cancel();
    }
    _projectSelectAckTimers.clear();
    _hostResponseTimer?.cancel();
    _terminalBufferRetry.dispose();
    _terminalInputBatcher.dispose();
    _terminalInputSender.dispose();
    _terminalUploadCompletion?.completeError(
      StateError('Terminal upload cancelled'),
    );
    _terminalUploadCompletion = null;
    _voiceService.dispose();
    _terminalRepaint.dispose();
    _terminalOutputController.dispose();
    unawaited(_closeActiveTransport());
    _settingsNameController.dispose();
    _fileEditorController.dispose();
    _projectNameController.dispose();
    _projectPathController.dispose();
    _edgeBackController.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    CoduxLog.info('[codux-flutter-lifecycle] state=${state.name}');
    if (state == AppLifecycleState.resumed) {
      _appInForeground = true;
      _appSuspended = false;
      final device = _activeDevice;
      if (device == null) return;
      if (_transportConnected) {
        CoduxLog.info(
          '[codux-flutter-lifecycle] resume keep existing transport host=${device.hostId} device=${device.deviceId}',
        );
        _recoverForegroundState();
        return;
      }
      CoduxLog.info(
        '[codux-flutter-lifecycle] resume reconnect host=${device.hostId} device=${device.deviceId}',
      );
      _connect(device, true);
      return;
    }
    if (state == AppLifecycleState.inactive) {
      return;
    }
    if (state == AppLifecycleState.paused && _showVoiceOverlay) {
      CoduxLog.info('[codux-flutter-lifecycle] pause ignored for voice input');
      return;
    }
    if (state == AppLifecycleState.detached) {
      _appInForeground = false;
      _appSuspended = true;
      _disconnectTransport(
        status: _t('app.disconnected'),
        closeTerminal: false,
      );
      if (mounted) {
        setState(() {
          _terminalBufferRetry.reset();
          _terminalOutputController.resetTransient();
          _setTerminalBufferLoading(false);
        });
      }
      return;
    }
    if (state == AppLifecycleState.paused ||
        state == AppLifecycleState.hidden) {
      _appInForeground = false;
      _appSuspended = true;
      _pauseLatencyProbe();
      // Hand the viewport straight back: the host flips the owner to the
      // desktop and broadcasts, so the desktop restores its own dimensions
      // immediately instead of waiting for the lease to expire.
      if (_transportConnected) {
        _releaseTerminalViewport();
      }
      CoduxLog.info(
        '[codux-flutter-lifecycle] background keep transport state=${state.name}',
      );
    }
  }

  Future<void> _bootstrap() async {
    final initialDevices = widget.initialDevices;
    if (initialDevices != null) {
      if (!mounted) return;
      setState(() {
        _devices = initialDevices;
        _activeDevice = initialDevices.isNotEmpty ? initialDevices.first : null;
        _showTerminal = false;
      });
      if (initialDevices.isNotEmpty) {
        unawaited(_restoreCachedProjects(initialDevices.first));
        _scheduleStartupConnect(initialDevices.first);
      }
      return;
    }
    await _loadDeviceName();
    final loadedSettings = await _storage.loadSettings();
    final devices = await _storage.loadDevices();
    final lastDeviceId = await _storage.loadLastDeviceId();
    if (!mounted) return;
    final next = _mobileSettingsController.startupSettings(
      stored: loadedSettings,
      detectedDeviceName: _detectedDeviceName,
    );
    final startupDevice = _deviceSelection.selectStartupDevice(
      devices,
      lastDeviceId,
    );
    CoduxLog.setLevelName(next.logLevel);
    widget.onChangeAccent(AccentChoices.byId(next.accentId));
    widget.onChangeLocale(LocaleChoices.byId(next.localeId));
    setState(() {
      _settings = next;
      _settingsNameController.text = next.localName;
      _devices = devices;
      _activeDevice = startupDevice.displayedDevice;
      _showTerminal = false;
    });
    final autoConnectDevice = startupDevice.autoConnectDevice;
    if (autoConnectDevice != null) {
      unawaited(_restoreCachedProjects(autoConnectDevice));
      _scheduleStartupConnect(autoConnectDevice);
    }
  }

  void _scheduleStartupConnect(StoredDevice device) {
    Timer(const Duration(milliseconds: 150), () {
      if (!mounted || _disposing || !_appInForeground) return;
      if (_activeDevice?.hostId != device.hostId ||
          _activeDevice?.deviceId != device.deviceId) {
        return;
      }
      _connect(device, true);
    });
  }

  Future<void> _restoreCachedProjects(StoredDevice device) async {
    try {
      final cached = await _storage.loadCachedProjects(device);
      if (!mounted ||
          _activeDevice?.hostId != device.hostId ||
          cached.isEmpty ||
          _projects.isNotEmpty) {
        return;
      }
      _remoteRuntime.restoreCachedProjects(cached);
      setState(() {
        _syncRuntimeViewState();
      });
      CoduxLog.info(
        '[codux-flutter-projects] cache restored count=${cached.length} host=${device.hostId}',
      );
    } catch (error) {
      CoduxLog.warn('[codux-flutter-projects] cache restore failed: $error');
    }
  }

  Future<void> _loadDeviceName() async {
    try {
      final plugin = DeviceInfoPlugin();
      final info = await plugin.deviceInfo;
      _detectedDeviceName = _mobileSettingsController
          .detectedNameFromDeviceInfo(info.data);
    } catch (_) {
      _detectedDeviceName = MobileSettingsController.fallbackDeviceName;
    }
  }

  Future<void> _saveDevices(List<StoredDevice> devices) async {
    final nextState = _deviceController.preserveActive(
      devices: devices,
      activeDevice: _activeDevice,
    );
    setState(() {
      _devices = nextState.devices;
      _activeDevice = nextState.activeDevice;
    });
    await _storage.saveDevices(nextState.devices);
  }

  void _rememberActiveDevice(StoredDevice device) {
    unawaited(_storage.saveLastDeviceId(device.deviceId));
  }

  Future<void> _saveDevice(StoredDevice device) async {
    final nextState = _deviceController.upsertAndActivate(
      devices: _devices,
      device: device,
    );
    await _saveDevices(nextState.devices);
    setState(() {
      _activeDevice = nextState.activeDevice;
      _showTerminal = false;
      _status = _t('pair.success');
    });
    _connect(device);
  }

  void _handleScannedPayload(String raw) {
    if (!_showScanner || _pendingPairing != null) return;
    unawaited(_prepareScannedPayload(raw));
  }

  Future<void> _prepareScannedPayload(String raw) async {
    try {
      final payload = await parsePairingPayload(raw);
      CoduxLog.debug(
        '[codux-flutter-pairing] scanned payload server=${payload.server} host=${payload.hostId ?? ''} pair=${payload.pairingId ?? ''} transports=${payload.transports.length}',
      );
      if (!mounted || !_showScanner || _pendingPairing != null) return;
      setState(() {
        _showScanner = false;
        _pendingPairing = payload;
        _pairingInFlight = false;
        _pairingCancelled = false;
        _pairingError = null;
      });
    } catch (error) {
      CoduxLog.warn('[codux-flutter-pairing] scan failed error=$error');
      if (!mounted) return;
      setState(() => _showScanner = false);
      _showToast(error.toString().replaceFirst('Exception: ', ''));
    }
  }

  void _cancelPairing() {
    if (_pairingInFlight) {
      setState(() => _pairingCancelled = true);
      return;
    }
    setState(() {
      _pendingPairing = null;
      _pairingInFlight = false;
      _pairingCancelled = false;
      _pairingError = null;
    });
  }

  void _pauseRemoteConnectionForPairing() {
    _stopRemoteConnectionForAuthChange();
  }

  Future<void> _confirmPairing() async {
    final payload = _pendingPairing;
    if (payload == null || _pairingInFlight) return;
    final name = _settings.localName.isNotEmpty
        ? _settings.localName
        : _detectedDeviceName;
    _pauseRemoteConnectionForPairing();
    setState(() {
      _transportReady = false;
      _hostResponsive = false;
      _backgroundConnect = false;
      _pairingInFlight = true;
      _pairingCancelled = false;
      _pairingError = null;
      _status = _t('pair.submitting');
    });
    CoduxLog.debug(
      '[codux-flutter-pairing] confirm start server=${payload.server} host=${payload.hostId ?? ''} pair=${payload.pairingId ?? ''}',
    );
    try {
      final confirmed = await _confirmIrohPairing(payload, name);
      if (!mounted) return;
      final hostName = confirmed.hostName?.trim().isNotEmpty == true
          ? confirmed.hostName!.trim()
          : confirmed.name;
      setState(() {
        _pendingPairing = null;
        _pairingInFlight = false;
        _pairingCancelled = false;
        _pairingError = null;
      });
      await _saveDevice(confirmed);
      _showToast(_t('device.bound', params: {'name': hostName}));
    } on PairingCancelledException {
      if (!mounted) return;
      setState(() {
        _pendingPairing = null;
        _pairingInFlight = false;
        _pairingCancelled = false;
        _pairingError = null;
        _status = _t('pair.cancelled');
      });
    } on PairingRejectedException {
      if (!mounted) return;
      setState(() {
        _pendingPairing = null;
        _pairingInFlight = false;
        _pairingCancelled = false;
        _pairingError = null;
        _status = _t('pair.rejected');
      });
      _showToast(_t('pair.rejected'));
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _pairingInFlight = false;
        _pairingCancelled = false;
        _pairingError = error.toString().replaceFirst('Exception: ', '');
        _status = _pairingError ?? _t('pair.failed');
      });
    }
  }

  Future<StoredDevice> _confirmIrohPairing(
    PairingPayload payload,
    String name,
  ) async {
    setState(() => _status = _t('pair.waiting'));
    try {
      return await Future.any<StoredDevice>([
        confirmPairingOverIroh(
          payload: payload,
          name: name,
          timeout: const Duration(seconds: 90),
        ),
        _waitPairingCancelled(),
      ]);
    } on PairingRejectedException {
      rethrow;
    }
  }

  Future<StoredDevice> _waitPairingCancelled() async {
    while (!_pairingCancelled) {
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    throw const PairingCancelledException();
  }

  Future<void> _saveSettings() async {
    final next = _mobileSettingsController.saveSettings(
      current: _settings,
      inputLocalName: _settingsNameController.text,
      detectedDeviceName: _detectedDeviceName,
    );
    await _storage.saveSettings(next);
    CoduxLog.setLevelName(next.logLevel);
    setState(() {
      _settings = next;
      _status = _t('settings.saved');
    });
    _popCupertinoPage(() {
      _showSettings = false;
    });
    _sendDeviceInfo(force: true);
  }

  void _connect([StoredDevice? device, bool background = false]) {
    final target = device ?? _activeDevice;
    if (target == null) {
      setState(() => _showScanner = true);
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
        setState(() => _status = _t('connection.upgradeRequired'));
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
    setState(() {
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
      setState(() => _status = _t('pair.repairRequired'));
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
        setState(() => _status = _t('connection.failedRetry'));
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
          setState(() => _status = _t('connection.failedRetry'));
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

  void _requestProjectList({bool resetRetry = false}) {
    if (!_remoteProtocolReady) return;
    if (resetRetry) {
      _projectListRetryTimer?.cancel();
      _projectListRetryTimer = null;
      _remoteSync.resetProjectListRetry();
    }
    if (!_remoteSync.shouldRequestProjectList(force: resetRetry)) return;
    _send(
      RelayEnvelope(type: RemoteMessageType.projectList),
      onResult: (_, result) {
        if (result != RemoteEnvelopeSendResult.delivered ||
            _projectListLoaded) {
          return;
        }
        _remoteSync.markProjectListRequested();
        CoduxLog.info(
          '[codux-flutter-projects] request project.list attempt=$_projectListRetryAttempt',
        );
        _scheduleProjectListRetry();
      },
    );
  }

  void _scheduleProjectListRetry() {
    if (!_transportReady || _projectListLoaded) return;
    _projectListRetryTimer?.cancel();
    if (!_remoteSync.canRetryProjectList(6)) return;
    final delay = Duration(
      milliseconds: (800 * (1 << _projectListRetryAttempt)).clamp(800, 5000),
    );
    _projectListRetryTimer = Timer(delay, () {
      if (!mounted || !_transportReady || _projectListLoaded) return;
      final attempt = _remoteSync.nextProjectListRetryAttempt();
      CoduxLog.info(
        '[codux-flutter-projects] retry project.list attempt=$attempt',
      );
      _requestProjectList();
    });
  }

  void _markProjectListReceived() {
    _remoteSync.markProjectListReceived();
    _projectListRetryTimer?.cancel();
    _projectListRetryTimer = null;
    CoduxLog.debug('[codux-flutter-projects] project.list received');
  }

  void _requestTerminalList({bool resetRetry = false}) {
    if (!_remoteProtocolReady) return;
    if (resetRetry) {
      _terminalListRetryTimer?.cancel();
      _terminalListRetryTimer = null;
      _remoteSync.resetTerminalListRetry();
    }
    if (!_remoteSync.shouldRequestTerminalList(force: resetRetry)) return;
    _send(
      RelayEnvelope(type: RemoteMessageType.terminalList),
      onResult: (_, result) {
        if (result != RemoteEnvelopeSendResult.delivered ||
            _terminalListLoaded) {
          return;
        }
        _remoteSync.markTerminalListRequested();
        CoduxLog.info(
          '[codux-flutter-terminal] request terminal.list attempt=$_terminalListRetryAttempt',
        );
        _scheduleTerminalListRetry();
      },
    );
  }

  void _requestWorktreeList({bool loading = false}) {
    final project = _selectedProject;
    if (!_remoteProtocolReady || project == null) return;
    if (loading) {
      setState(() {
        _worktreeListLoading = true;
      });
    }
    _send(_worktreeController.listEnvelope(project));
  }

  void _ensureSelectedProjectWorktrees({bool loading = false}) {
    final projectId = _selectedProjectId;
    if (projectId == null || _remoteRuntime.hasWorktreesForProject(projectId)) {
      return;
    }
    _requestWorktreeList(loading: loading);
  }

  void _scheduleTerminalListRetry() {
    if (!_transportReady || _terminalListLoaded) return;
    _terminalListRetryTimer?.cancel();
    if (!_remoteSync.canRetryTerminalList(6)) return;
    final delay = Duration(
      milliseconds: (800 * (1 << _terminalListRetryAttempt)).clamp(800, 5000),
    );
    _terminalListRetryTimer = Timer(delay, () {
      if (!mounted || !_transportReady || _terminalListLoaded) return;
      final attempt = _remoteSync.nextTerminalListRetryAttempt();
      CoduxLog.info(
        '[codux-flutter-terminal] retry terminal.list attempt=$attempt',
      );
      _requestTerminalList();
    });
  }

  void _markTerminalListReceived() {
    _remoteSync.markTerminalListReceived();
    _terminalListRetryTimer?.cancel();
    _terminalListRetryTimer = null;
    CoduxLog.debug('[codux-flutter-terminal] terminal.list received');
  }

  void _markActiveDeviceResponsive() {
    final device = _activeDevice;
    if (device != null) _rememberActiveDevice(device);
  }

  bool _sendProjectSelect(String projectId, {required String reason}) {
    final scope = _remoteRuntime.terminalScopeForProject(projectId);
    final payload = <String, Object>{
      'projectId': projectId,
      if (scope?.worktreeId != null && scope!.worktreeId!.trim().isNotEmpty)
        'worktreeId': scope.worktreeId!,
      if (scope?.projectPath != null && scope!.projectPath!.trim().isNotEmpty)
        'projectPath': scope.projectPath!,
    };
    CoduxLog.info(
      '[codux-flutter-projects] send project.select reason=$reason project=$projectId worktree=${payload['worktreeId'] ?? ''}',
    );
    final sent = _send(
      RelayEnvelope(type: RemoteMessageType.projectSelect, payload: payload),
      onResult: (message, result) {
        if (result == RemoteEnvelopeSendResult.delivered) {
          _scheduleProjectSelectAckTimeout(projectId);
          return;
        }
        _remoteRuntime.clearPendingProjectSelectSent(projectId);
        if (!mounted || _disposing) return;
        CoduxLog.warn(
          '[codux-flutter-projects] project.select delivery failed reason=$reason project=$projectId result=${result.name}',
        );
      },
    );
    if (sent) {
      _remoteRuntime.markProjectSelectSent(projectId);
    } else {
      CoduxLog.warn(
        '[codux-flutter-projects] project.select not sent reason=$reason project=$projectId connected=$_transportConnected ready=$_transportReady',
      );
    }
    return sent;
  }

  void _scheduleProjectSelectAckTimeout(String projectId) {
    _projectSelectAckTimers.remove(projectId)?.cancel();
    _projectSelectAckTimers[projectId] = Timer(const Duration(seconds: 3), () {
      _projectSelectAckTimers.remove(projectId);
      if (!mounted || !_transportReady || !_remoteProtocolReady) return;
      if (_remoteRuntime.pendingProjectSelect(includeSent: true) != projectId) {
        return;
      }
      CoduxLog.warn(
        '[codux-flutter-projects] project.select ack timeout project=$projectId',
      );
      _remoteRuntime.clearPendingProjectSelectSent(projectId);
      _drivePendingProjectSelect(reason: 'ack-timeout');
      _requestTerminalList(resetRetry: true);
    });
  }

  void _clearProjectSelectAck(String projectId) {
    if (_remoteRuntime.pendingProjectSelect(includeSent: true) != projectId) {
      return;
    }
    _projectSelectAckTimers.remove(projectId)?.cancel();
  }

  void _drivePendingProjectSelect({required String reason}) {
    final projectId = _remoteRuntime.pendingProjectSelect();
    if (projectId == null) return;
    _sendProjectSelect(projectId, reason: reason);
  }

  void _resubscribeVisibleTerminal({required String reason}) {
    if (!_terminalViewportClaimable) return;
    _terminalBindingCoordinator.resubscribeVisibleTerminal(
      transportConnected: _transportConnected,
      protocolReady: _remoteProtocolReady,
      activeSessionId: _sessionId,
      selectedProjectId: _selectedProjectId,
      capability: _terminalBufferCapability,
      reason: reason,
      ensureBoundBaseline: (sessionId, baselineRequested) {
        if (baselineRequested) {
          _trackTerminalBaselineRequest(sessionId);
        }
        _terminalBindingCoordinator.ensureBoundTerminalHasBaseline(
          sessionId: sessionId,
          baselineRequested: baselineRequested,
          reason: reason,
          capability: _terminalBufferCapability,
        );
      },
    );
  }

  void _syncRuntimeViewState() {
    _projects = _remoteRuntime.projects;
    _terminals = _remoteRuntime.terminals;
    _selectedProjectId = _remoteRuntime.selectedProjectId;
    _sessionId = _remoteRuntime.activeSessionId;
    _creatingTerminalProjectId = _remoteRuntime.creatingTerminalProjectId;
    if (_creatingTerminalProjectId == null) {
      _creatingTerminalLayoutKind = null;
    }
  }

  bool get _terminalBufferLoading =>
      _terminalBufferPhase != RemoteTerminalBufferPhase.idle;

  void _setTerminalBufferLoading(
    bool loading, {
    double? progress,
    RemoteTerminalBufferPhase phase = RemoteTerminalBufferPhase.requesting,
  }) {
    _terminalBufferPhase = loading ? phase : RemoteTerminalBufferPhase.idle;
    _terminalBufferProgress = loading ? progress : null;
  }

  String _terminalHistoryLoadingText() {
    if (_terminalBufferPhase == RemoteTerminalBufferPhase.rendering) {
      return _t('terminal.renderingHistory');
    }
    final progress = _terminalBufferProgress;
    if (progress == null) return _t('terminal.loadingHistory');
    final percent = (progress.clamp(0.0, 1.0) * 100).round();
    return _t(
      'terminal.loadingHistoryProgress',
      params: {'percent': '$percent'},
    );
  }

  void _applyRuntimePlan(RemoteRuntimePlan plan, {String reason = ''}) {
    final previousSessionId = _sessionId;
    final previousProjectId = _selectedProjectId;
    final previousWorktreeId = _selectedWorktreeId;
    if (plan.stateChanged ||
        plan.clearTerminal ||
        plan.resetTerminalBuffer ||
        plan.requestTerminalList ||
        plan.requestProjectSelectId != null ||
        plan.bindSessionId != null ||
        plan.removedSessionId != null) {
      CoduxLog.info(
        '[codux-flutter-runtime] plan reason=$reason state=${plan.stateChanged} clear=${plan.clearTerminal} resetBuffer=${plan.resetTerminalBuffer} requestTerminalList=${plan.requestTerminalList} requestProjectSelect=${plan.requestProjectSelectId ?? ''} pending=${_remoteRuntime.pendingProjectSelect(includeSent: true) ?? ''} bind=${plan.bindSessionId ?? ''} beforeProject=${previousProjectId ?? ''} beforeWorktree=${previousWorktreeId ?? ''} beforeSession=${previousSessionId ?? ''}',
      );
    }
    if (plan.removedSessionId != null) {
      final removed = plan.removedSessionId!;
      _terminalOutputController.removeSession(removed);
      _terminalRepaint.tick();
      _terminalInputSender.clear(sessionId: removed);
    }
    if (plan.resetTerminalInput) {
      _terminalInputBatcher.reset();
    }
    if (plan.resetTerminalBuffer) {
      _terminalBufferRetry.reset();
      _terminalOutputController.resetTransient();
      _setTerminalBufferLoading(false);
    }
    if (plan.stateChanged && mounted) {
      setState(_syncRuntimeViewState);
    } else {
      _syncRuntimeViewState();
    }
    if (previousSessionId != _sessionId ||
        previousProjectId != _selectedProjectId ||
        previousWorktreeId != _selectedWorktreeId) {
      CoduxLog.info(
        '[codux-flutter-runtime] state reason=$reason project=${previousProjectId ?? ''}->${_selectedProjectId ?? ''} worktree=${previousWorktreeId ?? ''}->${_selectedWorktreeId ?? ''} session=${previousSessionId ?? ''}->${_sessionId ?? ''}',
      );
    }
    if (plan.bindSessionId != null &&
        previousSessionId != null &&
        previousSessionId != plan.bindSessionId) {
      _releaseTerminalViewport(sessionId: previousSessionId);
    }
    if (plan.clearTerminal) {
      _clearTerminal();
    }
    if (plan.requestTerminalList) {
      _requestTerminalList(resetRetry: true);
    }
    if (plan.requestProjectSelectId != null) {
      _sendProjectSelect(plan.requestProjectSelectId!, reason: reason);
    }
    if (plan.bindSessionId != null && !_remoteProtocolReady) {
      CoduxLog.debug(
        '[codux-flutter-terminal] defer bind session=${plan.bindSessionId} reason=$reason protocolReady=false',
      );
      return;
    }
    if (plan.bindSessionId != null) {
      _applyTerminalBind(plan, reason);
    }
  }

  void _bindActiveTerminalAfterProtocolReady({required String reason}) {
    if (!_remoteProtocolReady) return;
    final sessionId = _sessionId;
    if (sessionId == null || sessionId.trim().isEmpty) return;
    _applyTerminalBind(RemoteRuntimePlan(bindSessionId: sessionId), reason);
  }

  void _applyTerminalBind(RemoteRuntimePlan plan, String reason) {
    if (plan.bindSessionId != null) {
      final bindSessionId = plan.bindSessionId!;
      _terminalSelectedText = null;
      final restored = _restoreTerminalSessionFromCache(bindSessionId);
      if (restored) {
        _closeTerminalSwitcherAfterPendingWorktreeBuffer(bindSessionId);
      }
      final bindResult = _terminalBindingCoordinator.bindSession(
        plan: plan,
        bindSessionId: bindSessionId,
        reason: reason,
        selectedProjectId: _selectedProjectId,
        capability: _terminalBufferCapability,
        restored: restored,
      );
      CoduxLog.info(
        '[codux-flutter-terminal] bind session=$bindSessionId project=${_selectedProjectId ?? ''} cached=${bindResult.restored}',
      );
      _focusTerminalViewSoon();
      // Bound live sessions so headless-screen worker threads from previously
      // visited projects do not accumulate across switches and stall the app.
      final evicted = _terminalOutputController.evictInactiveSessions(
        bindSessionId,
      );
      for (final sessionId in evicted) {
        _terminalInputSender.clear(sessionId: sessionId);
      }
      if (evicted.isNotEmpty) {
        CoduxLog.info(
          '[codux-flutter-terminal] evict inactive sessions=${evicted.length} keep=$bindSessionId',
        );
      }
      if (bindResult.baselineRequested) {
        _trackTerminalBaselineRequest(bindSessionId);
      }
      _terminalBindingCoordinator.ensureBoundTerminalHasBaseline(
        sessionId: bindSessionId,
        baselineRequested: bindResult.baselineRequested,
        reason: reason,
        capability: _terminalBufferCapability,
      );
      if (plan.flushTerminalInput) {
        _terminalInputBatcher.flush();
      }
    }
  }

  Future<void> _cacheProjects(List<ProjectInfo> projects) async {
    final device = _activeDevice;
    if (device == null) return;
    try {
      await _storage.saveCachedProjects(device, projects);
    } catch (error) {
      CoduxLog.warn('[codux-flutter-projects] cache save failed: $error');
    }
  }

  bool _send(
    RelayEnvelope message, {
    bool sendTerminal = false,
    RemoteSendResultHandler? onResult,
  }) {
    if (!_transportConnected) {
      setState(() => _status = _t('app.remoteNotConnected'));
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

  Future<void> _handleTransportEnvelope(
    RelayEnvelope message,
    StoredDevice target,
    int generation,
    RemoteTransport transport,
    int runtimeEpoch,
  ) async {
    try {
      final seq = message.seq;
      if (!_receiveSequenceGuard.accept(
        type: message.type,
        sessionId: message.sessionId,
        seq: seq,
      )) {
        CoduxLog.debug(
          '[codux-flutter-remote] drop duplicate seq=$seq type=${message.type} session=${message.sessionId ?? ''}',
        );
        return;
      }
      if (generation != _transportGeneration ||
          runtimeEpoch != _remoteRuntimeEpoch ||
          !identical(_activeTransport, transport)) {
        CoduxLog.debug(
          '[codux-flutter-remote] drop stale decoded envelope gen=$generation current=$_transportGeneration epoch=$runtimeEpoch currentEpoch=$_remoteRuntimeEpoch type=${message.type} session=${message.sessionId ?? ''}',
        );
        return;
      }
      _healthTimer?.cancel();
      _healthTimer = null;
      CoduxLog.debug(
        '[codux-flutter-remote] recv type=${message.type} session=${message.sessionId ?? ''}',
      );
      switch (message.type) {
        case final type when type == RemoteMessageType.hello:
          _reconnectAttempt = 0;
          CoduxLog.info('[codux-flutter-remote] hello received');
          if (!_transportConnected) {
            setState(() {
              _transportConnected = true;
              _hasShownTerminal = true;
              if (!_backgroundConnect) _status = _t('app.connecting');
            });
          }
          _sendHostInfoRequest(force: true);
          _startHostResponseProbe(reason: 'hello');
        case final type when type == RemoteMessageType.hostOffline:
          final payload = message.payload;
          final messageText = payload is Map
              ? '${payload['message'] ?? _t('connection.macDisconnected')}'
              : _t('connection.macDisconnected');
          _terminalInputBatcher.reset();
          _terminalInputSender.clear();
          _clearLatencyProbe();
          setState(() {
            _transportReady = false;
            _remoteSyncController.resetProtocolReady();
            _hostResponsive = false;
            _leaveTerminalUi();
            _resetRemoteSyncState();
            _status = messageText;
            _terminalBufferRetry.reset();
            _terminalOutputController.resetTransient();
            _setTerminalBufferLoading(false);
          });
          _clearConnectionGrace();
          _cancelHostResponseProbe();
          _scheduleReconnect(target);
        case final type when type == RemoteMessageType.transportPong:
          _handleTransportPong(message);
        case final type when type == RemoteMessageType.hostInfo:
          if (!_isCompatibleRemoteProtocol(message.payload)) {
            _failRemoteProtocol(target, message.payload);
            return;
          }
          final hostRuntimeChanged = _recordHostRuntimeInstance(
            message.payload,
          );
          _markHostResponsive(
            'host.info',
            transport: _deviceTransportKind(target),
          );
          _markActiveDeviceResponsive();
          _startLatencyProbe();
          final payload = message.payload;
          if (payload is Map) {
            _terminalBufferCapability = TerminalBufferCapability.fromHostInfo(
              payload,
            );
            if (payload['name'] != null) {
              _updateDevice(
                target.deviceId,
                hostName: payload['name']?.toString(),
              );
            }
          }
          _markRemoteProtocolReady(
            force:
                hostRuntimeChanged ||
                !_projectListLoaded ||
                !_terminalListLoaded,
          );
        case final type when type == RemoteMessageType.projectSelected:
          _handleProjectSelected(message);
        case final type when type == RemoteMessageType.projectList:
          _handleProjectList(message);
        case final type when type == RemoteMessageType.terminalList:
          _handleTerminalList(message);
        case final type when type == RemoteMessageType.terminalCreated:
          _handleTerminalCreated(message);
        case final type when type == RemoteMessageType.terminalClosed:
          _handleTerminalClosed(message);
        case final type when type == RemoteMessageType.terminalViewportState:
          _handleTerminalViewportState(message);
        case final type when type == RemoteMessageType.worktreeList:
          _handleWorktreeList(message);
        case final type when type == RemoteMessageType.worktreeUpdated:
          _handleWorktreeUpdated(message);
        case final type when type == RemoteMessageType.terminalOutput:
          _handleTerminalOutput(message);
        case final type when type == RemoteMessageType.error:
          _handleRemoteError(message);
        case final type when type == RemoteMessageType.fileList:
          _handleFileList(message);
        case final type when type == RemoteMessageType.projectUpdated:
          _refreshLists();
          _showToast(_t('project.updated'));
        case final type when type == RemoteMessageType.aiStats:
          final payload = message.payload;
          if (payload is Map<String, dynamic>) {
            setState(() {
              _currentAIStats = AIStatsInfo.fromJson(payload);
              _aiStatsLoading = false;
              _workspaceMode = 'stats';
            });
          }
        case final type when type == RemoteMessageType.gitStatus:
          final status = remoteGitStatusFromPayload(message.payload);
          if (status != null) {
            final plan = _remoteRuntime.applyGitStatus(status);
            _applyRuntimePlan(plan, reason: 'git-status');
          }
        case final type when type == RemoteMessageType.fileRead:
          _handleFileRead(message);
        case final type when type == RemoteMessageType.fileWritten:
          setState(() => _fileEditorSaving = false);
          _showToast(_t('file.saved'));
        case final type when type == RemoteMessageType.fileRenamed:
          _requestProjectFiles(_projectFilesPath);
          _showToast(_t('file.renamed'));
        case final type when type == RemoteMessageType.fileDeleted:
          _handleFileDeleted(message);
          _requestProjectFiles(_projectFilesPath);
          _showToast(_t('file.deleted'));
        case final type when type == RemoteMessageType.terminalUploaded:
          _handleTerminalUploaded(message);
        case final type when type == RemoteMessageType.terminalInputAck:
          _terminalInputSender.handleAck(message);
      }
    } catch (error) {
      CoduxLog.error('[codux-flutter-remote] receive failed: $error');
    }
  }

  void _handleProjectSelected(RelayEnvelope message) {
    _markHostResponsive('project.selected');
    _markActiveDeviceResponsive();
    final payload = message.payload;
    final projectId = payload is Map ? payload['projectId']?.toString() : null;
    final worktreeId = payload is Map
        ? payload['worktreeId']?.toString()
        : null;
    CoduxLog.info(
      '[codux-flutter-projects] project.selected project=${projectId ?? ''} worktree=${worktreeId ?? ''} current=${_selectedProjectId ?? ''}',
    );
    if (projectId != null && projectId.isNotEmpty) {
      _clearProjectSelectAck(projectId);
    }
    final plan = _remoteRuntime.projectSelected(
      projectId: projectId,
      worktreeId: worktreeId,
    );
    _applyRuntimePlan(plan, reason: 'project-selected');
  }

  void _handleProjectList(RelayEnvelope message) {
    _markHostResponsive('project.list');
    _markActiveDeviceResponsive();
    _markProjectListReceived();
    final payload = message.payload;
    final next = remoteProjectsFromPayload(payload);
    final worktrees = remoteWorktreesFromPayload(payload);
    final remoteSelectedProjectId = remoteSelectedProjectIdFromPayload(payload);
    final remoteSelectedWorktreeId = remoteSelectedWorktreeIdFromPayload(
      payload,
    );
    CoduxLog.info(
      '[codux-flutter-projects] recv project.list count=${next.length} remoteSelected=${remoteSelectedProjectId ?? ''} remoteWorktree=${remoteSelectedWorktreeId ?? ''} current=${_selectedProjectId ?? ''}',
    );
    final plan = _remoteRuntime.applyProjectList(
      projects: next,
      remoteSelectedProjectId: remoteSelectedProjectId,
      remoteSelectedWorktreeId: remoteSelectedWorktreeId,
      terminalVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
    );
    _applyRuntimePlan(plan, reason: 'missing-terminal');
    if (worktrees.isNotEmpty) {
      final worktreePlan = _remoteRuntime.applyWorktreeState(
        worktrees: worktrees,
        projectId: null,
        selectedWorktreeId: remoteSelectedWorktreeId,
        baseBranches: const [],
        defaultBaseBranch: null,
        allowRuntimeSelection: false,
        terminalVisible: _terminalDataVisible,
        terminalListLoaded: _terminalListLoaded,
      );
      if (mounted) setState(_syncRuntimeViewState);
      if (worktreePlan.hasRuntimeAction) {
        _applyRuntimePlan(worktreePlan, reason: 'project-list-worktrees');
      }
    }
    CoduxLog.debug(
      '[codux-flutter-projects] project.list count=${next.length} selected=${_selectedProjectId ?? ''}',
    );
    unawaited(_cacheProjects(next));
  }

  void _handleTerminalList(RelayEnvelope message) {
    _markHostResponsive('terminal.list');
    _markActiveDeviceResponsive();
    _markTerminalListReceived();
    final next = remoteTerminalsFromPayload(message.payload);
    CoduxLog.debug(
      '[codux-flutter-terminal] recv terminal.list count=${next.length} selected=${_selectedProjectId ?? ''} worktree=${_selectedWorktreeId ?? ''} active=${_sessionId ?? ''} projects=${next.map((item) => item.projectId).toSet().join(',')}',
    );
    CoduxLog.debug(
      '[codux-flutter-terminal] terminal.list items=${next.map((item) => '${item.projectId}/${item.worktreeId ?? '-'}:${item.id}:${item.layoutKind}:${item.layoutOrder ?? -1}').join('|')}',
    );
    final plan = _remoteRuntime.applyTerminalList(
      terminals: next,
      terminalVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
    );
    if (_creatingTerminalProjectId != null) {
      _creatingTerminalLayoutKind = null;
    }
    if (plan.bindSessionId != null && _selectedProjectId != null) {
      _clearProjectSelectAck(_selectedProjectId!);
    }
    _applyRuntimePlan(plan, reason: 'missing-terminal');
  }

  void _handleTerminalCreated(RelayEnvelope message) {
    final terminal = remoteTerminalFromPayload(message.payload);
    if (terminal == null) return;
    CoduxLog.info(
      '[codux-flutter-terminal] created session=${terminal.id} project=${terminal.projectId} worktree=${terminal.worktreeId ?? ''} kind=${terminal.layoutKind} order=${terminal.layoutOrder ?? -1}',
    );
    final plan = _remoteRuntime.terminalCreated(terminal);
    if (_creatingTerminalProjectId != null) {
      _creatingTerminalLayoutKind = null;
    }
    _applyRuntimePlan(plan, reason: 'terminal-created');
  }

  void _handleTerminalClosed(RelayEnvelope message) {
    final closedSessionId = message.sessionId;
    if (closedSessionId == null) return;
    final plan = _remoteRuntime.removeTerminal(closedSessionId);
    _applyRuntimePlan(plan, reason: 'terminal-closed');
  }

  void _handleWorktreeList(RelayEnvelope message) {
    _markHostResponsive('worktree.list');
    _applyWorktreeState(message, allowRuntimeSelection: false);
  }

  void _handleWorktreeUpdated(RelayEnvelope message) {
    _applyWorktreeState(message, allowRuntimeSelection: true);
  }

  void _applyWorktreeState(
    RelayEnvelope message, {
    required bool allowRuntimeSelection,
  }) {
    final worktreeState = _worktreeController.stateFromPayload(message.payload);
    if (worktreeState == null) return;
    final scopedProjectId = worktreeState.projectId;
    final currentProjectId =
        _selectedProjectId ?? _remoteRuntime.selectedProjectId;
    final effectiveProjectId = scopedProjectId ?? currentProjectId;
    final affectsCurrentProject =
        effectiveProjectId == null ||
        currentProjectId == null ||
        effectiveProjectId == currentProjectId;
    final canApplyRuntimeSelection =
        allowRuntimeSelection && affectsCurrentProject;
    final scopedWorktrees = effectiveProjectId == null
        ? worktreeState.worktrees
        : worktreeState.worktrees
              .where((worktree) => worktree.projectId == effectiveProjectId)
              .toList(growable: false);
    final confirmedWorktreeId = worktreeState.selectedWorktreeId;
    final pendingSwitch = _pendingWorktreeSwitch;
    final pendingWorktreeId = pendingSwitch?.worktreeId;
    final pendingCurrentProject =
        pendingSwitch != null &&
        canApplyRuntimeSelection &&
        (effectiveProjectId == null ||
            effectiveProjectId == pendingSwitch.projectId);
    if (allowRuntimeSelection && pendingWorktreeId != null) {
      CoduxLog.info(
        '[codux-flutter-worktree] apply type=${message.type} project=${effectiveProjectId ?? ''} current=${currentProjectId ?? ''} confirmed=${confirmedWorktreeId ?? ''} pendingProject=${pendingSwitch?.projectId ?? ''} pendingWorktree=$pendingWorktreeId currentProject=$pendingCurrentProject worktrees=${scopedWorktrees.map((item) => '${item.projectId}:${item.id}').join('|')}',
      );
    }
    final plan = _remoteRuntime.applyWorktreeState(
      worktrees: scopedWorktrees,
      projectId: effectiveProjectId,
      selectedWorktreeId: worktreeState.selectedWorktreeId,
      baseBranches: worktreeState.baseBranches,
      defaultBaseBranch: worktreeState.defaultBaseBranch,
      allowRuntimeSelection: canApplyRuntimeSelection,
      terminalVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
    );
    setState(() {
      _syncRuntimeViewState();
      _worktreeListLoading =
          pendingCurrentProject && !_pendingWorktreeSwitchHasActiveTerminal();
      _creatingWorktree = false;
    });
    if (plan.hasRuntimeAction) {
      _applyRuntimePlan(plan, reason: 'worktree-updated');
    }
    _closeTerminalSwitcherIfPendingWorktreeReady();
  }

  void _handleRemoteError(RelayEnvelope message) {
    final payload = message.payload;
    final code = payload is Map ? '${payload['code'] ?? ''}' : '';
    if (code == 'device_unauthorized') {
      _requireRepairPairing(payload);
      return;
    }
    final errorMessage =
        message.error ??
        (payload is Map
            ? '${payload['message'] ?? _t('remote.error')}'
            : _t('remote.error'));
    CoduxLog.warn(
      '[codux-flutter-remote] error type=${message.type} session=${message.sessionId ?? ''} message=$errorMessage',
    );
    final isActiveTerminalError =
        message.sessionId != null && message.sessionId == _sessionId;
    if (isActiveTerminalError) {
      _terminalBufferRetry.reset();
    }
    setState(() {
      _aiStatsLoading = false;
      _filePickerLoading = false;
      _worktreeListLoading = false;
      _creatingWorktree = false;
      _pendingWorktreeSwitch = null;
      _creatingTerminalLayoutKind = null;
      _blockingLoadingMessage = null;
      if (isActiveTerminalError) {
        _terminalOutputController.resetSessionTransient(message.sessionId!);
        _setTerminalBufferLoading(false);
      }
      _status = errorMessage;
    });
  }

  void _handleFileList(RelayEnvelope message) {
    final listState = _projectFileController.listStateFromPayload(
      message.payload,
    );
    if (listState != null) {
      _applyFileListState(listState);
    }
  }

  void _handleFileRead(RelayEnvelope message) {
    final fileState = _projectFileController.readStateFromPayload(
      message.payload,
    );
    if (fileState == null) return;
    setState(() {
      _applyFileEditorState(fileState);
    });
    if (!fileState.editable) {
      _showToast(_t('file.readOnlyLarge'));
    }
  }

  void _handleFileDeleted(RelayEnvelope message) {
    final deletedPath = _projectFileController.deletedPathFromPayload(
      message.payload,
    );
    if (_projectFileController.shouldCloseEditorAfterDelete(
      deletedPath: deletedPath,
      editingPath: _editingFilePath,
    )) {
      setState(() => _editingFilePath = null);
    }
  }

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
        setState(() => _latencyMs = nextLatency);
      }
      return;
    }
    if (timeoutValue != null || detail == 'lost') {
      CoduxLog.warn(
        '[codux-flutter-remote] latency ${detail.isEmpty ? 'timeout' : detail}',
      );
      if (detail == 'lost') {
        if (_latencyMs != null) setState(() => _latencyMs = null);
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
    setState(() {
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

  void _handleTerminalViewportState(RelayEnvelope message) {
    _terminalViewportController.applyRemoteState(message);
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
          if (mounted) {
            setState(
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
      setState(() {
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
      setState(() {
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

  String _nextTerminalBufferRequestId(String sessionId) {
    _terminalBufferRequestCounter += 1;
    return '${DateTime.now().microsecondsSinceEpoch}-$_terminalBufferRequestCounter-$sessionId';
  }

  void _markTerminalBufferReceived(String? sessionId) {
    _terminalBufferRetry.markReceived(
      sessionId: sessionId,
      activeSessionId: _sessionId,
    );
    if (_terminalBufferLoading && mounted) {
      setState(() => _setTerminalBufferLoading(false));
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
    if (mounted) setState(() {});
  }

  void _sendTerminalResize(int cols, int rows, {String? sessionId}) {
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

  void _claimTerminalViewport({String? sessionId}) {
    final id = sessionId ?? _sessionId;
    if (id == null || id.trim().isEmpty) return;
    if (!_terminalViewportClaimable) return;
    final terminal = _terminalById(id);
    if (terminal == null || !_canResizeTerminal(terminal)) return;
    _terminalViewportInteractive = true;
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
      setState(() => _status = _t('terminal.createOrSelectFirst'));
      return;
    }
    _claimTerminalViewport(sessionId: id);
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
      setState(() => _status = _t('project.noAvailable'));
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
    setState(_syncRuntimeViewState);
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

  void _selectTerminal(TerminalInfo terminal) {
    if (!_isAccessibleTerminal(terminal)) return;
    _terminalInputBatcher.flush();
    setState(() => _workspaceMode = 'terminal');
    final plan = _remoteRuntime.selectTerminal(terminal);
    _applyRuntimePlan(plan, reason: 'select-terminal');
    _focusTerminalViewSoon();
  }

  void _createCurrentProjectTerminal() {
    final projectId = _selectedProjectId;
    if (projectId == null) {
      _showToast(_t('project.selectFirst'));
      return;
    }
    setState(() => _workspaceMode = 'terminal');
    _createTerminal(projectId);
  }

  void _createCurrentProjectTabTerminal() {
    final projectId = _selectedProjectId;
    if (projectId == null) {
      _showToast(_t('project.selectFirst'));
      return;
    }
    setState(() => _workspaceMode = 'terminal');
    _createTerminal(projectId, 'tab');
  }

  void _closeCurrentTerminal() {
    final terminal = _currentTerminal();
    if (terminal == null || !_isAccessibleTerminal(terminal)) return;
    _closeTerminal(terminal);
  }

  void _closeTerminal(TerminalInfo terminal) {
    if (!_isAccessibleTerminal(terminal)) return;
    final scopedTerminals = _currentProjectTerminals();
    if (scopedTerminals.length <= 1 &&
        scopedTerminals.any((item) => item.id == terminal.id)) {
      _showToast(_t('terminal.keepOne'));
      return;
    }
    final plan = _remoteRuntime.removeTerminal(terminal.id);
    _applyRuntimePlan(plan, reason: 'close-terminal');
    _sendTerminalEnvelope(
      RelayEnvelope(
        type: RemoteMessageType.terminalClose,
        sessionId: terminal.id,
      ),
      terminal: terminal,
    );
  }

  Future<void> _openTerminalSwitcher() async {
    if (_showTerminalSwitcher) return;
    if (_workspaceMode == 'terminal') {
      _releaseTerminalViewport();
    }
    _ensureSelectedProjectWorktrees(loading: true);
    await _pushCupertinoPage(() {
      _showTerminalSwitcher = true;
      _terminalReady = false;
    });
  }

  void _closeTerminalSwitcher() {
    _pendingWorktreeSwitch = null;
    _popCupertinoPage(() {
      _showTerminalSwitcher = false;
    }).then((_) {
      if (mounted) _mountVisibleTerminal(reason: 'switcher-close');
    });
  }

  void _selectTerminalFromSwitcher(TerminalInfo terminal) {
    _selectTerminal(terminal);
    _closeTerminalSwitcher();
  }

  void _selectWorktree(RemoteWorktreeInfo worktree) {
    final project = _selectedProject;
    if (project == null) {
      _showToast(_t('project.selectFirst'));
      return;
    }
    if (worktree.projectId != project.id) {
      CoduxLog.warn(
        '[codux-flutter-worktree] ignore select project=${project.id} worktree=${worktree.id} worktreeProject=${worktree.projectId}',
      );
      return;
    }
    if (worktree.id == _selectedWorktreeId) {
      _closeTerminalSwitcher();
      return;
    }
    _terminalInputBatcher.flush();
    _pendingWorktreeSwitch = _PendingWorktreeSwitch(
      projectId: project.id,
      worktreeId: worktree.id,
    );
    final plan = _remoteRuntime.worktreeSelected(
      projectId: project.id,
      worktreeId: worktree.id,
      terminalVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
    );
    setState(() {
      _workspaceMode = 'terminal';
      _syncRuntimeViewState();
      _worktreeListLoading = true;
    });
    final sent = _send(_worktreeController.selectEnvelope(project, worktree));
    if (!sent) {
      _pendingWorktreeSwitch = null;
      setState(() => _worktreeListLoading = false);
      return;
    }
    _applyRuntimePlan(plan, reason: 'worktree-local-select');
  }

  Future<void> _createWorktree() async {
    final project = _selectedProject;
    if (project == null || project.path == null || project.path!.isEmpty) {
      _showToast(_t('project.selectPathFirst'));
      return;
    }
    final branchOptions = _worktreeCreatorBranchOptions();
    final request = await showDialog<WorktreeCreateDraft>(
      context: context,
      builder: (ctx) => WorktreeCreateDialog(
        title: _t('worktree.new'),
        baseBranchLabel: _t('worktree.baseBranch'),
        nameLabel: _t('worktree.name'),
        cancelLabel: _t('app.cancel'),
        createLabel: _t('common.create'),
        branchOptions: branchOptions,
        initialBaseBranch: _worktreeCreatorDefaultBaseBranch(branchOptions),
        initialName: defaultWorktreeName(),
      ),
    );
    if (request == null) return;
    if (request.baseBranch.isEmpty) {
      _showToast(_t('worktree.baseBranchRequired'));
      return;
    }
    if (request.name.isEmpty) {
      _showToast(_t('worktree.nameRequired'));
      return;
    }
    setState(() {
      _worktreeListLoading = true;
      _creatingWorktree = true;
    });
    _send(
      _worktreeController.createEnvelope(
        project: project,
        baseBranch: request.baseBranch,
        name: request.name,
      ),
    );
  }

  List<String> _worktreeCreatorBranchOptions() {
    final projectId = _selectedProjectId;
    return worktreeBranchOptions(
      defaultBaseBranch: _defaultWorktreeBaseBranch,
      baseBranches: _worktreeBaseBranches,
      worktrees: projectId == null ? const [] : _worktreesForProject(projectId),
    );
  }

  String _worktreeCreatorDefaultBaseBranch(List<String> options) {
    return defaultWorktreeBaseBranch(
      preferred: _defaultWorktreeBaseBranch,
      options: options,
    );
  }

  Future<void> _mergeWorktree(RemoteWorktreeInfo worktree) async {
    final confirmed = await _confirmWorktreeAction(
      title: _t('worktree.merge'),
      message: _t(
        'worktree.mergeConfirm',
        params: {'name': _worktreeTitle(worktree)},
      ),
      destructive: false,
    );
    if (!confirmed) return;
    _sendWorktreeOperation(RemoteMessageType.worktreeMerge, worktree);
  }

  Future<void> _deleteWorktree(RemoteWorktreeInfo worktree) async {
    final confirmed = await _confirmWorktreeAction(
      title: _t('worktree.delete'),
      message: _t(
        'worktree.deleteConfirm',
        params: {'name': _worktreeTitle(worktree)},
      ),
      destructive: true,
    );
    if (!confirmed) return;
    _sendWorktreeOperation(RemoteMessageType.worktreeDelete, worktree);
  }

  void _sendWorktreeOperation(String type, RemoteWorktreeInfo worktree) {
    final project = _selectedProject;
    if (project == null || project.path == null || project.path!.isEmpty) {
      _showToast(_t('project.selectPathFirst'));
      return;
    }
    setState(() => _worktreeListLoading = true);
    final envelope = type == RemoteMessageType.worktreeDelete
        ? _worktreeController.deleteEnvelope(project, worktree)
        : _worktreeController.mergeEnvelope(project, worktree);
    _send(envelope);
  }

  Future<bool> _confirmWorktreeAction({
    required String title,
    required String message,
    required bool destructive,
  }) async {
    return await showDialog<bool>(
          context: context,
          builder: (ctx) => WorktreeActionDialog(
            title: title,
            message: message,
            cancelLabel: _t('app.cancel'),
            destructive: destructive,
          ),
        ) ??
        false;
  }

  String _worktreeTitle(RemoteWorktreeInfo worktree) {
    return worktreeTitle(worktree);
  }

  Future<void> _refreshDeviceList() async {
    final device = _activeDevice;
    if (device == null) return;
    if (!_isConnected) {
      _connect(device);
      await Future<void>.delayed(const Duration(milliseconds: 350));
      return;
    }
    _refreshTransportRoute(reason: 'manual-refresh');
    _sendHostInfoRequest(force: true);
    _requestProjectList(resetRetry: true);
    _requestTerminalList(resetRetry: true);
    await Future<void>.delayed(const Duration(milliseconds: 350));
  }

  void _refreshLists() {
    _refreshTransportRoute(reason: 'manual-refresh');
    _sendHostInfoRequest(force: true);
    _requestProjectList(resetRetry: true);
    _requestTerminalList(resetRetry: true);
    _requestGitStatus();
  }

  void _rebuildCurrentTerminal() {
    final projectId = _selectedProjectId;
    if (projectId == null) {
      _showToast(_t('project.selectFirst'));
      return;
    }
    String? closingSessionId;
    TerminalInfo? closingTerminal;
    final current = _currentTerminal();
    final projectTerminals = _terminals
        .where(
          (terminal) =>
              terminal.projectId == projectId &&
              _isAccessibleTerminal(terminal),
        )
        .toList();
    if (current != null &&
        current.projectId == projectId &&
        _isAccessibleTerminal(current)) {
      closingSessionId = current.id;
      closingTerminal = current;
    } else if (projectTerminals.isNotEmpty) {
      closingTerminal = projectTerminals.first;
      closingSessionId = closingTerminal.id;
    }
    final shouldCreateReplacement = projectTerminals.length > 1;
    final canCloseCurrent = projectTerminals.length > 1;
    if (closingSessionId != null && canCloseCurrent) {
      final plan = _remoteRuntime.removeTerminal(closingSessionId);
      _applyRuntimePlan(plan, reason: 'rebuild-terminal');
      _sendTerminalEnvelope(
        RelayEnvelope(
          type: RemoteMessageType.terminalClose,
          sessionId: closingSessionId,
        ),
        terminal: closingTerminal,
      );
    } else {
      _clearTerminal();
    }
    if (shouldCreateReplacement) {
      _createTerminal(projectId);
    }
    _showToast(_t('terminal.rebuilding'));
  }

  void _ensureTerminalForSelectedProject() {
    final plan = _remoteRuntime.ensureTerminalForSelectedProject(
      terminalVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
    );
    _applyRuntimePlan(plan, reason: 'missing-terminal');
  }

  void _requestProjectEdit() {
    final project = _selectedProject;
    if (project == null) {
      _showSnack(_t('project.selectFirst'));
      return;
    }
    final draft = _projectController.editDraft(project);
    setState(() {
      _applyProjectFormDraft(draft);
      _showProjectForm = true;
    });
  }

  void _requestProjectAdd() {
    final draft = _projectController.addDraft();
    setState(() {
      _applyProjectFormDraft(draft);
      _showProjectForm = true;
    });
  }

  void _chooseProjectFormPath() {
    _filePickerMode = 'projectForm';
    final current = _projectPathController.text.trim();
    _openRemoteFilePicker(current.isEmpty ? null : current);
  }

  void _saveProjectForm() {
    final plan = _projectController.savePlan(
      mode: _projectFormMode,
      path: _projectPathController.text,
      name: _projectNameController.text,
      selectedProject: _selectedProject,
    );
    if (!plan.valid) {
      _showToast(_t('project.selectPathFirst'));
      return;
    }
    _send(plan.envelope!);
    setState(() => _showProjectForm = false);
    _showToast(_t('project.saveSubmitted'));
  }

  void _openRemoteFilePicker([String? path]) {
    _filePickerTimeoutTimer?.cancel();
    setState(() {
      _showFilePicker = true;
      _filePickerLoading = true;
      _filePickerPath = path ?? _filePickerPath;
    });
    _filePickerTimeoutTimer = Timer(const Duration(seconds: 8), () {
      if (!mounted || !_filePickerLoading) return;
      setState(() => _filePickerLoading = false);
      _showToast(_t('remote.dirTimeout'));
    });
    _send(_projectController.filePickerListEnvelope(path));
  }

  void _selectRemoteProjectFolder(RemoteFileEntry entry) {
    if (_filePickerMode == 'projectForm') {
      final selection = _projectController.selectFolder(
        entry: entry,
        currentName: _projectNameController.text,
      );
      setState(() {
        _projectPathController.text = selection.path;
        _projectNameController.text = selection.name;
        _showFilePicker = false;
      });
      return;
    }
    setState(() => _showFilePicker = false);
  }

  void _applyProjectFormDraft(ProjectFormDraft draft) {
    _projectFormMode = draft.mode;
    _projectNameController.text = draft.name;
    _projectPathController.text = draft.path;
  }

  void _requestProjectRemove() {
    final project = _selectedProject;
    if (project == null) {
      _showSnack(_t('project.selectFirst'));
      return;
    }
    _send(_projectController.removeEnvelope(project));
    _showToast(_t('project.removeRequested'));
  }

  void _requestAIStats() {
    final project = _selectedProject;
    if (project == null) {
      _showToast(_t('project.selectFirst'));
      return;
    }
    if (_workspaceMode == 'terminal') {
      _releaseTerminalViewport();
    }
    setState(() {
      _workspaceMode = 'stats';
      _aiStatsLoading = true;
    });
    _send(_projectController.aiStatsEnvelope(project));
  }

  void _requestGitStatus() {
    final project = _selectedProject;
    if (!_remoteProtocolReady || project == null) return;
    _send(_projectController.gitStatusEnvelope(project));
  }

  void _syncTerminalToSelectedProject({bool requestListIfMissing = true}) {
    if (!_terminalDataVisible) return;
    final plan = _remoteRuntime.ensureTerminalForSelectedProject(
      terminalVisible: _terminalDataVisible,
      terminalListLoaded: requestListIfMissing && _terminalListLoaded,
    );
    _applyRuntimePlan(plan, reason: 'missing-terminal');
  }

  void _showTerminalMode() {
    setState(() {
      _workspaceMode = 'terminal';
      _terminalReady = false;
    });
    _syncTerminalToSelectedProject();
    _mountVisibleTerminal(reason: 'mode');
    _requestGitStatus();
    _focusTerminalViewSoon();
  }

  void _showFilesMode() {
    final project = _selectedProject;
    if (project == null) {
      _showToast(_t('project.selectFirst'));
      return;
    }
    final targetPath = _projectFileController.pathForProject(
      project,
      currentPath: _projectFilesPath,
    );
    if (_workspaceMode == 'terminal') {
      _releaseTerminalViewport();
    }
    setState(() {
      _workspaceMode = 'files';
    });
    _requestGitStatus();
    _requestProjectFiles(targetPath);
  }

  void _requestProjectFiles([String? path]) {
    final project = _selectedProject;
    final target = path ?? project?.path;
    if (target == null || target.isEmpty) {
      _showToast(_t('project.currentNoDir'));
      return;
    }
    setState(() {
      _projectFilesLoading = true;
      _projectFilesPath = target;
      if (project != null) {
        _projectFileController.remember(projectId: project.id, path: target);
      }
    });
    _send(_projectFileController.listEnvelope(target));
  }

  Future<void> _copyProjectFilePath(RemoteFileEntry entry) async {
    final message = AppPreferences.of(context).t('file.pathCopied');
    await Clipboard.setData(ClipboardData(text: entry.path));
    _showToast(message);
  }

  Future<void> _renameProjectFile(RemoteFileEntry entry) async {
    final prefs = AppPreferences.of(context);
    final nextName = await showDialog<String>(
      context: context,
      builder: (ctx) => FileRenameDialog(
        title: prefs.t('file.renameTitle'),
        label: prefs.t('file.renameLabel'),
        cancelLabel: prefs.t('file.cancel'),
        saveLabel: prefs.t('file.save'),
        initialName: entry.name,
      ),
    );
    if (nextName == null) return;
    final plan = _projectFileController.renamePlan(entry, nextName);
    if (plan == null) return;
    if (!plan.valid) {
      _showToast(prefs.t('file.nameInvalid'));
      return;
    }
    _send(plan.envelope!);
  }

  Future<void> _deleteProjectFile(RemoteFileEntry entry) async {
    final prefs = AppPreferences.of(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => FileDeleteDialog(
        title: prefs.t('file.deleteTitle'),
        message: prefs.t('file.deleteConfirm', params: {'name': entry.name}),
        cancelLabel: prefs.t('file.cancel'),
        deleteLabel: prefs.t('file.menuDelete'),
      ),
    );
    if (confirmed != true) return;
    _send(_projectFileController.deleteEnvelope(entry));
  }

  void _openFileLocation(String path) {
    if (_showFilePicker) {
      _openRemoteFilePicker(path);
      return;
    }
    _requestProjectFiles(path);
  }

  void _requestFileRead(RemoteFileEntry entry) {
    if (entry.isDirectory) return;
    final fileState = _projectFileController.beginReadState(entry);
    setState(() {
      _applyFileEditorState(fileState);
    });
    _send(_projectFileController.readEnvelope(entry));
  }

  void _applyFileListState(RemoteFileListState state) {
    if (state.isProjectFiles) {
      setState(() {
        _projectFilesPath = state.path;
        _projectFilesParent = state.parent;
        _projectFileEntries = state.entries;
        _projectFilesLoading = false;
        final projectId = _selectedProjectId;
        if (projectId != null && state.path.isNotEmpty) {
          _projectFileController.remember(
            projectId: projectId,
            path: state.path,
          );
        }
      });
      return;
    }
    setState(() {
      _filePickerPath = state.path;
      _filePickerParent = state.parent;
      _filePickerEntries = state.entries;
      _filePickerLoading = false;
      _filePickerTimeoutTimer?.cancel();
      _showFilePicker = true;
    });
  }

  void _applyFileEditorState(RemoteFileEditorState state) {
    _editingFilePath = state.path;
    _fileEditorController.text = state.content;
    _fileEditorController.highlightEnabled = state.highlightEnabled;
    _fileEditorLoading = state.loading;
    _fileEditorSaving = state.saving;
    _fileEditorEditing = state.editing;
    _fileEditorEditable = state.editable;
  }

  void _saveEditingFile() {
    final path = _editingFilePath;
    if (path == null || _fileEditorSaving || !_fileEditorEditing) return;
    setState(() => _fileEditorSaving = true);
    _send(
      _projectFileController.writeEnvelope(
        path: path,
        content: _fileEditorController.text,
      ),
    );
  }

  void _focusTerminalSoon() {
    Future<void>.delayed(const Duration(milliseconds: 80), () {
      if (!mounted) return;
      setState(() {
        _keyboardRequested = true;
        _keyboardRequestSerial += 1;
        _keyboardShownSinceRequest = false;
      });
    });
  }

  void _toggleTerminalKeyboard() {
    if (_keyboardRequested || _keyboardVisible) {
      setState(() {
        _keyboardRequested = false;
        _keyboardRequestSerial += 1;
        _keyboardShownSinceRequest = false;
      });
      return;
    }
    _focusTerminalSoon();
  }

  void _focusTerminalViewSoon() {
    Future<void>.delayed(const Duration(milliseconds: 80), () {
      if (!mounted) return;
      if (_workspaceMode != 'terminal' || !_hasShownTerminal) return;
      _claimTerminalViewport();
      _flushPendingTerminalResize(force: true);
    });
  }

  Future<void> _removeDevice(StoredDevice device) async {
    final result = _deviceController.remove(
      devices: _devices,
      activeDevice: _activeDevice,
      device: device,
    );
    if (result.removedActive) {
      _shouldReconnect = false;
      _transportConnected = false;
      unawaited(_closeActiveTransport());
      _clearLatencyProbe();
    }
    await _saveDevices(result.state.devices);
    if (result.state.devices.isEmpty) {
      setState(() => _showTerminal = false);
    }
  }

  void _openDeviceTerminal(StoredDevice device) {
    if (device.deviceId != _activeDevice?.deviceId || !_isConnected) return;
    unawaited(
      _pushCupertinoPage(() {
        _showTerminal = true;
        _workspaceMode = 'terminal';
        _terminalReady = false;
        _setTerminalBufferLoading(false);
      }).then((_) {
        if (!mounted) return;
        _ensureTerminalForSelectedProject();
      }),
    );
    if (!_projectListLoaded) {
      _requestProjectList(resetRetry: true);
    }
    if (!_terminalListLoaded) {
      _requestTerminalList(resetRetry: true);
    }
    _ensureTerminalForSelectedProject();
    _mountVisibleTerminal(reason: 'open');
    _focusTerminalViewSoon();
  }

  Future<void> _editDevice(StoredDevice device) async {
    final next = await showDialog<StoredDevice>(
      context: context,
      builder: (ctx) => DeviceEditDialog(
        device: device,
        title: _t('device.editTitle'),
        nameLabel: _t('device.nameLabel'),
        cancelLabel: _t('app.cancel'),
        saveLabel: _t('common.save'),
      ),
    );
    if (next == null) return;
    final nextState = _deviceController.replace(
      devices: _devices,
      device: next,
      activeDevice: _activeDevice,
    );
    await _saveDevices(nextState.devices);
    if (_activeDevice?.deviceId == next.deviceId) {
      _connect(next, true);
    }
  }

  void _onProjectSelected(ProjectInfo project) {
    final projectChanged = _selectedProjectId != project.id;
    final resetTerminal = projectChanged && _workspaceMode == 'terminal';
    if (resetTerminal) {
      _releaseTerminalViewport();
    }
    CoduxLog.info(
      '[codux-flutter-projects] user select project=${project.id} previous=${_selectedProjectId ?? ''} changed=$projectChanged mode=$_workspaceMode terminalVisible=$resetTerminal currentSession=${_sessionId ?? ''}',
    );
    setState(() {
      _currentAIStats = null;
      _projectFileEntries = [];
      _projectFilesPath = project.path ?? '';
      _projectFilesParent = null;
      if (projectChanged) {
        _projectFileController.forget(project.id);
        _pendingWorktreeSwitch = null;
      }
    });
    final plan = _remoteRuntime.userSelectProject(
      project: project,
      terminalVisible: resetTerminal,
    );
    _applyRuntimePlan(plan, reason: 'user-select');
    _ensureSelectedProjectWorktrees(loading: _showTerminalSwitcher);
    if (_workspaceMode == 'stats') {
      _requestAIStats();
      return;
    }
    if (_workspaceMode == 'files') {
      _requestProjectFiles(project.path);
      return;
    }
    if (resetTerminal) {
      return;
    }
    final current = _terminals.any(
      (item) =>
          item.id == _sessionId &&
          item.projectId == project.id &&
          _isAccessibleTerminal(item),
    );
    if (!current) {
      _ensureTerminalForSelectedProject();
    }
  }

  Future<void> _pasteToTerminal() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    if (data?.text?.isNotEmpty == true) {
      _insertTerminalText(data!.text!);
    }
  }

  Future<void> _copyTerminalSelection() async {
    final prefs = AppPreferences.of(context);
    final text = _terminalSelectedText?.trim().isNotEmpty == true
        ? _terminalSelectedText!
        : _visibleTerminalText();
    final copied = text.trim().isNotEmpty;
    if (copied) {
      await Clipboard.setData(ClipboardData(text: text));
    }
    _showSnack(
      copied ? prefs.t('toolbar.copyDone') : prefs.t('toolbar.copyEmpty'),
    );
  }

  String _visibleTerminalText() {
    final sessionId = _sessionId;
    if (sessionId == null) return '';
    return _terminalOutputController.cachedOutput(sessionId)?.trimRight() ?? '';
  }

  Future<void> _startVoiceInput() async {
    if (_showVoiceOverlay) return;
    setState(() => _showVoiceOverlay = true);
  }

  Future<void> _chooseUploadForTerminal() async {
    if (_terminalUploadLoading) return;
    CoduxLog.info(
      '[codux-flutter-upload] choose start connected=$_isConnected path=$_connectionPath session=$_sessionId',
    );
    if (!_canUploadOverCurrentPath) {
      _showSnack(_t('upload.directRequired'));
      setState(() => _status = _t('upload.directRequired'));
      return;
    }
    final prefs = AppPreferences.of(context);
    final source = await showModalBottomSheet<TerminalUploadSource>(
      context: context,
      backgroundColor: AppColors.bgElevated,
      barrierColor: AppColors.backdrop,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(AppRadius.lg)),
      ),
      builder: (context) => TerminalUploadSourceSheet(
        fileLabel: prefs.t('upload.chooseFile'),
        imageLabel: prefs.t('upload.chooseImage'),
      ),
    );
    CoduxLog.info('[codux-flutter-upload] source selected source=$source');
    if (source == null || !mounted) return;
    await _uploadPickedFileToTerminal(source);
  }

  Future<void> _uploadPickedFileToTerminal(TerminalUploadSource source) async {
    if (_terminalUploadLoading) return;
    if (!_canUploadOverCurrentPath) {
      _showSnack(_t('upload.directRequired'));
      setState(() => _status = _t('upload.directRequired'));
      return;
    }
    final id = _sessionId;
    if (id == null) {
      setState(() => _status = _t('terminal.createOrSelectFirst'));
      return;
    }
    final result = await FilePicker.pickFiles(
      type: source == TerminalUploadSource.image
          ? FileType.image
          : FileType.any,
      allowMultiple: false,
      withData: true,
    );
    final files = result?.files;
    final picked = files == null || files.isEmpty ? null : files.single;
    CoduxLog.info(
      '[codux-flutter-upload] picker result selected=${picked != null} source=$source',
    );
    if (picked == null) return;
    if (picked.size > 20 * 1024 * 1024) {
      _showSnack(_t('upload.fileTooLarge'));
      return;
    }
    final bytes =
        picked.bytes ??
        (picked.path == null ? null : await File(picked.path!).readAsBytes());
    if (bytes == null) {
      _showSnack(_t('upload.fileReadFailed'));
      return;
    }
    if (bytes.isEmpty) {
      CoduxLog.warn('[codux-flutter-upload] picked file is empty');
      return;
    }
    if (!_canUploadOverCurrentPath) {
      _showSnack(_t('upload.directRequired'));
      setState(() => _status = _t('upload.directRequired'));
      return;
    }
    _terminalUploadCompletion?.completeError(
      StateError('Terminal upload superseded'),
    );
    final uploadCompletion = Completer<void>();
    _terminalUploadCompletion = uploadCompletion;
    final uploadingMessage = _t(terminalUploadUploadingKey(source));
    setState(() {
      _terminalUploadLoading = true;
      _terminalUploadStatus = uploadingMessage;
      _status = _terminalUploadStatus;
    });
    try {
      final sent = await _activeTransport?.sendTerminalUpload(
        deviceId: _activeDevice?.deviceId ?? '',
        sessionId: id,
        name: picked.name,
        mime: terminalUploadMime(
          picked.name,
          image: source == TerminalUploadSource.image,
        ),
        bytes: bytes,
        kind: terminalUploadKind(source),
      );
      CoduxLog.info(
        '[codux-flutter-upload] blob enqueue result=$sent session=$id name=${picked.name} bytes=${bytes.length}',
      );
      if (sent != true) {
        throw StateError('Upload transport is not connected');
      }
      CoduxLog.info(
        '[codux-flutter-upload] blob sent session=$id name=${picked.name} bytes=${bytes.length}',
      );
      if (!mounted) return;
      final insertingMessage = _t(terminalUploadInsertingKey(source));
      setState(() {
        _terminalUploadStatus = insertingMessage;
        _status = insertingMessage;
      });
      await uploadCompletion.future.timeout(const Duration(seconds: 30));
    } catch (error) {
      CoduxLog.warn('[codux-flutter-upload] upload failed: $error');
      if (!mounted) return;
      if (_terminalUploadCompletion == uploadCompletion) {
        _terminalUploadCompletion = null;
      }
      setState(() {
        _terminalUploadLoading = false;
        _terminalUploadStatus = '';
        _status = '${_t('remote.error')}: $error';
      });
    }
  }

  bool get _canUploadOverCurrentPath => _isConnected;

  Future<void> _checkUpdate() async {
    setState(() {
      _status = _t('update.checking');
      _blockingLoadingMessage = _t('update.loading');
    });
    try {
      final result = await _updateCheckService.check();
      if (!result.available) {
        final toastKey = result.toastKey;
        if (toastKey != null && toastKey.isNotEmpty) {
          _showToast(_t(toastKey, params: result.toastParams));
        }
        return;
      }
      if (!mounted) return;
      showDialog<void>(
        context: context,
        builder: (ctx) => UpdateAvailableDialog(
          title: _t(
            'update.foundTitle',
            params: {'version': result.version ?? ''},
          ),
          body: _t(
            result.isIos ? 'update.foundBodyAppStore' : 'update.foundBody',
            params: {'version': result.currentVersion},
          ),
          laterLabel: _t('common.later'),
          actionLabel: result.isIos
              ? _t('common.openAppStore')
              : _t('common.openGithub'),
          onOpen: () {
            if (result.url.isNotEmpty) _openUrl(result.url);
          },
        ),
      );
    } catch (error) {
      _showToast(_t('update.failed', params: {'reason': '$error'}));
    } finally {
      if (mounted) setState(() => _blockingLoadingMessage = null);
    }
  }

  Future<void> _showAboutDialogNow() async {
    final info = await PackageInfo.fromPlatform();
    if (!mounted) return;
    showDialog<void>(
      context: context,
      builder: (ctx) => CoduxAboutDialog(
        title: _t('app.about'),
        body: _t('app.aboutText'),
        versionText: 'v${info.version}+${info.buildNumber}',
        closeLabel: _t('app.close'),
        onOpenGithub: () => _openUrl('https://github.com/duxweb/codux-flutter'),
      ),
    );
  }

  Future<void> _openUrl(String value) async {
    final uri = Uri.parse(value);
    if (!await launchUrl(uri, mode: LaunchMode.externalApplication)) {
      await launchUrl(uri);
    }
  }

  void _showSnack(String message) => _showToast(message);

  void _showToast(String message) {
    if (!mounted) return;
    _toastTimer?.cancel();
    setState(() => _toastMessage = message);
    _toastTimer = Timer(const Duration(seconds: 2), () {
      if (mounted) setState(() => _toastMessage = null);
    });
  }

  void _showLogViewer() {
    showDialog<void>(
      context: context,
      builder: (ctx) => DebugLogDialog(
        title: _t('app.debugLogs'),
        emptyLabel: _t('logs.empty'),
        clearLabel: _t('logs.clear'),
        copyLabel: _t('logs.copy'),
        exportLabel: _t('logs.export'),
        closeLabel: _t('app.close'),
        onCopy: (text) async {
          await Clipboard.setData(ClipboardData(text: text));
          if (mounted) _showToast(_t('logs.copied'));
        },
        onExport: _exportLogs,
      ),
    );
  }

  Future<void> _exportLogs(String text) async {
    try {
      await _logExportService.export(text, shareText: _t('logs.shareText'));
      if (mounted) _showToast(_t('logs.exported'));
    } catch (error) {
      if (mounted) _showToast('${_t('logs.exportFailed')}: $error');
    }
  }

  void _confirmRemoveDevice(StoredDevice device) {
    showDialog<bool>(
      context: context,
      builder: (ctx) => DeviceRemoveDialog(
        title: _t('app.removeDevice'),
        message: _t(
          'app.removeDeviceConfirm',
          params: {'name': device.hostName ?? device.name},
        ),
        cancelLabel: _t('app.cancel'),
        removeLabel: _t('app.remove'),
      ),
    ).then((confirmed) {
      if (confirmed == true) _removeDevice(device);
    });
  }

  void _handleBackNavigation() {
    if (_editingFilePath != null) {
      setState(() {
        _editingFilePath = null;
        _fileEditorLoading = false;
        _fileEditorSaving = false;
        _fileEditorEditing = false;
      });
      return;
    }
    if (_showScanner) {
      setState(() => _showScanner = false);
      return;
    }
    if (_showFilePicker) {
      _filePickerTimeoutTimer?.cancel();
      setState(() => _showFilePicker = false);
      return;
    }
    if (_showProjectForm) {
      setState(() => _showProjectForm = false);
      return;
    }
    if (_showSettings) {
      _popCupertinoPage(() {
        _showSettings = false;
      });
      return;
    }
    if (_showTerminalSwitcher) {
      _popCupertinoPage(() {
        _showTerminalSwitcher = false;
      }).then((_) {
        if (mounted) _mountVisibleTerminal(reason: 'switcher-back');
      });
      return;
    }
    if (_pendingPairing != null) {
      _cancelPairing();
      return;
    }
    if (_showTerminal) {
      _releaseTerminalViewport();
      _popCupertinoPage(() {
        _showTerminal = false;
        _workspaceMode = 'terminal';
      });
      return;
    }
    _disconnectTransport(status: _t('app.disconnected'), closeTerminal: true);
    SystemNavigator.pop();
  }

  void _handleWorkspaceEdgeDragStart(DragStartDetails details) {
    if (!Platform.isIOS ||
        (!_showTerminal && !_showSettings && !_showTerminalSwitcher)) {
      return;
    }
    final edgeWidth = MediaQuery.viewPaddingOf(context).left + 24.0;
    final startX = details.localPosition.dx;
    if (startX > edgeWidth) {
      _edgeBackDragStartX = null;
      return;
    }
    _edgeBackDragStartX = startX;
    _edgeBackDragDeltaX = 0;
    _edgeBackDragDeltaY = 0;
    _edgeBackController.stop();
  }

  void _handleWorkspaceEdgeDragUpdate(DragUpdateDetails details) {
    if (_edgeBackDragStartX == null) return;
    _edgeBackDragDeltaX += details.delta.dx;
    _edgeBackDragDeltaY += details.delta.dy;
    final width = MediaQuery.sizeOf(context).width;
    if (width <= 0) return;
    _edgeBackController.value = (_edgeBackDragDeltaX / width).clamp(0.0, 1.0);
  }

  void _handleWorkspaceEdgeDragEnd(DragEndDetails details) {
    if (_edgeBackDragStartX == null) return;
    final dragX = _edgeBackDragDeltaX;
    final dragY = _edgeBackDragDeltaY.abs();
    final velocityX = details.velocity.pixelsPerSecond.dx;
    _edgeBackDragStartX = null;
    _edgeBackDragDeltaX = 0;
    _edgeBackDragDeltaY = 0;
    final width = MediaQuery.sizeOf(context).width;
    final progress = width <= 0 ? 0.0 : (dragX / width).clamp(0.0, 1.0);
    final shouldComplete =
        dragX > 72 &&
        dragX > dragY * 1.4 &&
        (velocityX > 260 || progress > 0.34);
    if (shouldComplete) {
      unawaited(_completeCupertinoPageBack());
    } else {
      unawaited(
        _edgeBackController.animateBack(
          0,
          duration: const Duration(milliseconds: 180),
          curve: Curves.easeOutCubic,
        ),
      );
    }
  }

  Future<void> _completeCupertinoPageBack() async {
    await _edgeBackController.animateTo(
      1,
      duration: const Duration(milliseconds: 180),
      curve: Curves.easeOutCubic,
    );
    if (!mounted) return;
    final closingTerminal = !_showSettings && !_showTerminalSwitcher;
    if (closingTerminal) {
      _releaseTerminalViewport();
    }
    final closingSwitcher = _showTerminalSwitcher;
    setState(() {
      if (_showSettings) {
        _showSettings = false;
      } else if (_showTerminalSwitcher) {
        _showTerminalSwitcher = false;
      } else {
        _showTerminal = false;
      }
      _workspaceMode = 'terminal';
    });
    _edgeBackController.value = 0;
    if (closingSwitcher) {
      _mountVisibleTerminal(reason: 'switcher-edge-back');
    }
  }

  Future<void> _pushCupertinoPage(VoidCallback updateState) async {
    _edgeBackController.value = 1;
    setState(updateState);
    await _edgeBackController.animateBack(
      0,
      duration: const Duration(milliseconds: 260),
      curve: Curves.easeOutCubic,
    );
  }

  Future<void> _popCupertinoPage(VoidCallback updateState) async {
    if (!Platform.isIOS) {
      setState(updateState);
      return;
    }
    await _edgeBackController.animateTo(
      1,
      duration: const Duration(milliseconds: 220),
      curve: Curves.easeOutCubic,
    );
    if (!mounted) return;
    setState(updateState);
    _edgeBackController.value = 0;
  }

  void _cancelWorkspaceEdgeBack() {
    _edgeBackDragStartX = null;
    _edgeBackDragDeltaX = 0;
    _edgeBackDragDeltaY = 0;
    unawaited(
      _edgeBackController.animateBack(
        0,
        duration: const Duration(milliseconds: 180),
        curve: Curves.easeOutCubic,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final media = MediaQuery.of(context);
    final topInset = media.viewPadding.top;
    final bottomInset = media.viewPadding.bottom;
    final leftInset = media.viewPadding.left;
    final keyboardVisible = media.viewInsets.bottom > bottomInset + 8.0;
    if (_keyboardVisible != keyboardVisible) {
      _keyboardVisible = keyboardVisible;
      if (keyboardVisible) {
        _keyboardShownSinceRequest = true;
      } else if (_keyboardShownSinceRequest) {
        _keyboardRequested = false;
        _keyboardShownSinceRequest = false;
      }
    }

    final textScale = _settings.appTextScale.clamp(0.75, 1.25);

    return MediaQuery(
      data: media.copyWith(textScaler: TextScaler.linear(textScale)),
      child: Builder(
        builder: (context) {
          final deviceHome = _buildDeviceHome(topInset, bottomInset);
          final settingsPage = _buildSettingsPage(topInset, bottomInset);
          final switcherPage = _buildTerminalSwitcherPage(
            topInset,
            bottomInset,
          );
          final workspacePage = _buildWorkspace(topInset, bottomInset);
          return CoduxHomeShell(
            metrics: CoduxHomeShellMetrics(
              topInset: topInset,
              bottomInset: bottomInset,
              leftInset: leftInset,
              edgeBackAnimation: _edgeBackController,
            ),
            pages: CoduxHomeShellPages(
              deviceHome: deviceHome,
              settingsPage: settingsPage,
              switcherPage: switcherPage,
              workspacePage: workspacePage,
            ),
            state: CoduxHomeShellState(
              showSettings: _showSettings,
              showTerminal: _showTerminal,
              showTerminalSwitcher: _showTerminalSwitcher,
            ),
            overlays: CoduxHomeOverlayState(
              showScanner: _showScanner,
              pendingPairing: _pendingPairing,
              pairingInFlight: _pairingInFlight,
              pairingError: _pairingError,
              showProjectForm: _showProjectForm,
              projectFormTitle: _projectFormMode == ProjectFormMode.edit
                  ? _t('project.edit')
                  : _t('project.add'),
              projectNameController: _projectNameController,
              projectPathController: _projectPathController,
              showFilePicker: _showFilePicker,
              filePickerTitle: _t('project.pathLabel'),
              filePickerPath: _filePickerPath,
              filePickerParent: _filePickerParent,
              filePickerEntries: _filePickerEntries,
              filePickerLoading: _filePickerLoading,
              showVoiceOverlay: _showVoiceOverlay,
              voiceService: _voiceService,
              editingFilePath: _editingFilePath,
              fileEditorController: _fileEditorController,
              fileEditorLoading: _fileEditorLoading,
              fileEditorSaving: _fileEditorSaving,
              fileEditorEditing: _fileEditorEditing,
              fileEditorEditable: _fileEditorEditable,
              blockingLoadingMessage: _blockingLoadingMessage,
              toastMessage: _toastMessage,
            ),
            actions: CoduxHomeShellActions(
              onBack: _handleBackNavigation,
              onEdgeDragStart: _handleWorkspaceEdgeDragStart,
              onEdgeDragUpdate: _handleWorkspaceEdgeDragUpdate,
              onEdgeDragEnd: _handleWorkspaceEdgeDragEnd,
              onEdgeDragCancel: _cancelWorkspaceEdgeBack,
              onScannerDetected: _handleScannedPayload,
              onCloseScanner: () => setState(() => _showScanner = false),
              onCancelPairing: _cancelPairing,
              onConfirmPairing: _confirmPairing,
              onCloseProjectForm: () =>
                  setState(() => _showProjectForm = false),
              onChooseProjectPath: _chooseProjectFormPath,
              onSaveProjectForm: _saveProjectForm,
              onCloseFilePicker: () {
                _filePickerTimeoutTimer?.cancel();
                setState(() => _showFilePicker = false);
              },
              onOpenFilePickerPath: _openRemoteFilePicker,
              onSelectFilePickerEntry: _selectRemoteProjectFolder,
              onOpenFilePickerHome: () => _openRemoteFilePicker(),
              onOpenFilePickerRoot: () => _openRemoteFilePicker('/'),
              onOpenFilePickerVolumes: () => _openRemoteFilePicker('/Volumes'),
              onCloseVoice: () => setState(() => _showVoiceOverlay = false),
              onSendVoiceText: (text) {
                _insertTerminalText(text);
                setState(() => _showVoiceOverlay = false);
              },
              onCloseFileEditor: () => setState(() => _editingFilePath = null),
              onEditFile: () => setState(() => _fileEditorEditing = true),
              onSaveFile: _saveEditingFile,
            ),
          );
        },
      ),
    );
  }

  Widget _buildDeviceHome(double topInset, double bottomInset) {
    return DeviceHomeScreen(
      devices: _devices,
      activeDeviceId: _activeDevice?.deviceId,
      ready: _isDeviceListConnected,
      status: _deviceListStatusText,
      latencyMs: _isConnected ? _latencyMs : null,
      deviceSubtitle: _deviceSubtitle,
      topInset: topInset,
      bottomInset: bottomInset,
      onOpen: _openDeviceTerminal,
      onConnect: (device) => _connect(device),
      onAdd: () => setState(() => _showScanner = true),
      onEdit: _editDevice,
      onDelete: _confirmRemoveDevice,
      onRefresh: _refreshDeviceList,
      onSettings: () => _pushCupertinoPage(() {
        _showSettings = true;
      }),
      onLogs: _showLogViewer,
      onCheckUpdate: _checkUpdate,
      onAbout: _showAboutDialogNow,
    );
  }

  Widget _buildSettingsPage(double topInset, double bottomInset) {
    final prefs = AppPreferences.of(context);
    return SettingsScreen(
      nameController: _settingsNameController,
      detectedName: _detectedDeviceName,
      topInset: topInset,
      bottomInset: bottomInset,
      currentAccent: prefs.accent,
      currentLocale: prefs.locale,
      currentLogLevel: _settings.logLevel,
      appTextScale: _settings.appTextScale,
      terminalFontSize: _settings.terminalFontSize,
      onChangeAccent: (next) {
        widget.onChangeAccent(next);
        setState(() => _settings = _settings.copyWith(accentId: next.id));
      },
      onChangeLocale: (next) {
        widget.onChangeLocale(next);
        setState(() => _settings = _settings.copyWith(localeId: next.id));
      },
      onChangeLogLevel: (next) {
        CoduxLog.setLevelName(next);
        setState(() => _settings = _settings.copyWith(logLevel: next));
      },
      onChangeAppTextScale: (next) {
        final settings = _settings.copyWith(appTextScale: next);
        setState(() => _settings = settings);
        unawaited(_storage.saveSettings(settings));
      },
      onChangeTerminalFontSize: (next) {
        final settings = _settings.copyWith(terminalFontSize: next);
        setState(() => _settings = settings);
        unawaited(_storage.saveSettings(settings));
      },
      onUseDetectedName: () =>
          setState(() => _settingsNameController.text = _detectedDeviceName),
      onSave: _saveSettings,
      onBack: () => _popCupertinoPage(() {
        _showSettings = false;
      }),
    );
  }

  Widget _buildTerminalSwitcherPage(double topInset, double bottomInset) {
    return TerminalSwitcherScreen(
      topInset: topInset,
      bottomInset: bottomInset,
      terminals: _currentProjectTerminals(),
      worktrees: _worktrees,
      activeTerminalId: _sessionId,
      selectedProjectId: _selectedProjectId,
      selectedWorktreeId: _selectedWorktreeId,
      switchingWorktreeId: _pendingWorktreeSwitch?.worktreeId,
      loadingWorktrees: _worktreeListLoading,
      creatingSplit:
          _creatingTerminalProjectId == _selectedProjectId &&
          _creatingTerminalLayoutKind == 'split',
      creatingTab:
          _creatingTerminalProjectId == _selectedProjectId &&
          _creatingTerminalLayoutKind == 'tab',
      creatingWorktree: _creatingWorktree,
      onBack: _closeTerminalSwitcher,
      onSelectTerminal: _selectTerminalFromSwitcher,
      onCreateSplit: _createCurrentProjectTerminal,
      onCreateTab: _createCurrentProjectTabTerminal,
      onCloseTerminal: _closeTerminal,
      onSelectWorktree: _selectWorktree,
      onCreateWorktree: _createWorktree,
      onMergeWorktree: _mergeWorktree,
      onDeleteWorktree: _deleteWorktree,
      onOpenWorktrees: _ensureSelectedProjectWorktrees,
      onRefreshWorktrees: () => _requestWorktreeList(loading: true),
      onRefreshTerminals: () => _requestTerminalList(resetRetry: true),
    );
  }

  Widget _buildWorkspace(double topInset, double bottomInset) {
    final terminalBody = RemoteTerminalPane(
      connected: _isConnected,
      showTerminal: _hasShownTerminal,
      hasDevice: _activeDevice != null,
      status: _status,
      workspaceMode: _workspaceMode,
      projectListLoaded: _projectListLoaded,
      projectCount: _projects.length,
      terminalUploadLoading: _terminalUploadLoading,
      terminalUploadStatus: _terminalUploadStatus,
      terminalBufferLoading: _terminalBufferLoading,
      sessionId: _sessionId,
      pendingBufferSessionId: _terminalBufferRetry.pendingSessionId,
      connectionStatusText: _connectionStatusText,
      terminalHistoryLoadingText: _terminalHistoryLoadingText(),
      keyboardVisible: _keyboardVisible,
      keyboardRequested: _keyboardRequested,
      keyboardRequestSerial: _keyboardRequestSerial,
      repaintSignal: _terminalRepaint,
      outputController: _terminalOutputController,
      terminalFontSize: _settings.terminalFontSize,
      onConnect: () => _connect(),
      onInput: _queueTerminalTyping,
      onResize: (cols, rows) {
        final firstResize = !_terminalReady;
        _terminalReady = true;
        _sendTerminalResize(cols, rows);
        if (firstResize) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (!mounted) return;
            CoduxLog.debug(
              '[codux-flutter-terminal] first resize ready selected=${_selectedProjectId ?? ''} session=${_sessionId ?? ''} terminalListLoaded=$_terminalListLoaded',
            );
            _ensureTerminalForSelectedProject();
            _mountVisibleTerminal(reason: 'first-resize');
          });
        }
      },
      onSelectionChanged: (text) {
        if (_terminalSelectedText == text) return;
        setState(() => _terminalSelectedText = text);
      },
      onSendKey: _sendTerminalKey,
      onToggleKeyboard: _toggleTerminalKeyboard,
      onPaste: _pasteToTerminal,
      onCopy: _copyTerminalSelection,
      onUpload: _chooseUploadForTerminal,
      onVoiceInput: _startVoiceInput,
    );

    return RemoteWorkspaceView(
      topInset: topInset,
      workspaceMode: _workspaceMode,
      connected: _isConnected,
      latencyMs: _latencyMs,
      projects: _projects,
      selectedProjectId: _selectedProjectId,
      projectListLoaded: _projectListLoaded,
      terminals: _currentProjectTerminals(),
      activeTerminalId: _sessionId,
      hasCurrentTerminal: _currentTerminal() != null,
      aiStats: _currentAIStats,
      aiStatsLoading: _aiStatsLoading,
      projectFilesPath: _projectFilesPath,
      projectFilesParent: _projectFilesParent,
      projectFileEntries: _projectFileEntries,
      projectFilesLoading: _projectFilesLoading,
      terminalBody: terminalBody,
      onShowTerminal: _showTerminalMode,
      onShowStats: _requestAIStats,
      onShowFiles: _showFilesMode,
      onBack: () => setState(() {
        _showTerminal = false;
        _workspaceMode = 'terminal';
      }),
      onEditProject: _requestProjectEdit,
      onAddProject: _requestProjectAdd,
      onRemoveProject: _requestProjectRemove,
      onSelectProject: _onProjectSelected,
      onSelectTerminal: _selectTerminal,
      onRefreshLists: _refreshLists,
      onCreateTerminal: _createCurrentProjectTerminal,
      onCloseCurrentTerminal: _closeCurrentTerminal,
      onRebuildTerminal: _rebuildCurrentTerminal,
      onOpenTerminalSwitcher: _openTerminalSwitcher,
      onRequestProjectFiles: _requestProjectFiles,
      onOpenProjectFile: _requestFileRead,
      onOpenProjectHome: () => _openFileLocation(_selectedProject?.path ?? ''),
      onOpenProjectRoot: () => _openFileLocation('/'),
      onOpenProjectVolumes: () => _openFileLocation('/Volumes'),
      onRenameProjectFile: _renameProjectFile,
      onCopyProjectFilePath: _copyProjectFilePath,
      onDeleteProjectFile: _deleteProjectFile,
    );
  }
}
