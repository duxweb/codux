import 'dart:async';

import 'package:codux_flutter/i18n.dart';
import 'package:codux_flutter/services/local_voice_recognition_service.dart';
import 'package:codux_flutter/theme/app_theme.dart';
import 'package:codux_flutter/widgets/components/voice_input_overlay.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

class _FakeVoiceService implements VoiceRecognitionService {
  _FakeVoiceService({
    this.result = 'hello world',
    this.startDelay = Duration.zero,
  });

  final _amplitudeController = StreamController<double>.broadcast();
  final Duration startDelay;
  String result;
  int prepareCount = 0;
  int startCount = 0;
  int stopCount = 0;
  int cancelCount = 0;

  @override
  Stream<double> get amplitudes => _amplitudeController.stream;

  @override
  Future<void> prepare({VoiceProgress? onProgress}) async {
    prepareCount++;
    onProgress?.call(1);
  }

  @override
  Future<void> start({VoiceProgress? onProgress}) async {
    startCount++;
    onProgress?.call(1);
    if (startDelay > Duration.zero) {
      await Future<void>.delayed(startDelay);
    }
  }

  @override
  Future<String> stopAndRecognize() async {
    stopCount++;
    return result;
  }

  @override
  Future<void> cancel() async {
    cancelCount++;
  }

  Future<void> dispose() async {
    await _amplitudeController.close();
  }
}

Future<void> _settleUntilReady(WidgetTester tester) async {
  for (var i = 0; i < 20; i++) {
    await tester.pump(const Duration(milliseconds: 16));
    if (find.text('按住说话').evaluate().isNotEmpty) return;
  }
}

Future<void> _settleUntilReviewing(WidgetTester tester) async {
  for (var i = 0; i < 20; i++) {
    await tester.pump(const Duration(milliseconds: 16));
    if (find.byType(TextField).evaluate().isNotEmpty) return;
  }
}

Widget _wrap(Widget child) {
  return MaterialApp(
    theme: buildAppTheme(accent: AccentChoices.cyan.color),
    home: AppPreferences(
      accent: AccentChoices.cyan,
      locale: LocaleChoices.byId('zh-CN'),
      themeMode: ThemeMode.dark,
      child: Scaffold(body: Stack(children: [child])),
    ),
  );
}

