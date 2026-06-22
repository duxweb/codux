import 'dart:async';

import 'package:flutter/material.dart';
import 'package:codux_protocol_ffi/codux_protocol_ffi.dart';

import '../../i18n.dart';
import '../../theme/app_theme.dart';

class Toolbar extends StatefulWidget {
  const Toolbar({
    super.key,
    required this.onSendKey,
    required this.onPaste,
    required this.onCopy,
    required this.onUpload,
    required this.onVoiceInput,
    required this.applicationCursor,
    required this.keyboardVisible,
    required this.uploading,
    required this.bottomInset,
    required this.onToggleKeyboard,
  });

  final ValueChanged<String> onSendKey;
  final VoidCallback onPaste;
  final VoidCallback onCopy;
  final VoidCallback onUpload;
  final VoidCallback onVoiceInput;
  final bool applicationCursor;
  final bool keyboardVisible;
  final bool uploading;
  final double bottomInset;
  final VoidCallback onToggleKeyboard;

  @override
  State<Toolbar> createState() => _ToolbarState();
}

class _ToolbarState extends State<Toolbar> {
  bool _ctrl = false;
  bool _shift = false;
  bool _alt = false;

  void _clearModifiers() {
    if (!_ctrl && !_shift && !_alt) return;
    setState(() {
      _ctrl = false;
      _shift = false;
      _alt = false;
    });
  }

  void _send(String key, {String keyChar = ''}) {
    final input = keyChar.isNotEmpty && !_ctrl && !_shift && !_alt
        ? terminalTextInput(keyChar)
        : terminalKeyInput(
            key: key,
            keyChar: keyChar,
            shift: _shift,
            alt: _alt,
            control: _ctrl,
            applicationCursor: widget.applicationCursor,
          );
    widget.onSendKey(input);
    _clearModifiers();
  }

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final row1 = [
      _ToolItem(
        label: 'esc',
        kind: _ToolKind.special,
        onTap: () => _send('escape'),
      ),
      _ToolItem(
        label: 'tab',
        kind: _ToolKind.special,
        onTap: () => _send('tab'),
      ),
      _ToolItem(
        icon: Icons.mic_none_rounded,
        label: prefs.t('toolbar.voice'),
        kind: _ToolKind.special,
        onTap: widget.onVoiceInput,
      ),
      _ToolItem(
        icon: Icons.content_copy_rounded,
        label: 'copy',
        kind: _ToolKind.special,
        onTap: widget.onCopy,
      ),
      _ToolItem(
        icon: Icons.content_paste_rounded,
        label: 'paste',
        kind: _ToolKind.special,
        onTap: widget.onPaste,
      ),
      _ToolItem(
        icon: Icons.upload_file_rounded,
        label: prefs.t('toolbar.upload'),
        kind: _ToolKind.special,
        busy: widget.uploading,
        onTap: widget.onUpload,
      ),
      _ToolItem(
        icon: Icons.keyboard_arrow_up_rounded,
        label: '↑',
        kind: _ToolKind.icon,
        repeatable: true,
        onTap: () => _send('up'),
      ),
      _ToolItem(
        icon: Icons.backspace_outlined,
        label: 'del',
        kind: _ToolKind.special,
        repeatable: true,
        onTap: () => _send('backspace'),
      ),
      _ToolItem(
        icon: Icons.keyboard_return_rounded,
        label: prefs.t('toolbar.enter'),
        kind: _ToolKind.enter,
        onTap: () => _send('enter'),
      ),
    ];
    final row2 = [
      _ToolItem(
        label: '^C',
        kind: _ToolKind.danger,
        onTap: () {
          widget.onSendKey('\u0003');
          _clearModifiers();
        },
      ),
      _ToolItem(
        label: 'ctrl',
        kind: _ToolKind.modifier,
        active: _ctrl,
        onTap: () => setState(() => _ctrl = !_ctrl),
      ),
      _ToolItem(
        label: 'shft',
        kind: _ToolKind.modifier,
        active: _shift,
        onTap: () => setState(() => _shift = !_shift),
      ),
      _ToolItem(
        label: 'alt',
        kind: _ToolKind.modifier,
        active: _alt,
        onTap: () => setState(() => _alt = !_alt),
      ),
      _ToolItem(
        label: '/',
        kind: _ToolKind.special,
        onTap: () => _send('/', keyChar: '/'),
      ),
      _ToolItem(
        icon: Icons.keyboard_arrow_left_rounded,
        label: '←',
        kind: _ToolKind.icon,
        repeatable: true,
        onTap: () => _send('left'),
      ),
      _ToolItem(
        icon: Icons.keyboard_arrow_down_rounded,
        label: '↓',
        kind: _ToolKind.icon,
        repeatable: true,
        onTap: () => _send('down'),
      ),
      _ToolItem(
        icon: Icons.keyboard_arrow_right_rounded,
        label: '→',
        kind: _ToolKind.icon,
        repeatable: true,
        onTap: () => _send('right'),
      ),
      _ToolItem(
        icon: widget.keyboardVisible
            ? Icons.keyboard_hide_rounded
            : Icons.keyboard_rounded,
        label: prefs.t('toolbar.keyboard'),
        kind: _ToolKind.special,
        onTap: widget.onToggleKeyboard,
      ),
    ];

