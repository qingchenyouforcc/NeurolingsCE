import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_zh.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppLocalizations
/// returned by `AppLocalizations.of(context)`.
///
/// Applications need to include `AppLocalizations.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppLocalizations.localizationsDelegates,
///   supportedLocales: AppLocalizations.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppLocalizations.supportedLocales
/// property.
abstract class AppLocalizations {
  AppLocalizations(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppLocalizations of(BuildContext context) {
    return Localizations.of<AppLocalizations>(context, AppLocalizations)!;
  }

  static const LocalizationsDelegate<AppLocalizations> delegate =
      _AppLocalizationsDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('zh'),
  ];

  /// No description provided for @appTitle.
  ///
  /// In en, this message translates to:
  /// **'NeurolingsCE Manager'**
  String get appTitle;

  /// No description provided for @navHome.
  ///
  /// In en, this message translates to:
  /// **'Home'**
  String get navHome;

  /// No description provided for @navCreate.
  ///
  /// In en, this message translates to:
  /// **'Create'**
  String get navCreate;

  /// No description provided for @navStore.
  ///
  /// In en, this message translates to:
  /// **'Store'**
  String get navStore;

  /// No description provided for @navCombinations.
  ///
  /// In en, this message translates to:
  /// **'Combinations'**
  String get navCombinations;

  /// No description provided for @navCodex.
  ///
  /// In en, this message translates to:
  /// **'Codex'**
  String get navCodex;

  /// No description provided for @navSettings.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get navSettings;

  /// No description provided for @navAbout.
  ///
  /// In en, this message translates to:
  /// **'About'**
  String get navAbout;

  /// No description provided for @runtimeOnline.
  ///
  /// In en, this message translates to:
  /// **'Runtime online'**
  String get runtimeOnline;

  /// No description provided for @runtimeOffline.
  ///
  /// In en, this message translates to:
  /// **'Runtime offline'**
  String get runtimeOffline;

  /// No description provided for @startRuntime.
  ///
  /// In en, this message translates to:
  /// **'Start runtime'**
  String get startRuntime;

  /// No description provided for @loadedMascots.
  ///
  /// In en, this message translates to:
  /// **'Installed mascots'**
  String get loadedMascots;

  /// No description provided for @runningMascots.
  ///
  /// In en, this message translates to:
  /// **'Running mascots'**
  String get runningMascots;

  /// No description provided for @spawn.
  ///
  /// In en, this message translates to:
  /// **'Summon'**
  String get spawn;

  /// No description provided for @dismiss.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get dismiss;

  /// No description provided for @dismissAll.
  ///
  /// In en, this message translates to:
  /// **'Close all'**
  String get dismissAll;

  /// No description provided for @refresh.
  ///
  /// In en, this message translates to:
  /// **'Refresh'**
  String get refresh;

  /// No description provided for @noTemplates.
  ///
  /// In en, this message translates to:
  /// **'No mascot templates installed. Import one from the Create page.'**
  String get noTemplates;

  /// No description provided for @noRunning.
  ///
  /// In en, this message translates to:
  /// **'No mascots are running.'**
  String get noRunning;

  /// No description provided for @createTitle.
  ///
  /// In en, this message translates to:
  /// **'Import a mascot package'**
  String get createTitle;

  /// No description provided for @createHint.
  ///
  /// In en, this message translates to:
  /// **'Select a Shimeji-ee zip archive or a .mascot package to import it into the local storage.'**
  String get createHint;

  /// No description provided for @pickArchive.
  ///
  /// In en, this message translates to:
  /// **'Choose archive...'**
  String get pickArchive;

  /// No description provided for @importing.
  ///
  /// In en, this message translates to:
  /// **'Importing...'**
  String get importing;

  /// No description provided for @importDone.
  ///
  /// In en, this message translates to:
  /// **'Import finished: {result}'**
  String importDone(Object result);

  /// No description provided for @storePlaceholder.
  ///
  /// In en, this message translates to:
  /// **'The mascot store arrives in milestone M7.'**
  String get storePlaceholder;

  /// No description provided for @combinationsPlaceholder.
  ///
  /// In en, this message translates to:
  /// **'Mascot combinations arrive in milestone M8.'**
  String get combinationsPlaceholder;

  /// No description provided for @codexPlaceholder.
  ///
  /// In en, this message translates to:
  /// **'Codex integration arrives in milestone M8.'**
  String get codexPlaceholder;

  /// No description provided for @settingsLanguage.
  ///
  /// In en, this message translates to:
  /// **'Language'**
  String get settingsLanguage;

  /// No description provided for @settingsRuntime.
  ///
  /// In en, this message translates to:
  /// **'Runtime'**
  String get settingsRuntime;

  /// No description provided for @settingsStorage.
  ///
  /// In en, this message translates to:
  /// **'Mascot storage'**
  String get settingsStorage;

  /// No description provided for @settingsHttpHint.
  ///
  /// In en, this message translates to:
  /// **'The HTTP API listens on 127.0.0.1:32456 when enabled via NEUROLINGSCE_HTTP=1.'**
  String get settingsHttpHint;

  /// No description provided for @aboutDescription.
  ///
  /// In en, this message translates to:
  /// **'NeurolingsCE is a cross-platform desktop mascot (Shimeji) runner, rewritten in Rust + Flutter.'**
  String get aboutDescription;

  /// No description provided for @version.
  ///
  /// In en, this message translates to:
  /// **'Version'**
  String get version;

  /// No description provided for @license.
  ///
  /// In en, this message translates to:
  /// **'License'**
  String get license;

  /// No description provided for @error.
  ///
  /// In en, this message translates to:
  /// **'Error'**
  String get error;

  /// No description provided for @ok.
  ///
  /// In en, this message translates to:
  /// **'OK'**
  String get ok;
}

class _AppLocalizationsDelegate
    extends LocalizationsDelegate<AppLocalizations> {
  const _AppLocalizationsDelegate();

  @override
  Future<AppLocalizations> load(Locale locale) {
    return SynchronousFuture<AppLocalizations>(lookupAppLocalizations(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en', 'zh'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppLocalizationsDelegate old) => false;
}

AppLocalizations lookupAppLocalizations(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppLocalizationsEn();
    case 'zh':
      return AppLocalizationsZh();
  }

  throw FlutterError(
    'AppLocalizations.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
