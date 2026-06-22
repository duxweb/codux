import 'dart:async';

import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/services/local_voice_recognition_service.dart';
import 'package:codux_flutter/theme/app_theme.dart';
import 'package:codux_flutter/widgets/components/codux_home_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('shows device home when no secondary page is active', (
    tester,
  ) async {
    final controller = AnimationController(vsync: tester);
    addTearDown(controller.dispose);

    await tester.pumpWidget(_wrap(_shell(controller: controller)));

    expect(find.text('Devices'), findsOneWidget);
    expect(find.text('Workspace'), findsNothing);
  });

  testWidgets('shows settings page over device home', (tester) async {
    final controller = AnimationController(vsync: tester);
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      _wrap(
        _shell(
          controller: controller,
          state: const CoduxHomeShellState(
            showSettings: true,
            showTerminal: false,
            showTerminalSwitcher: false,
          ),
        ),
      ),
    );

    expect(find.text('Devices'), findsOneWidget);
    expect(find.text('Settings'), findsOneWidget);
  });

  testWidgets('shows workspace page and toast overlay', (tester) async {
    final controller = AnimationController(vsync: tester);
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      _wrap(
        _shell(
          controller: controller,
          state: const CoduxHomeShellState(
            showSettings: false,
            showTerminal: true,
            showTerminalSwitcher: false,
          ),
          overlays: _overlayState(toastMessage: 'Saved'),
        ),
      ),
    );

    expect(find.text('Devices'), findsOneWidget);
    expect(find.text('Workspace'), findsOneWidget);
    expect(find.text('Saved'), findsOneWidget);
  });
}

Widget _wrap(Widget child) {
  return MaterialApp(theme: buildAppTheme(), home: child);
}

CoduxHomeShell _shell({
  required AnimationController controller,
  CoduxHomeShellState state = const CoduxHomeShellState(
    showSettings: false,
    showTerminal: false,
    showTerminalSwitcher: false,
  ),
  CoduxHomeOverlayState? overlays,
}) {
  return CoduxHomeShell(
    metrics: CoduxHomeShellMetrics(
      topInset: 0,
      bottomInset: 0,
      leftInset: 0,
      edgeBackAnimation: controller,
    ),
    pages: const CoduxHomeShellPages(
      deviceHome: Center(child: Text('Devices')),
      settingsPage: Center(child: Text('Settings')),
      switcherPage: Center(child: Text('Switcher')),
      workspacePage: Center(child: Text('Workspace')),
    ),
    state: state,
    overlays: overlays ?? _overlayState(),
    actions: CoduxHomeShellActions(
      onBack: () {},
      onEdgeDragStart: (_) {},
      onEdgeDragUpdate: (_) {},
      onEdgeDragEnd: (_) {},
      onEdgeDragCancel: () {},
      onScannerDetected: (_) {},
      onCloseScanner: () {},
      onCancelPairing: () {},
      onConfirmPairing: () {},
      onCloseProjectForm: () {},
      onChooseProjectPath: () {},
      onSaveProjectForm: () {},
      onCloseFilePicker: () {},
      onOpenFilePickerPath: (_) {},
      onSelectFilePickerEntry: (_) {},
      onOpenFilePickerHome: () {},
      onOpenFilePickerRoot: () {},
      onOpenFilePickerVolumes: () {},
      onCloseVoice: () {},
      onSendVoiceText: (_) {},
      onCloseFileEditor: () {},
      onEditFile: () {},
      onSaveFile: () {},
    ),
  );
}

CoduxHomeOverlayState _overlayState({String? toastMessage}) {
  return CoduxHomeOverlayState(
    showScanner: false,
    pendingPairing: null,
    pairingInFlight: false,
    pairingError: null,
    showProjectForm: false,
    projectFormTitle: 'Project',
    projectNameController: TextEditingController(),
    projectPathController: TextEditingController(),
    showFilePicker: false,
    filePickerTitle: 'Files',
    filePickerPath: '',
    filePickerParent: null,
    filePickerEntries: const <RemoteFileEntry>[],
    filePickerLoading: false,
    showVoiceOverlay: false,
    voiceService: const _FakeVoiceRecognitionService(),
    editingFilePath: null,
    fileEditorController: TextEditingController(),
    fileEditorLoading: false,
    fileEditorSaving: false,
    fileEditorEditing: false,
    fileEditorEditable: true,
    blockingLoadingMessage: null,
    toastMessage: toastMessage,
  );
}

class _FakeVoiceRecognitionService implements VoiceRecognitionService {
  const _FakeVoiceRecognitionService();

  @override
  Stream<double> get amplitudes => const Stream<double>.empty();

  @override
  Future<void> cancel() async {}

  @override
  Future<void> prepare({VoiceProgress? onProgress}) async {}

  @override
  Future<void> start({VoiceProgress? onProgress}) async {}

  @override
  Future<String> stopAndRecognize() async => '';
}