    return Container(
      color: AppColors.terminalChrome,
      child: SizedBox(
        height: 76 + widget.bottomInset,
        child: Padding(
          padding: EdgeInsets.fromLTRB(6, 4, 6, 4 + widget.bottomInset),
          child: _ToolGrid(row1: row1, row2: row2),
        ),
      ),
    );
  }
}

enum _ToolKind { special, modifier, icon, enter, danger }

class _ToolItem {
  _ToolItem({
    this.icon,
    this.label,
    required this.kind,
    required this.onTap,
    this.active = false,
    this.repeatable = false,
    this.busy = false,
  }) : assert(icon != null || label != null);

  final IconData? icon;
  final String? label;
  final _ToolKind kind;
  final VoidCallback onTap;
  final bool active;
  final bool repeatable;
  final bool busy;
}

class _ToolGrid extends StatelessWidget {
  const _ToolGrid({required this.row1, required this.row2});

  final List<_ToolItem> row1;
  final List<_ToolItem> row2;

  static const double _gap = 4;

  @override
  Widget build(BuildContext context) => Column(
    children: [
      Expanded(
        child: _ToolRow(items: row1, gap: _gap),
      ),
      const SizedBox(height: 4),
      Expanded(
        child: _ToolRow(items: row2, gap: _gap),
      ),
    ],
  );
}

class _ToolRow extends StatelessWidget {
  const _ToolRow({required this.items, required this.gap});

  final List<_ToolItem> items;
  final double gap;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      for (var index = 0; index < items.length; index += 1) ...[
        Expanded(child: _ToolButton(item: items[index])),
        if (index < items.length - 1) SizedBox(width: gap),
      ],
    ],
  );
}

class _ToolButton extends StatefulWidget {
  const _ToolButton({required this.item});

  final _ToolItem item;

  @override
  State<_ToolButton> createState() => _ToolButtonState();
}

class _ToolButtonState extends State<_ToolButton> {
  Timer? _repeatDelayTimer;
  Timer? _repeatTimer;

  void _startRepeat() {
    if (!widget.item.repeatable) return;
    _repeatDelayTimer?.cancel();
    _repeatTimer?.cancel();
    _repeatDelayTimer = Timer(const Duration(milliseconds: 320), () {
      widget.item.onTap();
      _repeatTimer = Timer.periodic(const Duration(milliseconds: 72), (_) {
        widget.item.onTap();
      });
    });
  }

  void _stopRepeat() {
    _repeatDelayTimer?.cancel();
    _repeatDelayTimer = null;
    _repeatTimer?.cancel();
    _repeatTimer = null;
  }

  @override
  void dispose() {
    _stopRepeat();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final item = widget.item;
    final accent = Theme.of(context).colorScheme.secondary;
    final color = switch (item.kind) {
      _ToolKind.enter => accent.withValues(alpha: 0.16),
      _ToolKind.danger => AppColors.danger.withValues(alpha: 0.16),
      _ToolKind.modifier when item.active => accent.withValues(alpha: 0.16),
      _ => AppColors.terminalElevated,
    };
    final foreground = switch (item.kind) {
      _ToolKind.enter => accent,
      _ToolKind.danger => AppColors.danger,
      _ToolKind.modifier when item.active => accent,
      _ => AppColors.terminalText,
    };

    return Material(
      color: color,
      borderRadius: BorderRadius.circular(8),
      child: InkWell(
        borderRadius: BorderRadius.circular(8),
        onTapDown: item.busy ? null : (_) => _startRepeat(),
        onTapUp: item.busy ? null : (_) => _stopRepeat(),
        onTapCancel: item.busy ? null : _stopRepeat,
        onTap: item.busy ? null : item.onTap,
        child: Semantics(
          label: item.label,
          button: true,
          enabled: !item.busy,
          child: Container(
            width: double.infinity,
            height: double.infinity,
            alignment: Alignment.center,
            child: item.busy
                ? SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(
                      strokeWidth: 2,
                      color: foreground,
                    ),
                  )
                : item.icon != null
                ? Icon(
                    item.icon,
                    size: item.kind == _ToolKind.enter ? 20 : 17,
                    color: foreground,
                  )
                : Text(
                    item.label!,
                    style: TextStyle(
                      color: foreground,
                      fontSize: 12,
                      height: 1,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 0.1,
                    ),
                  ),
          ),
        ),
      ),
    );
  }
}
