import 'package:flutter/material.dart';

import 'pad_theme.dart';

/// A single entry that can be grouped into the pad file tree (a git change or a
/// review change). Both the git panel and the review sidebar drill into the
/// same directory structure, so they share one grouping engine.
abstract class PadTreeEntry {
  String get status;
  String get path;
}

extension PadTreeEntryName on PadTreeEntry {
  /// The leaf name, tolerant of a trailing slash on whole-directory entries.
  String get name {
    final trimmed = path.endsWith('/')
        ? path.substring(0, path.length - 1)
        : path;
    final index = trimmed.lastIndexOf('/');
    return index < 0 ? trimmed : trimmed.substring(index + 1);
  }

  String get parent {
    final index = path.lastIndexOf('/');
    return index <= 0 ? '' : path.substring(0, index);
  }
}

/// An immediate child folder at the current directory level.
class PadTreeFolder<T extends PadTreeEntry> {
  PadTreeFolder({required this.name, required this.path});

  final String name;
  final String path;
  final List<T> entries = <T>[];

  int get count => entries.length;

  void add(T entry) => entries.add(entry);
}

/// One directory level: the immediate child folders plus the files that live
/// directly inside [basePath].
class PadDirectorySnapshot<T extends PadTreeEntry> {
  PadDirectorySnapshot({required this.folders, required this.files});

  final List<PadTreeFolder<T>> folders;
  final List<T> files;

  factory PadDirectorySnapshot.from(String basePath, List<T> changes) {
    final folders = <String, PadTreeFolder<T>>{};
    final files = <T>[];

    for (final change in changes) {
      final relativePath = padRelativePath(basePath, change.path);
      if (relativePath == null || relativePath.isEmpty) {
        continue;
      }
      final slashIndex = relativePath.indexOf('/');
      // A trailing slash with nothing after it means the change *is* this
      // directory (e.g. a whole untracked dir) — render it as a tappable leaf
      // instead of a folder that drills into nothing.
      if (slashIndex < 0 || slashIndex == relativePath.length - 1) {
        files.add(change);
        continue;
      }
      final folderName = relativePath.substring(0, slashIndex);
      final folderPath = padJoinPath(basePath, folderName);
      folders
          .putIfAbsent(
            folderName,
            () => PadTreeFolder<T>(name: folderName, path: folderPath),
          )
          .add(change);
    }

    final sortedFolders = folders.values.toList()
      ..sort((left, right) => left.name.compareTo(right.name));
    files.sort((left, right) => left.name.compareTo(right.name));
    return PadDirectorySnapshot<T>(folders: sortedFolders, files: files);
  }
}

String? padParentPath(String path) {
  if (path.isEmpty) {
    return null;
  }
  final index = path.lastIndexOf('/');
  return index < 0 ? '' : path.substring(0, index);
}

String? padRelativePath(String basePath, String path) {
  if (basePath.isEmpty) {
    return path;
  }
  final prefix = '$basePath/';
  if (!path.startsWith(prefix)) {
    return null;
  }
  return path.substring(prefix.length);
}

String padJoinPath(String basePath, String child) {
  return basePath.isEmpty ? child : '$basePath/$child';
}

/// Status accent shared by the git panel and the review sidebar. Untracked
/// ('?') files only appear in the git panel and read the same as additions.
Color padStatusColor(String status, Color accent) {
  return switch (status) {
    'A' || '?' => PadColors.success,
    'D' => PadColors.danger,
    'R' => PadColors.warning,
    _ => accent,
  };
}

IconData padFileIcon(String status) {
  return switch (status) {
    'A' || '?' => Icons.note_add_rounded,
    'D' => Icons.note_alt_outlined,
    'R' => Icons.drive_file_rename_outline_rounded,
    _ => Icons.description_outlined,
  };
}
