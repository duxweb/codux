import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';

import '../i18n.dart';
import '../models/remote_models.dart';
import '../screens/home/home_page.dart';
import '../services/remote_transport.dart';
import '../theme/app_theme.dart';

class CoduxFlutterApp extends StatefulWidget {
  const CoduxFlutterApp({
    super.key,
    this.initialDevices,
    this.transportFactory,
  });

  final List<StoredDevice>? initialDevices;
  final RemoteTransportFactory? transportFactory;

  @override
  State<CoduxFlutterApp> createState() => _CoduxFlutterAppState();
}

class _CoduxFlutterAppState extends State<CoduxFlutterApp>
    with WidgetsBindingObserver {
  AccentOption _accent = AccentChoices.cyan;
  LocaleOption _locale = LocaleChoices.zhCN;
  ThemeMode _themeMode = ThemeMode.system;

  void _setAccent(AccentOption next) => setState(() => _accent = next);
  void _setLocale(LocaleOption next) => setState(() => _locale = next);
  void _setThemeMode(ThemeMode next) => setState(() => _themeMode = next);

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangePlatformBrightness() {
    // When following the system, rebuild so the resolved brightness updates.
    if (_themeMode == ThemeMode.system && mounted) setState(() {});
  }

  Brightness get _brightness {
    switch (_themeMode) {
      case ThemeMode.light:
        return Brightness.light;
      case ThemeMode.dark:
        return Brightness.dark;
      case ThemeMode.system:
        return WidgetsBinding.instance.platformDispatcher.platformBrightness;
    }
  }

  @override
  Widget build(BuildContext context) {
    final brightness = _brightness;
    // Publish the resolved brightness before the tree builds so the color
    // tokens (AppColors / PadColors) read the right values this frame.
    CoduxTheme.brightness = brightness;
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      title: 'Codux Mobile',
      theme: buildAppTheme(accent: _accent.color, brightness: brightness),
      locale: flutterLocaleForOption(_locale),
      supportedLocales: supportedFlutterLocales,
      localizationsDelegates: const [
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ],
      home: AppPreferences(
        accent: _accent,
        locale: _locale,
        themeMode: _themeMode,
        child: CoduxHomePage(
          onChangeAccent: _setAccent,
          onChangeLocale: _setLocale,
          onChangeThemeMode: _setThemeMode,
          initialDevices: widget.initialDevices,
          transportFactory: widget.transportFactory,
        ),
      ),
    );
  }
}
