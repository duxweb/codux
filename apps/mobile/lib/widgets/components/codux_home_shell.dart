import 'dart:io';

import 'package:flutter/cupertino.dart' as cupertino;
import 'package:flutter/material.dart';

import '../../models/remote_models.dart';
import '../../screens/scanner_screen.dart';
import '../../services/local_voice_recognition_service.dart';
import 'app_toast.dart';
import 'pairing_overlay.dart';
import 'project_files_panel.dart';
import 'project_form_overlay.dart';
import 'remote_file_picker.dart';
import 'voice_input_overlay.dart';

class CoduxHomeShell extends StatelessWidget {
  const CoduxHomeShell({
    super.key,
    required this.metrics,
    required this.pages,
    required this.state,
    required this.overlays,
    required this.actions,
  });

  final CoduxHomeShellMetrics metrics;
  final CoduxHomeShellPages pages;
  final CoduxHomeShellState state;
  final CoduxHomeOverlayState overlays;
  final CoduxHomeShellActions actions;

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, result) {
        if (!didPop) actions.onBack();
      },
      child: Scaffold(
        resizeToAvoidBottomInset: false,
        body: Stack(
          children: [
            _body(),
            if (_edgeBackEnabled)
              Positioned(
                left: 0,
                top: 0,
                bottom: 0,
                width: metrics.leftInset + 36,
                child: GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onHorizontalDragStart: actions.onEdgeDragStart,
                  onHorizontalDragUpdate: actions.onEdgeDragUpdate,
                  onHorizontalDragEnd: actions.onEdgeDragEnd,
                  onHorizontalDragCancel: actions.onEdgeDragCancel,
                ),
              ),
            if (overlays.showScanner)
              ScannerScreen(
                bottomInset: metrics.bottomInset,
                onDetected: actions.onScannerDetected,
                onClose: actions.onCloseScanner,
              ),
            if (overlays.pendingPairing != null)
              PairingOverlay(
                payload: overlays.pendingPairing!,
                waiting: overlays.pairingInFlight,
                errorMessage: overlays.pairingError,
                onCancel: actions.onCancelPairing,
                onConfirm: actions.onConfirmPairing,
              ),
            if (overlays.showProjectForm)
              ProjectFormOverlay(
                topInset: metrics.topInset,
                bottomInset: metrics.bottomInset,
                title: overlays.projectFormTitle,
                nameController: overlays.projectNameController,
                pathController: overlays.projectPathController,
                onClose: actions.onCloseProjectForm,
                onChoosePath: actions.onChooseProjectPath,
                onSave: actions.onSaveProjectForm,
              ),
            if (overlays.showFilePicker)
              RemoteFilePicker(
                topInset: metrics.topInset,
                bottomInset: metrics.bottomInset,
                title: overlays.filePickerTitle,
                path: overlays.filePickerPath,
                parent: overlays.filePickerParent,
                entries: overlays.filePickerEntries,
                loading: overlays.filePickerLoading,
                onClose: actions.onCloseFilePicker,
                onOpenPath: actions.onOpenFilePickerPath,
                onSelect: actions.onSelectFilePickerEntry,
                onOpenHome: actions.onOpenFilePickerHome,
                onOpenRoot: actions.onOpenFilePickerRoot,
                onOpenVolumes: actions.onOpenFilePickerVolumes,
              ),
            if (overlays.showVoiceOverlay)
              VoiceInputOverlay(
                topInset: metrics.topInset,
                bottomInset: metrics.bottomInset,
                service: overlays.voiceService,
                onClose: actions.onCloseVoice,
                onSend: actions.onSendVoiceText,
              ),
            if (overlays.editingFilePath != null)
              FileEditorOverlay(
                path: overlays.editingFilePath!,
                controller: overlays.fileEditorController,
                loading: overlays.fileEditorLoading,
                saving: overlays.fileEditorSaving,
                editing: overlays.fileEditorEditing,
                editable: overlays.fileEditorEditable,
                onClose: actions.onCloseFileEditor,
                onEdit: actions.onEditFile,
                onSave: actions.onSaveFile,
              ),
            if (overlays.blockingLoadingMessage != null)
              BlockingLoading(message: overlays.blockingLoadingMessage!),
            if (overlays.toastMessage != null)
              AppToast(
                message: overlays.toastMessage!,
                bottomInset: metrics.bottomInset,
              ),
          ],
        ),
      ),
    );
  }

  bool get _edgeBackEnabled =>
      Platform.isIOS &&
      (state.showTerminal || state.showSettings || state.showTerminalSwitcher);

  Widget _body() {
    if (state.showSettings) {
      return _CupertinoBackPage(
        animation: metrics.edgeBackAnimation,
        base: pages.deviceHome,
        page: pages.settingsPage,
      );
    }
    if (state.showTerminalSwitcher) {
      return _CupertinoBackPage(
        animation: metrics.edgeBackAnimation,
        base: pages.workspacePage,
        page: pages.switcherPage,
      );
    }
    if (!state.showTerminal) {
      return pages.deviceHome;
    }
    return _CupertinoBackPage(
      animation: metrics.edgeBackAnimation,
      base: pages.deviceHome,
      page: pages.workspacePage,
    );
  }
}

class CoduxHomeShellMetrics {
  const CoduxHomeShellMetrics({
    required this.topInset,
    required this.bottomInset,
    required this.leftInset,
    required this.edgeBackAnimation,
  });

