import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';
import '../i18n.dart';
import '../theme/app_theme.dart';

class ScannerScreen extends StatefulWidget {
  const ScannerScreen({
    super.key,
    required this.bottomInset,
    required this.onDetected,
    required this.onClose,
    this.scannerBuilder,
  });

  final double bottomInset;
  final ValueChanged<String> onDetected;
  final VoidCallback onClose;
  final WidgetBuilder? scannerBuilder;

  @override
  State<ScannerScreen> createState() => _ScannerScreenState();
}

class _ScannerScreenState extends State<ScannerScreen>
    with WidgetsBindingObserver {
  late final MobileScannerController _controller;
  final _pairingTokenController = TextEditingController();
  bool _startPending = false;
  bool _handledPayload = false;
  bool _recognized = false;
  bool _showManualConnect = false;
  String? _importError;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    // Without an explicit resolution mobile_scanner defaults to 640x480 on
    // Android (see its `cameraResolution` doc) — too few pixels for a desktop QR
    // seen from arm's length on a tablet, which is why the live scan couldn't
    // lock on. Request 720p: ~2.25x the pixels (enough to decode a small QR)
    // while staying a standard CameraX ImageAnalysis size, so it shouldn't
    // starve the analysis stream the way a forced max resolution did before.
    // autoZoom stays off (it zoomed the centred QR out of frame); the photo
    // import button is the guaranteed fallback when the live scan still misses.
    _controller = MobileScannerController(
      autoStart: false,
      formats: const [BarcodeFormat.qrCode],
      detectionSpeed: DetectionSpeed.noDuplicates,
      cameraResolution: const Size(1280, 720),
    );
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _startScanner();
    });
  }

  Future<void> _startScanner() async {
    if (widget.scannerBuilder != null) return;
    if (_startPending || _handledPayload || _controller.value.isRunning) {
      return;
    }
    _startPending = true;
    await _controller.start();
    _startPending = false;
  }

  Future<void> _stopScanner() async {
    if (widget.scannerBuilder != null) return;
    if (_controller.value.isRunning) {
      await _controller.stop();
    }
  }

  void _handleDetected(String? value) {
    final payload = value?.trim();
    if (payload == null || payload.isEmpty || _handledPayload) return;
    _handledPayload = true;
    // Surface a loading state the INSTANT a code is recognized — before handing
    // off to pairing. If this spinner appears, scanning worked and any failure is
    // in pairing; if it never appears, the QR was never recognized. Removes the
    // "nothing happened" ambiguity between a scan miss and a pairing failure.
    if (mounted) setState(() => _recognized = true);
    _stopScanner();
    widget.onDetected(payload);
  }

  void _openManualConnect() {
    setState(() => _showManualConnect = true);
  }

  /// Decode a QR from a picked screenshot/photo instead of the live camera.
  /// A static full-resolution image is far easier to decode than the live
  /// preview stream, so this reliably pairs even on low-resolution tablet
  /// cameras where the live scan struggles. Reuses the existing scanner's
  /// `analyzeImage` — no extra dependency.
  Future<void> _importFromGallery() async {
    if (_handledPayload) return;
    if (_importError != null) {
      setState(() => _importError = null);
    }
    final noCodeMessage = AppPreferences.of(context).t('pair.importNoCode');
    try {
      final result = await FilePicker.pickFiles(type: FileType.image);
      final path = result?.files.firstOrNull?.path;
      if (path == null) return;
      final capture = await _controller.analyzeImage(path);
      final value = capture?.barcodes.firstOrNull?.rawValue?.trim();
      if (value != null && value.isNotEmpty) {
        _handleDetected(value);
        return;
      }
    } catch (_) {
      // Fall through to the not-found message below.
    }
    if (mounted) setState(() => _importError = noCodeMessage);
  }

  void _submitManualPayload(String token) {
    final value = token.trim();
    if (value.isEmpty) return;
    final uri = Uri.tryParse(value);
    if (uri != null && uri.scheme == 'codux' && uri.host == 'pair') {
      _handleDetected(value);
      return;
    }
    _handleDetected(
      Uri(
        scheme: 'codux',
        host: 'pair',
        queryParameters: {'payload': value},
      ).toString(),
    );
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (!mounted || _handledPayload) return;
    switch (state) {
      case AppLifecycleState.resumed:
        _startScanner();
      case AppLifecycleState.inactive:
      case AppLifecycleState.hidden:
      case AppLifecycleState.paused:
      case AppLifecycleState.detached:
        _stopScanner();
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _pairingTokenController.dispose();
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    return Positioned.fill(
      child: ColoredBox(
        color: Colors.black,
        child: Stack(
          children: [
            widget.scannerBuilder?.call(context) ??
                MobileScanner(
                  controller: _controller,
                  useAppLifecycleState: false,
                  onDetect: (capture) {
                    final value = capture.barcodes.firstOrNull?.rawValue;
                    _handleDetected(value);
                  },
                ),
            Center(
              child: Container(
                width: 240,
                height: 240,
                decoration: BoxDecoration(
                  border: Border.all(color: accent, width: 2),
                  borderRadius: BorderRadius.circular(AppRadius.lg),
                ),
              ),
            ),
            Positioned(
              left: 18,
              right: 18,
              bottom: 36 + widget.bottomInset,
              child: Column(
                children: [
                  Text(
                    prefs.t('pair.scanTitle'),
                    style: const TextStyle(
                      color: Colors.white,
                      fontSize: AppTextSize.title,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                  const SizedBox(height: AppSpacing.s),
                  Text(
                    prefs.t('pair.scanHint'),
                    style: const TextStyle(
                      color: Color(0xFFCBD5E1),
                      fontSize: 14,
                    ),
                  ),
                  if (_importError != null) ...[
                    const SizedBox(height: AppSpacing.s),
                    Text(
                      _importError!,
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                        color: Color(0xFFF87171),
                        fontSize: 13,
                      ),
                    ),
                  ],
                  const SizedBox(height: AppSpacing.l),
                  Wrap(
                    alignment: WrapAlignment.center,
                    spacing: AppSpacing.s,
                    runSpacing: AppSpacing.s,
                    children: [
                      _ScannerAction(
                        label: prefs.t('pair.importImage'),
                        onTap: _importFromGallery,
                      ),
                      _ScannerAction(
                        label: prefs.t('pair.manualConnect'),
                        onTap: _openManualConnect,
                      ),
                      _ScannerAction(
                        label: prefs.t('pair.close'),
                        onTap: () {
                          _stopScanner();
                          widget.onClose();
                        },
                      ),
                    ],
                  ),
                ],
              ),
            ),
            if (_showManualConnect)
              _ManualConnectOverlay(
                tokenController: _pairingTokenController,
                onSubmit: _submitManualPayload,
                onCancel: () => setState(() => _showManualConnect = false),
              ),
            // Shown the moment a code is recognized, on top of everything, so the
            // user sees recognition succeeded before the pairing handoff resolves.
            if (_recognized)
              Positioned.fill(
                child: ColoredBox(
                  color: AppColors.backdrop,
                  child: Center(
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        CircularProgressIndicator(color: accent),
                        const SizedBox(height: AppSpacing.l),
                        Text(
                          prefs.t('pair.recognized'),
                          textAlign: TextAlign.center,
                          style: const TextStyle(
                            color: Colors.white,
                            fontSize: AppTextSize.body,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _ScannerAction extends StatelessWidget {
  const _ScannerAction({required this.label, required this.onTap});
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Material(
    color: Colors.black54,
    borderRadius: BorderRadius.circular(AppRadius.sm),
    child: InkWell(
      borderRadius: BorderRadius.circular(AppRadius.sm),
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: AppSpacing.l,
          vertical: AppSpacing.m,
        ),
        child: Text(
          label,
          style: const TextStyle(
            color: Colors.white,
            fontWeight: FontWeight.w700,
          ),
        ),
      ),
    ),
  );
}

class _ManualConnectOverlay extends StatefulWidget {
  const _ManualConnectOverlay({
    required this.tokenController,
    required this.onSubmit,
    required this.onCancel,
  });

  final TextEditingController tokenController;
  final ValueChanged<String> onSubmit;
  final VoidCallback onCancel;

  @override
  State<_ManualConnectOverlay> createState() => _ManualConnectOverlayState();
}

class _ManualConnectOverlayState extends State<_ManualConnectOverlay> {
  @override
  void initState() {
    super.initState();
    widget.tokenController.addListener(_onInputChanged);
  }

  @override
  void dispose() {
    widget.tokenController.removeListener(_onInputChanged);
    super.dispose();
  }

  void _onInputChanged() {
    if (mounted) setState(() {});
  }

  bool get _canSubmit => widget.tokenController.text.trim().isNotEmpty;

  void _submit() {
    if (!_canSubmit) return;
    widget.onSubmit(widget.tokenController.text);
  }

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final accent = Theme.of(context).colorScheme.secondary;
    return Positioned.fill(
      child: ColoredBox(
        color: AppColors.backdrop,
        child: SafeArea(
          child: Center(
            child: SingleChildScrollView(
              padding: EdgeInsets.only(
                left: AppSpacing.l,
                right: AppSpacing.l,
                top: AppSpacing.l,
                bottom: MediaQuery.viewInsetsOf(context).bottom + AppSpacing.l,
              ),
              child: Container(
                width: 340,
                padding: const EdgeInsets.all(AppSpacing.l),
                decoration: BoxDecoration(
                  color: AppColors.bgSurface,
                  borderRadius: BorderRadius.circular(AppRadius.lg),
                  border: Border.all(color: AppColors.border, width: 0.5),
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Text(
                      prefs.t('pair.manualConnect'),
                      style: TextStyle(
                        color: AppColors.textPrimary,
                        fontSize: AppTextSize.body,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    const SizedBox(height: AppSpacing.m),
                    TextField(
                      controller: widget.tokenController,
                      autofocus: true,
                      keyboardType: TextInputType.multiline,
                      textInputAction: TextInputAction.done,
                      minLines: 3,
                      maxLines: 5,
                      onSubmitted: (_) => _submit(),
                      style: TextStyle(
                        color: AppColors.textPrimary,
                        fontSize: AppTextSize.body,
                        fontWeight: FontWeight.w500,
                        letterSpacing: 0,
                      ),
                      decoration: InputDecoration(
                        filled: true,
                        fillColor: AppColors.bgElevated,
                        hintText: prefs.t('pair.tokenHint'),
                        hintStyle: TextStyle(color: AppColors.textSubtle),
                        border: OutlineInputBorder(
                          borderRadius: BorderRadius.circular(AppRadius.sm),
                          borderSide: BorderSide.none,
                        ),
                      ),
                    ),
                    const SizedBox(height: AppSpacing.s),
                    Text(
                      prefs.t('pair.manualHelp'),
                      style: TextStyle(
                        color: AppColors.textMuted,
                        fontSize: AppTextSize.small,
                      ),
                    ),
                    const SizedBox(height: AppSpacing.m),
                    Row(
                      children: [
                        Expanded(
                          child: OutlinedButton(
                            onPressed: widget.onCancel,
                            child: Text(
                              prefs.t('app.cancel'),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                        ),
                        const SizedBox(width: AppSpacing.s),
                        Expanded(
                          child: FilledButton(
                            onPressed: _canSubmit ? _submit : null,
                            style: FilledButton.styleFrom(
                              backgroundColor: accent,
                              foregroundColor: AppColors.bgBase,
                            ),
                            child: Text(
                              prefs.t('pair.submit'),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
