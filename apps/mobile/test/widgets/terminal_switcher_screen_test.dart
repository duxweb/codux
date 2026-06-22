import 'package:codux_flutter/i18n.dart';
import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/theme/app_theme.dart';
import 'package:codux_flutter/widgets/components/terminal_switcher_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('new split action is not rendered as the active split', (
    tester,
  ) async {
    await tester.pumpWidget(
      _wrap(
        _switcher(
          terminals: const [
            TerminalInfo(
              id: 'split-1',
              title: 'One',
              projectId: 'project-1',
              layoutOrder: 0,
            ),
            TerminalInfo(
              id: 'split-2',
              title: 'Two',
              projectId: 'project-1',
              layoutOrder: 1,
            ),
          ],
          activeTerminalId: 'split-2',
          creatingSplit: true,
        ),
      ),
    );

    expect(find.byIcon(Icons.check_rounded), findsOneWidget);

    final addIcon = tester.widget<Icon>(
      find.descendant(
        of: find.byKey(const ValueKey('terminal-switcher-split-add')),
        matching: find.byIcon(Icons.add_rounded),
      ),
    );
    final activeIcon = tester.widget<Icon>(
      find.descendant(
        of: find.byKey(
          const ValueKey('terminal-switcher-split-terminal-split-2'),
        ),
        matching: find.byIcon(Icons.terminal_rounded),
      ),
    );

    expect(addIcon.color, isNot(activeIcon.color));
  });
}

Widget _wrap(Widget child) {
  return MaterialApp(
    theme: buildAppTheme(accent: AccentChoices.cyan.color),
    home: AppPreferences(
      accent: AccentChoices.cyan,
      locale: LocaleChoices.english,
      themeMode: ThemeMode.dark,
      child: child,
    ),
  );
}

TerminalSwitcherScreen _switcher({
  required List<TerminalInfo> terminals,
  required String? activeTerminalId,
  bool creatingSplit = false,
}) {
  return TerminalSwitcherScreen(
    topInset: 0,
    bottomInset: 0,
    terminals: terminals,
    worktrees: const [],
    activeTerminalId: activeTerminalId,
    selectedProjectId: 'project-1',
    selectedWorktreeId: 'project-1',
    switchingWorktreeId: null,
    loadingWorktrees: false,
    creatingSplit: creatingSplit,
    creatingTab: false,
    creatingWorktree: false,
    onBack: () {},
    onSelectTerminal: (_) {},
    onCreateSplit: () {},
    onCreateTab: () {},
    onCloseTerminal: (_) {},
    onSelectWorktree: (_) {},
    onCreateWorktree: () {},
    onMergeWorktree: (_) {},
    onDeleteWorktree: (_) {},
    onOpenWorktrees: () {},
    onRefreshWorktrees: () {},
    onRefreshTerminals: () {},
  );
}
