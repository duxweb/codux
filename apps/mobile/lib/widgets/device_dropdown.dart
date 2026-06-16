import 'package:flutter/material.dart';
import '../i18n.dart';
import '../models/remote_models.dart';
import '../services/remote_path_utils.dart';
import '../services/remote_protocol.dart';
import '../theme/app_theme.dart';
import 'dropdown_overlay.dart';

class DeviceDropdown extends StatelessWidget {
  const DeviceDropdown({
    super.key,
    required this.top,
    required this.devices,
    required this.activeDeviceId,
    required this.onClose,
    required this.onSelect,
    required this.onLongPress,
    required this.onAdd,
  });

  final double top;
  final List<StoredDevice> devices;
  final String? activeDeviceId;
  final VoidCallback onClose;
  final ValueChanged<StoredDevice> onSelect;
  final ValueChanged<StoredDevice> onLongPress;
  final VoidCallback onAdd;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    return DropdownOverlay(
      top: top,
      onClose: onClose,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (devices.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(
                vertical: AppSpacing.l,
                horizontal: AppSpacing.l,
              ),
              child: Text(
                prefs.t('app.devicesEmpty'),
                style: const TextStyle(
                  color: AppColors.textMuted,
                  fontSize: 14,
                ),
              ),
            ),
          for (final device in devices)
            _DeviceRow(
              device: device,
              active: device.deviceId == activeDeviceId,
              accent: accent,
              onTap: () => onSelect(device),
              onLongPress: () => onLongPress(device),
            ),
          const Divider(color: AppColors.border, height: 0.5, thickness: 0.5),
          InkWell(
            onTap: onAdd,
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: AppSpacing.l,
                vertical: AppSpacing.m,
              ),
              child: Row(
                children: [
                  Icon(Icons.add_circle_outline, size: 20, color: accent),
                  const SizedBox(width: AppSpacing.s),
                  Text(
                    prefs.t('app.addDevice'),
                    style: TextStyle(
                      color: accent,
                      fontSize: 14,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _DeviceRow extends StatelessWidget {
  const _DeviceRow({
    required this.device,
    required this.active,
    required this.accent,
    required this.onTap,
    required this.onLongPress,
  });
  final StoredDevice device;
  final bool active;
  final Color accent;
  final VoidCallback onTap;
  final VoidCallback onLongPress;

  @override
  Widget build(BuildContext context) {
    final name = device.hostName?.isNotEmpty == true
        ? device.hostName!
        : device.name;
    final protocol = _deviceConnectionLabel(context, device);
    return InkWell(
      onTap: onTap,
      onLongPress: onLongPress,
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: AppSpacing.l,
          vertical: AppSpacing.m,
        ),
        child: Row(
          children: [
            Container(
              width: 8,
              height: 8,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: active ? AppColors.success : AppColors.textSubtle,
              ),
            ),
            const SizedBox(width: AppSpacing.m),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      color: active ? accent : AppColors.textPrimary,
                      fontSize: 14,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    protocol,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: AppColors.textMuted,
                      fontSize: 12,
                    ),
                  ),
                ],
              ),
            ),
            if (active) Icon(Icons.check, color: accent, size: 18),
          ],
        ),
      ),
    );
  }
}

String _deviceProtocolLabel(BuildContext context, String transport) {
  return switch (transport.toLowerCase()) {
    RemoteTransportKind.iroh => 'Iroh',
    _ => transport.toUpperCase(),
  };
}

String _deviceConnectionLabel(BuildContext context, StoredDevice device) {
  final relay = _deviceRelayEndpoint(device);
  if (relay.isNotEmpty) return remoteRelayDisplayName(relay);
  return _deviceProtocolLabel(context, _deviceTransportKind(device));
}

String _deviceRelayEndpoint(StoredDevice device) {
  for (final candidate in device.transports) {
    final relayUrl = candidate.relayUrl.trim();
    if (relayUrl.isNotEmpty) return cleanRemoteTransportEndpoint(relayUrl);
  }
  for (final candidate in device.transports) {
    final url = candidate.url.trim();
    if (url.isNotEmpty) return cleanRemoteTransportEndpoint(url);
  }
  return cleanRemoteTransportEndpoint(device.server);
}

String _deviceTransportKind(StoredDevice device) {
  final kind = remotePreferredTransportKind(device.transports, pairing: false);
  return kind.isEmpty ? RemoteTransportKind.iroh : kind;
}
