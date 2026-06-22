import 'package:codux_flutter/screens/scanner_screen.dart';
import 'package:codux_flutter/i18n.dart';
import 'package:codux_flutter/theme/app_theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('manual pairing accepts pasted iroh token payload', (
    tester,
  ) async {
    String? payload;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildAppTheme(),
        home: Scaffold(
          body: AppPreferences(
            accent: AccentChoices.cyan,
            locale: LocaleChoices.simplifiedChinese,
            themeMode: ThemeMode.dark,
            child: Stack(
              children: [
                ScannerScreen(
                  bottomInset: 0,
                  onDetected: (value) => payload = value,
                  onClose: () {},
                  scannerBuilder: (_) => const ColoredBox(color: Colors.black),
                ),
              ],
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('手动连接'));
    await tester.pumpAndSettle();

    final tokenField = find.widgetWithText(TextField, '粘贴电脑端显示的配对 Token');
    expect(tokenField, findsOneWidget);

    await tester.enterText(tokenField, 'iroh-ticket-token');
    await tester.pump();
    await tester.tap(find.text('配对'));
    await tester.pump();

    expect(payload, 'codux://pair?payload=iroh-ticket-token');
  });

  testWidgets('manual pairing submits full codux pair link unchanged', (
    tester,
  ) async {
    String? payload;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildAppTheme(),
        home: Scaffold(
          body: AppPreferences(
            accent: AccentChoices.cyan,
            locale: LocaleChoices.simplifiedChinese,
            themeMode: ThemeMode.dark,
            child: Stack(
              children: [
                ScannerScreen(
                  bottomInset: 0,
                  onDetected: (value) => payload = value,
                  onClose: () {},
                  scannerBuilder: (_) => const ColoredBox(color: Colors.black),
                ),
              ],
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('手动连接'));
    await tester.pumpAndSettle();

    const pairLink = 'codux://pair?payload=embedded-iroh-ticket';
    await tester.enterText(
      find.widgetWithText(TextField, '粘贴电脑端显示的配对 Token'),
      pairLink,
    );
    await tester.pump();
    await tester.tap(find.text('配对'));
    await tester.pump();

    expect(payload, pairLink);
  });
}
