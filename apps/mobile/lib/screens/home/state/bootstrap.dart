part of '../home_page.dart';

/// Startup + device persistence: bootstrap, cached-project restore, device
/// name/save/remember, QR scan handling, pairing handshake and settings save.
///
/// Split into a part + extension to keep the State class navigable; behaviour
/// is unchanged. Rebuilds route through [_CoduxHomePageState._applyState]
/// (`setState` is `@protected` and cannot be called from an extension).
extension _HomePageBootstrap on HomeController {
  Future<void> _bootstrap() async {
    final initialDevices = widget.initialDevices;
    if (initialDevices != null) {
      if (!mounted) return;
      _applyState(() {
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
    widget.onChangeThemeMode(themeModeFromId(next.themeModeId));
    _applyState(() {
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
      _applyState(() {
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
    _applyState(() {
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
    _applyState(() {
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
      _applyState(() {
        _showScanner = false;
        _pendingPairing = payload;
        _pairingInFlight = false;
        _pairingCancelled = false;
        _pairingError = null;
      });
    } catch (error) {
      CoduxLog.warn('[codux-flutter-pairing] scan failed error=$error');
      if (!mounted) return;
      _applyState(() => _showScanner = false);
      _showToast(error.toString().replaceFirst('Exception: ', ''));
    }
  }

  void _cancelPairing() {
    if (_pairingInFlight) {
      _applyState(() => _pairingCancelled = true);
      return;
    }
    _applyState(() {
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
    _applyState(() {
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
      _applyState(() {
        _pendingPairing = null;
        _pairingInFlight = false;
        _pairingCancelled = false;
        _pairingError = null;
      });
      await _saveDevice(confirmed);
      _showToast(_t('device.bound', params: {'name': hostName}));
    } on PairingCancelledException {
      if (!mounted) return;
      _applyState(() {
        _pendingPairing = null;
        _pairingInFlight = false;
        _pairingCancelled = false;
        _pairingError = null;
        _status = _t('pair.cancelled');
      });
    } on PairingRejectedException {
      if (!mounted) return;
      _applyState(() {
        _pendingPairing = null;
        _pairingInFlight = false;
        _pairingCancelled = false;
        _pairingError = null;
        _status = _t('pair.rejected');
      });
      _showToast(_t('pair.rejected'));
    } catch (error) {
      if (!mounted) return;
      _applyState(() {
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
    _applyState(() => _status = _t('pair.waiting'));
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
    _applyState(() {
      _settings = next;
      _status = _t('settings.saved');
    });
    _popCupertinoPage(() {
      _showSettings = false;
    });
    _sendDeviceInfo(force: true);
  }

}
