import 'package:flutter/material.dart';
import '../i18n.dart';
import '../models/remote_models.dart';
import '../theme/app_theme.dart';
import 'more_menu.dart';
import 'swipe_list_tile.dart';

class DeviceHomeScreen extends StatelessWidget {
  const DeviceHomeScreen({
    super.key,
    required this.devices,
    required this.activeDeviceId,
    required this.ready,
    required this.status,
    required this.latencyMs,
    required this.deviceSubtitle,
    required this.topInset,
    required this.bottomInset,
    required this.onOpen,
    required this.onConnect,
    required this.onAdd,
    required this.onEdit,
    required this.onDelete,
    required this.onRefresh,
    required this.onSettings,
    required this.onLogs,
    required this.onCheckUpdate,
    required this.onAbout,
  });

  final List<StoredDevice> devices;
  final String? activeDeviceId;
  final bool ready;
  final String status;
  final int? latencyMs;
  final String Function(StoredDevice device) deviceSubtitle;
  final double topInset;
  final double bottomInset;
  final ValueChanged<StoredDevice> onOpen;
  final ValueChanged<StoredDevice> onConnect;
  final VoidCallback onAdd;
  final ValueChanged<StoredDevice> onEdit;
  final ValueChanged<StoredDevice> onDelete;
  final Future<void> Function() onRefresh;
  final VoidCallback onSettings;
  final VoidCallback onLogs;
  final VoidCallback onCheckUpdate;
  final VoidCallback onAbout;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    return Container(
      color: AppColors.bgBase,
      padding: EdgeInsets.fromLTRB(
        AppSpacing.l,
        topInset + AppSpacing.l,
        AppSpacing.l,
        bottomInset + AppSpacing.l,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              Expanded(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Text(
                      'Codux',
                      style: TextStyle(
                        color: AppColors.textPrimary,
                        fontSize: 22,
                        height: 1.05,
                        fontWeight: FontWeight.w900,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      prefs.t('device.homeHint'),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        color: AppColors.textMuted,
                        fontSize: 12,
                        height: 1,
                      ),
                    ),
                  ],
                ),
              ),
              _CircleButton(icon: Icons.settings_outlined, onTap: onSettings),
              const SizedBox(width: AppSpacing.s),
              MoreMenu(
                onAddDevice: onAdd,
                onLogs: onLogs,
                onCheckUpdate: onCheckUpdate,
                onAbout: onAbout,
              ),
            ],
          ),
          const SizedBox(height: AppSpacing.xl),
          Expanded(
            child: RefreshIndicator(
              color: accent,
              backgroundColor: AppColors.bgSurface,
              onRefresh: onRefresh,
              child: devices.isEmpty
                  ? ListView(
                      physics: const AlwaysScrollableScrollPhysics(),
                      padding: EdgeInsets.zero,
                      children: [
                        SizedBox(
                          height: 360,
                          child: _EmptyDeviceState(
                            accent: accent,
                            onAdd: onAdd,
                          ),
                        ),
                      ],
                    )
                  : ListView.separated(
                      physics: const AlwaysScrollableScrollPhysics(),
                      padding: EdgeInsets.zero,
                      itemCount: devices.length,
                      separatorBuilder: (_, _) =>
                          const SizedBox(height: AppSpacing.s),
                      itemBuilder: (context, index) {
                        final device = devices[index];
                        final isActive = device.deviceId == activeDeviceId;
                        final isReady = isActive && ready;
                        final state = isActive
                            ? status
                            : prefs.t('app.notConnected');
                        final title = device.hostName?.isNotEmpty == true
                            ? device.hostName!
                            : device.name;
                        return SwipeListTile(
                          title: title,
                          subtitle: deviceSubtitle(device),
                          leadingIcon: Icons.desktop_mac_outlined,
                          active: isActive,
                          onTap: isReady
                              ? () => onOpen(device)
                              : () => onConnect(device),
                          trailing: Column(
                            mainAxisAlignment: MainAxisAlignment.center,
                            crossAxisAlignment: CrossAxisAlignment.end,
                            children: [
                              _TransportText(
                                active: isActive,
                                ready: isReady,
                                status: state,
                              ),
                              const SizedBox(height: 7),
                              _LatencyText(
                                latencyMs: isReady ? latencyMs : null,
                                ready: isReady,
                              ),
                            ],
                          ),
                          actions: [
                            SwipeListAction(
                              label: prefs.t('device.edit'),
                              color: accent,
                              icon: Icons.edit_outlined,
                              onTap: () => onEdit(device),
                            ),
                            SwipeListAction(
                              label: prefs.t('device.delete'),
                              color: AppColors.danger,
                              icon: Icons.delete_outline_rounded,
                              onTap: () => onDelete(device),
                            ),
                          ],
                        );
                      },
                    ),
            ),
          ),
          SizedBox(
            width: double.infinity,
            height: 48,
            child: FilledButton.icon(
              style: FilledButton.styleFrom(
                backgroundColor: accent.withValues(alpha: 0.16),
                foregroundColor: accent,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(AppRadius.md),
                ),
              ),
              onPressed: onAdd,
              icon: const Icon(Icons.add, size: 18),
              label: Text(prefs.t('device.addByScan')),
            ),
          ),
        ],
      ),
    );
  }
}

