import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/services/mobile_settings_controller.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const controller = MobileSettingsController();

  test('detects device name using platform data priority', () {
    expect(
      controller.detectedNameFromDeviceInfo({
        'name': '',
        'model': 'Pixel',
        'product': 'Ignored',
      }),
      'Pixel',
    );
    expect(controller.detectedNameFromDeviceInfo({}), 'Codux Mobile');
  });

  test('uses stored settings when available', () {
    const stored = MobileSettings(localName: 'Phone', accentId: 'green');

    final settings = controller.startupSettings(
      stored: stored,
      detectedDeviceName: 'Detected',
    );

    expect(settings, stored);
  });

  test('creates default settings from detected device name', () {
    final settings = controller.startupSettings(
      stored: null,
      detectedDeviceName: 'iPhone',
    );

    expect(settings.localName, 'iPhone');
    expect(settings.appTextScale, 1);
    expect(settings.terminalFontSize, 12);
  });

  test('uses smaller terminal text by default', () {
    final settings = MobileSettings.fromJson({'localName': 'Phone'});

    expect(settings.appTextScale, MobileSettings.defaultAppTextScale);
    expect(MobileSettings.appTextScaleSteps, [0.875, 1.0, 1.125]);
    expect(settings.terminalFontSize, MobileSettings.defaultTerminalFontSize);
    expect(MobileSettings.defaultTerminalFontSize, 12);
    expect(MobileSettings.standardTerminalFontSize, 14);
  });

  test('normalizes local name when saving settings', () {
    const current = MobileSettings(localName: 'Old', logLevel: 'debug');

    final named = controller.saveSettings(
      current: current,
      inputLocalName: '  Studio Phone  ',
      detectedDeviceName: 'Detected',
    );
    expect(named.localName, 'Studio Phone');
    expect(named.logLevel, 'debug');

    final fallback = controller.saveSettings(
      current: current,
      inputLocalName: '   ',
      detectedDeviceName: 'Detected',
    );
    expect(fallback.localName, 'Detected');
  });
}