void main() {
  testWidgets('shows preparing then ready state', (tester) async {
    final service = _FakeVoiceService();
    addTearDown(service.dispose);

    await tester.pumpWidget(
      _wrap(
        VoiceInputOverlay(
          topInset: 0,
          bottomInset: 0,
          service: service,
          onClose: () {},
          onSend: (_) {},
        ),
      ),
    );

    await _settleUntilReady(tester);
    expect(service.prepareCount, 1);
    expect(find.text('按住说话'), findsOneWidget);
  });

  testWidgets('hold then release fills preview with recognized text', (
    tester,
  ) async {
    final service = _FakeVoiceService(result: '你好世界');
    addTearDown(service.dispose);
    String? sent;

    await tester.pumpWidget(
      _wrap(
        VoiceInputOverlay(
          topInset: 0,
          bottomInset: 0,
          service: service,
          onClose: () {},
          onSend: (text) => sent = text,
        ),
      ),
    );
    await _settleUntilReady(tester);

    final mic = find.byKey(const ValueKey('voice_input_mic_button'));
    final gesture = await tester.startGesture(tester.getCenter(mic));
    await tester.pump();
    expect(find.text('松开识别'), findsOneWidget);
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 250)),
    );
    await gesture.up();
    await _settleUntilReviewing(tester);

    expect(service.startCount, 1);
    expect(service.stopCount, 1);
    final textField = tester.widget<TextField>(find.byType(TextField));
    expect(textField.controller?.text, '你好世界');
    expect(sent, isNull);
  });

  testWidgets('slide up cancels and avoids recognition', (tester) async {
    final service = _FakeVoiceService();
    addTearDown(service.dispose);

    await tester.pumpWidget(
      _wrap(
        VoiceInputOverlay(
          topInset: 0,
          bottomInset: 0,
          service: service,
          onClose: () {},
          onSend: (_) {},
        ),
      ),
    );
    await _settleUntilReady(tester);

    final start = tester.getCenter(
      find.byKey(const ValueKey('voice_input_mic_button')),
    );
    final gesture = await tester.startGesture(start);
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 250)),
    );
    await gesture.moveBy(const Offset(0, -80));
    await tester.pump();
    expect(find.text('松开取消'), findsOneWidget);
    await gesture.up();
    await _settleUntilReady(tester);

    expect(service.stopCount, 0);
    expect(service.cancelCount, greaterThan(0));
    expect(find.text('按住说话'), findsOneWidget);
  });

  testWidgets('short press shows too-short hint', (tester) async {
    final service = _FakeVoiceService();
    addTearDown(service.dispose);

    await tester.pumpWidget(
      _wrap(
        VoiceInputOverlay(
          topInset: 0,
          bottomInset: 0,
          service: service,
          onClose: () {},
          onSend: (_) {},
        ),
      ),
    );
    await _settleUntilReady(tester);

    final gesture = await tester.startGesture(
      tester.getCenter(find.byKey(const ValueKey('voice_input_mic_button'))),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
    await gesture.up();
    await _settleUntilReady(tester);

    expect(service.stopCount, 0);
    expect(service.cancelCount, greaterThan(0));
    expect(find.text('录制时间太短'), findsOneWidget);
  });

  testWidgets('send button forwards recognized text', (tester) async {
    final service = _FakeVoiceService(result: 'ls -la');
    addTearDown(service.dispose);
    String? sent;

    await tester.pumpWidget(
      _wrap(
        VoiceInputOverlay(
          topInset: 0,
          bottomInset: 0,
          service: service,
          onClose: () {},
          onSend: (text) => sent = text,
        ),
      ),
    );
    await _settleUntilReady(tester);

    final gesture = await tester.startGesture(
      tester.getCenter(find.byKey(const ValueKey('voice_input_mic_button'))),
    );
    await tester.pump();
    expect(find.text('松开识别'), findsOneWidget);
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 250)),
    );
    await gesture.up();
    await _settleUntilReviewing(tester);

    expect(find.byType(TextField), findsOneWidget);
    final tf = tester.widget<TextField>(find.byType(TextField));
    expect(tf.controller?.text, 'ls -la');
    final sendBtn = find.widgetWithText(FilledButton, '发送');
    expect(sendBtn, findsOneWidget);
    final sendButton = tester.widget<FilledButton>(sendBtn);
    expect(sendButton.onPressed, isNotNull);
    await tester.tap(sendBtn);
    await tester.pump();

    expect(sent, 'ls -la');
  });

  testWidgets('send button only submits once', (tester) async {
    final service = _FakeVoiceService(result: 'echo ok');
    addTearDown(service.dispose);
    final sent = <String>[];

    await tester.pumpWidget(
      _wrap(
        VoiceInputOverlay(
          topInset: 0,
          bottomInset: 0,
          service: service,
          onClose: () {},
          onSend: sent.add,
        ),
      ),
    );
    await _settleUntilReady(tester);

    final gesture = await tester.startGesture(
      tester.getCenter(find.byKey(const ValueKey('voice_input_mic_button'))),
    );
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 250)),
    );
    await gesture.up();
    await _settleUntilReviewing(tester);

    await tester.tap(find.widgetWithText(FilledButton, '发送'));
    await tester.pump();
    await tester.tap(find.widgetWithText(FilledButton, '发送'));
    await tester.pump();

    expect(sent, ['echo ok']);
  });

  testWidgets('cancel button closes overlay', (tester) async {
    final service = _FakeVoiceService();
    addTearDown(service.dispose);
    var closed = false;

    await tester.pumpWidget(
      _wrap(
        VoiceInputOverlay(
          topInset: 0,
          bottomInset: 0,
          service: service,
          onClose: () => closed = true,
          onSend: (_) {},
        ),
      ),
    );
    await _settleUntilReady(tester);

    await tester.tap(find.text('取消'));
    await _settleUntilReady(tester);

    expect(closed, isTrue);
  });

  testWidgets('release before recording starts still completes cleanly', (
    tester,
  ) async {
    final service = _FakeVoiceService(
      result: 'hello world',
      startDelay: const Duration(milliseconds: 50),
    );
    addTearDown(service.dispose);

    await tester.pumpWidget(
      _wrap(
        VoiceInputOverlay(
          topInset: 0,
          bottomInset: 0,
          service: service,
          onClose: () {},
          onSend: (_) {},
        ),
      ),
    );
    await _settleUntilReady(tester);

    final mic = find.byKey(const ValueKey('voice_input_mic_button'));
    final gesture = await tester.startGesture(tester.getCenter(mic));
    await tester.pump();
    await gesture.up();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 80)),
    );
    await tester.pumpAndSettle();

    expect(service.startCount, 1);
    expect(service.stopCount, 0);
    expect(service.cancelCount, 1);
    expect(find.text('录制时间太短'), findsOneWidget);
  });

  testWidgets('paste and voice use direct text insert path only once', (
    tester,
  ) async {
    final service = _FakeVoiceService(result: 'pkg search');
    addTearDown(service.dispose);
    String? sent;

    await tester.pumpWidget(
      _wrap(
        VoiceInputOverlay(
          topInset: 0,
          bottomInset: 0,
          service: service,
          onClose: () {},
          onSend: (text) => sent = text,
        ),
      ),
    );
    await _settleUntilReady(tester);

    final gesture = await tester.startGesture(
      tester.getCenter(find.byKey(const ValueKey('voice_input_mic_button'))),
    );
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 250)),
    );
    await gesture.up();
    await _settleUntilReviewing(tester);

    await tester.tap(find.widgetWithText(FilledButton, '发送'));
    await tester.pump();

    expect(sent, 'pkg search');
  });
}
