import 'package:flutter/material.dart';

import '../../i18n.dart';
import '../../models/remote_models.dart';
import '../../models/workspace_mode.dart';
import '../components/project_files_panel.dart';
import 'pad_theme.dart';

/// Center workspace: the terminal tab strip and the terminal body. View
/// switching and the contextual panels (files / AI stats) live in the top bar
/// and right column respectively.
class PadWorkspaceMainPane extends StatelessWidget {
  const PadWorkspaceMainPane({
    super.key,
    required this.terminals,
    required this.activeTerminalId,
    required this.workspaceMode,
    required this.terminalBody,
    required this.gitDiff,
    required this.reviewSelectedPath,
    required this.editingFilePath,
    required this.fileEditorController,
    required this.fileEditorLoading,
    required this.fileEditorSaving,
    required this.fileEditorEditing,
    required this.fileEditorEditable,
    required this.onEditFile,
    required this.onSaveFile,
    required this.onCancelFileEdit,
    required this.onCloseFileEditor,
    required this.onSelectTerminal,
    required this.onCreateTerminal,
    required this.onCloseTerminal,
  });

  final List<TerminalInfo> terminals;
  final String? activeTerminalId;
  final WorkspaceMode workspaceMode;
  final Widget terminalBody;
  final RemoteGitDiff? gitDiff;
  final String? reviewSelectedPath;
  final String? editingFilePath;
  final TextEditingController fileEditorController;
  final bool fileEditorLoading;
  final bool fileEditorSaving;
  final bool fileEditorEditing;
  final bool fileEditorEditable;
  final VoidCallback onEditFile;
  final VoidCallback onSaveFile;
  final VoidCallback onCancelFileEdit;
  final VoidCallback onCloseFileEditor;
  final ValueChanged<TerminalInfo> onSelectTerminal;
  final VoidCallback onCreateTerminal;
  final ValueChanged<TerminalInfo> onCloseTerminal;

  @override
  Widget build(BuildContext context) {
    if (workspaceMode == WorkspaceMode.files) {
      if (editingFilePath != null) {
        return FileEditorView(
          path: editingFilePath!,
          controller: fileEditorController,
          loading: fileEditorLoading,
          saving: fileEditorSaving,
          editing: fileEditorEditing,
          editable: fileEditorEditable,
          onClose: onCloseFileEditor,
          onEdit: onEditFile,
          onSave: onSaveFile,
          onCancelEdit: onCancelFileEdit,
          showClose: false,
        );
      }
      return _PadEditorEmpty(
        text: AppPreferences.of(context).t('file.selectToOpen'),
      );
    }
    if (workspaceMode == WorkspaceMode.review) {
      return PadDiffView(diff: gitDiff, path: reviewSelectedPath);
    }
    return Column(
      children: [
        _PadTerminalTabs(
          terminals: terminals,
          activeTerminalId: activeTerminalId,
          onSelectTerminal: onSelectTerminal,
          onCreateTerminal: onCreateTerminal,
          onCloseTerminal: onCloseTerminal,
        ),
        Expanded(child: terminalBody),
      ],
    );
  }
}

/// Renders the unified diff for the selected review file (from `git.read diff`).
class PadDiffView extends StatelessWidget {
  const PadDiffView({super.key, required this.diff, required this.path});

  final RemoteGitDiff? diff;
  final String? path;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    if (path == null) {
      return const _DiffEmpty(text: 'Select a file to view its diff');
    }
    return Column(
      children: [
        Container(
          height: 44,
          decoration: BoxDecoration(
            color: PadColors.panel,
            border: Border(bottom: BorderSide(color: PadColors.border)),
          ),
          padding: const EdgeInsets.symmetric(horizontal: 14),
          alignment: Alignment.centerLeft,
          child: Row(
            children: [
              Icon(Icons.difference_rounded, size: 15, color: PadColors.textMuted),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  path!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: PadColors.textSecondary,
                    fontSize: 12.5,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ],
          ),
        ),
        Expanded(
          child: diff == null
              ? Center(child: CircularProgressIndicator(color: accent))
              : (diff!.diff.trim().isEmpty
                    ? const _DiffEmpty(text: 'No changes')
                    : _DiffBody(diff: diff!.diff, accent: accent)),
        ),
      ],
    );
  }
}

