import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../i18n.dart';
import '../../services/local_voice_recognition_service.dart';
import '../../theme/app_theme.dart';

enum _VoiceStage {
  preparing,
  ready,
  starting,
  recording,
  cancelling,
  recognizing,
  reviewing,
  sending,
  error,
}

class VoiceInputOverlay extends StatefulWidget {
  const VoiceInputOverlay({
    super.key,
    required this.topInset,
    required this.bottomInset,
    required this.service,
    required this.onClose,
    required this.onSend,
  });

  final double topInset;
  final double bottomInset;
  final VoiceRecognitionService service;
  final VoidCallback onClose;
  final ValueChanged<String> onSend;

  @override
  State<VoiceInputOverlay> createState() => _VoiceInputOverlayState();
}

class _VoiceInputOverlayState extends State<VoiceInputOverlay> {
  static const _cancelDistance = 60.0;
  static const _minRecordingMs = 200;

  final _textController = TextEditingController();
  StreamSubscription<double>? _amplitudeSubscription;
  _VoiceStage _stage = _VoiceStage.preparing;
  double _amplitude = 0;
  double _progress = 0;
  String? _error;
  String _transcript = '';
  Offset? _pressOrigin;
  DateTime? _pressStartedAt;
  Future<void>? _startFuture;
  bool _releaseRequestedBeforeStart = false;
  bool _cancelRequestedBeforeStart = false;
  bool _sent = false;
  static const _historyLength = 60;
  List<double> _amplitudeHistory = const <double>[];

  @override
  void initState() {
    super.initState();
    _amplitudeSubscription = widget.service.amplitudes.listen((value) {
      if (!mounted) return;
      final clamped = value.clamp(0.0, 1.0);
      final next = [..._amplitudeHistory, clamped];
      if (next.length > _historyLength) {
        next.removeRange(0, next.length - _historyLength);
      }
      setState(() {
        _amplitude = clamped;
        _amplitudeHistory = next;
      });
    });
    _textController.addListener(() {
      if (mounted) setState(() {});
    });
    unawaited(_prepare());
  }

  @override
  void dispose() {
    _amplitudeSubscription?.cancel();
    final startFuture = _startFuture;
    if (startFuture != null) {
      unawaited(
        startFuture.then((_) => widget.service.cancel()).catchError((_) {}),
      );
    }
    _textController.dispose();
    super.dispose();
  }

  Future<void> _prepare() async {
    try {
      await widget.service.prepare(
        onProgress: (value) {
          if (!mounted) return;
          setState(() => _progress = value);
        },
      );
      if (!mounted) return;
      setState(() {
        _stage = _VoiceStage.ready;
        _progress = 1;
      });
    } on VoiceRecognitionException catch (error) {
      if (!mounted) return;
      _showError(_voiceErrorText(error.code));
    } catch (error) {
      if (!mounted) return;
      _showError(error.toString());
    }
  }

  void _showError(String message) {
    setState(() {
      _stage = _VoiceStage.error;
      _error = message;
    });
  }

  String _voiceErrorText(String code) {
    final prefs = AppPreferences.of(context);
    return switch (code) {
      'microphonePermissionDenied' => prefs.t('voice.permissionDenied'),
      'voiceModelDownloadFailed' => prefs.t('voice.downloadFailed'),
      'voiceModelInvalid' => prefs.t('voice.modelInvalid'),
      _ => prefs.t('voice.failed', params: {'reason': code}),
    };
  }

  bool get _canPressMic =>
      _stage == _VoiceStage.ready ||
      _stage == _VoiceStage.reviewing ||
      _stage == _VoiceStage.error;

