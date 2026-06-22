import 'package:flutter/material.dart';
import '../../i18n.dart';
import '../../models/remote_models.dart';
import '../../services/remote_protocol.dart';
import '../../theme/app_theme.dart';

class PairingOverlay extends StatelessWidget {
  const PairingOverlay({
    super.key,
    required this.payload,
    required this.waiting,
    required this.errorMessage,
    required this.onCancel,
    required this.onConfirm,
  });

  final PairingPayload payload;
  final bool waiting;
  final String? errorMessage;
  final VoidCallback onCancel;
  final VoidCallback onConfirm;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    final hostName = payload.hostName?.trim().isNotEmpty == true
        ? payload.hostName!.trim()
        : 'Codux Mac';
    final code = payload.code;
    final transport = _pairingTransportKind(payload).toUpperCase();

    return Positioned.fill(
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: waiting ? null : onCancel,
        child: ColoredBox(
          color: AppColors.backdrop,
          child: Center(
            child: GestureDetector(
              onTap: () {},
              child: Container(
                width: 320,
                margin: const EdgeInsets.symmetric(horizontal: AppSpacing.l),
                padding: const EdgeInsets.fromLTRB(
                  AppSpacing.xl,
                  AppSpacing.xl,
                  AppSpacing.xl,
                  AppSpacing.l,
                ),
                decoration: BoxDecoration(
                  color: AppColors.bgSurface,
                  borderRadius: BorderRadius.circular(AppRadius.lg),
                  border: Border.all(color: AppColors.border, width: 0.5),
                  boxShadow: const [
                    BoxShadow(
                      color: Color(0x66000000),
                      blurRadius: 32,
                      offset: Offset(0, 12),
                    ),
                  ],
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Container(
                      width: 56,
                      height: 56,
                      decoration: BoxDecoration(
                        color: accent.withValues(alpha: 0.16),
                        borderRadius: BorderRadius.circular(999),
                      ),
                      alignment: Alignment.center,
                      child: waiting
                          ? SizedBox(
                              width: 28,
                              height: 28,
                              child: CircularProgressIndicator(
                                strokeWidth: 2.4,
                                color: accent,
                              ),
                            )
                          : Icon(
                              Icons.desktop_mac_outlined,
                              size: 28,
                              color: accent,
                            ),
                    ),
                    const SizedBox(height: AppSpacing.l),
                    Text(
                      waiting
                          ? prefs.t('pair.waitingTitle')
                          : prefs.t('pair.confirmTitle'),
                      style: TextStyle(
                        color: AppColors.textPrimary,
                        fontSize: AppTextSize.title,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    const SizedBox(height: AppSpacing.s),
                    Text(
                      waiting
                          ? prefs.t('pair.waitingBody')
                          : prefs.t('pair.confirmBody'),
                      textAlign: TextAlign.center,
                      style: TextStyle(
                        color: AppColors.textMuted,
                        fontSize: 12,
                        height: 1.5,
                      ),
                    ),
                    const SizedBox(height: AppSpacing.l),
                    _InfoRow(
                      label: prefs.t('pair.device'),
                      value: hostName,
                      hint: transport,
                    ),
                    const SizedBox(height: AppSpacing.s),
                    _CodeBlock(code: code, accent: accent),
                    if (errorMessage != null) ...[
                      const SizedBox(height: AppSpacing.m),
                      Text(
                        errorMessage!,
                        textAlign: TextAlign.center,
                        style: const TextStyle(
                          color: AppColors.danger,
                          fontSize: 12,
                          height: 1.4,
                        ),
                      ),
                    ],
                    const SizedBox(height: AppSpacing.l),
                    Row(
                      children: [
                        Expanded(
                          child: SizedBox(
                            height: 44,
                            child: OutlinedButton(
                              onPressed: onCancel,
                              style: OutlinedButton.styleFrom(
                                foregroundColor: AppColors.textPrimary,
                                backgroundColor: AppColors.bgElevated,
                                side: BorderSide.none,
                                shape: RoundedRectangleBorder(
                                  borderRadius: BorderRadius.circular(
                                    AppRadius.md,
                                  ),
                                ),
                              ),
                              child: Text(
                                prefs.t('app.cancel'),
                                style: const TextStyle(
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                            ),
                          ),
                        ),
                        const SizedBox(width: AppSpacing.s),
                        Expanded(
                          child: SizedBox(
                            height: 44,
                            child: FilledButton(
                              onPressed: waiting ? null : onConfirm,
                              style: FilledButton.styleFrom(
                                backgroundColor: accent,
                                foregroundColor: AppColors.bgBase,
                                disabledBackgroundColor: accent.withValues(
                                  alpha: 0.32,
                                ),
                                disabledForegroundColor: AppColors.bgBase
                                    .withValues(alpha: 0.6),
                                shape: RoundedRectangleBorder(
                                  borderRadius: BorderRadius.circular(
                                    AppRadius.md,
                                  ),
                                ),
                              ),
                              child: Text(
                                waiting
                                    ? prefs.t('pair.pairing')
                                    : prefs.t('pair.submit'),
                                style: const TextStyle(
                                  fontWeight: FontWeight.w700,
                                ),
                              ),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

String _pairingTransportKind(PairingPayload payload) {
  final kind = remotePreferredTransportKind(payload.transports, pairing: true);
  return kind.isEmpty ? RemoteTransportKind.iroh : kind;
}

class _InfoRow extends StatelessWidget {
  const _InfoRow({
    required this.label,
    required this.value,
    required this.hint,
  });
  final String label;
  final String value;
  final String hint;

  static const double _labelWidth = 72;

  @override
  Widget build(BuildContext context) => Container(
    width: double.infinity,
    padding: const EdgeInsets.symmetric(
      horizontal: AppSpacing.m,
      vertical: AppSpacing.s,
    ),
    decoration: BoxDecoration(
      color: AppColors.bgElevated,
      borderRadius: BorderRadius.circular(AppRadius.sm),
    ),
    child: Row(
      children: [
        SizedBox(
          width: _labelWidth,
          child: Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            softWrap: false,
            style: TextStyle(color: AppColors.textMuted, fontSize: 12),
          ),
        ),
        const SizedBox(width: AppSpacing.s),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                value,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: AppColors.textPrimary,
                  fontSize: 14,
                  fontWeight: FontWeight.w600,
                ),
              ),
              if (hint.isNotEmpty)
                Text(
                  hint,
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
      ],
    ),
  );
}

class _CodeBlock extends StatelessWidget {
  const _CodeBlock({required this.code, required this.accent});
  final String code;
  final Color accent;

  static const double _labelWidth = 72;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        horizontal: AppSpacing.m,
        vertical: AppSpacing.m,
      ),
      decoration: BoxDecoration(
        color: accent.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(AppRadius.sm),
      ),
      child: Row(
        children: [
          SizedBox(
            width: _labelWidth,
            child: Text(
              prefs.t('pair.matchCode'),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              softWrap: false,
              style: TextStyle(color: AppColors.textMuted, fontSize: 12),
            ),
          ),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Text(
              code,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                color: accent,
                fontSize: 18,
                fontWeight: FontWeight.w800,
                letterSpacing: 2.0,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
