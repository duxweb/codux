import 'package:flutter/material.dart';
import '../i18n.dart';
import '../models/remote_models.dart';
import '../theme/app_theme.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({
    super.key,
    required this.nameController,
    required this.detectedName,
    required this.topInset,
    required this.bottomInset,
    required this.currentAccent,
    required this.currentLocale,
    required this.currentThemeMode,
    required this.currentLogLevel,
    required this.appTextScale,
    required this.terminalFontSize,
    required this.onChangeAccent,
    required this.onChangeLocale,
    required this.onChangeThemeMode,
    required this.onChangeLogLevel,
    required this.onChangeAppTextScale,
    required this.onChangeTerminalFontSize,
    required this.onUseDetectedName,
    required this.onSave,
    required this.onBack,
  });

  final TextEditingController nameController;
  final String detectedName;
  final double topInset;
  final double bottomInset;
  final AccentOption currentAccent;
  final LocaleOption currentLocale;
  final ThemeMode currentThemeMode;
  final String currentLogLevel;
  final double appTextScale;
  final double terminalFontSize;
  final ValueChanged<AccentOption> onChangeAccent;
  final ValueChanged<LocaleOption> onChangeLocale;
  final ValueChanged<ThemeMode> onChangeThemeMode;
  final ValueChanged<String> onChangeLogLevel;
  final ValueChanged<double> onChangeAppTextScale;
  final ValueChanged<double> onChangeTerminalFontSize;
  final VoidCallback onUseDetectedName;
  final VoidCallback onSave;
  final VoidCallback onBack;

  @override
  Widget build(BuildContext context) {
    final accent = Theme.of(context).colorScheme.secondary;
    final prefs = AppPreferences.of(context);
    return Container(
      color: AppColors.bgBase,
      child: Column(
        children: [
          _LargeHeader(
            title: prefs.t('settings.title'),
            topInset: topInset,
            onBack: onBack,
          ),
          Expanded(
            child: ListView(
              physics: const BouncingScrollPhysics(),
              padding: EdgeInsets.fromLTRB(
                AppSpacing.l,
                AppSpacing.s,
                AppSpacing.l,
                AppSpacing.xxl + bottomInset,
              ),
              children: [
                _SectionLabel(prefs.t('settings.nameLabel')),
                _Card(
                  children: [
                    _TextFieldTile(
                      controller: nameController,
                      hint: prefs.t('settings.nameHint'),
                      accent: accent,
                    ),
                    _Divider(),
                    _ActionTile(
                      icon: Icons.phone_android,
                      label: prefs.t(
                        'settings.useDeviceName',
                        params: {'name': detectedName},
                      ),
                      accent: accent,
                      onTap: onUseDetectedName,
                    ),
                  ],
                ),
                const SizedBox(height: AppSpacing.l),
                _SectionLabel(prefs.t('settings.themeLabel')),
                _Card(
                  children: [
                    _ThemeModeRow(
                      current: currentThemeMode,
                      accent: accent,
                      onSelect: onChangeThemeMode,
                    ),
                    _Divider(),
                    _AccentRow(
                      current: currentAccent,
                      onSelect: onChangeAccent,
                    ),
                  ],
                ),
                const SizedBox(height: AppSpacing.l),
                _SectionLabel(prefs.t('settings.displayLabel')),
                _Card(
                  children: [
                    _StepSliderTile(
                      label: prefs.t('settings.appFontSize'),
                      valueLabel: _appTextScaleLabel(prefs, appTextScale),
                      value: appTextScale,
                      steps: MobileSettings.appTextScaleSteps,
                      stepLabels: _appFontStepLabels(prefs),
                      accent: accent,
                      onChanged: onChangeAppTextScale,
                    ),
                    _Divider(),
                    _StepSliderTile(
                      label: prefs.t('settings.terminalFontSize'),
                      valueLabel: _fontSizeValueLabel(
                        prefs,
                        terminalFontSize,
                        MobileSettings.standardTerminalFontSize,
                      ),
                      value: terminalFontSize,
                      steps: MobileSettings.terminalFontSizeSteps,
                      stepLabels: _terminalFontStepLabels(prefs),
                      accent: accent,
                      onChanged: onChangeTerminalFontSize,
                    ),
                  ],
                ),
                const SizedBox(height: AppSpacing.l),
                _SectionLabel(prefs.t('settings.localeLabel')),
                _Card(
                  children: [
                    _PickerTile(
                      label: prefs.t('settings.localeLabel'),
                      value: currentLocale.label,
                      onTap: () => _showLocalePicker(
                        context,
                        accent: accent,
                        current: currentLocale,
                        title: prefs.t('settings.localeLabel'),
                        cancelLabel: prefs.t('app.cancel'),
                        onSelect: onChangeLocale,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: AppSpacing.l),
                _SectionLabel(prefs.t('settings.logLevelLabel')),
                _Card(
                  children: [
                    _PickerTile(
                      label: prefs.t('settings.logLevelLabel'),
                      value: prefs.t('logLevel.$currentLogLevel'),
                      onTap: () => _showLogLevelPicker(
                        context,
                        accent: accent,
                        current: currentLogLevel,
                        title: prefs.t('settings.logLevelLabel'),
                        cancelLabel: prefs.t('app.cancel'),
                        onSelect: onChangeLogLevel,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: AppSpacing.xl),
                _PrimaryButton(
                  label: prefs.t('settings.save'),
                  accent: accent,
                  onTap: onSave,
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

const _logLevels = ['warn', 'info', 'debug'];

void _showLogLevelPicker(
  BuildContext context, {
  required Color accent,
  required String current,
  required String title,
  required String cancelLabel,
  required ValueChanged<String> onSelect,
}) {
  final prefs = AppPreferences.of(context);
  showModalBottomSheet<void>(
    context: context,
    backgroundColor: Colors.transparent,
    isScrollControlled: true,
    barrierColor: AppColors.backdrop,
    builder: (ctx) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.s),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: double.infinity,
              decoration: BoxDecoration(
                color: AppColors.bgSurface,
                borderRadius: BorderRadius.circular(AppRadius.lg),
              ),
              clipBehavior: Clip.antiAlias,
              child: Column(
                children: [
                  Container(
                    height: 48,
                    alignment: Alignment.center,
                    child: Text(
                      title,
                      style: TextStyle(
                        color: AppColors.textMuted,
                        fontSize: AppTextSize.body,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  Divider(
                    color: AppColors.border,
                    height: 0.5,
                    thickness: 0.5,
                  ),
                  for (final level in _logLevels) ...[
                    InkWell(
                      onTap: () {
                        onSelect(level);
                        Navigator.of(ctx).pop();
                      },
                      child: Container(
                        height: 56,
                        alignment: Alignment.centerLeft,
                        padding: const EdgeInsets.symmetric(
                          horizontal: AppSpacing.l,
                        ),
                        child: Row(
                          children: [
                            Expanded(
                              child: Text(
                                prefs.t('logLevel.$level'),
                                style: TextStyle(
                                  color: level == current
                                      ? accent
                                      : AppColors.textPrimary,
                                  fontSize: AppTextSize.title,
                                  fontWeight: level == current
                                      ? FontWeight.w700
                                      : FontWeight.w500,
                                ),
                              ),
                            ),
                            if (level == current)
                              Icon(Icons.check, color: accent),
                          ],
                        ),
                      ),
                    ),
                    if (level != _logLevels.last)
                      Divider(
                        color: AppColors.border,
                        height: 0.5,
                        thickness: 0.5,
                      ),
                  ],
                ],
              ),
            ),
            const SizedBox(height: AppSpacing.s),
            Container(
              width: double.infinity,
              decoration: BoxDecoration(
                color: AppColors.bgSurface,
                borderRadius: BorderRadius.circular(AppRadius.lg),
              ),
              clipBehavior: Clip.antiAlias,
              child: InkWell(
                onTap: () => Navigator.pop(ctx),
                child: Container(
                  height: 56,
                  alignment: Alignment.center,
                  child: Text(
                    cancelLabel,
                    style: TextStyle(
                      color: AppColors.textPrimary,
                      fontSize: AppTextSize.title,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    ),
  );
}

void _showLocalePicker(
  BuildContext context, {
  required Color accent,
  required LocaleOption current,
  required String title,
  required String cancelLabel,
  required ValueChanged<LocaleOption> onSelect,
}) {
  showModalBottomSheet<void>(
    context: context,
    backgroundColor: Colors.transparent,
    isScrollControlled: true,
    barrierColor: AppColors.backdrop,
    builder: (ctx) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(AppSpacing.s),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: double.infinity,
              decoration: BoxDecoration(
                color: AppColors.bgSurface,
                borderRadius: BorderRadius.circular(AppRadius.lg),
              ),
              clipBehavior: Clip.antiAlias,
              child: Column(
                children: [
                  Container(
                    height: 48,
                    alignment: Alignment.center,
                    child: Text(
                      title,
                      style: TextStyle(
                        color: AppColors.textMuted,
                        fontSize: AppTextSize.body,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  Divider(
                    color: AppColors.border,
                    height: 0.5,
                    thickness: 0.5,
                  ),
                  SizedBox(
                    height: (LocaleChoices.all.length * 56.0).clamp(
                      0.0,
                      MediaQuery.sizeOf(ctx).height * 0.56,
                    ),
                    child: ListView.separated(
                      padding: EdgeInsets.zero,
                      physics: const BouncingScrollPhysics(),
                      itemCount: LocaleChoices.all.length,
                      separatorBuilder: (_, _) => Divider(
                        color: AppColors.border,
                        height: 0.5,
                        thickness: 0.5,
                      ),
                      itemBuilder: (_, index) {
                        final option = LocaleChoices.all[index];
                        final active = option.id == current.id;
                        return InkWell(
                          onTap: () {
                            onSelect(option);
                            Navigator.of(ctx).pop();
                          },
                          child: Container(
                            height: 56,
                            alignment: Alignment.centerLeft,
                            padding: const EdgeInsets.symmetric(
                              horizontal: AppSpacing.l,
                            ),
                            child: Row(
                              children: [
                                Expanded(
                                  child: Text(
                                    option.label,
                                    style: TextStyle(
                                      color: active
                                          ? accent
                                          : AppColors.textPrimary,
                                      fontSize: AppTextSize.title,
                                      fontWeight: active
                                          ? FontWeight.w700
                                          : FontWeight.w500,
                                    ),
                                  ),
                                ),
                                if (active) Icon(Icons.check, color: accent),
                              ],
                            ),
                          ),
                        );
                      },
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(height: AppSpacing.s),
            Container(
              width: double.infinity,
              decoration: BoxDecoration(
                color: AppColors.bgSurface,
                borderRadius: BorderRadius.circular(AppRadius.lg),
              ),
              clipBehavior: Clip.antiAlias,
              child: InkWell(
                onTap: () => Navigator.pop(ctx),
                child: Container(
                  height: 56,
                  alignment: Alignment.center,
                  child: Text(
                    cancelLabel,
                    style: TextStyle(
                      color: AppColors.textPrimary,
                      fontSize: AppTextSize.title,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    ),
  );
}

class _LargeHeader extends StatelessWidget {
  const _LargeHeader({
    required this.title,
    required this.topInset,
    required this.onBack,
  });
  final String title;
  final double topInset;
  final VoidCallback onBack;

  @override
  Widget build(BuildContext context) => Container(
    height: AppLayout.topBarHeight + topInset,
    padding: EdgeInsets.only(top: topInset),
    decoration: BoxDecoration(
      color: AppColors.bgBase,
      border: Border(bottom: BorderSide(color: AppColors.border, width: 0.5)),
    ),
    child: Stack(
      alignment: Alignment.center,
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: Padding(
            padding: const EdgeInsets.only(left: AppSpacing.s),
            child: IconButton(
              onPressed: onBack,
              icon: Icon(
                Icons.chevron_left,
                color: AppColors.textPrimary,
                size: 28,
              ),
              visualDensity: VisualDensity.compact,
            ),
          ),
        ),
        Center(
          child: Text(
            title,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              color: AppColors.textPrimary,
              fontSize: AppTextSize.title,
              fontWeight: FontWeight.w700,
            ),
          ),
        ),
      ],
    ),
  );
}

class _SectionLabel extends StatelessWidget {
  const _SectionLabel(this.text);
  final String text;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(
      AppSpacing.m,
      AppSpacing.l,
      AppSpacing.m,
      AppSpacing.s,
    ),
    child: Text(
      text.toUpperCase(),
      style: TextStyle(
        color: AppColors.textMuted,
        fontSize: AppTextSize.small,
        fontWeight: FontWeight.w700,
        letterSpacing: 0.8,
      ),
    ),
  );
}

class _Card extends StatelessWidget {
  const _Card({required this.children});
  final List<Widget> children;
  @override
  Widget build(BuildContext context) => Container(
    decoration: BoxDecoration(
      color: AppColors.bgSurface,
      borderRadius: BorderRadius.circular(AppRadius.md),
      border: Border.all(color: AppColors.border, width: 0.5),
    ),
    clipBehavior: Clip.antiAlias,
    child: Column(children: children),
  );
}

class _Divider extends StatelessWidget {
  @override
  Widget build(BuildContext context) => Padding(
    padding: EdgeInsets.only(left: AppSpacing.l),
    child: Divider(height: 0.5, thickness: 0.5, color: AppColors.border),
  );
}

class _TextFieldTile extends StatelessWidget {
  const _TextFieldTile({
    required this.controller,
    required this.hint,
    required this.accent,
  });
  final TextEditingController controller;
  final String hint;
  final Color accent;
  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: AppSpacing.l),
    child: TextField(
      controller: controller,
      style: TextStyle(
        color: AppColors.textPrimary,
        fontSize: AppTextSize.body,
      ),
      cursorColor: accent,
      decoration: InputDecoration(
        hintText: hint,
        hintStyle: TextStyle(color: AppColors.textSubtle),
        border: InputBorder.none,
        focusedBorder: InputBorder.none,
        enabledBorder: InputBorder.none,
        contentPadding: const EdgeInsets.symmetric(vertical: 14),
        isDense: true,
      ),
    ),
  );
}

class _ActionTile extends StatelessWidget {
  const _ActionTile({
    required this.icon,
    required this.label,
    required this.accent,
    required this.onTap,
  });
  final IconData icon;
  final String label;
  final Color accent;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) => InkWell(
    onTap: onTap,
    child: Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: AppSpacing.l,
        vertical: AppSpacing.m,
      ),
      child: Row(
        children: [
          Icon(icon, size: 18, color: accent),
          const SizedBox(width: AppSpacing.s),
          Expanded(
            child: Text(
              label,
              style: TextStyle(color: accent, fontSize: AppTextSize.body),
            ),
          ),
          Icon(
            Icons.chevron_right,
            size: 18,
            color: AppColors.textSubtle,
          ),
        ],
      ),
    ),
  );
}

class _PickerTile extends StatelessWidget {
  const _PickerTile({
    required this.label,
    required this.value,
    required this.onTap,
  });
  final String label;
  final String value;
  final VoidCallback onTap;
  @override
  Widget build(BuildContext context) => InkWell(
    onTap: onTap,
    child: Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: AppSpacing.l,
        vertical: AppSpacing.m + 2,
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              label,
              style: TextStyle(
                color: AppColors.textPrimary,
                fontSize: AppTextSize.body,
              ),
            ),
          ),
          Text(
            value,
            style: TextStyle(
              color: AppColors.textMuted,
              fontSize: AppTextSize.body,
            ),
          ),
          const SizedBox(width: AppSpacing.xs),
          Icon(
            Icons.chevron_right,
            size: 18,
            color: AppColors.textSubtle,
          ),
        ],
      ),
    ),
  );
}

class _StepSliderTile extends StatelessWidget {
  const _StepSliderTile({
    required this.label,
    required this.valueLabel,
    required this.value,
    required this.steps,
    required this.stepLabels,
    required this.accent,
    required this.onChanged,
  });

  final String label;
  final String valueLabel;
  final double value;
  final List<double> steps;
  final List<String> stepLabels;
  final Color accent;
  final ValueChanged<double> onChanged;

  int get _index {
    var selected = 0;
    for (var i = 1; i < steps.length; i++) {
      if ((steps[i] - value).abs() < (steps[selected] - value).abs()) {
        selected = i;
      }
    }
    return selected;
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(
      AppSpacing.l,
      AppSpacing.m,
      AppSpacing.l,
      AppSpacing.s,
    ),
    child: Column(
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                label,
                style: TextStyle(
                  color: AppColors.textPrimary,
                  fontSize: AppTextSize.body,
                ),
              ),
            ),
            Text(
              valueLabel,
              style: TextStyle(
                color: accent,
                fontSize: AppTextSize.body,
                fontWeight: FontWeight.w700,
              ),
            ),
          ],
        ),
        SliderTheme(
          data: SliderTheme.of(context).copyWith(
            activeTrackColor: accent,
            inactiveTrackColor: AppColors.border,
            thumbColor: accent,
            overlayColor: accent.withValues(alpha: 0.16),
            trackHeight: 4,
          ),
          child: Slider(
            value: _index.toDouble(),
            min: 0,
            max: (steps.length - 1).toDouble(),
            divisions: steps.length - 1,
            onChanged: (next) => onChanged(steps[next.round()]),
          ),
        ),
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            for (final label in stepLabels)
              Text(
                label,
                style: TextStyle(
                  color: AppColors.textMuted,
                  fontSize: AppTextSize.small,
                ),
              ),
          ],
        ),
      ],
    ),
  );
}

List<String> _appFontStepLabels(AppPreferences prefs) => [
  prefs.t('settings.fontSmall'),
  prefs.t('settings.fontStandard'),
  prefs.t('settings.fontLarge'),
];

List<String> _terminalFontStepLabels(AppPreferences prefs) => [
  prefs.t('settings.fontSmall'),
  prefs.t('settings.fontSmaller'),
  prefs.t('settings.fontStandard'),
  prefs.t('settings.fontLarge'),
  prefs.t('settings.fontExtraLarge'),
];

String _appTextScaleLabel(AppPreferences prefs, double value) {
  if (value < MobileSettings.defaultAppTextScale) {
    return prefs.t('settings.fontSmall');
  }
  if (value > MobileSettings.defaultAppTextScale) {
    return prefs.t('settings.fontLarge');
  }
  return prefs.t('settings.fontStandard');
}

String _fontSizeValueLabel(
  AppPreferences prefs,
  double value,
  double standard,
) {
  if (value <= 12) return prefs.t('settings.fontSmall');
  if (value < standard) return prefs.t('settings.fontSmaller');
  if (value >= 18) return prefs.t('settings.fontExtraLarge');
  if (value > standard) return prefs.t('settings.fontLarge');
  return prefs.t('settings.fontStandard');
}

class _AccentRow extends StatelessWidget {
  const _AccentRow({required this.current, required this.onSelect});
  final AccentOption current;
  final ValueChanged<AccentOption> onSelect;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(
      horizontal: AppSpacing.l,
      vertical: AppSpacing.m,
    ),
    child: SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      physics: const BouncingScrollPhysics(),
      child: Row(
        children: [
          for (final option in AccentChoices.all) ...[
            _AccentDot(
              option: option,
              active: option.id == current.id,
              onTap: () => onSelect(option),
            ),
            const SizedBox(width: AppSpacing.m),
          ],
        ],
      ),
    ),
  );
}

class _ThemeModeRow extends StatelessWidget {
  const _ThemeModeRow({
    required this.current,
    required this.accent,
    required this.onSelect,
  });
  final ThemeMode current;
  final Color accent;
  final ValueChanged<ThemeMode> onSelect;

  @override
  Widget build(BuildContext context) {
    final prefs = AppPreferences.of(context);
    final options = <(ThemeMode, IconData, String)>[
      (ThemeMode.system, Icons.brightness_auto_rounded, prefs.t('settings.themeSystem')),
      (ThemeMode.light, Icons.light_mode_rounded, prefs.t('settings.themeLight')),
      (ThemeMode.dark, Icons.dark_mode_rounded, prefs.t('settings.themeDark')),
    ];
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: AppSpacing.l,
        vertical: AppSpacing.m,
      ),
      child: Container(
        padding: const EdgeInsets.all(3),
        decoration: BoxDecoration(
          color: AppColors.bgElevated,
          borderRadius: BorderRadius.circular(AppRadius.md),
        ),
        child: Row(
          children: [
            for (final (mode, icon, label) in options)
              Expanded(
                child: GestureDetector(
                  onTap: () => onSelect(mode),
                  child: AnimatedContainer(
                    duration: const Duration(milliseconds: 160),
                    height: 36,
                    decoration: BoxDecoration(
                      color: mode == current
                          ? accent.withValues(alpha: 0.18)
                          : Colors.transparent,
                      borderRadius: BorderRadius.circular(AppRadius.sm),
                    ),
                    alignment: Alignment.center,
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          icon,
                          size: 15,
                          color: mode == current ? accent : AppColors.textMuted,
                        ),
                        const SizedBox(width: 5),
                        Text(
                          label,
                          style: TextStyle(
                            color: mode == current
                                ? accent
                                : AppColors.textSecondary,
                            fontSize: 12.5,
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                      ],
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

class _AccentDot extends StatelessWidget {
  const _AccentDot({
    required this.option,
    required this.active,
    required this.onTap,
  });
  final AccentOption option;
  final bool active;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => GestureDetector(
    onTap: onTap,
    child: SizedBox(
      width: 38,
      height: 38,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 160),
        padding: const EdgeInsets.all(4),
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          border: active
              ? Border.all(color: AppColors.textPrimary, width: 2)
              : null,
        ),
        child: Container(
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: option.color,
            border: Border.all(
              color: active ? AppColors.bgBase : Colors.transparent,
              width: 2,
            ),
          ),
          child: active
              ? Icon(Icons.check, size: 16, color: AppColors.bgBase)
              : null,
        ),
      ),
    ),
  );
}

class _PrimaryButton extends StatelessWidget {
  const _PrimaryButton({
    required this.label,
    required this.accent,
    required this.onTap,
  });
  final String label;
  final Color accent;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: double.infinity,
    height: 50,
    child: FilledButton(
      onPressed: onTap,
      style: FilledButton.styleFrom(
        backgroundColor: accent,
        foregroundColor: AppColors.bgBase,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(AppRadius.md),
        ),
      ),
      child: Text(
        label,
        style: const TextStyle(
          fontSize: AppTextSize.title,
          fontWeight: FontWeight.w700,
          letterSpacing: 0.2,
        ),
      ),
    ),
  );
}
