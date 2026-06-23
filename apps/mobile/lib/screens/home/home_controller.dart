part of 'home_page.dart';

/// Owns the home screen's state + logic as a [ChangeNotifier] store, fully
/// separated from the [_CoduxHomePageState] view. The view observes this and
/// only renders; all mutation flows through [_applyState] (notifyListeners).
/// Concern logic lives in the `state/` part extensions on this class.
const double _padLayoutMinWidth = 900;
const int _terminalBufferMaxChars =
      TerminalBufferCapability.mobileMaxChars;

class HomeController extends ChangeNotifier with WidgetsBindingObserver {
  // HomeController is an internal same-library store; the view type is private.
  // ignore: library_private_types_in_public_api
  HomeController(this._view);

  final _CoduxHomePageState _view;

  BuildContext get context => _view.context;
  bool get mounted => _view.mounted;
  CoduxHomePage get widget => _view.widget;



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
  // When we last sent a viewport.claim, used to throttle the per-input claim so
  // a fling over a mouse-tracking TUI (which forwards a wheel event -- and used
  // to re-claim -- every cell-height) does not flood the transport with claims
  // and spike measured latency. A claim only renews the lease / asserts
  // ownership, so one per window is enough; a desktop steal still reclaims
  // reactively via the viewport-state broadcast.
  DateTime? _lastViewportClaimAt;
  String? _lastViewportClaimSession;
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
      _workspaceMode == WorkspaceMode.terminal;

  bool get _terminalDataVisible =>
      _showTerminal && _workspaceMode == WorkspaceMode.terminal;

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
  WorkspaceMode _workspaceMode = WorkspaceMode.terminal;
  String _projectFilesPath = '';
  String? _projectFilesParent;
  String? _editingFilePath;
  RemoteGitDiff? _gitDiff;
  String? _gitDiffPath;
  List<AISessionRecord> _aiSessions = const [];
  String? _aiSessionsProjectId;
  List<RemoteSshProfile> _sshProfiles = const [];
  String? _toastMessage;
  String? _blockingLoadingMessage;
  bool _projectFilesLoading = false;
  bool _worktreeListLoading = false;
  bool _creatingWorktree = false;
  bool _fileEditorLoading = false;
  bool _fileEditorSaving = false;
  bool _fileEditorEditing = false;
  bool _fileEditorEditable = true;
  String _fileEditorOriginal = '';
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
  PendingWorktreeSwitch? _pendingWorktreeSwitch;
  int? _latencyMs;
  Timer? _latencyProbeTimer;
  int _latencyProbeCounter = 0;
  final Map<String, DateTime> _latencyProbeSentAt = {};
  Timer? _connectionGraceTimer;
  // After the fast retry burst gives up on a baseline, a slow heartbeat keeps
  // re-requesting it so a still-connected (merely slow) link heals on its own
  // instead of staying truncated until a manual project switch / reconnect.
  Timer? _terminalBaselineRearmTimer;

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

