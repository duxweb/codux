import 'dart:math' as math;

import 'package:flutter/material.dart';
import '../../i18n.dart';
import '../../models/remote_models.dart';
import '../../theme/app_theme.dart';

/// Theme-aware fallback palette for the phone stats panel (the pad passes its own
/// via `PadColors.statsPanel`). Resolved at build time so it follows dark/light.
AIStatsPanelColors defaultStatsPanelColors() => AIStatsPanelColors(
  background: AppColors.bgBase,
  card: AppColors.bgSurface,
  cardHeader: AppColors.bgSurface,
  cardBorder: AppColors.border,
  track: AppColors.bgElevated,
);

class AIStatsPanel extends StatelessWidget {
  const AIStatsPanel({
    super.key,
    required this.stats,
    required this.loading,
    required this.onRefresh,
    this.title,
    this.contentPadding,
    this.cardBordered = false,
    this.colors,
  });

  final AIStatsInfo? stats;
  final bool loading;
  final VoidCallback onRefresh;

  /// When set (pad layout), the top metrics are wrapped in a card whose header
  /// is this title. When null (phone), the metrics render headerless as before.
  final String? title;
  final EdgeInsetsGeometry? contentPadding;
  final bool cardBordered;
  final AIStatsPanelColors? colors;

  /// Wraps the top metric row in a titled card (or returns it unchanged).
  static Widget _wrapWithHeader({
    required String? title,
    required bool bordered,
    required AIStatsPanelColors colors,
    required Widget child,
  }) {
    if (title == null) return child;
    final radius = BorderRadius.circular(AppRadius.lg);
    return ClipRRect(
      borderRadius: radius,
      child: Container(
        decoration: BoxDecoration(color: colors.card),
        foregroundDecoration: BoxDecoration(
          borderRadius: radius,
          border: bordered
              ? Border.all(color: colors.cardBorder, width: 0.5)
              : null,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Container(
              height: 48,
              width: double.infinity,
              alignment: Alignment.centerLeft,
              color: colors.cardHeader,
              padding: const EdgeInsets.symmetric(horizontal: AppSpacing.l),
              child: Text(
                title,
                style: TextStyle(
                  color: AppColors.textPrimary,
                  fontSize: 15,
                  fontWeight: FontWeight.w700,
                ),
              ),
            ),
            Padding(padding: const EdgeInsets.all(AppSpacing.m), child: child),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    final colors = this.colors ?? defaultStatsPanelColors();
    if (loading && stats == null) {
      return ColoredBox(
        color: colors.background,
        child: Center(child: CircularProgressIndicator(color: accent)),
      );
    }
    final data = stats;
    return ColoredBox(
      color: colors.background,
      child: RefreshIndicator(
        color: accent,
        backgroundColor: colors.card,
        onRefresh: () async => onRefresh(),
        child: ListView(
          physics: const BouncingScrollPhysics(
            parent: AlwaysScrollableScrollPhysics(),
          ),
          padding:
              contentPadding ??
              const EdgeInsets.fromLTRB(
                AppSpacing.l,
                AppSpacing.m,
                AppSpacing.l,
                AppSpacing.xxl,
              ),
          children: [
            if (data == null)
              _EmptyStats(
                accent: accent,
                onRefresh: onRefresh,
                bordered: cardBordered,
                colors: colors,
              )
            else ...[
              _wrapWithHeader(
                title: title,
                bordered: cardBordered,
                colors: colors,
                child: Row(
                  children: [
                    Expanded(
                      child: _MetricTile(
                        label: prefs.t('stats.currentProject'),
                        value: _formatInt(data.totalTokens),
                        subValue: data.projectCachedInputTokens > 0
                            ? prefs.t(
                                'stats.cached',
                                params: {
                                  'value': _formatInt(
                                    data.projectCachedInputTokens,
                                  ),
                                },
                              )
                            : prefs.t(
                                'stats.requestCount',
                                params: {'count': '${data.requestCount}'},
                              ),
                        accent: accent,
                        bordered: false,
                        colors: colors,
                      ),
                    ),
                    const SizedBox(width: AppSpacing.s),
                    Expanded(
                      child: _MetricTile(
                        label: prefs.t('stats.todayTotal'),
                        value: _formatInt(data.todayTokens),
                        subValue: data.todayCachedInputTokens > 0
                            ? prefs.t(
                                'stats.cached',
                                params: {
                                  'value': _formatInt(
                                    data.todayCachedInputTokens,
                                  ),
                                },
                              )
                            : 'Token',
                        accent: accent,
                        bordered: false,
                        colors: colors,
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: AppSpacing.m),
              _CurrentSessionCard(
                data: data,
                accent: accent,
                bordered: cardBordered,
                colors: colors,
              ),
              const SizedBox(height: AppSpacing.m),
              _TodayBarsCard(
                buckets: data.todayTimeBuckets,
                accent: accent,
                bordered: cardBordered,
                colors: colors,
              ),
              const SizedBox(height: AppSpacing.m),
              _HeatmapCard(
                days: data.heatmap,
                accent: accent,
                bordered: cardBordered,
                colors: colors,
              ),
              const SizedBox(height: AppSpacing.m),
              _RankingCard(
                title: prefs.t('stats.toolRank'),
                icon: Icons.auto_awesome_rounded,
                items: data.toolBreakdown,
                accent: accent,
                bordered: cardBordered,
                colors: colors,
              ),
              const SizedBox(height: AppSpacing.m),
              _RankingCard(
                title: prefs.t('stats.modelRank'),
                icon: Icons.memory_rounded,
                items: data.modelBreakdown,
                accent: accent,
                bordered: cardBordered,
                colors: colors,
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _MetricTile extends StatelessWidget {
  const _MetricTile({
    required this.label,
    required this.value,
    required this.subValue,
    required this.accent,
    required this.bordered,
    required this.colors,
  });

  final String label;
  final String value;
  final String subValue;
  final Color accent;
  final bool bordered;
  final AIStatsPanelColors colors;

  @override
  Widget build(BuildContext context) {
    return _PanelCard(
      bordered: bordered,
      colors: colors,
      padding: const EdgeInsets.symmetric(
        horizontal: AppSpacing.l,
        vertical: AppSpacing.m,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            style: TextStyle(
              color: AppColors.textMuted,
              fontSize: AppTextSize.small,
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          Text(
            value,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              color: AppColors.textPrimary,
              fontSize: 22,
              height: 1.05,
              fontWeight: FontWeight.w700,
              letterSpacing: -0.4,
            ),
          ),
          const SizedBox(height: 2),
          Text(
            subValue,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              color: AppColors.textSubtle,
              fontSize: AppTextSize.small,
            ),
          ),
        ],
      ),
    );
  }
}

class _TodayBarsCard extends StatefulWidget {
  const _TodayBarsCard({
    required this.buckets,
    required this.accent,
    required this.bordered,
    required this.colors,
  });

  final List<AIStatsTimeBucket> buckets;
  final Color accent;
  final bool bordered;
  final AIStatsPanelColors colors;

  @override
  State<_TodayBarsCard> createState() => _TodayBarsCardState();
}

class _TodayBarsCardState extends State<_TodayBarsCard> {
  int? _selectedIndex;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final visible = _normalizedTodayBuckets(widget.buckets);
    final maxValue = visible.fold<int>(
      0,
      (max, item) => math.max(max, item.totalTokens),
    );
    final selected = _selectedIndex != null && _selectedIndex! < visible.length
        ? visible[_selectedIndex!]
        : null;
    return _PanelCard(
      bordered: widget.bordered,
      colors: widget.colors,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _SectionTitle(
            title: prefs.t('stats.todayUsage'),
            trailing: selected == null
                ? prefs.t('stats.tapBarHint')
                : prefs.t(
                    'stats.bucketDetail',
                    params: {
                      'time': _formatTimeLabel(selected.start),
                      'tokens': _formatInt(selected.totalTokens),
                      'count': '${selected.requestCount}',
                    },
                  ),
            accent: widget.accent,
          ),
          const SizedBox(height: AppSpacing.m),
          SizedBox(
            height: 78,
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                for (var i = 0; i < visible.length; i += 1)
                  Expanded(
                    child: GestureDetector(
                      onTap: () => setState(
                        () => _selectedIndex = _selectedIndex == i ? null : i,
                      ),
                      behavior: HitTestBehavior.translucent,
                      child: Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 1.5),
                        child: _TokenBar(
                          value: visible[i].totalTokens,
                          maxValue: maxValue,
                          accent: widget.accent,
                          trackColor: widget.colors.track,
                          highlighted: _selectedIndex == i,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text('00:00', style: _mutedLabel),
              Text('12:00', style: _mutedLabel),
              Text(prefs.t('stats.now'), style: _mutedLabel),
            ],
          ),
        ],
      ),
    );
  }

  static String _formatTimeLabel(String start) {
    if (start.isEmpty) return '-';
    final asInt = int.tryParse(start);
    if (asInt != null && asInt < 48) {
      final hour = asInt ~/ 2;
      final minute = (asInt % 2) * 30;
      final hourStr = hour.toString().padLeft(2, '0');
      final minuteStr = minute.toString().padLeft(2, '0');
      return '$hourStr:$minuteStr';
    }
    return _formatDateTime(start);
  }
}

class _TokenBar extends StatelessWidget {
  const _TokenBar({
    required this.value,
    required this.maxValue,
    required this.accent,
    required this.trackColor,
    this.highlighted = false,
  });

  final int value;
  final int maxValue;
  final Color accent;
  final Color trackColor;
  final bool highlighted;

  @override
  Widget build(BuildContext context) {
    final ratio = maxValue <= 0 ? 0.0 : value / maxValue;
    final base = value <= 0
        ? trackColor
        : Color.lerp(accent.withValues(alpha: 0.32), accent, ratio);
    return Align(
      alignment: Alignment.bottomCenter,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 220),
        curve: Curves.easeOutCubic,
        height: 8 + 70 * ratio,
        decoration: BoxDecoration(
          color: highlighted ? accent : base,
          borderRadius: BorderRadius.circular(4),
          border: highlighted ? Border.all(color: accent, width: 1.4) : null,
        ),
      ),
    );
  }
}

class _HeatmapCard extends StatefulWidget {
  const _HeatmapCard({
    required this.days,
    required this.accent,
    required this.bordered,
    required this.colors,
  });

  final List<AIStatsHeatmapDay> days;
  final Color accent;
  final bool bordered;
  final AIStatsPanelColors colors;

  @override
  State<_HeatmapCard> createState() => _HeatmapCardState();
}

class _HeatmapCardState extends State<_HeatmapCard> {
  int? _selectedIndex;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final visible = _normalizedHeatmap(widget.days);
    final maxValue = visible.fold<int>(
      0,
      (max, item) => math.max(max, item.totalTokens),
    );
    final selected = _selectedIndex != null && _selectedIndex! < visible.length
        ? visible[_selectedIndex!]
        : null;
    return _PanelCard(
      bordered: widget.bordered,
      colors: widget.colors,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _SectionTitle(
            title: prefs.t('stats.recentUsage'),
            trailing: selected == null
                ? prefs.t('stats.last28Days')
                : prefs.t(
                    'stats.dayDetail',
                    params: {
                      'day': _formatDayLabel(selected.day),
                      'tokens': _formatInt(selected.totalTokens),
                      'count': '${selected.requestCount}',
                    },
                  ),
            accent: widget.accent,
          ),
          const SizedBox(height: AppSpacing.m),
          LayoutBuilder(
            builder: (context, constraints) {
              const cols = 14;
              const spacing = 4.0;
              final maxCell = 14.0;
              final fitCell =
                  (constraints.maxWidth - spacing * (cols - 1)) / cols;
              final cell = math.min(maxCell, fitCell);
              final rows = (visible.length / cols).ceil();
              return Center(
                child: SizedBox(
                  width: cell * cols + spacing * (cols - 1),
                  height: cell * rows + spacing * (rows - 1),
                  child: Column(
                    children: [
                      for (var r = 0; r < rows; r += 1) ...[
                        if (r > 0) const SizedBox(height: spacing),
                        Row(
                          children: [
                            for (var c = 0; c < cols; c += 1) ...[
                              if (c > 0) const SizedBox(width: spacing),
                              SizedBox(
                                width: cell,
                                height: cell,
                                child: _HeatmapCell(
                                  index: r * cols + c,
                                  visible: visible,
                                  maxValue: maxValue,
                                  accent: widget.accent,
                                  trackColor: widget.colors.track,
                                  selectedIndex: _selectedIndex,
                                  onTap: (i) => setState(
                                    () => _selectedIndex = _selectedIndex == i
                                        ? null
                                        : i,
                                  ),
                                ),
                              ),
                            ],
                          ],
                        ),
                      ],
                    ],
                  ),
                ),
              );
            },
          ),
        ],
      ),
    );
  }

  static String _formatDayLabel(String day) {
    if (day.isEmpty) return '-';
    return _formatDate(day);
  }
}

class _HeatmapCell extends StatelessWidget {
  const _HeatmapCell({
    required this.index,
    required this.visible,
    required this.maxValue,
    required this.accent,
    required this.trackColor,
    required this.selectedIndex,
    required this.onTap,
  });

  final int index;
  final List<AIStatsHeatmapDay> visible;
  final int maxValue;
  final Color accent;
  final Color trackColor;
  final int? selectedIndex;
  final ValueChanged<int> onTap;

  @override
  Widget build(BuildContext context) {
    if (index >= visible.length) return const SizedBox.shrink();
    final value = visible[index].totalTokens;
    final ratio = maxValue <= 0 ? 0.0 : value / maxValue;
    final color = value <= 0
        ? trackColor
        : Color.lerp(accent.withValues(alpha: 0.2), accent, ratio);
    final selected = selectedIndex == index;
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: () => onTap(index),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: color,
          borderRadius: BorderRadius.circular(3),
          border: selected
              ? Border.all(color: AppColors.textPrimary, width: 1.2)
              : null,
        ),
      ),
    );
  }
}

class _RankingCard extends StatelessWidget {
  const _RankingCard({
    required this.title,
    required this.icon,
    required this.items,
    required this.accent,
    required this.bordered,
    required this.colors,
  });

  final String title;
  final IconData icon;
  final List<AIStatsBreakdownItem> items;
  final Color accent;
  final bool bordered;
  final AIStatsPanelColors colors;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final visible = items.take(6).toList(growable: false);
    final maxValue = visible.fold<int>(
      0,
      (max, item) => math.max(max, item.totalTokens),
    );
    final totalSum = items.fold<int>(0, (sum, item) => sum + item.totalTokens);
    return _PanelCard(
      bordered: bordered,
      colors: colors,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(icon, color: accent, size: 16),
              const SizedBox(width: AppSpacing.s),
              Expanded(
                child: Text(
                  title,
                  style: TextStyle(
                    color: AppColors.textPrimary,
                    fontSize: AppTextSize.body,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.l),
          if (visible.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: AppSpacing.s),
              child: Text(
                prefs.t('stats.noRankData'),
                style: TextStyle(
                  color: AppColors.textMuted,
                  fontSize: AppTextSize.body,
                ),
              ),
            )
          else
            for (var i = 0; i < visible.length; i += 1) ...[
              if (i > 0) const SizedBox(height: AppSpacing.l),
              _RankingRow(
                item: visible[i],
                totalSum: totalSum,
                maxValue: maxValue,
                accent: accent,
                colors: colors,
              ),
            ],
        ],
      ),
    );
  }
}

class _RankingRow extends StatelessWidget {
  const _RankingRow({
    required this.item,
    required this.totalSum,
    required this.maxValue,
    required this.accent,
    required this.colors,
  });