  Future<void> _onPressDown(PointerDownEvent event) async {
    if (!_canPressMic) return;
    _pressOrigin = event.position;
    _pressStartedAt = DateTime.now();
    _releaseRequestedBeforeStart = false;
    _cancelRequestedBeforeStart = false;
    setState(() {
      _stage = _VoiceStage.starting;
      _error = null;
    });
    unawaited(HapticFeedback.lightImpact());
    final startFuture = widget.service.start();
    _startFuture = startFuture;
    try {
      await startFuture;
      if (!mounted || _startFuture != startFuture) return;
      if (_cancelRequestedBeforeStart) {
        await widget.service.cancel();
        if (!mounted) return;
        _resetPressState();
        setState(() => _stage = _VoiceStage.ready);
        return;
      }
      if (_releaseRequestedBeforeStart) {
        final startedAt = _pressStartedAt;
        final tooShort =
            startedAt != null &&
            DateTime.now().difference(startedAt).inMilliseconds <
                _minRecordingMs;
        if (tooShort) {
          await widget.service.cancel();
          if (!mounted) return;
          final prefs = AppPreferences.of(context);
          _resetPressState();
          setState(() {
            _stage = _VoiceStage.ready;
            _error = prefs.t('voice.tooShort');
          });
          return;
        }
        setState(() => _stage = _VoiceStage.recognizing);
        final text = (await widget.service.stopAndRecognize()).trim();
        if (!mounted || _startFuture != startFuture) return;
        _resetPressState();
        setState(() {
          _stage = _VoiceStage.reviewing;
          _transcript = text;
          _textController.text = text;
        });
        return;
      }
      if (!mounted) return;
      _startFuture = null;
      setState(() => _stage = _VoiceStage.recording);
    } on VoiceRecognitionException catch (error) {
      if (_startFuture != startFuture) return;
      if (!mounted) return;
      _showError(_voiceErrorText(error.code));
    } catch (error) {
      if (_startFuture != startFuture) return;
      if (!mounted) return;
      _showError(error.toString());
    }
  }

  void _onPressMove(PointerMoveEvent event) {
    if (_stage != _VoiceStage.starting &&
        _stage != _VoiceStage.recording &&
        _stage != _VoiceStage.cancelling) {
      return;
    }
    final origin = _pressOrigin;
    if (origin == null) return;
    final dy = origin.dy - event.position.dy;
    if (_stage == _VoiceStage.starting) {
      _cancelRequestedBeforeStart = dy > _cancelDistance;
      _releaseRequestedBeforeStart = !_cancelRequestedBeforeStart;
      return;
    }
    if (dy > _cancelDistance && _stage != _VoiceStage.cancelling) {
      setState(() => _stage = _VoiceStage.cancelling);
      unawaited(HapticFeedback.selectionClick());
    } else if (dy <= _cancelDistance && _stage == _VoiceStage.cancelling) {
      setState(() => _stage = _VoiceStage.recording);
    }
  }

  Future<void> _finishPress({required bool cancelOverride}) async {
    final stage = _stage;
    _pressOrigin = null;
    if (stage == _VoiceStage.starting) {
      _cancelRequestedBeforeStart = cancelOverride;
      _releaseRequestedBeforeStart = !cancelOverride;
      return;
    }
    final startedAt = _pressStartedAt;
    _pressStartedAt = null;

    if (stage != _VoiceStage.recording && stage != _VoiceStage.cancelling) {
      return;
    }

    final cancelled = cancelOverride || stage == _VoiceStage.cancelling;
    final tooShort =
        !cancelled &&
        startedAt != null &&
        DateTime.now().difference(startedAt).inMilliseconds < _minRecordingMs;

    if (cancelled || tooShort) {
      await widget.service.cancel();
      if (!mounted) return;
      final prefs = AppPreferences.of(context);
      setState(() {
        _stage = _VoiceStage.ready;
        _error = tooShort ? prefs.t('voice.tooShort') : null;
      });
      return;
    }

    setState(() => _stage = _VoiceStage.recognizing);
    try {
      final text = (await widget.service.stopAndRecognize()).trim();
      if (!mounted) return;
      setState(() {
        _stage = _VoiceStage.reviewing;
        _transcript = text;
        _textController.text = text;
      });
    } catch (error) {
      if (!mounted) return;
      _showError(error.toString());
    }
  }

  Future<void> _close() async {
    _cancelRequestedBeforeStart = true;
    _releaseRequestedBeforeStart = false;
    final startFuture = _startFuture;
    if (startFuture != null) {
      try {
        await startFuture;
      } catch (_) {
        // Ignore startup errors here and fall through to cancel.
      }
    }
    await widget.service.cancel();
    if (!mounted) return;
    widget.onClose();
  }

