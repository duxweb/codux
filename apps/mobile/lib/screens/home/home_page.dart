import 'dart:async';
import 'dart:io';

import 'package:device_info_plus/device_info_plus.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:codux_protocol_ffi/codux_protocol_ffi.dart'
    as codux_terminal_core;
import 'package:package_info_plus/package_info_plus.dart';
import 'package:url_launcher/url_launcher.dart';
import 'package:uuid/uuid.dart';
import '../../i18n.dart';
import '../../models/remote_models.dart';
import '../../models/workspace_mode.dart';
import 'home_terminal_actions.dart';
import 'home_pending_worktree_switch.dart';
import 'home_worktree_actions.dart';
import 'home_workspace_mode_actions.dart';
import 'home_workspace_builder.dart';
import 'home_workspace_shell_data.dart';
import 'home_runtime_coordinator.dart';
import '../settings_screen.dart';
import '../../services/log_service.dart';
import '../../services/log_export_service.dart';
import '../../services/local_voice_recognition_service.dart';
import '../../services/mobile_settings_controller.dart';
import '../../services/connection_status_presenter.dart';
import '../../services/device_selection_service.dart';
import '../../services/remote_device_controller.dart';
import '../../services/remote_envelope_send_queue.dart';
import '../../services/remote_capabilities.dart';
import '../../services/remote_connection_sync_controller.dart';
import '../../services/terminal_repaint_signal.dart';
import '../../services/remote_project_controller.dart';
import '../../services/remote_network_route_refresh_controller.dart';
import '../../services/remote_protocol_service.dart';
import '../../services/remote_runtime_payloads.dart';
import '../../services/remote_runtime_store.dart';
import '../../services/remote_sequence_guard.dart';
import '../../services/remote_sync_state.dart';
import '../../services/remote_project_file_controller.dart';
import '../../services/remote_path_utils.dart';
import '../../services/remote_terminal_binding_coordinator.dart';
import '../../services/remote_terminal_output_controller.dart';
import '../../services/remote_terminal_scope.dart';
import '../../services/remote_transport.dart';
import '../../services/storage_service.dart';
import '../../services/terminal_buffer_retry.dart';
import '../../services/terminal_input_batcher.dart';
import '../../services/terminal_input_reliable_sender.dart';
import '../../services/terminal_upload_metadata.dart';
import '../../services/update_check_service.dart';
import '../../services/terminal_viewport_controller.dart';
import '../../services/remote_state_versions.dart';
import '../../theme/app_theme.dart';
import '../../services/worktree_utils.dart';
import '../../widgets/components/codux_home_shell.dart';
import '../../widgets/components/device_home_screen.dart';
import '../../widgets/components/project_files_panel.dart';
import '../../widgets/components/remote_terminal_pane.dart';
import '../../widgets/components/terminal_switcher_screen.dart';
import '../../widgets/components/worktree_action_dialog.dart';
import '../../widgets/components/terminal_upload_source_sheet.dart';
import '../../widgets/components/update_available_dialog.dart';
import '../../widgets/components/codux_about_dialog.dart';
import '../../widgets/components/debug_log_dialog.dart';
import '../../widgets/components/device_action_dialogs.dart';
import '../../widgets/components/file_action_dialogs.dart';

part 'state/protocol.dart';
part 'state/transport.dart';
part 'state/terminal.dart';
part 'state/connection.dart';
part 'state/bootstrap.dart';
part 'state/sync.dart';
part 'state/workspace.dart';
part 'state/actions.dart';
part 'home_controller.dart';

final String _remoteProtocolVersion = remoteProtocolVersion;
const Duration _remoteStartupProbeTimeout = Duration(seconds: 15);
const Duration _remoteLatencyProbeInterval = Duration(seconds: 3);
const Duration _remoteLatencyProbeTimeout = Duration(seconds: 8);
// Bind, focus and first layout can all request the same automatic claim.
const Duration _viewportClaimThrottle = Duration(seconds: 2);

class CoduxHomePage extends StatefulWidget {
  const CoduxHomePage({
    super.key,
    required this.onChangeAccent,
    required this.onChangeLocale,
    required this.onChangeThemeMode,
    this.initialDevices,
    this.transportFactory,
  });

