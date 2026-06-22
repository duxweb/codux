import 'package:codux_flutter/i18n.dart';
import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/models/workspace_mode.dart';
import 'package:codux_flutter/theme/app_theme.dart';
import 'package:codux_flutter/widgets/phone/remote_workspace_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('shows terminal body in terminal mode', (tester) async {
    await tester.pumpWidget(_wrap(_workspace(workspaceMode: WorkspaceMode.terminal)));

    expect(find.text('Terminal body'), findsOneWidget);
  });

  testWidgets('shows stats panel in stats mode', (tester) async {
    await tester.pumpWidget(_wrap(_workspace(workspaceMode: WorkspaceMode.stats)));

    expect(find.text('Terminal body'), findsNothing);
    expect(find.text('Project'), findsWidgets);
  });

  testWidgets('shows file panel in files mode', (tester) async {
    await tester.pumpWidget(_wrap(_workspace(workspaceMode: WorkspaceMode.files)));

    expect(find.text('/repo'), findsOneWidget);
    expect(find.text('main.dart'), findsOneWidget);
  });
}

Widget _wrap(Widget child) {
  return MaterialApp(
    theme: buildAppTheme(accent: AccentChoices.cyan.color),
    home: AppPreferences(
      accent: AccentChoices.cyan,
      locale: LocaleChoices.english,
      themeMode: ThemeMode.dark,
      child: Scaffold(body: child),
    ),
  );
}

RemoteWorkspaceView _workspace({required WorkspaceMode workspaceMode}) {
  const project = ProjectInfo(id: 'project-1', name: 'Project', path: '/repo');
  return RemoteWorkspaceView(
    topInset: 0,
    workspaceMode: workspaceMode,
    connected: true,
    latencyMs: 42,
    projects: const [project],
    selectedProjectId: 'project-1',
    projectListLoaded: true,
    terminals: const [],
    activeTerminalId: null,
    hasCurrentTerminal: false,
    aiStats: const AIStatsInfo(
      projectName: 'Project',
      todayTokens: 1,
      totalTokens: 2,
      currentSessionTokens: 3,
      requestCount: 4,
    ),
    aiStatsLoading: false,
    projectFilesPath: '/repo',
    projectFilesParent: null,
    projectFileEntries: const [
      RemoteFileEntry(
        name: 'main.dart',
        path: '/repo/main.dart',
        isDirectory: false,
      ),
    ],
    projectFilesLoading: false,
    terminalBody: const Center(child: Text('Terminal body')),
    onShowTerminal: () {},
    onShowStats: () {},
    onShowFiles: () {},
    onBack: () {},
    onEditProject: () {},
    onAddProject: () {},
    onRemoveProject: () {},
    onSelectProject: (_) {},
    onSelectTerminal: (_) {},
    onRefreshLists: () {},
    onCreateTerminal: () {},
    onCloseCurrentTerminal: () {},
    onRebuildTerminal: () {},
    onOpenTerminalSwitcher: () {},
    onRequestProjectFiles: (_) {},
    onOpenProjectFile: (_) {},
    onOpenProjectHome: () {},
    onOpenProjectRoot: () {},
    onOpenProjectVolumes: () {},
    onRenameProjectFile: (_) {},
    onCopyProjectFilePath: (_) {},
    onDeleteProjectFile: (_) {},
  );
}