class _DiffEmpty extends StatelessWidget {
  const _DiffEmpty({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: PadColors.bg,
      child: Center(
        child: Text(
          text,
          style: TextStyle(color: PadColors.textSubtle, fontSize: 13),
        ),
      ),
    );
  }
}

/// Empty state shown in the center pane when files mode is active but no file
/// is open yet (files are opened one at a time from the right-column tree, so
/// there is no editor tab strip — the open file shows its title in its header).
class _PadEditorEmpty extends StatelessWidget {
  const _PadEditorEmpty({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: PadColors.bg,
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.description_outlined,
              size: 34,
              color: PadColors.textSubtle,
            ),
            const SizedBox(height: 12),
            Text(
              text,
              style: TextStyle(color: PadColors.textSubtle, fontSize: 13),
            ),
          ],
        ),
      ),
    );
  }
}

enum _CellKind { context, add, del, empty, hunk }

/// One cell on one side of the comparison: line number + code + how it differs.
/// `empty` is a blank placeholder kept so both sides stay row-aligned.
class _DiffCell {
  const _DiffCell(this.kind, this.no, this.text);
  const _DiffCell.empty() : kind = _CellKind.empty, no = null, text = '';

  final _CellKind kind;
  final int? no;
  final String text;
}

/// One aligned row: original (left) cell paired with the current (right) cell.
class _DiffPair {
  const _DiffPair(this.left, this.right);
  final _DiffCell left;
  final _DiffCell right;
}

/// Two-column side-by-side file comparison built from a unified diff: 原始 (HEAD)
/// on the left, 现在 (worktree) on the right. Deleted lines sit on the left (red),
/// added lines on the right (green); the opposite side keeps a blank placeholder
/// so rows line up. Both columns share one vertical scroll and their horizontal
/// scroll is mirrored, so the two sides always move together.
class _DiffBody extends StatefulWidget {
  const _DiffBody({required this.diff, required this.accent});

  final String diff;
  final Color accent;

  @override
  State<_DiffBody> createState() => _DiffBodyState();
}

class _DiffBodyState extends State<_DiffBody> {
  static const double _rowHeight = 19;
  static const int _maxRows = 800;
  static final RegExp _hunkHeader = RegExp(
    r'^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@',
  );

  // Each side is its own viewport that fills half the area; vertical and
  // horizontal scrolling are mirrored across the pair so they move together.
  final ScrollController _vLeft = ScrollController();
  final ScrollController _vRight = ScrollController();
  final ScrollController _hLeft = ScrollController();
  final ScrollController _hRight = ScrollController();
  bool _syncing = false;

  @override
  void initState() {
    super.initState();
    _vLeft.addListener(() => _mirror(_vLeft, _vRight));
    _vRight.addListener(() => _mirror(_vRight, _vLeft));
    _hLeft.addListener(() => _mirror(_hLeft, _hRight));
    _hRight.addListener(() => _mirror(_hRight, _hLeft));
  }

  @override
  void dispose() {
    _vLeft.dispose();
    _vRight.dispose();
    _hLeft.dispose();
    _hRight.dispose();
    super.dispose();
  }

  void _mirror(ScrollController from, ScrollController to) {
    if (_syncing || !to.hasClients || !from.hasClients) return;
    final target = from.offset.clamp(
      to.position.minScrollExtent,
      to.position.maxScrollExtent,
    );
    if ((to.offset - target).abs() < 0.5) return;
    _syncing = true;
    to.jumpTo(target);
    _syncing = false;
  }