  final ValueChanged<AccentOption> onChangeAccent;
  final ValueChanged<LocaleOption> onChangeLocale;
  final ValueChanged<ThemeMode> onChangeThemeMode;
  final List<StoredDevice>? initialDevices;
  final RemoteTransportFactory? transportFactory;

  @override
  State<CoduxHomePage> createState() => _CoduxHomePageState();
}

class _CoduxHomePageState extends State<CoduxHomePage>
    with TickerProviderStateMixin {
  late final HomeController c;

  @override
  void initState() {
    super.initState();
    c = HomeController(this);
    c.addListener(_onControllerChanged);
    c.init();
  }

  void _onControllerChanged() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    c.removeListener(_onControllerChanged);
    c.disposeController();
    c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final media = MediaQuery.of(context);
    final topInset = media.viewPadding.top;
    final bottomInset = media.viewPadding.bottom;
    final leftInset = media.viewPadding.left;
    final keyboardVisible = media.viewInsets.bottom > bottomInset + 8.0;
    if (c._keyboardVisible != keyboardVisible) {
      c._keyboardVisible = keyboardVisible;
      if (keyboardVisible) {
        c._keyboardShownSinceRequest = true;
      } else if (c._keyboardShownSinceRequest) {
        c._keyboardRequested = false;
        c._keyboardShownSinceRequest = false;
      }
    }

    final textScale = c._settings.appTextScale.clamp(0.75, 1.25);

    return MediaQuery(
      data: media.copyWith(textScaler: TextScaler.linear(textScale)),
      child: Builder(
        builder: (context) {
          final deviceHome = _buildDeviceHome(topInset, bottomInset);
          final settingsPage = _buildSettingsPage(topInset, bottomInset);
          final switcherPage = _buildTerminalSwitcherPage(
            topInset,
            bottomInset,
          );
          final workspacePage = _buildWorkspace(topInset, bottomInset);
          return CoduxHomeShell(
            metrics: CoduxHomeShellMetrics(
              topInset: topInset,
              bottomInset: bottomInset,
              leftInset: leftInset,
              edgeBackAnimation: c._edgeBackController,
            ),
            pages: CoduxHomeShellPages(
              deviceHome: deviceHome,
              settingsPage: settingsPage,
              switcherPage: switcherPage,
              workspacePage: workspacePage,
            ),
            state: CoduxHomeShellState(
              showSettings: c._showSettings,
              showTerminal: c._showTerminal,
              showTerminalSwitcher: c._showTerminalSwitcher,
            ),
            overlays: CoduxHomeOverlayState(
              showScanner: c._showScanner,
              pendingPairing: c._pendingPairing,
              pairingInFlight: c._pairingInFlight,
              pairingError: c._pairingError,
              showProjectForm: c._showProjectForm,
              projectFormTitle: c._projectFormMode == ProjectFormMode.edit
                  ? c._t('project.edit')
                  : c._t('project.add'),
              projectNameController: c._projectNameController,
              projectPathController: c._projectPathController,
              showFilePicker: c._showFilePicker,
              filePickerTitle: c._t('project.pathLabel'),
              filePickerPath: c._filePickerPath,
              filePickerParent: c._filePickerParent,
              filePickerEntries: c._filePickerEntries,
              filePickerLoading: c._filePickerLoading,
              showVoiceOverlay: c._showVoiceOverlay,
              voiceService: c._voiceService,
              // On the pad the editor renders inline in the center pane, so the
              // bottom-sheet overlay is suppressed there to avoid a double editor.
              editingFilePath: media.size.width >= _padLayoutMinWidth
                  ? null
                  : c._editingFilePath,
              fileEditorController: c._fileEditorController,
              fileEditorLoading: c._fileEditorLoading,
              fileEditorSaving: c._fileEditorSaving,
              fileEditorEditing: c._fileEditorEditing,
              fileEditorEditable: c._fileEditorEditable,
              blockingLoadingMessage: c._blockingLoadingMessage,
              toastMessage: c._toastMessage,
            ),
            actions: CoduxHomeShellActions(
              onBack: c._handleBackNavigation,
              onEdgeDragStart: c._handleWorkspaceEdgeDragStart,
              onEdgeDragUpdate: c._handleWorkspaceEdgeDragUpdate,
              onEdgeDragEnd: c._handleWorkspaceEdgeDragEnd,
              onEdgeDragCancel: c._cancelWorkspaceEdgeBack,
              onScannerDetected: c._handleScannedPayload,
              onCloseScanner: () => c._applyState(() => c._showScanner = false),
              onCancelPairing: c._cancelPairing,
              onConfirmPairing: c._confirmPairing,
              onCloseProjectForm: () =>
                  c._applyState(() => c._showProjectForm = false),
              onChooseProjectPath: c._chooseProjectFormPath,
              onSaveProjectForm: c._saveProjectForm,
              onCloseFilePicker: () {
                c._filePickerTimeoutTimer?.cancel();
                c._applyState(() => c._showFilePicker = false);
              },
              onOpenFilePickerPath: c._openRemoteFilePicker,
              onSelectFilePickerEntry: c._selectRemoteProjectFolder,
              onOpenFilePickerHome: () => c._openRemoteFilePicker(),
              onOpenFilePickerRoot: () => c._openRemoteFilePicker('/'),
              onOpenFilePickerVolumes: () =>
                  c._openRemoteFilePicker('/Volumes'),
              onCloseVoice: () =>
                  c._applyState(() => c._showVoiceOverlay = false),
              onSendVoiceText: (text) {
                c._insertTerminalText(text);
                c._applyState(() => c._showVoiceOverlay = false);
              },
              onCloseFileEditor: () =>
                  c._applyState(() => c._editingFilePath = null),
              onEditFile: c._beginEditingFile,
              onSaveFile: c._saveEditingFile,
            ),
          );
        },
      ),
    );
  }

  Widget _buildDeviceHome(double topInset, double bottomInset) {
    return DeviceHomeScreen(
      devices: c._devices,
      activeDeviceId: c._activeDevice?.deviceId,
      ready: c._isDeviceListConnected,
      status: c._deviceListStatusText,
      latencyMs: c._isConnected ? c._latencyMs : null,
      deviceSubtitle: c._deviceSubtitle,
      topInset: topInset,
      bottomInset: bottomInset,
      onOpen: c._openDeviceTerminal,
      onConnect: (device) => c._connect(device),
      onAdd: () => c._applyState(() => c._showScanner = true),
      onEdit: c._editDevice,
      onDelete: c._confirmRemoveDevice,
      onRefresh: c._refreshDeviceList,
      onSettings: () => c._pushCupertinoPage(() {
        c._showSettings = true;
      }),
      onLogs: c._showLogViewer,
      onCheckUpdate: c._checkUpdate,
      onAbout: c._showAboutDialogNow,
    );
  }

  Widget _buildSettingsPage(double topInset, double bottomInset) {
    final prefs = AppPreferences.of(context);
    return SettingsScreen(
      nameController: c._settingsNameController,
      detectedName: c._detectedDeviceName,
      topInset: topInset,
      bottomInset: bottomInset,
      currentAccent: prefs.accent,
      currentLocale: prefs.locale,
      currentThemeMode: prefs.themeMode,
      currentLogLevel: c._settings.logLevel,
      appTextScale: c._settings.appTextScale,
      terminalFontSize: c._settings.terminalFontSize,
      onChangeAccent: (next) {
        widget.onChangeAccent(next);
        c._applyState(
          () => c._settings = c._settings.copyWith(accentId: next.id),
        );
      },
      onChangeLocale: (next) {
        widget.onChangeLocale(next);
        c._applyState(
          () => c._settings = c._settings.copyWith(localeId: next.id),
        );
      },
      onChangeThemeMode: (next) {
        widget.onChangeThemeMode(next);
        final settings = c._settings.copyWith(themeModeId: themeModeToId(next));
        c._applyState(() => c._settings = settings);
        unawaited(c._storage.saveSettings(settings));
      },
      onChangeLogLevel: (next) {
        CoduxLog.setLevelName(next);
        c._applyState(() => c._settings = c._settings.copyWith(logLevel: next));
      },
      onChangeAppTextScale: (next) {
        final settings = c._settings.copyWith(appTextScale: next);
        c._applyState(() => c._settings = settings);
        unawaited(c._storage.saveSettings(settings));
      },
      onChangeTerminalFontSize: (next) {
        final settings = c._settings.copyWith(terminalFontSize: next);
        c._applyState(() => c._settings = settings);
        unawaited(c._storage.saveSettings(settings));
      },
      onUseDetectedName: () => c._applyState(
        () => c._settingsNameController.text = c._detectedDeviceName,
      ),
      onSave: c._saveSettings,
      onBack: () => c._popCupertinoPage(() {
        c._showSettings = false;
      }),
    );
  }

  Widget _buildTerminalSwitcherPage(double topInset, double bottomInset) {
    return TerminalSwitcherScreen(
      topInset: topInset,
      bottomInset: bottomInset,
      terminals: c._currentProjectTerminals(),
      worktrees: c._worktrees,
      activeTerminalId: c._sessionId,
      selectedProjectId: c._selectedProjectId,
      selectedWorktreeId: c._selectedWorktreeId,
      switchingWorktreeId: c._pendingWorktreeSwitch?.worktreeId,
      loadingWorktrees: c._worktreeListLoading,
      creating: c._creatingTerminalProjectId == c._selectedProjectId,
      creatingWorktree: c._creatingWorktree,
      onBack: c._closeTerminalSwitcher,
      onSelectTerminal: c._selectTerminalFromSwitcher,
      onCreateTerminal: c._createCurrentProjectTerminal,
      onCloseTerminal: c._closeTerminal,
      onSelectWorktree: c._selectWorktree,
      onCreateWorktree: c._createWorktree,
      onMergeWorktree: c._mergeWorktree,
      onDeleteWorktree: c._deleteWorktree,
      onOpenWorktrees: c._ensureSelectedProjectWorktrees,
      onRefreshWorktrees: () => c._requestWorktreeList(loading: true),
      onRefreshTerminals: () => c._requestTerminalList(resetRetry: true),
    );
  }

  Widget _buildWorkspace(double topInset, double bottomInset) {
    // On the pad the terminal stays centered while the right column shows a tool
    // (stats/ssh/git), so the terminal toolbar must stay visible. Report an
    // effective 'terminal' mode whenever the terminal body is actually centered.
    final isPadLayout = MediaQuery.of(context).size.width >= _padLayoutMinWidth;
    final terminalCentered = isPadLayout
        ? (c._workspaceMode != WorkspaceMode.review &&
              c._editingFilePath == null)
        : (c._workspaceMode == WorkspaceMode.terminal);
    final terminalBody = RemoteTerminalPane(
      connected: c._isConnected,
      showTerminal: c._hasShownTerminal,
      hasDevice: c._activeDevice != null,
      status: c._status,
      workspaceMode: terminalCentered
          ? WorkspaceMode.terminal
          : c._workspaceMode,
      projectListLoaded: c._projectListLoaded,
      projectCount: c._projects.length,
      terminalUploadLoading: c._terminalUploadLoading,
      terminalUploadStatus: c._terminalUploadStatus,
      terminalBufferLoading: c._terminalBufferLoading,
      sessionId: c._sessionId,
      pendingBufferSessionId: c._terminalBufferRetry.pendingSessionId,
      connectionStatusText: c._connectionStatusText,
      terminalHistoryLoadingText: c._terminalHistoryLoadingText(),
      keyboardVisible: c._keyboardVisible,
      keyboardRequested: c._keyboardRequested,
      keyboardRequestSerial: c._keyboardRequestSerial,
      repaintSignal: c._terminalRepaint,
      outputController: c._terminalOutputController,
      terminalFontSize: c._settings.terminalFontSize,
      onConnect: () => c._connect(),
      onInput: c._queueTerminalTyping,
      onResize: (cols, rows) {
        final firstResize = !c._terminalReady;
        c._terminalReady = true;
        c._sendTerminalResize(cols, rows);
        if (firstResize) {
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (!mounted) return;
            CoduxLog.debug(
              '[codux-flutter-terminal] first resize ready selected=${c._selectedProjectId ?? ''} session=${c._sessionId ?? ''} terminalListLoaded=$c._terminalListLoaded',
            );
            c._ensureTerminalForSelectedProject();
            c._mountVisibleTerminal(reason: 'first-resize');
          });
        }
      },
      onSelectionChanged: (text) {
        if (c._terminalSelectedText == text) return;
        c._applyState(() => c._terminalSelectedText = text);
      },
      onSendKey: c._sendTerminalKey,
      onToggleKeyboard: c._toggleTerminalKeyboard,
      onRequestKeyboard: c._requestTerminalKeyboard,
      onPaste: c._pasteToTerminal,
      onCopy: c._copyTerminalSelection,
      onUpload: c._chooseUploadForTerminal,
      onVoiceInput: c._startVoiceInput,
      handedAway: c._remoteHandedAway,
      onTakeOver: () => c._takeOverTerminalViewport(),
    );

    return _buildWorkspaceShell(topInset, terminalBody);
  }

  Widget _buildWorkspaceShell(double topInset, Widget terminalBody) {
    final shellData = c._workspaceShellData;
    return c._workspaceBuilder.build(
      context: context,
      topInset: topInset,
      workspaceMode: c._workspaceMode,
      connected: c._isConnected,
      latencyMs: c._latencyMs,
      deviceName: c._activeDevice?.hostName?.trim().isNotEmpty == true
          ? c._activeDevice!.hostName!.trim()
          : (c._activeDevice?.name ?? ''),
      projects: c._projects,
      selectedProjectId: c._selectedProjectId,
      projectListLoaded: c._projectListLoaded,
      selectedWorktreeId: c._selectedWorktreeId,
      activeTerminalId: c._sessionId,
      hasCurrentTerminal: c._currentTerminal() != null,
      shellData: shellData,
      terminalBody: terminalBody,
      onShowTerminal: c._showTerminalMode,
      onShowStats: () =>
          c._toggleWorkspaceTool(WorkspaceMode.stats, c._requestAIStats),
      onShowFiles: c._showFilesMode,
      onShowReview: c._showReviewMode,
      onShowSsh: () =>
          c._toggleWorkspaceTool(WorkspaceMode.ssh, c._showSshMode),
      onShowGit: () =>
          c._toggleWorkspaceTool(WorkspaceMode.git, c._showGitMode),
      onGitAction: (op, args) => c._gitAction(op, args: args),
      onRefreshGit: c._requestGitStatus,
      onSshUpsert: c._sshUpsert,
      onSshRemove: c._sshRemove,
      gitDiff: c._gitDiff,
      reviewSelectedPath: c._gitDiffPath,
      onSelectReviewFile: c._openReviewFile,
      editingFilePath: c._editingFilePath,
      fileEditorController: c._fileEditorController,
      fileEditorLoading: c._fileEditorLoading,
      fileEditorSaving: c._fileEditorSaving,
      fileEditorEditing: c._fileEditorEditing,
      fileEditorEditable: c._fileEditorEditable,
      onEditFile: c._beginEditingFile,
      onSaveFile: c._saveEditingFile,
      onCancelFileEdit: c._cancelEditingFile,
      onCloseFileEditor: () => c._applyState(() => c._editingFilePath = null),
      onBack: () => c._applyState(() {
        c._showTerminal = false;
        c._workspaceMode = WorkspaceMode.terminal;
      }),
      onEditProject: c._requestProjectEdit,
      onAddProject: c._requestProjectAdd,
      onRemoveProject: c._requestProjectRemove,
      onSelectProject: c._onProjectSelected,
      onSelectWorktree: c._selectWorktree,
      onCreateWorktree: c._createWorktree,
      onMergeWorktree: c._mergeWorktree,
      onDeleteWorktree: c._deleteWorktree,
      onSelectTerminal: c._selectTerminal,
      onRefreshLists: c._refreshLists,
      // Mobile “+” creates another terminal session; desktop arranges sessions in the main grid.
      onCreateTerminal: c._createCurrentProjectTerminal,
      onCloseCurrentTerminal: c._closeCurrentTerminal,
      onCloseTerminal: c._closeTerminal,
      onRebuildTerminal: c._rebuildCurrentTerminal,
      onOpenTerminalSwitcher: c._openTerminalSwitcher,
      onRequestProjectFiles: c._requestProjectFiles,
      onOpenProjectFile: c._requestFileRead,
      onOpenProjectHome: c._openSelectedProjectHome,
      onOpenProjectRoot: c._openProjectRoot,
      onOpenProjectVolumes: c._openProjectVolumes,
      onRenameProjectFile: c._renameProjectFile,
      onCopyProjectFilePath: c._copyProjectFilePath,
      onDeleteProjectFile: c._deleteProjectFile,
      onOpenSession: c._openAISession,
      onRenameSession: c._renameAISession,
      onDeleteSession: c._deleteAISession,
    );
  }
}