  void _resetPressState() {
    _pressOrigin = null;
    _pressStartedAt = null;
    _startFuture = null;
    _releaseRequestedBeforeStart = false;
    _cancelRequestedBeforeStart = false;
  }

  void _send() {
    if (_sent || _stage != _VoiceStage.reviewing) return;
    final text = _textController.text.trim();
    if (text.isEmpty) return;
    _sent = true;
    setState(() => _stage = _VoiceStage.sending);
    widget.onSend(text);
  }

  String _percent(double value) =>
      (value * 100).clamp(0, 100).round().toString();

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final accent = Theme.of(context).colorScheme.secondary;

    return Stack(
      fit: StackFit.expand,
      children: [
        GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: _close,
          child: ColoredBox(color: AppColors.backdrop),
        ),
        Positioned(
          left: 0,
          right: 0,
          bottom: 0,
          child: Material(
            color: Colors.transparent,
            child: GestureDetector(
              onTap: () {},
              behavior: HitTestBehavior.opaque,
              child: Container(
                margin: EdgeInsets.only(bottom: widget.bottomInset),
                padding: const EdgeInsets.fromLTRB(
                  AppSpacing.l,
                  AppSpacing.s,
                  AppSpacing.l,
                  AppSpacing.l,
                ),
                decoration: BoxDecoration(
                  color: AppColors.bgSurface,
                  borderRadius: const BorderRadius.vertical(
                    top: Radius.circular(AppRadius.lg),
                  ),
                  border: Border(
                    top: BorderSide(color: AppColors.border, width: 0.5),
                  ),
                  boxShadow: const [
                    BoxShadow(
                      color: Color(0x66000000),
                      blurRadius: 32,
                      offset: Offset(0, -8),
                    ),
                  ],
                ),
                child: SafeArea(
                  top: false,
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      const SizedBox(height: AppSpacing.xs),
                      _buildPreview(prefs, accent),
                      const SizedBox(height: AppSpacing.l),
                      _buildBigMicButton(accent),
                      const SizedBox(height: AppSpacing.s),
                      _buildHint(prefs),
                      const SizedBox(height: AppSpacing.l),
                      _buildBottomActions(prefs, accent),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildPreview(AppPreferences prefs, Color accent) {
    if (_stage == _VoiceStage.reviewing || _stage == _VoiceStage.sending) {
      return Container(
        padding: const EdgeInsets.all(AppSpacing.m),
        decoration: BoxDecoration(
          color: AppColors.bgElevated,
          borderRadius: BorderRadius.circular(AppRadius.md),
        ),
        child: TextField(
          controller: _textController,
          enabled: _stage == _VoiceStage.reviewing,
          minLines: 2,
          maxLines: 4,
          style: TextStyle(
            color: AppColors.textPrimary,
            fontSize: AppTextSize.body,
          ),
          decoration: InputDecoration(
            hintText: prefs.t('voice.resultHint'),
            hintStyle: TextStyle(color: AppColors.textSubtle),
            border: InputBorder.none,
            isDense: true,
          ),
        ),
      );
    }
    return Container(
      height: 96,
      decoration: BoxDecoration(
        color: AppColors.bgElevated,
        borderRadius: BorderRadius.circular(AppRadius.md),
      ),
      padding: const EdgeInsets.symmetric(
        horizontal: AppSpacing.m,
        vertical: AppSpacing.s,
      ),
      child: switch (_stage) {
        _VoiceStage.preparing => _buildProgress(accent),
        _VoiceStage.recognizing => Center(
          child: Text(
            prefs.t('voice.recognizing'),
            style: TextStyle(
              color: AppColors.textMuted,
              fontSize: AppTextSize.body,
            ),
          ),
        ),
        _ => _buildWaveform(accent),
      },
    );
  }

  Widget _buildProgress(Color accent) {
    return Center(
      child: SizedBox(
        width: 200,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(999),
          child: LinearProgressIndicator(
            value: _progress == 0 ? null : _progress,
            minHeight: 4,
            color: accent,
            backgroundColor: AppColors.bgBase,
          ),
        ),
      ),
    );
  }

  Widget _buildWaveform(Color accent) {
    final isActive =
        _stage == _VoiceStage.starting ||
        _stage == _VoiceStage.recording ||
        _stage == _VoiceStage.cancelling;
    final color = _stage == _VoiceStage.cancelling ? AppColors.danger : accent;
    return CustomPaint(
      size: Size.infinite,
      painter: _WaveformPainter(
        history: _amplitudeHistory,
        color: color,
        active: isActive,
      ),
    );
  }

  Widget _buildBigMicButton(Color accent) {
    final pressing =
        _stage == _VoiceStage.starting ||
        _stage == _VoiceStage.recording ||
        _stage == _VoiceStage.cancelling;
    final cancelling = _stage == _VoiceStage.cancelling;
    final color = cancelling
        ? AppColors.danger
        : _canPressMic
        ? accent
        : accent.withValues(alpha: 0.32);
    final size = pressing ? 96.0 : 84.0;
    final pulseSize = pressing ? size + (_amplitude * 28) : size;

    return Center(
      child: Listener(
        key: const ValueKey('voice_input_mic_button'),
        behavior: HitTestBehavior.opaque,
        onPointerDown: (event) => unawaited(_onPressDown(event)),
        onPointerMove: _onPressMove,
        onPointerUp: (_) => unawaited(_finishPress(cancelOverride: false)),
        onPointerCancel: (_) => unawaited(_finishPress(cancelOverride: true)),
        child: SizedBox(
          width: 132,
          height: 132,
          child: Stack(
            alignment: Alignment.center,
            children: [
              if (pressing)
                AnimatedContainer(
                  duration: const Duration(milliseconds: 120),
                  curve: Curves.easeOutCubic,
                  width: pulseSize,
                  height: pulseSize,
                  decoration: BoxDecoration(
                    color: color.withValues(alpha: 0.18),
                    shape: BoxShape.circle,
                  ),
                ),
              AnimatedContainer(
                duration: const Duration(milliseconds: 120),
                curve: Curves.easeOutCubic,
                width: size,
                height: size,
                decoration: BoxDecoration(
                  color: color,
                  shape: BoxShape.circle,
                  boxShadow: pressing
                      ? [
                          BoxShadow(
                            color: color.withValues(alpha: 0.4),
                            blurRadius: 20,
                            offset: const Offset(0, 6),
                          ),
                        ]
                      : const [],
                ),
                child: Icon(
                  cancelling ? Icons.close_rounded : Icons.mic_rounded,
                  color: AppColors.bgBase,
                  size: 32,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildHint(AppPreferences prefs) {
    final text = switch (_stage) {
      _VoiceStage.preparing =>
        _progress > 0 && _progress < 1
            ? '${prefs.t('voice.downloading')} ${_percent(_progress)}%'
            : prefs.t('voice.preparing'),
      _VoiceStage.starting => prefs.t('voice.starting'),
      _VoiceStage.ready => _error ?? prefs.t('voice.holdToTalk'),
      _VoiceStage.recording => prefs.t('voice.releaseToFinish'),
      _VoiceStage.cancelling => prefs.t('voice.releaseToCancel'),
      _VoiceStage.recognizing => prefs.t('voice.recognizing'),
      _VoiceStage.reviewing =>
        _transcript.isEmpty
            ? prefs.t('voice.empty')
            : prefs.t('voice.tapToReRecord'),
      _VoiceStage.sending => '',
      _VoiceStage.error => _error ?? '',
    };
    final isError =
        _stage == _VoiceStage.error ||
        (_stage == _VoiceStage.ready && _error != null) ||
        (_stage == _VoiceStage.reviewing && _transcript.isEmpty);
    return SizedBox(
      height: 18,
      child: Center(
        child: Text(
          text,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(
            color: isError ? AppColors.danger : AppColors.textMuted,
            fontSize: AppTextSize.small,
          ),
        ),
      ),
    );
  }

  Widget _buildBottomActions(AppPreferences prefs, Color accent) {
    final canSend =
        _stage == _VoiceStage.reviewing &&
        _textController.text.trim().isNotEmpty;
    return Row(
      children: [
        Expanded(
          child: SizedBox(
            height: 44,
            child: OutlinedButton(
              onPressed: _stage == _VoiceStage.recognizing ? null : _close,
              style: OutlinedButton.styleFrom(
                foregroundColor: AppColors.textPrimary,
                backgroundColor: AppColors.bgElevated,
                side: BorderSide.none,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(AppRadius.md),
                ),
              ),
              child: Text(prefs.t('voice.cancel')),
            ),
          ),
        ),
        const SizedBox(width: AppSpacing.s),
        Expanded(
          child: SizedBox(
            height: 44,
            child: FilledButton(
              onPressed: canSend ? _send : null,
              style: FilledButton.styleFrom(
                backgroundColor: accent,
                foregroundColor: AppColors.bgBase,
                disabledBackgroundColor: accent.withValues(alpha: 0.32),
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(AppRadius.md),
                ),
              ),
              child: Text(
                prefs.t('voice.send'),
                style: const TextStyle(fontWeight: FontWeight.w700),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _WaveformPainter extends CustomPainter {
  _WaveformPainter({
    required this.history,
    required this.color,
    required this.active,
  });

  final List<double> history;
  final Color color;
  final bool active;

  @override
  void paint(Canvas canvas, Size size) {
    final mid = size.height / 2;
    final amp = math.max(0.0, size.height / 2 - 4);

    if (!active || history.length < 2) {
      final idle = Paint()
        ..shader = LinearGradient(
          colors: [
            color.withValues(alpha: 0),
            color.withValues(alpha: 0.45),
            color.withValues(alpha: 0),
          ],
        ).createShader(Rect.fromLTWH(0, mid - 1, size.width, 2))
        ..strokeWidth = 1.5
        ..strokeCap = StrokeCap.round;
      canvas.drawLine(Offset(0, mid), Offset(size.width, mid), idle);
      return;
    }

    final step = size.width / (history.length - 1);
    final top = Path();
    final bottom = Path();
    final fill = Path();
    for (var i = 0; i < history.length; i++) {
      final v = history[i].clamp(0.0, 1.0);
      final x = i * step;
      final dy = v * amp;
      if (i == 0) {
        top.moveTo(x, mid - dy);
        bottom.moveTo(x, mid + dy);
        fill.moveTo(x, mid - dy);
      } else {
        top.lineTo(x, mid - dy);
        bottom.lineTo(x, mid + dy);
        fill.lineTo(x, mid - dy);
      }
    }
    for (var i = history.length - 1; i >= 0; i--) {
      final v = history[i].clamp(0.0, 1.0);
      final x = i * step;
      fill.lineTo(x, mid + v * amp);
    }
    fill.close();

    final fadeRect = Rect.fromLTWH(0, 0, size.width, size.height);
    canvas.drawPath(
      fill,
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.centerLeft,
          end: Alignment.centerRight,
          colors: [
            color.withValues(alpha: 0),
            color.withValues(alpha: 0.28),
            color.withValues(alpha: 0.28),
            color.withValues(alpha: 0),
          ],
          stops: const [0, 0.25, 0.85, 1],
        ).createShader(fadeRect),
    );

    final glow = Paint()
      ..color = color.withValues(alpha: 0.55)
      ..strokeWidth = 4
      ..style = PaintingStyle.stroke
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 4);
    canvas.drawPath(top, glow);
    canvas.drawPath(bottom, glow);

    final stroke = Paint()
      ..color = color.withValues(alpha: 0.95)
      ..strokeWidth = 1.5
      ..style = PaintingStyle.stroke
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    canvas.drawPath(top, stroke);
    canvas.drawPath(bottom, stroke);

    final last = history.last.clamp(0.0, 1.0);
    final lastX = (history.length - 1) * step;
    final lastDy = last * amp;
    final radius = 2 + last * 3;
    final dotGlow = Paint()
      ..color = color.withValues(alpha: 0.6)
      ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 6);
    canvas.drawCircle(Offset(lastX, mid - lastDy), radius * 1.6, dotGlow);
    canvas.drawCircle(Offset(lastX, mid + lastDy), radius * 1.6, dotGlow);
    final dot = Paint()..color = color;
    canvas.drawCircle(Offset(lastX, mid - lastDy), radius, dot);
    canvas.drawCircle(Offset(lastX, mid + lastDy), radius, dot);
  }

  @override
  bool shouldRepaint(_WaveformPainter old) =>
      old.active != active ||
      old.color != color ||
      !identical(old.history, history);
}