  final double topInset;
  final double bottomInset;
  final double leftInset;
  final Animation<double> edgeBackAnimation;
}

class CoduxHomeShellPages {
  const CoduxHomeShellPages({
    required this.deviceHome,
    required this.settingsPage,
    required this.switcherPage,
    required this.workspacePage,
  });

  final Widget deviceHome;
  final Widget settingsPage;
  final Widget switcherPage;
  final Widget workspacePage;
}

class CoduxHomeShellState {
  const CoduxHomeShellState({
    required this.showSettings,
    required this.showTerminal,
    required this.showTerminalSwitcher,
  });

  final bool showSettings;
  final bool showTerminal;
  final bool showTerminalSwitcher;
}

class CoduxHomeOverlayState {
  const CoduxHomeOverlayState({
    required this.showScanner,
    required this.pendingPairing,
    required this.pairingInFlight,
    required this.pairingError,
    required this.showProjectForm,
    required this.projectFormTitle,
    required this.projectNameController,
    required this.projectPathController,
    required this.showFilePicker,
    required this.filePickerTitle,
    required this.filePickerPath,
    required this.filePickerParent,
    required this.filePickerEntries,
    required this.filePickerLoading,
    required this.showVoiceOverlay,
    required this.voiceService,
    required this.editingFilePath,
    required this.fileEditorController,
    required this.fileEditorLoading,
    required this.fileEditorSaving,
    required this.fileEditorEditing,
    required this.fileEditorEditable,
    required this.blockingLoadingMessage,
    required this.toastMessage,
  });

  final bool showScanner;
  final PairingPayload? pendingPairing;
  final bool pairingInFlight;
  final String? pairingError;
  final bool showProjectForm;
  final String projectFormTitle;
  final TextEditingController projectNameController;
  final TextEditingController projectPathController;
  final bool showFilePicker;
  final String filePickerTitle;
  final String filePickerPath;
  final String? filePickerParent;
  final List<RemoteFileEntry> filePickerEntries;
  final bool filePickerLoading;
  final bool showVoiceOverlay;
  final VoiceRecognitionService voiceService;
  final String? editingFilePath;
  final TextEditingController fileEditorController;
  final bool fileEditorLoading;
  final bool fileEditorSaving;
  final bool fileEditorEditing;
  final bool fileEditorEditable;
  final String? blockingLoadingMessage;
  final String? toastMessage;
}

class CoduxHomeShellActions {
  const CoduxHomeShellActions({
    required this.onBack,
    required this.onEdgeDragStart,
    required this.onEdgeDragUpdate,
    required this.onEdgeDragEnd,
    required this.onEdgeDragCancel,
    required this.onScannerDetected,
    required this.onCloseScanner,
    required this.onCancelPairing,
    required this.onConfirmPairing,
    required this.onCloseProjectForm,
    required this.onChooseProjectPath,
    required this.onSaveProjectForm,
    required this.onCloseFilePicker,
    required this.onOpenFilePickerPath,
    required this.onSelectFilePickerEntry,
    required this.onOpenFilePickerHome,
    required this.onOpenFilePickerRoot,
    required this.onOpenFilePickerVolumes,
    required this.onCloseVoice,
    required this.onSendVoiceText,
    required this.onCloseFileEditor,
    required this.onEditFile,
    required this.onSaveFile,
  });

  final VoidCallback onBack;
  final GestureDragStartCallback onEdgeDragStart;
  final GestureDragUpdateCallback onEdgeDragUpdate;
  final GestureDragEndCallback onEdgeDragEnd;
  final GestureDragCancelCallback onEdgeDragCancel;
  final ValueChanged<String> onScannerDetected;
  final VoidCallback onCloseScanner;
  final VoidCallback onCancelPairing;
  final VoidCallback onConfirmPairing;
  final VoidCallback onCloseProjectForm;
  final VoidCallback onChooseProjectPath;
  final VoidCallback onSaveProjectForm;
  final VoidCallback onCloseFilePicker;
  final ValueChanged<String> onOpenFilePickerPath;
  final ValueChanged<RemoteFileEntry> onSelectFilePickerEntry;
  final VoidCallback onOpenFilePickerHome;
  final VoidCallback onOpenFilePickerRoot;
  final VoidCallback onOpenFilePickerVolumes;
  final VoidCallback onCloseVoice;
  final ValueChanged<String> onSendVoiceText;
  final VoidCallback onCloseFileEditor;
  final VoidCallback onEditFile;
  final VoidCallback onSaveFile;
}

class _CupertinoBackPage extends StatelessWidget {
  const _CupertinoBackPage({
    required this.animation,
    required this.base,
    required this.page,
  });

  final Animation<double> animation;
  final Widget base;
  final Widget page;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: animation,
      builder: (context, child) {
        return Stack(
          children: [
            base,
            cupertino.CupertinoPageTransition(
              primaryRouteAnimation: ReverseAnimation(animation),
              secondaryRouteAnimation: const AlwaysStoppedAnimation<double>(0),
              linearTransition: true,
              child: DecoratedBox(
                decoration: BoxDecoration(
                  boxShadow: [
                    BoxShadow(
                      color: Colors.black.withValues(alpha: 0.24),
                      blurRadius: 16,
                      offset: const Offset(-8, 0),
                    ),
                  ],
                ),
                child: page,
              ),
            ),
          ],
        );
      },
    );
  }
}