  List<_DiffPair> _align() {
    final rows = <_DiffPair>[];
    final dels = <_DiffCell>[];
    final adds = <_DiffCell>[];
    var oldLine = 0;
    var newLine = 0;

    void flushChanges() {
      final count = dels.length > adds.length ? dels.length : adds.length;
      for (var i = 0; i < count; i++) {
        rows.add(
          _DiffPair(
            i < dels.length ? dels[i] : const _DiffCell.empty(),
            i < adds.length ? adds[i] : const _DiffCell.empty(),
          ),
        );
      }
      dels.clear();
      adds.clear();
    }

    for (final raw in widget.diff.split('\n')) {
      if (rows.length >= _maxRows) break;
      final hunk = _hunkHeader.firstMatch(raw);
      if (hunk != null) {
        flushChanges();
        oldLine = int.parse(hunk.group(1)!);
        newLine = int.parse(hunk.group(2)!);
        rows.add(
          _DiffPair(
            _DiffCell(_CellKind.hunk, null, raw),
            const _DiffCell(_CellKind.hunk, null, ''),
          ),
        );
        continue;
      }
      if (raw.startsWith('diff ') ||
          raw.startsWith('index ') ||
          raw.startsWith('--- ') ||
          raw.startsWith('+++ ') ||
          raw.startsWith('new file') ||
          raw.startsWith('deleted file') ||
          raw.startsWith('similarity ') ||
          raw.startsWith('rename ') ||
          raw.startsWith('\\')) {
        continue;
      }
      if (raw.startsWith('+')) {
        adds.add(_DiffCell(_CellKind.add, newLine, raw.substring(1)));
        newLine++;
        continue;
      }
      if (raw.startsWith('-')) {
        dels.add(_DiffCell(_CellKind.del, oldLine, raw.substring(1)));
        oldLine++;
        continue;
      }
      flushChanges();
      final text = raw.startsWith(' ') ? raw.substring(1) : raw;
      rows.add(
        _DiffPair(
          _DiffCell(_CellKind.context, oldLine, text),
          _DiffCell(_CellKind.context, newLine, text),
        ),
      );
      oldLine++;
      newLine++;
    }
    flushChanges();
    return rows;
  }