class _CircleButton extends StatelessWidget {
  const _CircleButton({required this.icon, required this.onTap});
  final IconData icon;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Material(
    color: AppColors.bgElevated,
    shape: const CircleBorder(),
    child: InkWell(
      customBorder: const CircleBorder(),
      onTap: onTap,
      child: SizedBox(
        width: 42,
        height: 42,
        child: Icon(icon, size: 20, color: AppColors.textPrimary),
      ),
    ),
  );
}

class _EmptyDeviceState extends StatelessWidget {
  const _EmptyDeviceState({required this.accent, required this.onAdd});
  final Color accent;
  final VoidCallback onAdd;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.devices_other_outlined, size: 48, color: accent),
          const SizedBox(height: AppSpacing.m),
          Text(
            prefs.t('device.emptyTitle'),
            style: const TextStyle(
              color: AppColors.textPrimary,
              fontSize: AppTextSize.title,
              fontWeight: FontWeight.w700,
            ),
          ),
          const SizedBox(height: AppSpacing.s),
          Text(
            prefs.t('device.emptySubtitle'),
            style: const TextStyle(
              color: AppColors.textMuted,
              fontSize: AppTextSize.small,
            ),
          ),
          const SizedBox(height: AppSpacing.l),
          FilledButton(
            onPressed: onAdd,
            child: Text(prefs.t('device.scanAdd')),
          ),
        ],
      ),
    );
  }
}

class _TransportText extends StatelessWidget {
  const _TransportText({
    required this.active,
    required this.ready,
    required this.status,
  });
  final bool active;
  final bool ready;
  final String status;

  @override
  Widget build(BuildContext context) {
    final label = status;
    final prefs = AppPreferences.of(context);
    final relayLabel = prefs.t('connection.relay');
    final pending =
        status == prefs.t('app.connecting') ||
        status == prefs.t('app.syncing') ||
        status == prefs.t('app.reconnecting') ||
        status == prefs.t('app.reconnectingShort');
    final color = !active
        ? AppColors.danger
        : pending
        ? AppColors.warning
        : !ready
        ? AppColors.danger
        : status == relayLabel || status.toLowerCase() == 'relay'
        ? AppColors.cyan
        : AppColors.success;
    return Text(
      label,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: TextStyle(
        color: color,
        fontSize: 12,
        height: 1,
        fontWeight: FontWeight.w800,
      ),
    );
  }
}

class _LatencyText extends StatelessWidget {
  const _LatencyText({required this.latencyMs, required this.ready});
  final int? latencyMs;
  final bool ready;

  @override
  Widget build(BuildContext context) {
    final text = ready && latencyMs != null ? '${latencyMs}ms' : '-- ms';
    final color = ready ? AppColors.textSubtle : AppColors.textMuted;
    return Text(
      text,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: TextStyle(
        color: color,
        fontSize: 11,
        height: 1,
        fontWeight: FontWeight.w600,
      ),
    );
  }
}
