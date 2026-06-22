import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/theme/app_theme.dart';
import 'package:codux_flutter/widgets/components/device_action_dialogs.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const device = StoredDevice(
    server: 'https://relay.example',
    hostId: 'host-1',
    deviceId: 'device-1',
    token: 'token',
    name: 'Mac',
  );

  testWidgets('returns edited device name', (tester) async {
    StoredDevice? edited;
    await tester.pumpWidget(
      _wrap(
        Builder(
          builder: (context) => TextButton(
            onPressed: () async {
              edited = await showDialog<StoredDevice>(
                context: context,
                builder: (_) => const DeviceEditDialog(
                  device: device,
                  title: 'Edit',
                  nameLabel: 'Name',
                  cancelLabel: 'Cancel',
                  saveLabel: 'Save',
                ),
              );
            },
            child: const Text('Open'),
          ),
        ),
      ),
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'Studio Mac');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();

    expect(edited?.hostName, 'Studio Mac');
  });

  testWidgets('confirms device removal', (tester) async {
    bool? confirmed;
    await tester.pumpWidget(
      _wrap(
        Builder(
          builder: (context) => TextButton(
            onPressed: () async {
              confirmed = await showDialog<bool>(
                context: context,
                builder: (_) => const DeviceRemoveDialog(
                  title: 'Remove',
                  message: 'Remove Mac?',
                  cancelLabel: 'Cancel',
                  removeLabel: 'Remove',
                ),
              );
            },
            child: const Text('Open'),
          ),
        ),
      ),
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Remove').last);
    await tester.pumpAndSettle();

    expect(confirmed, isTrue);
  });
}

Widget _wrap(Widget child) {
  return MaterialApp(
    theme: buildAppTheme(),
    home: Scaffold(body: child),
  );
}