  @override
  Widget build(BuildContext context) {
    final rows = _align();
    return ColoredBox(
      color: PadColors.bg,
      child: Column(
        children: [
          Row(
            children: [
              Expanded(child: _columnHeader('原始')),
              const SizedBox(width: 1),
              Expanded(child: _columnHeader('现在')),
            ],
          ),
          Expanded(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  child: _side(
                    rows,
                    left: true,
                    vCtrl: _vLeft,
                    hCtrl: _hLeft,
                  ),
                ),
                Container(width: 1, color: PadColors.border),
                Expanded(
                  child: _side(
                    rows,
                    left: false,
                    vCtrl: _vRight,
                    hCtrl: _hRight,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _columnHeader(String label) {
    return Container(
      height: 28,
      color: PadColors.panel,
      alignment: Alignment.centerLeft,
      padding: const EdgeInsets.symmetric(horizontal: 12),
      child: Text(
        label,
        style: TextStyle(
          color: PadColors.textMuted,
          fontSize: 11.5,
          fontWeight: FontWeight.w700,
        ),
      ),
    );
  }

  Widget _side(
    List<_DiffPair> rows, {
    required bool left,
    required ScrollController vCtrl,
    required ScrollController hCtrl,
  }) {
    // Vertical viewport fills the half; the code overflows and scrolls
    // horizontally inside it. Both axes are mirrored to the other side. The
    // content is forced to at least the half-width (minWidth) so the row
    // backgrounds span the whole column instead of just the code length.
    return LayoutBuilder(
      builder: (context, constraints) {
        final minWidth = constraints.maxWidth;
        return SingleChildScrollView(
          controller: vCtrl,
          scrollDirection: Axis.vertical,
          child: SingleChildScrollView(
            controller: hCtrl,
            scrollDirection: Axis.horizontal,
            child: ConstrainedBox(
              constraints: BoxConstraints(minWidth: minWidth),
              child: IntrinsicWidth(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    for (final pair in rows)
                      _cell(left ? pair.left : pair.right),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  Widget _cell(_DiffCell cell) {
    final bg = switch (cell.kind) {
      _CellKind.add => PadColors.success.withValues(alpha: 0.12),
      _CellKind.del => PadColors.danger.withValues(alpha: 0.12),
      _CellKind.empty => PadColors.panelTrack.withValues(alpha: 0.4),
      _CellKind.hunk => widget.accent.withValues(alpha: 0.10),
      _ => Colors.transparent,
    };
    final color = switch (cell.kind) {
      _CellKind.add => PadColors.success,
      _CellKind.del => PadColors.danger,
      _CellKind.hunk => widget.accent,
      _ => PadColors.textSecondary,
    };
    return Container(
      height: _rowHeight,
      color: bg,
      alignment: Alignment.centerLeft,
      child: Row(
        children: [
          SizedBox(
            width: 40,
            child: Padding(
              padding: const EdgeInsets.only(right: 8),
              child: Text(
                cell.no?.toString() ?? '',
                textAlign: TextAlign.right,
                style: TextStyle(
                  fontFamily: 'MapleMonoNFCN',
                  fontSize: 11,
                  height: 1.5,
                  color: PadColors.textSubtle,
                ),
              ),
            ),
          ),
          Text(
            cell.text.isEmpty ? ' ' : cell.text,
            style: TextStyle(
              fontFamily: 'MapleMonoNFCN',
              fontSize: 12,
              height: 1.5,
              color: color,
            ),
          ),
          const SizedBox(width: 12),
        ],
      ),
    );
  }
}

class _PadTerminalTabs extends StatelessWidget {
  const _PadTerminalTabs({
    required this.terminals,
    required this.activeTerminalId,
    required this.onSelectTerminal,
    required this.onCreateTerminal,
    required this.onCloseTerminal,
  });

  final List<TerminalInfo> terminals;
  final String? activeTerminalId;
  final ValueChanged<TerminalInfo> onSelectTerminal;
  final VoidCallback onCreateTerminal;
  final ValueChanged<TerminalInfo> onCloseTerminal;

  @override
  Widget build(BuildContext context) {
    return Container(
      // Exactly matches the sidebar / right-column header height (48) so the
      // strip's top + bottom edges line up with them across the workspace.
      height: 48,
      // Header follows the light/dark theme (only the bottom toolbar is dark).
      color: PadColors.header,
      // Breathing room above and to the left so the tabs float inside the strip
      // rather than butting against the panel edge.
      padding: const EdgeInsets.only(top: 8, left: 10),
      child: Row(
        children: [
          Expanded(
            child: ListView.builder(
              scrollDirection: Axis.horizontal,
              itemCount: terminals.length,
              itemBuilder: (context, index) {
                final terminal = terminals[index];
                final active = terminal.id == activeTerminalId;
                final title = terminal.title.trim().isNotEmpty
                    ? terminal.title.trim()
                    : terminal.id;
                return _TerminalTab(
                  title: title,
                  active: active,
                  onTap: () => onSelectTerminal(terminal),
                  onClose: () => onCloseTerminal(terminal),
                );
              },
            ),
          ),
          _TabBarAction(icon: Icons.add_rounded, onTap: onCreateTerminal),
          const SizedBox(width: 6),
        ],
      ),
    );
  }
}

class _TerminalTab extends StatelessWidget {
  const _TerminalTab({
    required this.title,
    required this.active,
    required this.onTap,
    required this.onClose,
  });

  final String title;
  final bool active;
  final VoidCallback onTap;
  final VoidCallback onClose;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    return InkWell(
      onTap: onTap,
      borderRadius: const BorderRadius.vertical(top: Radius.circular(10)),
      child: Container(
        constraints: const BoxConstraints(minWidth: 116, maxWidth: 188),
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(
          // Selected tab uses a neutral wash (shared with the worktree row);
          // only the icon/label carry the accent.
          color: active ? PadColors.surfaceActive : Colors.transparent,
          borderRadius: const BorderRadius.vertical(top: Radius.circular(10)),
        ),
        child: Row(
          children: [
            Icon(
              Icons.terminal_rounded,
              size: 14,
              color: active ? accent : PadColors.textMuted,
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: active
                      ? PadColors.textPrimary
                      : PadColors.textSecondary,
                  fontSize: 13.5,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
            const SizedBox(width: 10),
            GestureDetector(
              onTap: onClose,
              child: Icon(
                Icons.close_rounded,
                size: 15,
                color: active ? PadColors.textSecondary : PadColors.textMuted,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _TabBarAction extends StatelessWidget {
  const _TabBarAction({required this.icon, this.onTap});

  final IconData icon;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(8),
      child: SizedBox(
        width: 36,
        height: 36,
        child: Center(
          child: Icon(icon, size: 18, color: PadColors.textMuted),
        ),
      ),
    );
  }
}
