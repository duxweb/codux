import '../../models/remote_models.dart';

/// Compact local "MM-DD HH:mm" from an ISO-8601 timestamp (falls back to raw).
String formatCreatedAt(String raw) {
  final parsed = DateTime.tryParse(raw);
  if (parsed == null) return raw;
  final local = parsed.toLocal();
  String two(int value) => value.toString().padLeft(2, '0');
  return '${two(local.month)}-${two(local.day)} ${two(local.hour)}:${two(local.minute)}';
}

/// Compact local "MM-DD HH:mm" from epoch seconds (AI session `time`).
String formatEpochSeconds(double seconds) {
  if (seconds <= 0) return '';
  final dt = DateTime.fromMillisecondsSinceEpoch(
    (seconds * 1000).round(),
  ).toLocal();
  String two(int value) => value.toString().padLeft(2, '0');
  return '${two(dt.month)}-${two(dt.day)} ${two(dt.hour)}:${two(dt.minute)}';
}

/// Compact token count, e.g. 1234 -> "1.2k", 2_000_000 -> "2.0M".
String formatTokenSize(int tokens) {
  if (tokens >= 1000000) return '${(tokens / 1000000).toStringAsFixed(1)}M';
  if (tokens >= 1000) return '${(tokens / 1000).toStringAsFixed(1)}k';
  return '$tokens';
}

ProjectInfo? selectedProjectOf(List<ProjectInfo> projects, String? selectedProjectId) {
  for (final project in projects) {
    if (project.id == selectedProjectId) return project;
  }
  return null;
}