  void init() {
    // The host viewport lease (20s TTL) is renewed by input and output
    // acks; an idle session emits neither, so keep the lease alive while
    // the terminal is actually on screen. Claims for the current owner are
    // idempotent renewals on the host.
    _viewportLeaseKeepalive = Timer.periodic(const Duration(seconds: 8), (_) {
      if (!mounted || !_appInForeground) return;
      if (_workspaceMode != WorkspaceMode.terminal || !_hasShownTerminal) return;
      if (_sessionId == null) return;
      // Renew only: a phone left idle on the terminal screen must not
      // steal the viewport back from an actively-used desktop. Explicit
      // interaction (scroll, input) reclaims instead.
      if (!_terminalViewportInteractive) return;
      _claimTerminalViewport();
    });
    WidgetsBinding.instance.addObserver(this);
    _edgeBackController = AnimationController(
      vsync: _view,
      duration: const Duration(milliseconds: 220),
    );
    _terminalBufferRetry = TerminalBufferRetryCoordinator(
      // Chunked baselines over a slow relay can take seconds; a transfer that is
      // still making progress (chunks arriving) is never re-issued -- only one
      // whose chunks actually stopped (a dropped chunk under high latency) is.
      retryDelay: const Duration(milliseconds: 2500),
      // More attempts than the old 3: each re-issue restarts the whole chunked
      // download, so under sustained latency we need a longer window to catch a
      // run where every chunk lands before the next stall check.
      maxRetries: 6,
      onRetryExhausted: (sessionId) {
        if (!mounted || _sessionId != sessionId) return;
        _terminalOutputController.resetSessionTransient(
          sessionId,
          resetSequence: true,
        );
        _terminalBufferRetry.resetLastBuffered();
        // Unfreeze: stop blocking on the spinner and show whatever arrived, but
        // keep trying in the background so the session heals when the link
        // recovers instead of staying truncated until a manual switch.
        _applyState(() => _setTerminalBufferLoading(false));
        _scheduleTerminalBaselineRearm(sessionId);
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

  void disposeController() {
    final wasConnected = _transportConnected;
    if (wasConnected) {
      _notifyHostBeforeTransportClose();
    }
    _viewportLeaseKeepalive?.cancel();
    _terminalBaselineRearmTimer?.cancel();
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
        _applyState(() {
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

  bool get _terminalBufferLoading =>
      _terminalBufferPhase != RemoteTerminalBufferPhase.idle;

  void _applyState(VoidCallback fn) {
    fn();
    notifyListeners();
  }

  HomeTerminalActions get _terminalActions {
    return HomeTerminalActions(
      context: context,
      t: _t,
      mounted: mounted,
      selectedProjectId: _selectedProjectId,
      workspaceMode: _workspaceMode,
      showToast: _showToast,
      showTerminalWorkspace: _showTerminalWorkspace,
      focusTerminalViewSoon: _focusTerminalViewSoon,
      releaseTerminalViewport: _releaseTerminalViewport,
      ensureSelectedProjectWorktrees: ({bool loading = false}) =>
          _ensureSelectedProjectWorktrees(loading: loading),
      pushTerminalSwitcher: (mutate) => _pushCupertinoPage(() {
        _showTerminalSwitcher = true;
        mutate();
      }),
      hideTerminalSwitcher: _hideTerminalSwitcher,
      mountVisibleTerminal: ({required String reason}) =>
          _mountVisibleTerminal(reason: reason),
      currentTerminal: _currentTerminal,
      currentProjectTerminals: _currentProjectTerminals,
      isAccessibleTerminal: (terminal) =>
          terminal != null && _isAccessibleTerminal(terminal),
      runtime: _remoteRuntime,
      inputBatcher: _terminalInputBatcher,
      applyRuntimePlan: (plan, {required reason}) =>
          _applyRuntimePlan(plan, reason: reason),
      sendTerminalClose: (terminal) {
        _sendTerminalEnvelope(
          RelayEnvelope(
            type: RemoteMessageType.terminalClose,
            sessionId: terminal.id,
          ),
          terminal: terminal,
        );
      },
      setTerminalReady: (ready) => _terminalReady = ready,
    );
  }

  HomeRuntimeCoordinator get _runtimeCoordinator {
    return HomeRuntimeCoordinator(
      remoteProtocolReady: _remoteProtocolReady,
      selectedProjectId: _selectedProjectId,
      terminalBufferCapability: _terminalBufferCapability,
      outputController: _terminalOutputController,
      terminalRepaint: _terminalRepaint,
      terminalInputSender: _terminalInputSender,
      terminalInputBatcher: _terminalInputBatcher,
      terminalBufferRetry: _terminalBufferRetry,
      terminalBindingCoordinator: _terminalBindingCoordinator,
      captureSnapshot: () => HomeRuntimeSnapshot(
        selectedProjectId: _selectedProjectId,
        selectedWorktreeId: _selectedWorktreeId,
        sessionId: _sessionId,
      ),
      syncRuntimeViewState: _syncRuntimeViewState,
      setTerminalBufferLoading: (loading) => _setTerminalBufferLoading(loading),
      restoreTerminalSessionFromCache: _restoreTerminalSessionFromCache,
      closeTerminalSwitcherAfterPendingWorktreeBuffer:
          _closeTerminalSwitcherAfterPendingWorktreeBuffer,
      trackTerminalBaselineRequest: _trackTerminalBaselineRequest,
      releaseTerminalViewport: ({String? sessionId}) =>
          _releaseTerminalViewport(sessionId: sessionId),
      clearTerminal: _clearTerminal,
      requestTerminalList: () => _requestTerminalList(resetRetry: true),
      sendProjectSelect: (projectId, {required reason}) =>
          _sendProjectSelect(projectId, reason: reason),
      focusTerminalViewSoon: _focusTerminalViewSoon,
      onSessionStateChanged: (previous, reason) {},
    );
  }

  HomeWorkspaceBuilder get _workspaceBuilder {
    return const HomeWorkspaceBuilder(padLayoutMinWidth: _padLayoutMinWidth);
  }

  HomeWorkspaceModeActions get _workspaceModeActions {
    return HomeWorkspaceModeActions(
      remoteProtocolReady: _remoteProtocolReady,
      workspaceMode: _workspaceMode,
      terminalDataVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
      selectedProject: _selectedProject,
      selectedWorktreeId: _selectedWorktreeId,
      projectFilesPath: _projectFilesPath,
      releaseTerminalViewport: _releaseTerminalViewport,
      showToast: _showToast,
      setModeState:
          (mode, {bool terminalReady = false, bool aiStatsLoading = false}) {
            _applyState(() {
              _workspaceMode = mode;
              _terminalReady = terminalReady;
              _aiStatsLoading = aiStatsLoading;
            });
          },
      setProjectFilesState: (path, {required bool loading}) {
        _applyState(() {
          _projectFilesLoading = loading;
          _projectFilesPath = path;
        });
      },
      focusTerminalViewSoon: _focusTerminalViewSoon,
      mountVisibleTerminal: ({required String reason}) =>
          _mountVisibleTerminal(reason: reason),
      sendEnvelope: _send,
      applyRuntimePlan: (plan, {required reason}) =>
          _applyRuntimePlan(plan, reason: reason),
      runtime: _remoteRuntime,
      projectController: _projectController,
      projectFileController: _projectFileController,
    );
  }

  HomeWorktreeActions get _worktreeActions {
    return HomeWorktreeActions(
      context: context,
      t: _t,
      selectedProject: _selectedProject,
      selectedProjectId: _selectedProjectId,
      selectedWorktreeId: _selectedWorktreeId,
      terminalDataVisible: _terminalDataVisible,
      terminalListLoaded: _terminalListLoaded,
      preferredBaseBranch: _defaultWorktreeBaseBranch ?? '',
      worktreeBaseBranches: _worktreeBaseBranches,
      worktreesForProject: _worktreesForProject,
      showToast: _showToast,
      flushTerminalInput: _terminalInputBatcher.flush,
      closeTerminalSwitcher: _closeTerminalSwitcher,
      markPendingSwitch: (projectId, worktreeId) {
        _pendingWorktreeSwitch = PendingWorktreeSwitch(
          projectId: projectId,
          worktreeId: worktreeId,
        );
      },
      clearPendingSwitch: () => _pendingWorktreeSwitch = null,
      setWorktreeListLoading: (loading) => _applyState(() {
        _worktreeListLoading = loading;
      }),
      setCreatingWorktree: (creating) => _applyState(() {
        _creatingWorktree = creating;
      }),
      syncRuntimeViewState: _syncRuntimeViewState,
      showTerminalWorkspace: _showTerminalWorkspace,
      sendEnvelope: _send,
      applyRuntimePlan: (plan, {required reason}) =>
          _applyRuntimePlan(plan, reason: reason),
      runtime: _remoteRuntime,
      confirmAction:
          ({
            required String title,
            required String message,
            required bool destructive,
          }) => _confirmWorktreeAction(
            title: title,
            message: message,
            destructive: destructive,
          ),
      worktreeTitle: _worktreeTitle,
      selectEnvelope: _worktreeController.selectEnvelope,
      createEnvelope:
          ({
            required ProjectInfo project,
            required String baseBranch,
            required String name,
          }) => _worktreeController.createEnvelope(
            project: project,
            baseBranch: baseBranch,
            name: name,
          ),
      deleteEnvelope: _worktreeController.deleteEnvelope,
      mergeEnvelope: _worktreeController.mergeEnvelope,
    );
  }

  bool get _canUploadOverCurrentPath => _isConnected;

  void _showSnack(String message) => _showToast(message);

  void _showToast(String message) {
    if (!mounted) return;
    _toastTimer?.cancel();
    _applyState(() => _toastMessage = message);
    _toastTimer = Timer(const Duration(seconds: 2), () {
      if (mounted) _applyState(() => _toastMessage = null);
    });
  }

  List<TerminalInfo> get _workspaceTerminals {
    return _currentProjectTerminals();
  }

  WorkspaceShellData get _workspaceShellData {
    return WorkspaceShellData(
      terminals: _workspaceTerminals,
      worktrees: _selectedProjectWorktrees,
      aiStats: _currentAIStats,
      aiStatsLoading: _aiStatsLoading,
      gitStatus: _remoteRuntime.selectedGitStatus,
      currentSessions: _currentAIStats?.currentSessions ?? const [],
      aiSessions: _aiSessions,
      sshProfiles: _sshProfiles,
      projectFilesPath: _projectFilesPath,
      projectFilesParent: _projectFilesParent,
      projectFileEntries: _projectFileEntries,
      projectFilesLoading: _projectFilesLoading,
    );
  }

  List<RemoteWorktreeInfo> get _selectedProjectWorktrees {
    final projectId = _selectedProjectId;
    if (projectId == null) return const [];
    return _worktreesForProject(projectId);
  }

  void _openSelectedProjectHome() {
    _openFileLocation(_selectedProject?.path ?? '');
  }

  void _openProjectRoot() {
    _openFileLocation('/');
  }

  void _openProjectVolumes() {
    _openFileLocation('/Volumes');
  }


}
