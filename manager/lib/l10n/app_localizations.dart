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
  /// **'NeurolingsCE — Mascot Manager'**
  String get appTitle;

  /// No description provided for @navHome.
  ///
  /// In en, this message translates to:
  /// **'Home'**
  String get navHome;

  /// No description provided for @navStore.
  ///
  /// In en, this message translates to:
  /// **'Store'**
  String get navStore;

  /// No description provided for @navCreate.
  ///
  /// In en, this message translates to:
  /// **'Create'**
  String get navCreate;

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

  /// No description provided for @closeConfirmTitle.
  ///
  /// In en, this message translates to:
  /// **'Close NeurolingsCE'**
  String get closeConfirmTitle;

  /// No description provided for @closeConfirmBody.
  ///
  /// In en, this message translates to:
  /// **'Do you want to close NeurolingsCE?'**
  String get closeConfirmBody;

  /// No description provided for @closeKeepOpen.
  ///
  /// In en, this message translates to:
  /// **'Keep open'**
  String get closeKeepOpen;

  /// No description provided for @closeConfirmClose.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get closeConfirmClose;

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
  /// **'Mascot Library'**
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

  /// No description provided for @spawnRandom.
  ///
  /// In en, this message translates to:
  /// **'Spawn Random'**
  String get spawnRandom;

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

  /// No description provided for @importButton.
  ///
  /// In en, this message translates to:
  /// **'Import'**
  String get importButton;

  /// No description provided for @showFolder.
  ///
  /// In en, this message translates to:
  /// **'Show Folder'**
  String get showFolder;

  /// No description provided for @deleteSelected.
  ///
  /// In en, this message translates to:
  /// **'Delete Selected'**
  String get deleteSelected;

  /// No description provided for @noTemplates.
  ///
  /// In en, this message translates to:
  /// **'No imported mascots yet'**
  String get noTemplates;

  /// No description provided for @noTemplatesHint.
  ///
  /// In en, this message translates to:
  /// **'Import a .mascot package or Shimeji archive to get started.'**
  String get noTemplatesHint;

  /// No description provided for @importMascot.
  ///
  /// In en, this message translates to:
  /// **'Import Mascot...'**
  String get importMascot;

  /// No description provided for @noRunning.
  ///
  /// In en, this message translates to:
  /// **'No mascots are running.'**
  String get noRunning;

  /// No description provided for @version.
  ///
  /// In en, this message translates to:
  /// **'Version'**
  String get version;

  /// No description provided for @author.
  ///
  /// In en, this message translates to:
  /// **'Author'**
  String get author;

  /// No description provided for @description.
  ///
  /// In en, this message translates to:
  /// **'Description'**
  String get description;

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

  /// No description provided for @cancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get cancel;

  /// No description provided for @save.
  ///
  /// In en, this message translates to:
  /// **'Save'**
  String get save;

  /// No description provided for @delete.
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get delete;

  /// No description provided for @close.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get close;

  /// No description provided for @homeTitle.
  ///
  /// In en, this message translates to:
  /// **'Mascot Manager'**
  String get homeTitle;

  /// No description provided for @homePageDescription.
  ///
  /// In en, this message translates to:
  /// **'Browse your mascot library, start companions, and manage installed packages.'**
  String get homePageDescription;

  /// No description provided for @homeSelectTemplate.
  ///
  /// In en, this message translates to:
  /// **'No mascot selected'**
  String get homeSelectTemplate;

  /// No description provided for @statusBar.
  ///
  /// In en, this message translates to:
  /// **'  Mascots: {mascots}  |  Templates: {templates}'**
  String statusBar(int mascots, int templates);

  /// No description provided for @inspectorTitle.
  ///
  /// In en, this message translates to:
  /// **'Inspector — {name}'**
  String inspectorTitle(Object name);

  /// No description provided for @inspectorClose.
  ///
  /// In en, this message translates to:
  /// **'Close inspector'**
  String get inspectorClose;

  /// No description provided for @combinationsOfflineHint.
  ///
  /// In en, this message translates to:
  /// **'Ensure the runtime is running.'**
  String get combinationsOfflineHint;

  /// No description provided for @homeImportDone.
  ///
  /// In en, this message translates to:
  /// **'Import finished'**
  String get homeImportDone;

  /// No description provided for @homeAndMore.
  ///
  /// In en, this message translates to:
  /// **'and {count} more'**
  String homeAndMore(int count);

  /// No description provided for @homeDeleteConfirm.
  ///
  /// In en, this message translates to:
  /// **'Delete {count} selected mascot(s)?'**
  String homeDeleteConfirm(int count);

  /// No description provided for @createTitle.
  ///
  /// In en, this message translates to:
  /// **'Create Mascot Package'**
  String get createTitle;

  /// No description provided for @createHint.
  ///
  /// In en, this message translates to:
  /// **'Convert a Shimeji-ee zip archive into .mascot packages.'**
  String get createHint;

  /// No description provided for @createStep1.
  ///
  /// In en, this message translates to:
  /// **'Choose a zip archive and check its content'**
  String get createStep1;

  /// No description provided for @createStep2.
  ///
  /// In en, this message translates to:
  /// **'Select mascots and edit info.json'**
  String get createStep2;

  /// No description provided for @createStep3.
  ///
  /// In en, this message translates to:
  /// **'Choose an output folder and generate packages'**
  String get createStep3;

  /// No description provided for @createChooseZip.
  ///
  /// In en, this message translates to:
  /// **'Choose Zip...'**
  String get createChooseZip;

  /// No description provided for @createCheckContent.
  ///
  /// In en, this message translates to:
  /// **'Check Content'**
  String get createCheckContent;

  /// No description provided for @createChooseFolder.
  ///
  /// In en, this message translates to:
  /// **'Choose Folder...'**
  String get createChooseFolder;

  /// No description provided for @createGenerate.
  ///
  /// In en, this message translates to:
  /// **'Generate .mascot'**
  String get createGenerate;

  /// No description provided for @createValidJson.
  ///
  /// In en, this message translates to:
  /// **'Valid JSON.'**
  String get createValidJson;

  /// No description provided for @createInvalidJson.
  ///
  /// In en, this message translates to:
  /// **'Invalid JSON: {error}'**
  String createInvalidJson(Object error);

  /// No description provided for @createNotConvertible.
  ///
  /// In en, this message translates to:
  /// **'not convertible'**
  String get createNotConvertible;

  /// No description provided for @createNoCandidates.
  ///
  /// In en, this message translates to:
  /// **'No convertible mascots were found in this archive.'**
  String get createNoCandidates;

  /// No description provided for @createCreated.
  ///
  /// In en, this message translates to:
  /// **'Created: {path}'**
  String createCreated(Object path);

  /// No description provided for @createFailed.
  ///
  /// In en, this message translates to:
  /// **'Failed: {name} - {error}'**
  String createFailed(Object name, Object error);

  /// No description provided for @createConvertedCount.
  ///
  /// In en, this message translates to:
  /// **'Converted {count} mascot(s).'**
  String createConvertedCount(int count);

  /// No description provided for @settingsGroupInteraction.
  ///
  /// In en, this message translates to:
  /// **'Interaction'**
  String get settingsGroupInteraction;

  /// No description provided for @settingsGroupCodex.
  ///
  /// In en, this message translates to:
  /// **'Codex'**
  String get settingsGroupCodex;

  /// No description provided for @settingsGroupDisplay.
  ///
  /// In en, this message translates to:
  /// **'Display'**
  String get settingsGroupDisplay;

  /// No description provided for @settingsGroupStartup.
  ///
  /// In en, this message translates to:
  /// **'Startup'**
  String get settingsGroupStartup;

  /// No description provided for @settingsGroupUpdates.
  ///
  /// In en, this message translates to:
  /// **'Updates'**
  String get settingsGroupUpdates;

  /// No description provided for @settingsMultiplication.
  ///
  /// In en, this message translates to:
  /// **'Multiplication'**
  String get settingsMultiplication;

  /// No description provided for @settingsMultiplicationHint.
  ///
  /// In en, this message translates to:
  /// **'Allow mascots to breed through interactions'**
  String get settingsMultiplicationHint;

  /// No description provided for @settingsWindowPushing.
  ///
  /// In en, this message translates to:
  /// **'Window pushing'**
  String get settingsWindowPushing;

  /// No description provided for @settingsWindowPushingHint.
  ///
  /// In en, this message translates to:
  /// **'Mascots can push the active window'**
  String get settingsWindowPushingHint;

  /// No description provided for @settingsSpeechBubble.
  ///
  /// In en, this message translates to:
  /// **'Speech bubbles'**
  String get settingsSpeechBubble;

  /// No description provided for @settingsSpeechBubbleHint.
  ///
  /// In en, this message translates to:
  /// **'Show a random speech bubble when clicking a mascot'**
  String get settingsSpeechBubbleHint;

  /// No description provided for @settingsBubbleClicks.
  ///
  /// In en, this message translates to:
  /// **'Speech bubble click count'**
  String get settingsBubbleClicks;

  /// No description provided for @settingsBubbleClicksHint.
  ///
  /// In en, this message translates to:
  /// **'Trigger the bubble after {count} clicks (1-10)'**
  String settingsBubbleClicksHint(int count);

  /// No description provided for @settingsCodexEnabled.
  ///
  /// In en, this message translates to:
  /// **'Codex notifications'**
  String get settingsCodexEnabled;

  /// No description provided for @settingsCodexEnabledHint.
  ///
  /// In en, this message translates to:
  /// **'Install the notify hook in ~/.codex/config.toml'**
  String get settingsCodexEnabledHint;

  /// No description provided for @settingsCodexConfirmTitle.
  ///
  /// In en, this message translates to:
  /// **'Enable Codex notifications?'**
  String get settingsCodexConfirmTitle;

  /// No description provided for @settingsCodexConfirmBody.
  ///
  /// In en, this message translates to:
  /// **'The following line will be added to {config}:'**
  String settingsCodexConfirmBody(Object config);

  /// No description provided for @settingsCodexTemplate.
  ///
  /// In en, this message translates to:
  /// **'Companion template'**
  String get settingsCodexTemplate;

  /// No description provided for @settingsCodexTemplateHint.
  ///
  /// In en, this message translates to:
  /// **'Mascot used for Codex notifications'**
  String get settingsCodexTemplateHint;

  /// No description provided for @settingsCodexTemplateDefault.
  ///
  /// In en, this message translates to:
  /// **'Default Mascot'**
  String get settingsCodexTemplateDefault;

  /// No description provided for @settingsCodexTemplateMissing.
  ///
  /// In en, this message translates to:
  /// **'Missing: {name} (will use Default Mascot)'**
  String settingsCodexTemplateMissing(Object name);

  /// No description provided for @settingsCodexTest.
  ///
  /// In en, this message translates to:
  /// **'Send test notification'**
  String get settingsCodexTest;

  /// No description provided for @settingsCodexTestHint.
  ///
  /// In en, this message translates to:
  /// **'Show a test bubble to verify the integration'**
  String get settingsCodexTestHint;

  /// No description provided for @settingsCodexTestSend.
  ///
  /// In en, this message translates to:
  /// **'Send test'**
  String get settingsCodexTestSend;

  /// No description provided for @settingsCodexAppServer.
  ///
  /// In en, this message translates to:
  /// **'Codex app server'**
  String get settingsCodexAppServer;

  /// No description provided for @settingsCodexAppServerHint.
  ///
  /// In en, this message translates to:
  /// **'Enable interactive Codex sessions (experimental)'**
  String get settingsCodexAppServerHint;

  /// No description provided for @settingsCodexExecutable.
  ///
  /// In en, this message translates to:
  /// **'Codex executable'**
  String get settingsCodexExecutable;

  /// No description provided for @settingsCodexExecutableHint.
  ///
  /// In en, this message translates to:
  /// **'Leave empty to use codex from PATH'**
  String get settingsCodexExecutableHint;

  /// No description provided for @settingsBrowse.
  ///
  /// In en, this message translates to:
  /// **'Browse...'**
  String get settingsBrowse;

  /// No description provided for @settingsApprovalBubble.
  ///
  /// In en, this message translates to:
  /// **'Approval reminder bubbles'**
  String get settingsApprovalBubble;

  /// No description provided for @settingsPlanBubble.
  ///
  /// In en, this message translates to:
  /// **'Plan and completion bubbles'**
  String get settingsPlanBubble;

  /// No description provided for @settingsDetachSpeed.
  ///
  /// In en, this message translates to:
  /// **'Detach Speed'**
  String get settingsDetachSpeed;

  /// No description provided for @settingsDetachSpeedHint.
  ///
  /// In en, this message translates to:
  /// **'Current: {value}'**
  String settingsDetachSpeedHint(Object value);

  /// No description provided for @settingsWindowedMode.
  ///
  /// In en, this message translates to:
  /// **'Windowed Mode'**
  String get settingsWindowedMode;

  /// No description provided for @settingsWindowedModeHint.
  ///
  /// In en, this message translates to:
  /// **'Run mascots in a sandbox window (640x480)'**
  String get settingsWindowedModeHint;

  /// No description provided for @settingsWindowedBg.
  ///
  /// In en, this message translates to:
  /// **'Background Color'**
  String get settingsWindowedBg;

  /// No description provided for @settingsWindowedBgHint.
  ///
  /// In en, this message translates to:
  /// **'Sandbox canvas background (#RRGGBB)'**
  String get settingsWindowedBgHint;

  /// No description provided for @settingsScale.
  ///
  /// In en, this message translates to:
  /// **'Scale'**
  String get settingsScale;

  /// No description provided for @settingsScaleHint.
  ///
  /// In en, this message translates to:
  /// **'Current: {value}'**
  String settingsScaleHint(Object value);

  /// No description provided for @settingsColorSaved.
  ///
  /// In en, this message translates to:
  /// **'Background color saved'**
  String get settingsColorSaved;

  /// No description provided for @settingsScaleSaved.
  ///
  /// In en, this message translates to:
  /// **'Scale saved'**
  String get settingsScaleSaved;

  /// No description provided for @settingsDetachSaved.
  ///
  /// In en, this message translates to:
  /// **'Detach speed saved'**
  String get settingsDetachSaved;

  /// No description provided for @settingsEdit.
  ///
  /// In en, this message translates to:
  /// **'Edit...'**
  String get settingsEdit;

  /// No description provided for @settingsConfigure.
  ///
  /// In en, this message translates to:
  /// **'Configure...'**
  String get settingsConfigure;

  /// No description provided for @settingsLanguage.
  ///
  /// In en, this message translates to:
  /// **'Language'**
  String get settingsLanguage;

  /// No description provided for @settingsLanguageHint.
  ///
  /// In en, this message translates to:
  /// **'Interface language (applies immediately)'**
  String get settingsLanguageHint;

  /// No description provided for @settingsAutostart.
  ///
  /// In en, this message translates to:
  /// **'Start at Login'**
  String get settingsAutostart;

  /// No description provided for @settingsAutostartHint.
  ///
  /// In en, this message translates to:
  /// **'Start automatically when you log in'**
  String get settingsAutostartHint;

  /// No description provided for @settingsSilent.
  ///
  /// In en, this message translates to:
  /// **'Silent startup'**
  String get settingsSilent;

  /// No description provided for @settingsSilentHint.
  ///
  /// In en, this message translates to:
  /// **'Do not show the manager window on autostart'**
  String get settingsSilentHint;

  /// No description provided for @settingsStartupCombo.
  ///
  /// In en, this message translates to:
  /// **'Startup combination'**
  String get settingsStartupCombo;

  /// No description provided for @settingsStartupLast.
  ///
  /// In en, this message translates to:
  /// **'Last combination before close'**
  String get settingsStartupLast;

  /// No description provided for @settingsStartupNone.
  ///
  /// In en, this message translates to:
  /// **'Do not restore'**
  String get settingsStartupNone;

  /// No description provided for @settingsStartupSaved.
  ///
  /// In en, this message translates to:
  /// **'Saved combination...'**
  String get settingsStartupSaved;

  /// No description provided for @settingsStartupSavedNamed.
  ///
  /// In en, this message translates to:
  /// **'Saved: {name}'**
  String settingsStartupSavedNamed(Object name);

  /// No description provided for @settingsStartupChoose.
  ///
  /// In en, this message translates to:
  /// **'Choose startup combination'**
  String get settingsStartupChoose;

  /// No description provided for @settingsUpdateCheck.
  ///
  /// In en, this message translates to:
  /// **'Check for updates on startup'**
  String get settingsUpdateCheck;

  /// No description provided for @settingsUpdateCheckHint.
  ///
  /// In en, this message translates to:
  /// **'Automatically check for new releases'**
  String get settingsUpdateCheckHint;

  /// No description provided for @settingsUpdateProxy.
  ///
  /// In en, this message translates to:
  /// **'Update Proxy'**
  String get settingsUpdateProxy;

  /// No description provided for @settingsUpdateProxyHint.
  ///
  /// In en, this message translates to:
  /// **'Current: {mode}'**
  String settingsUpdateProxyHint(Object mode);

  /// No description provided for @settingsProxySystem.
  ///
  /// In en, this message translates to:
  /// **'System'**
  String get settingsProxySystem;

  /// No description provided for @settingsProxyDirect.
  ///
  /// In en, this message translates to:
  /// **'Direct'**
  String get settingsProxyDirect;

  /// No description provided for @settingsProxyHttp.
  ///
  /// In en, this message translates to:
  /// **'HTTP'**
  String get settingsProxyHttp;

  /// No description provided for @settingsProxySocks5.
  ///
  /// In en, this message translates to:
  /// **'SOCKS5'**
  String get settingsProxySocks5;

  /// No description provided for @settingsProxyHost.
  ///
  /// In en, this message translates to:
  /// **'Host'**
  String get settingsProxyHost;

  /// No description provided for @settingsProxyPort.
  ///
  /// In en, this message translates to:
  /// **'Port'**
  String get settingsProxyPort;

  /// No description provided for @settingsProxyUser.
  ///
  /// In en, this message translates to:
  /// **'Username'**
  String get settingsProxyUser;

  /// No description provided for @settingsProxyPass.
  ///
  /// In en, this message translates to:
  /// **'Password'**
  String get settingsProxyPass;

  /// No description provided for @settingsOfflineHint.
  ///
  /// In en, this message translates to:
  /// **'Runtime offline — some settings require the runtime to be online'**
  String get settingsOfflineHint;

  /// No description provided for @settingsSaveFailed.
  ///
  /// In en, this message translates to:
  /// **'Failed to save setting'**
  String get settingsSaveFailed;

  /// No description provided for @aboutVersion.
  ///
  /// In en, this message translates to:
  /// **'Version'**
  String get aboutVersion;

  /// No description provided for @aboutCurrent.
  ///
  /// In en, this message translates to:
  /// **'Current Version'**
  String get aboutCurrent;

  /// No description provided for @aboutLatest.
  ///
  /// In en, this message translates to:
  /// **'Latest Release'**
  String get aboutLatest;

  /// No description provided for @aboutLatestNotChecked.
  ///
  /// In en, this message translates to:
  /// **'Not checked yet'**
  String get aboutLatestNotChecked;

  /// No description provided for @aboutCopyVersion.
  ///
  /// In en, this message translates to:
  /// **'Copy Version Info'**
  String get aboutCopyVersion;

  /// No description provided for @aboutCopyFormat.
  ///
  /// In en, this message translates to:
  /// **'NeurolingsCE {version} (latest: {latest})'**
  String aboutCopyFormat(Object version, Object latest);

  /// No description provided for @aboutCopied.
  ///
  /// In en, this message translates to:
  /// **'Version info copied'**
  String get aboutCopied;

  /// No description provided for @aboutUpdates.
  ///
  /// In en, this message translates to:
  /// **'Updates'**
  String get aboutUpdates;

  /// No description provided for @aboutOpenReleasePage.
  ///
  /// In en, this message translates to:
  /// **'Open Release Page'**
  String get aboutOpenReleasePage;

  /// No description provided for @aboutViewReleaseNotes.
  ///
  /// In en, this message translates to:
  /// **'View Release Notes'**
  String get aboutViewReleaseNotes;

  /// No description provided for @aboutProject.
  ///
  /// In en, this message translates to:
  /// **'Project & Support'**
  String get aboutProject;

  /// No description provided for @aboutUpstream.
  ///
  /// In en, this message translates to:
  /// **'Upstream'**
  String get aboutUpstream;

  /// No description provided for @aboutQQGroup.
  ///
  /// In en, this message translates to:
  /// **'QQ Group'**
  String get aboutQQGroup;

  /// No description provided for @aboutReportIssue.
  ///
  /// In en, this message translates to:
  /// **'Report Issue'**
  String get aboutReportIssue;

  /// No description provided for @aboutLicenses.
  ///
  /// In en, this message translates to:
  /// **'View Licenses'**
  String get aboutLicenses;

  /// No description provided for @aboutThirdParty.
  ///
  /// In en, this message translates to:
  /// **'Third-party'**
  String get aboutThirdParty;

  /// No description provided for @storeIndex.
  ///
  /// In en, this message translates to:
  /// **'Store index'**
  String get storeIndex;

  /// No description provided for @storeNotConfigured.
  ///
  /// In en, this message translates to:
  /// **'(not configured)'**
  String get storeNotConfigured;

  /// No description provided for @storeRuntimeOffline.
  ///
  /// In en, this message translates to:
  /// **'Runtime offline'**
  String get storeRuntimeOffline;

  /// No description provided for @storeRuntimeOfflineHint.
  ///
  /// In en, this message translates to:
  /// **'Start the runtime from Home before browsing the store.'**
  String get storeRuntimeOfflineHint;

  /// No description provided for @storeUnconfigured.
  ///
  /// In en, this message translates to:
  /// **'Store is not configured'**
  String get storeUnconfigured;

  /// No description provided for @storeUnconfiguredHint.
  ///
  /// In en, this message translates to:
  /// **'Set NEUROLINGSCE_MASCOT_INDEX_URL to a store index URL and restart the runtime.'**
  String get storeUnconfiguredHint;

  /// No description provided for @storeLoadFailed.
  ///
  /// In en, this message translates to:
  /// **'Failed to load the index'**
  String get storeLoadFailed;

  /// No description provided for @storeCacheWarning.
  ///
  /// In en, this message translates to:
  /// **'Cache warning'**
  String get storeCacheWarning;

  /// No description provided for @storeSearchHint.
  ///
  /// In en, this message translates to:
  /// **'Search by name, summary, ID, or author...'**
  String get storeSearchHint;

  /// No description provided for @storeAllTags.
  ///
  /// In en, this message translates to:
  /// **'All tags'**
  String get storeAllTags;

  /// No description provided for @storeEmpty.
  ///
  /// In en, this message translates to:
  /// **'No mascot packages in the store'**
  String get storeEmpty;

  /// No description provided for @storeNoMatch.
  ///
  /// In en, this message translates to:
  /// **'No mascots match your filters'**
  String get storeNoMatch;

  /// No description provided for @storeCount.
  ///
  /// In en, this message translates to:
  /// **'{count} mascots'**
  String storeCount(int count);

  /// No description provided for @storeTag.
  ///
  /// In en, this message translates to:
  /// **'tag: {tag}'**
  String storeTag(Object tag);

  /// No description provided for @storeFromCache.
  ///
  /// In en, this message translates to:
  /// **'from cache'**
  String get storeFromCache;

  /// No description provided for @storeDetails.
  ///
  /// In en, this message translates to:
  /// **'Details'**
  String get storeDetails;

  /// No description provided for @storeInstall.
  ///
  /// In en, this message translates to:
  /// **'Install'**
  String get storeInstall;

  /// No description provided for @storeAuthors.
  ///
  /// In en, this message translates to:
  /// **'Authors: {names}'**
  String storeAuthors(Object names);

  /// No description provided for @storeMinVersion.
  ///
  /// In en, this message translates to:
  /// **'Minimum version: {version}'**
  String storeMinVersion(Object version);

  /// No description provided for @storeInstallOk.
  ///
  /// In en, this message translates to:
  /// **'Installed: {name}'**
  String storeInstallOk(Object name);

  /// No description provided for @storeInstallOkHint.
  ///
  /// In en, this message translates to:
  /// **'SHA-256 verified and imported. Summon it from Home.'**
  String get storeInstallOkHint;

  /// No description provided for @storeInstallFailed.
  ///
  /// In en, this message translates to:
  /// **'Install failed: {name}'**
  String storeInstallFailed(Object name);

  /// No description provided for @storeCommunity.
  ///
  /// In en, this message translates to:
  /// **'Community'**
  String get storeCommunity;

  /// No description provided for @storeCommunityHint.
  ///
  /// In en, this message translates to:
  /// **'Sign in with GitHub to submit mascots to the registry.'**
  String get storeCommunityHint;

  /// No description provided for @storeCommunityHintSignedIn.
  ///
  /// In en, this message translates to:
  /// **'Submit your own mascots to the registry.'**
  String get storeCommunityHintSignedIn;

  /// No description provided for @storeSignIn.
  ///
  /// In en, this message translates to:
  /// **'Sign in with GitHub'**
  String get storeSignIn;

  /// No description provided for @storeSignInUnavailable.
  ///
  /// In en, this message translates to:
  /// **'GitHub sign-in is not configured'**
  String get storeSignInUnavailable;

  /// No description provided for @storeSignInHint.
  ///
  /// In en, this message translates to:
  /// **'Enter this code on GitHub to finish signing in:'**
  String get storeSignInHint;

  /// No description provided for @storeSignInDone.
  ///
  /// In en, this message translates to:
  /// **'Signed in as {login}'**
  String storeSignInDone(Object login);

  /// No description provided for @storeSignInFailed.
  ///
  /// In en, this message translates to:
  /// **'Sign-in did not complete'**
  String get storeSignInFailed;

  /// No description provided for @storeSignOut.
  ///
  /// In en, this message translates to:
  /// **'Sign out'**
  String get storeSignOut;

  /// No description provided for @storeCopyCode.
  ///
  /// In en, this message translates to:
  /// **'Copy code'**
  String get storeCopyCode;

  /// No description provided for @storeSignedInAs.
  ///
  /// In en, this message translates to:
  /// **'Signed in as {login}'**
  String storeSignedInAs(Object login);

  /// No description provided for @storeSubmit.
  ///
  /// In en, this message translates to:
  /// **'Submit a mascot...'**
  String get storeSubmit;

  /// No description provided for @storeSubmitPickPackage.
  ///
  /// In en, this message translates to:
  /// **'Package (.mascot)'**
  String get storeSubmitPickPackage;

  /// No description provided for @storeSubmitPick.
  ///
  /// In en, this message translates to:
  /// **'Browse...'**
  String get storeSubmitPick;

  /// No description provided for @storeSubmitName.
  ///
  /// In en, this message translates to:
  /// **'Name'**
  String get storeSubmitName;

  /// No description provided for @storeSubmitSummary.
  ///
  /// In en, this message translates to:
  /// **'Summary'**
  String get storeSubmitSummary;

  /// No description provided for @storeSubmitMaintainers.
  ///
  /// In en, this message translates to:
  /// **'Maintainers'**
  String get storeSubmitMaintainers;

  /// No description provided for @storeSubmitConfirm.
  ///
  /// In en, this message translates to:
  /// **'I confirm I have the rights to distribute this mascot'**
  String get storeSubmitConfirm;

  /// No description provided for @storeSubmitDone.
  ///
  /// In en, this message translates to:
  /// **'Submitted. Pull request: {url}'**
  String storeSubmitDone(Object url);

  /// No description provided for @storeSubmitFailed.
  ///
  /// In en, this message translates to:
  /// **'Submission failed ({code}): {error}'**
  String storeSubmitFailed(Object code, Object error);

  /// No description provided for @codexSession.
  ///
  /// In en, this message translates to:
  /// **'Session'**
  String get codexSession;

  /// No description provided for @codexStatus.
  ///
  /// In en, this message translates to:
  /// **'Status'**
  String get codexStatus;

  /// No description provided for @codexThread.
  ///
  /// In en, this message translates to:
  /// **'Thread'**
  String get codexThread;

  /// No description provided for @codexWorkspace.
  ///
  /// In en, this message translates to:
  /// **'Workspace'**
  String get codexWorkspace;

  /// No description provided for @codexTurn.
  ///
  /// In en, this message translates to:
  /// **'Turn'**
  String get codexTurn;

  /// No description provided for @codexMode.
  ///
  /// In en, this message translates to:
  /// **'Mode'**
  String get codexMode;

  /// No description provided for @codexConnection.
  ///
  /// In en, this message translates to:
  /// **'Connection'**
  String get codexConnection;

  /// No description provided for @codexConnect.
  ///
  /// In en, this message translates to:
  /// **'Connect Codex'**
  String get codexConnect;

  /// No description provided for @codexDisconnect.
  ///
  /// In en, this message translates to:
  /// **'Disconnect'**
  String get codexDisconnect;

  /// No description provided for @codexNewSession.
  ///
  /// In en, this message translates to:
  /// **'New session'**
  String get codexNewSession;

  /// No description provided for @codexResume.
  ///
  /// In en, this message translates to:
  /// **'Resume recent'**
  String get codexResume;

  /// No description provided for @codexApprovals.
  ///
  /// In en, this message translates to:
  /// **'Approvals'**
  String get codexApprovals;

  /// No description provided for @codexNoApprovals.
  ///
  /// In en, this message translates to:
  /// **'No pending approvals.'**
  String get codexNoApprovals;

  /// No description provided for @codexPlan.
  ///
  /// In en, this message translates to:
  /// **'Plan'**
  String get codexPlan;

  /// No description provided for @codexNoPlan.
  ///
  /// In en, this message translates to:
  /// **'No plan yet.'**
  String get codexNoPlan;

  /// No description provided for @codexMessage.
  ///
  /// In en, this message translates to:
  /// **'Message'**
  String get codexMessage;

  /// No description provided for @codexModeDefault.
  ///
  /// In en, this message translates to:
  /// **'Default'**
  String get codexModeDefault;

  /// No description provided for @codexModePlan.
  ///
  /// In en, this message translates to:
  /// **'Plan'**
  String get codexModePlan;

  /// No description provided for @codexPlanUnsupported.
  ///
  /// In en, this message translates to:
  /// **'not supported'**
  String get codexPlanUnsupported;

  /// No description provided for @codexAskPlaceholder.
  ///
  /// In en, this message translates to:
  /// **'Ask Codex something...'**
  String get codexAskPlaceholder;

  /// No description provided for @codexSend.
  ///
  /// In en, this message translates to:
  /// **'Send'**
  String get codexSend;

  /// No description provided for @codexImplementPlan.
  ///
  /// In en, this message translates to:
  /// **'Implement this plan'**
  String get codexImplementPlan;

  /// No description provided for @codexModifyPlan.
  ///
  /// In en, this message translates to:
  /// **'Modify plan'**
  String get codexModifyPlan;

  /// No description provided for @codexAbort.
  ///
  /// In en, this message translates to:
  /// **'Abort task'**
  String get codexAbort;

  /// No description provided for @codexDecline.
  ///
  /// In en, this message translates to:
  /// **'Decline'**
  String get codexDecline;

  /// No description provided for @codexAllowOnce.
  ///
  /// In en, this message translates to:
  /// **'Allow once'**
  String get codexAllowOnce;

  /// No description provided for @codexAllowSession.
  ///
  /// In en, this message translates to:
  /// **'Allow for session'**
  String get codexAllowSession;

  /// No description provided for @codexDeclineStop.
  ///
  /// In en, this message translates to:
  /// **'Decline and stop'**
  String get codexDeclineStop;

  /// No description provided for @codexInputTitle.
  ///
  /// In en, this message translates to:
  /// **'Codex needs input'**
  String get codexInputTitle;

  /// No description provided for @codexNoReply.
  ///
  /// In en, this message translates to:
  /// **'The task completed without a reply to display.'**
  String get codexNoReply;

  /// No description provided for @codexEmptyInput.
  ///
  /// In en, this message translates to:
  /// **'Enter a message first'**
  String get codexEmptyInput;

  /// No description provided for @codexDisabledTitle.
  ///
  /// In en, this message translates to:
  /// **'Codex app server is disabled'**
  String get codexDisabledTitle;

  /// No description provided for @codexDisabledHint.
  ///
  /// In en, this message translates to:
  /// **'Enable \'Codex app server\' in Settings to use interactive sessions.'**
  String get codexDisabledHint;

  /// No description provided for @aboutCheckForUpdates.
  ///
  /// In en, this message translates to:
  /// **'Check for Updates'**
  String get aboutCheckForUpdates;

  /// No description provided for @aboutDownloadInstall.
  ///
  /// In en, this message translates to:
  /// **'Download & Install...'**
  String get aboutDownloadInstall;

  /// No description provided for @aboutInstall.
  ///
  /// In en, this message translates to:
  /// **'Install {version}'**
  String aboutInstall(Object version);

  /// No description provided for @aboutIgnoreVersion.
  ///
  /// In en, this message translates to:
  /// **'Ignore This Version'**
  String get aboutIgnoreVersion;

  /// No description provided for @aboutRemindLater.
  ///
  /// In en, this message translates to:
  /// **'Remind Me Later'**
  String get aboutRemindLater;

  /// No description provided for @aboutUpdateAvailable.
  ///
  /// In en, this message translates to:
  /// **'NeurolingsCE {version} is available.'**
  String aboutUpdateAvailable(Object version);

  /// No description provided for @aboutInstallConfirmTitle.
  ///
  /// In en, this message translates to:
  /// **'Install update?'**
  String get aboutInstallConfirmTitle;

  /// No description provided for @aboutInstallConfirmBody.
  ///
  /// In en, this message translates to:
  /// **'Version {version} is downloaded. Launch the installer now? The app will close.'**
  String aboutInstallConfirmBody(Object version);

  /// No description provided for @aboutInstallNow.
  ///
  /// In en, this message translates to:
  /// **'Install'**
  String get aboutInstallNow;
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
