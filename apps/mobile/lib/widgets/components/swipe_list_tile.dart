import 'package:flutter/material.dart';

import '../../theme/app_theme.dart';

class SwipeListAction {
  const SwipeListAction({
    required this.label,
    required this.color,
    required this.onTap,
    this.icon,
  });

  final String label;
  final Color color;
  final VoidCallback onTap;
  final IconData? icon;
}

class SwipeListTile extends StatefulWidget {
  const SwipeListTile({
    super.key,
    required this.title,
    this.subtitle,
    this.leadingIcon,
    this.trailing,
    this.active = false,
    this.height = 74,
    this.actions = const [],
    this.onTap,
  });

  final String title;
  final String? subtitle;
  final IconData? leadingIcon;
  final Widget? trailing;
  final bool active;
  final double height;
  final List<SwipeListAction> actions;
  final VoidCallback? onTap;

  @override
  State<SwipeListTile> createState() => _SwipeListTileState();
}

class _SwipeListTileState extends State<SwipeListTile> {
  double _offset = 0;

  double get _actionWidth => widget.actions.length * 76.0;

  void _settle() {
    if (widget.actions.isEmpty) return;
    setState(() => _offset = _offset.abs() > 54 ? -_actionWidth : 0);
  }

  void _runAction(SwipeListAction action) {
    setState(() => _offset = 0);
    action.onTap();
  }

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final subtitle = widget.subtitle?.trim();
    final hasSubtitle = subtitle != null && subtitle.isNotEmpty;
    return ClipRRect(
      borderRadius: BorderRadius.circular(AppRadius.md),
      child: SizedBox(
        height: widget.height,
        child: Stack(
          children: [
            if (widget.actions.isNotEmpty)
              Positioned.fill(
                child: Align(
                  alignment: Alignment.centerRight,
                  child: SizedBox(
                    width: _actionWidth,
                    child: Row(
                      children: [
                        for (final action in widget.actions)
                          Expanded(
                            child: _SwipeActionButton(
                              action: action,
                              onTap: () => _runAction(action),
                            ),
                          ),
                      ],
                    ),
                  ),
                ),
              ),
            AnimatedPositioned(
              duration: const Duration(milliseconds: 160),
              curve: Curves.easeOutCubic,
              left: _offset,
              right: -_offset,
              top: 0,
              bottom: 0,
              child: GestureDetector(
                onHorizontalDragUpdate: widget.actions.isEmpty
                    ? null
                    : (details) {
                        setState(() {
                          _offset = (_offset + details.delta.dx).clamp(
                            -_actionWidth,
                            0,
                          );
                        });
                      },
                onHorizontalDragEnd: widget.actions.isEmpty
                    ? null
                    : (_) => _settle(),
                child: Material(
                  color: AppColors.bgSurface,
                  child: InkWell(
                    onTap: widget.onTap,
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: AppSpacing.m,
                      ),
                      child: Row(
                        children: [
                          if (widget.leadingIcon != null) ...[
                            Container(
                              width: 42,
                              height: 42,
                              decoration: BoxDecoration(
                                color: widget.active
                                    ? accent.withValues(alpha: 0.14)
                                    : AppColors.bgElevated,
                                borderRadius: BorderRadius.circular(
                                  AppRadius.sm,
                                ),
                              ),
                              child: Icon(
                                widget.leadingIcon,
                                color: widget.active
                                    ? accent
                                    : AppColors.textMuted,
                                size: 21,
                              ),
                            ),
                            const SizedBox(width: AppSpacing.m),
                          ],
                          Expanded(
                            child: Column(
                              mainAxisAlignment: MainAxisAlignment.center,
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  widget.title,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                    color: widget.active
                                        ? AppColors.textPrimary
                                        : AppColors.textSecondary,
                                    fontSize: AppTextSize.body,
                                    fontWeight: FontWeight.w700,
                                  ),
                                ),
                                if (hasSubtitle) ...[
                                  const SizedBox(height: 4),
                                  Text(
                                    subtitle,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: TextStyle(
                                      color: AppColors.textSubtle,
                                      fontSize: 11,
                                    ),
                                  ),
                                ],
                              ],
                            ),
                          ),
                          if (widget.trailing != null) ...[
                            const SizedBox(width: AppSpacing.s),
                            widget.trailing!,
                          ],
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _SwipeActionButton extends StatelessWidget {
  const _SwipeActionButton({required this.action, required this.onTap});

  final SwipeListAction action;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return Material(
      color: action.color.withValues(alpha: 0.16),
      child: InkWell(
        onTap: onTap,
        child: Center(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: AppSpacing.xs),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (action.icon != null) ...[
                  Icon(action.icon, size: 18, color: action.color),
                  const SizedBox(width: AppSpacing.xs),
                ],
                Flexible(
                  child: Text(
                    action.label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      color: action.color,
                      fontSize: AppTextSize.small,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
