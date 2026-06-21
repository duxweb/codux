import 'package:flutter/material.dart';

import 'pad_theme.dart';

/// Fixed-size status tag (A / M / D / ? …) shared by the git and review lists so
/// the letters read at one consistent size across panels.
class PadStatusTag extends StatelessWidget {
  const PadStatusTag({super.key, required this.label, required this.color});

  final String label;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 24,
      height: 24,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.14),
        borderRadius: BorderRadius.circular(6),
      ),
      child: Text(
        label,
        style: TextStyle(
          color: color,
          fontSize: 11,
          fontWeight: FontWeight.w800,
        ),
      ),
    );
  }
}

/// Small rounded count chip (folder change count, etc.).
class PadCountChip extends StatelessWidget {
  const PadCountChip({super.key, required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 20,
      constraints: const BoxConstraints(minWidth: 24),
      alignment: Alignment.center,
      padding: const EdgeInsets.symmetric(horizontal: 7),
      decoration: BoxDecoration(
        color: PadColors.cardActive,
        borderRadius: BorderRadius.circular(7),
      ),
      child: Text(
        label,
        style: const TextStyle(
          color: PadColors.textMuted,
          fontSize: 11,
          fontWeight: FontWeight.w800,
        ),
      ),
    );
  }
}

/// Unified list row for the files / git / review panels: a rounded card with a
/// leading icon, the file (or folder) name on top, and its path below (relative
/// to the project root). No trailing chevron — the whole row is the tap target.
class PadFileListItem extends StatelessWidget {
  const PadFileListItem({
    super.key,
    required this.icon,
    required this.name,
    required this.path,
    this.iconColor,
    this.trailing,
    this.selected = false,
    this.onTap,
    this.onLongPress,
  });

  final IconData icon;
  final String name;

  /// Path shown under the name, already formatted relative to the project root
  /// (see [padRootRelativePath]).
  final String path;
  final Color? iconColor;
  final Widget? trailing;
  final bool selected;
  final VoidCallback? onTap;
  final VoidCallback? onLongPress;

  @override
  Widget build(BuildContext context) {
    final content = AnimatedContainer(
      duration: const Duration(milliseconds: 120),
      curve: Curves.easeOutCubic,
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 9),
      decoration: BoxDecoration(
        color: selected ? PadColors.cardActive : PadColors.card,
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        children: [
          Icon(icon, size: 20, color: iconColor ?? PadColors.textMuted),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    color: PadColors.textPrimary,
                    fontSize: 13,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const SizedBox(height: 3),
                Text(
                  path,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    color: PadColors.textSubtle,
                    fontSize: 11,
                  ),
                ),
              ],
            ),
          ),
          if (trailing != null) ...[const SizedBox(width: 8), trailing!],
        ],
      ),
    );
    if (onTap == null && onLongPress == null) return content;
    return Material(
      color: Colors.transparent,
      child: InkWell(
        borderRadius: BorderRadius.circular(10),
        onTap: onTap,
        onLongPress: onLongPress,
        child: content,
      ),
    );
  }
}

/// Format a path for display under a [PadFileListItem], rooted at `/`: the
/// directory portion of `relativePath` (relative to its context root) prefixed
/// with `/`. An item at the root shows just `/`. Example: `apps/mobile/foo.dart`
/// → `/apps/mobile`; a root-level file → `/`.
String padRootRelativePath(String relativePath) {
  final slash = relativePath.lastIndexOf('/');
  final dir = slash <= 0 ? '' : relativePath.substring(0, slash);
  return dir.isEmpty ? '/' : '/$dir';
}