  final AIStatsBreakdownItem item;
  final int totalSum;
  final int maxValue;
  final Color accent;
  final AIStatsPanelColors colors;

  @override
  Widget build(BuildContext context) {
    final barRatio = maxValue <= 0 ? 0.0 : item.totalTokens / maxValue;
    final share = totalSum <= 0 ? 0.0 : item.totalTokens / totalSum;
    final pctText = '${(share * 100).round()}%';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          crossAxisAlignment: CrossAxisAlignment.baseline,
          textBaseline: TextBaseline.alphabetic,
          children: [
            Expanded(
              child: Text(
                item.key,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: AppColors.textPrimary,
                  fontSize: AppTextSize.body,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
            const SizedBox(width: AppSpacing.s),
            Text(
              _formatInt(item.totalTokens),
              style: TextStyle(
                color: AppColors.textPrimary,
                fontSize: AppTextSize.body,
                fontWeight: FontWeight.w700,
                letterSpacing: -0.2,
              ),
            ),
            const SizedBox(width: AppSpacing.s),
            SizedBox(
              width: 44,
              child: Text(
                pctText,
                textAlign: TextAlign.end,
                style: TextStyle(
                  color: AppColors.textSubtle,
                  fontSize: AppTextSize.small,
                  fontFeatures: [FontFeature.tabularFigures()],
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: AppSpacing.s),
        ClipRRect(
          borderRadius: BorderRadius.circular(99),
          child: LinearProgressIndicator(
            value: barRatio,
            minHeight: 4,
            color: accent,
            backgroundColor: colors.track,
          ),
        ),
      ],
    );
  }
}

/// Live "current session" card: active tool/model, context usage, and the
/// running sessions list — all from already-available AIStatsInfo fields.
class _CurrentSessionCard extends StatelessWidget {
  const _CurrentSessionCard({
    required this.data,
    required this.accent,
    required this.bordered,
    required this.colors,
  });

  final AIStatsInfo data;
  final Color accent;
  final bool bordered;
  final AIStatsPanelColors colors;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final contextPercent = data.contextUsagePercent;
    // Mirror the desktop "当前会话累计" card: render only the live AI runtime
    // sessions (each with its own tool/model/usage), never history metadata, so
    // the card stays empty when no session is actually running.
    final sessions = data.currentSessions;
    return _PanelCard(
      bordered: bordered,
      colors: colors,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _SectionTitle(
            title: prefs.t('stats.currentSession'),
            trailing: contextPercent != null
                ? '${contextPercent.toStringAsFixed(0)}%'
                : '',
            accent: accent,
          ),
          if (sessions.isEmpty) ...[
            const SizedBox(height: 16),
            Center(
              child: Text(
                prefs.t('stats.currentSession.empty'),
                textAlign: TextAlign.center,
                style: TextStyle(
                  color: AppColors.textSubtle,
                  fontSize: 12.5,
                  fontWeight: FontWeight.w500,
                ),
              ),
            ),
            const SizedBox(height: 8),
          ],
          for (final session in sessions) ...[
            const SizedBox(height: 10),
            _liveSessionRow(session, prefs),
          ],
        ],
      ),
    );
  }

  Widget _liveSessionRow(AIStatsSessionInfo session, AppPreferences prefs) {
    final tool = (session.tool ?? '').trim();
    final model = (session.model ?? '').trim();
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      decoration: BoxDecoration(
        color: colors.track,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  tool.isEmpty ? '-' : tool,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: AppColors.textPrimary,
                    fontSize: 13.5,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: 2),
                Text(
                  model.isEmpty ? '-' : model,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: AppColors.textSubtle,
                    fontSize: 12,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(width: 12),
          Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Text(
                _formatInt(session.totalTokens),
                style: TextStyle(
                  color: AppColors.textPrimary,
                  fontSize: 14.5,
                  fontWeight: FontWeight.w700,
                ),
              ),
              const SizedBox(height: 2),
              Text(
                prefs.t('stats.sessionTotal'),
                style: TextStyle(
                  color: AppColors.textMuted,
                  fontSize: 11,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _PanelCard extends StatelessWidget {
  const _PanelCard({
    required this.child,
    required this.colors,
    this.bordered = false,
    this.padding,
  });

  final Widget child;
  final AIStatsPanelColors colors;
  final bool bordered;
  final EdgeInsetsGeometry? padding;

  @override
  Widget build(BuildContext context) {
    final radius = BorderRadius.circular(AppRadius.lg);
    return ClipRRect(
      borderRadius: radius,
      child: Container(
        padding: padding ?? const EdgeInsets.all(AppSpacing.l),
        decoration: BoxDecoration(color: colors.card),
        foregroundDecoration: BoxDecoration(
          borderRadius: radius,
          border: bordered
              ? Border.all(color: colors.cardBorder, width: 0.5)
              : null,
        ),
        child: child,
      ),
    );
  }
}

class _SectionTitle extends StatelessWidget {
  const _SectionTitle({
    required this.title,
    required this.trailing,
    required this.accent,
  });

  final String title;
  final String trailing;
  final Color accent;

  @override
  Widget build(BuildContext context) => Row(
    crossAxisAlignment: CrossAxisAlignment.baseline,
    textBaseline: TextBaseline.alphabetic,
    children: [
      Text(
        title,
        style: TextStyle(
          color: AppColors.textPrimary,
          fontSize: AppTextSize.body,
          fontWeight: FontWeight.w600,
        ),
      ),
      const SizedBox(width: AppSpacing.s),
      Expanded(
        child: Text(
          trailing,
          textAlign: TextAlign.right,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(
            color: AppColors.textMuted,
            fontSize: AppTextSize.small,
            fontWeight: FontWeight.w500,
          ),
        ),
      ),
    ],
  );
}

class _EmptyStats extends StatelessWidget {
  const _EmptyStats({
    required this.accent,
    required this.onRefresh,
    required this.bordered,
    required this.colors,
  });
  final Color accent;
  final VoidCallback onRefresh;
  final bool bordered;
  final AIStatsPanelColors colors;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    return _PanelCard(
      bordered: bordered,
      colors: colors,
      child: Column(
        children: [
          Icon(Icons.query_stats_rounded, color: accent, size: 32),
          const SizedBox(height: AppSpacing.m),
          Text(
            prefs.t('stats.noStats'),
            style: TextStyle(
              color: AppColors.textPrimary,
              fontSize: AppTextSize.title,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          Text(
            prefs.t('stats.emptyHint'),
            textAlign: TextAlign.center,
            style: TextStyle(
              color: AppColors.textMuted,
              fontSize: AppTextSize.body,
            ),
          ),
          const SizedBox(height: AppSpacing.l),
          TextButton(onPressed: onRefresh, child: Text(prefs.t('app.refresh'))),
        ],
      ),
    );
  }
}

final _mutedLabel = TextStyle(
  color: AppColors.textSubtle,
  fontSize: AppTextSize.small,
);

String _formatInt(int value) {
  if (value >= 1000000) return '${(value / 1000000).toStringAsFixed(1)}M';
  if (value >= 1000) return '${(value / 1000).toStringAsFixed(1)}K';
  return '$value';
}

String _formatDate(String raw) {
  final parsed = _parseDateValue(raw);
  if (parsed != null) {
    final local = parsed.toLocal();
    return '${local.year.toString().padLeft(4, '0')}-'
        '${local.month.toString().padLeft(2, '0')}-'
        '${local.day.toString().padLeft(2, '0')}';
  }
  if (raw.length >= 10) return raw.substring(0, 10);
  return raw;
}

String _formatDateTime(String raw) {
  final parsed = _parseDateValue(raw);
  if (parsed != null) {
    final local = parsed.toLocal();
    return '${local.year.toString().padLeft(4, '0')}-'
        '${local.month.toString().padLeft(2, '0')}-'
        '${local.day.toString().padLeft(2, '0')} '
        '${local.hour.toString().padLeft(2, '0')}:'
        '${local.minute.toString().padLeft(2, '0')}';
  }
  return raw;
}

DateTime? _parseDateValue(String raw) {
  final parsed = DateTime.tryParse(raw);
  if (parsed != null) return parsed;
  final numeric = double.tryParse(raw);
  if (numeric == null || !numeric.isFinite || numeric <= 0) return null;
  final value = numeric.round();
  final millis = value >= 1000000000000 ? value : value * 1000;
  return DateTime.fromMillisecondsSinceEpoch(millis, isUtc: true);
}

List<AIStatsTimeBucket> _normalizedTodayBuckets(
  List<AIStatsTimeBucket> buckets,
) {
  if (buckets.isEmpty) {
    return List<AIStatsTimeBucket>.generate(
      48,
      (index) => AIStatsTimeBucket(start: '$index', totalTokens: 0),
    );
  }
  return buckets.take(48).toList(growable: false);
}

List<AIStatsHeatmapDay> _normalizedHeatmap(List<AIStatsHeatmapDay> days) {
  if (days.length >= 28) {
    return days.skip(math.max(0, days.length - 28)).toList(growable: false);
  }
  final result = List<AIStatsHeatmapDay>.generate(
    28 - days.length,
    (index) => AIStatsHeatmapDay(day: '$index', totalTokens: 0),
  );
  return [...result, ...days];
}
