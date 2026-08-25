// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get appTitle => 'NeurolingsCE — Mascot Manager';

  @override
  String get navHome => 'Home';

  @override
  String get navStore => 'Store';

  @override
  String get navCreate => 'Create';

  @override
  String get navCombinations => 'Combinations';

  @override
  String get navCodex => 'Codex';

  @override
  String get navSettings => 'Settings';

  @override
  String get navAbout => 'About';

  @override
  String get closeConfirmTitle => 'Close NeurolingsCE';

  @override
  String get closeConfirmBody => 'Do you want to close NeurolingsCE?';

  @override
  String get closeKeepOpen => 'Keep open';

  @override
  String get closeConfirmClose => 'Close';

  @override
  String get runtimeOnline => 'Runtime online';

  @override
  String get runtimeOffline => 'Runtime offline';

  @override
  String get startRuntime => 'Start runtime';

  @override
  String get loadedMascots => 'Mascot Library';

  @override
  String get runningMascots => 'Running mascots';

  @override
  String get spawn => 'Summon';

  @override
  String get spawnRandom => 'Spawn Random';

  @override
  String get dismiss => 'Close';

  @override
  String get dismissAll => 'Close all';

  @override
  String get refresh => 'Refresh';

  @override
  String get importButton => 'Import';

  @override
  String get showFolder => 'Show Folder';

  @override
  String get deleteSelected => 'Delete Selected';

  @override
  String get noTemplates => 'No imported mascots yet';

  @override
  String get noTemplatesHint =>
      'Import a .mascot package or Shimeji archive to get started.';

  @override
  String get importMascot => 'Import Mascot...';

  @override
  String get noRunning => 'No mascots are running.';

  @override
  String get version => 'Version';

  @override
  String get author => 'Author';

  @override
  String get description => 'Description';

  @override
  String get license => 'License';

  @override
  String get error => 'Error';

  @override
  String get ok => 'OK';

  @override
  String get cancel => 'Cancel';

  @override
  String get save => 'Save';

  @override
  String get delete => 'Delete';

  @override
  String get close => 'Close';

  @override
  String get homeTitle => 'Mascot Manager';

  @override
  String get homePageDescription =>
      'Browse your mascot library, start companions, and manage installed packages.';

  @override
  String get homeSelectTemplate => 'No mascot selected';

  @override
  String statusBar(int mascots, int templates) {
    return '  Mascots: $mascots  |  Templates: $templates';
  }

  @override
  String inspectorTitle(Object name) {
    return 'Inspector — $name';
  }

  @override
  String get inspectorClose => 'Close inspector';

  @override
  String get combinationsOfflineHint => 'Ensure the runtime is running.';

  @override
  String get homeImportDone => 'Import finished';

  @override
  String homeAndMore(int count) {
    return 'and $count more';
  }

  @override
  String homeDeleteConfirm(int count) {
    return 'Delete $count selected mascot(s)?';
  }

  @override
  String get createTitle => 'Create Mascot Package';

  @override
  String get createHint =>
      'Convert a Shimeji-ee zip archive into .mascot packages.';

  @override
  String get createStep1 => 'Choose a zip archive and check its content';

  @override
  String get createStep2 => 'Select mascots and edit info.json';

  @override
  String get createStep3 => 'Choose an output folder and generate packages';

  @override
  String get createChooseZip => 'Choose Zip...';

  @override
  String get createCheckContent => 'Check Content';

  @override
  String get createChooseFolder => 'Choose Folder...';

  @override
  String get createGenerate => 'Generate .mascot';

  @override
  String get createValidJson => 'Valid JSON.';

  @override
  String createInvalidJson(Object error) {
    return 'Invalid JSON: $error';
  }

  @override
  String get createNotConvertible => 'not convertible';

  @override
  String get createNoCandidates =>
      'No convertible mascots were found in this archive.';

  @override
  String createCreated(Object path) {
    return 'Created: $path';
  }

  @override
  String createFailed(Object name, Object error) {
    return 'Failed: $name - $error';
  }

  @override
  String createConvertedCount(int count) {
    return 'Converted $count mascot(s).';
  }

  @override
  String get settingsGroupInteraction => 'Interaction';

  @override
  String get settingsGroupCodex => 'Codex';

  @override
  String get settingsGroupDisplay => 'Display';

  @override
  String get settingsGroupStartup => 'Startup';

  @override
  String get settingsGroupUpdates => 'Updates';

  @override
  String get settingsMultiplication => 'Multiplication';

  @override
  String get settingsMultiplicationHint =>
      'Allow mascots to breed through interactions';

  @override
  String get settingsWindowPushing => 'Window pushing';

  @override
  String get settingsWindowPushingHint => 'Mascots can push the active window';

  @override
  String get settingsSpeechBubble => 'Speech bubbles';

  @override
  String get settingsSpeechBubbleHint =>
      'Show a random speech bubble when clicking a mascot';

  @override
  String get settingsBubbleClicks => 'Speech bubble click count';

  @override
  String settingsBubbleClicksHint(int count) {
    return 'Trigger the bubble after $count clicks (1-10)';
  }

  @override
  String get settingsCodexEnabled => 'Codex notifications';

  @override
  String get settingsCodexEnabledHint =>
      'Install the notify hook in ~/.codex/config.toml';

  @override
  String get settingsCodexConfirmTitle => 'Enable Codex notifications?';

  @override
  String settingsCodexConfirmBody(Object config) {
    return 'The following line will be added to $config:';
  }

  @override
  String get settingsCodexTemplate => 'Companion template';

  @override
  String get settingsCodexTemplateHint => 'Mascot used for Codex notifications';

  @override
  String get settingsCodexTemplateDefault => 'Default Mascot';

  @override
  String settingsCodexTemplateMissing(Object name) {
    return 'Missing: $name (will use Default Mascot)';
  }

  @override
  String get settingsCodexTest => 'Send test notification';

  @override
  String get settingsCodexTestHint =>
      'Show a test bubble to verify the integration';

  @override
  String get settingsCodexTestSend => 'Send test';

  @override
  String get settingsCodexAppServer => 'Codex app server';

  @override
  String get settingsCodexAppServerHint =>
      'Enable interactive Codex sessions (experimental)';

  @override
  String get settingsCodexExecutable => 'Codex executable';

  @override
  String get settingsCodexExecutableHint =>
      'Leave empty to use codex from PATH';

  @override
  String get settingsBrowse => 'Browse...';

  @override
  String get settingsApprovalBubble => 'Approval reminder bubbles';

  @override
  String get settingsPlanBubble => 'Plan and completion bubbles';

  @override
  String get settingsDetachSpeed => 'Detach Speed';

  @override
  String settingsDetachSpeedHint(Object value) {
    return 'Current: $value';
  }

  @override
  String get settingsWindowedMode => 'Windowed Mode';

  @override
  String get settingsWindowedModeHint =>
      'Run mascots in a sandbox window (640x480)';

  @override
  String get settingsWindowedBg => 'Background Color';

  @override
  String get settingsWindowedBgHint => 'Sandbox canvas background (#RRGGBB)';

  @override
  String get settingsScale => 'Scale';

  @override
  String settingsScaleHint(Object value) {
    return 'Current: $value';
  }

  @override
  String get settingsColorSaved => 'Background color saved';

  @override
  String get settingsScaleSaved => 'Scale saved';

  @override
  String get settingsDetachSaved => 'Detach speed saved';

  @override
  String get settingsEdit => 'Edit...';

  @override
  String get settingsConfigure => 'Configure...';

  @override
  String get settingsLanguage => 'Language';

  @override
  String get settingsLanguageHint => 'Interface language (applies immediately)';

  @override
  String get settingsAutostart => 'Start at Login';

  @override
  String get settingsAutostartHint => 'Start automatically when you log in';

  @override
  String get settingsSilent => 'Silent startup';

  @override
  String get settingsSilentHint =>
      'Do not show the manager window on autostart';

  @override
  String get settingsStartupCombo => 'Startup combination';

  @override
  String get settingsStartupLast => 'Last combination before close';

  @override
  String get settingsStartupNone => 'Do not restore';

  @override
  String get settingsStartupSaved => 'Saved combination...';

  @override
  String settingsStartupSavedNamed(Object name) {
    return 'Saved: $name';
  }

  @override
  String get settingsStartupChoose => 'Choose startup combination';

  @override
  String get settingsUpdateCheck => 'Check for updates on startup';

  @override
  String get settingsUpdateCheckHint => 'Automatically check for new releases';

  @override
  String get settingsUpdateProxy => 'Update Proxy';

  @override
  String settingsUpdateProxyHint(Object mode) {
    return 'Current: $mode';
  }

  @override
  String get settingsProxySystem => 'System';

  @override
  String get settingsProxyDirect => 'Direct';

  @override
  String get settingsProxyHttp => 'HTTP';

  @override
  String get settingsProxySocks5 => 'SOCKS5';

  @override
  String get settingsProxyHost => 'Host';

  @override
  String get settingsProxyPort => 'Port';

  @override
  String get settingsProxyUser => 'Username';

  @override
  String get settingsProxyPass => 'Password';

  @override
  String get settingsOfflineHint =>
      'Runtime offline — some settings require the runtime to be online';

  @override
  String get settingsSaveFailed => 'Failed to save setting';

  @override
  String get aboutVersion => 'Version';

  @override
  String get aboutCurrent => 'Current Version';

  @override
  String get aboutLatest => 'Latest Release';

  @override
  String get aboutLatestNotChecked => 'Not checked yet';

  @override
  String get aboutCopyVersion => 'Copy Version Info';

  @override
  String aboutCopyFormat(Object version, Object latest) {
    return 'NeurolingsCE $version (latest: $latest)';
  }

  @override
  String get aboutCopied => 'Version info copied';

  @override
  String get aboutUpdates => 'Updates';

  @override
  String get aboutOpenReleasePage => 'Open Release Page';

  @override
  String get aboutViewReleaseNotes => 'View Release Notes';

  @override
  String get aboutProject => 'Project & Support';

  @override
  String get aboutUpstream => 'Upstream';

  @override
  String get aboutQQGroup => 'QQ Group';

  @override
  String get aboutReportIssue => 'Report Issue';

  @override
  String get aboutLicenses => 'View Licenses';

  @override
  String get aboutThirdParty => 'Third-party';

  @override
  String get storeIndex => 'Store index';

  @override
  String get storeNotConfigured => '(not configured)';

  @override
  String get storeRuntimeOffline => 'Runtime offline';

  @override
  String get storeRuntimeOfflineHint =>
      'Start the runtime from Home before browsing the store.';

  @override
  String get storeUnconfigured => 'Store is not configured';

  @override
  String get storeUnconfiguredHint =>
      'Set NEUROLINGSCE_MASCOT_INDEX_URL to a store index URL and restart the runtime.';

  @override
  String get storeLoadFailed => 'Failed to load the index';

  @override
  String get storeCacheWarning => 'Cache warning';

  @override
  String get storeSearchHint => 'Search by name, summary, ID, or author...';

  @override
  String get storeAllTags => 'All tags';

  @override
  String get storeEmpty => 'No mascot packages in the store';

  @override
  String get storeNoMatch => 'No mascots match your filters';

  @override
  String storeCount(int count) {
    return '$count mascots';
  }

  @override
  String storeTag(Object tag) {
    return 'tag: $tag';
  }

  @override
  String get storeFromCache => 'from cache';

  @override
  String get storeDetails => 'Details';

  @override
  String get storeInstall => 'Install';

  @override
  String storeAuthors(Object names) {
    return 'Authors: $names';
  }

  @override
  String storeMinVersion(Object version) {
    return 'Minimum version: $version';
  }

  @override
  String storeInstallOk(Object name) {
    return 'Installed: $name';
  }

  @override
  String get storeInstallOkHint =>
      'SHA-256 verified and imported. Summon it from Home.';

  @override
  String storeInstallFailed(Object name) {
    return 'Install failed: $name';
  }

  @override
  String get storeCommunity => 'Community';

  @override
  String get storeCommunityHint =>
      'Sign in with GitHub to submit mascots to the registry.';

  @override
  String get storeCommunityHintSignedIn =>
      'Submit your own mascots to the registry.';

  @override
  String get storeSignIn => 'Sign in with GitHub';

  @override
  String get storeSignInUnavailable => 'GitHub sign-in is not configured';

  @override
  String get storeSignInHint =>
      'Enter this code on GitHub to finish signing in:';

  @override
  String storeSignInDone(Object login) {
    return 'Signed in as $login';
  }

  @override
  String get storeSignInFailed => 'Sign-in did not complete';

  @override
  String get storeSignOut => 'Sign out';

  @override
  String get storeCopyCode => 'Copy code';

  @override
  String storeSignedInAs(Object login) {
    return 'Signed in as $login';
  }

  @override
  String get storeSubmit => 'Submit a mascot...';

  @override
  String get storeSubmitPickPackage => 'Package (.mascot)';

  @override
  String get storeSubmitPick => 'Browse...';

  @override
  String get storeSubmitName => 'Name';

  @override
  String get storeSubmitSummary => 'Summary';

  @override
  String get storeSubmitMaintainers => 'Maintainers';

  @override
  String get storeSubmitConfirm =>
      'I confirm I have the rights to distribute this mascot';

  @override
  String storeSubmitDone(Object url) {
    return 'Submitted. Pull request: $url';
  }

  @override
  String storeSubmitFailed(Object code, Object error) {
    return 'Submission failed ($code): $error';
  }

  @override
  String get codexSession => 'Session';

  @override
  String get codexStatus => 'Status';

  @override
  String get codexThread => 'Thread';

  @override
  String get codexWorkspace => 'Workspace';

  @override
  String get codexTurn => 'Turn';

  @override
  String get codexMode => 'Mode';

  @override
  String get codexConnection => 'Connection';

  @override
  String get codexConnect => 'Connect Codex';

  @override
  String get codexDisconnect => 'Disconnect';

  @override
  String get codexNewSession => 'New session';

  @override
  String get codexResume => 'Resume recent';

  @override
  String get codexApprovals => 'Approvals';

  @override
  String get codexNoApprovals => 'No pending approvals.';

  @override
  String get codexPlan => 'Plan';

  @override
  String get codexNoPlan => 'No plan yet.';

  @override
  String get codexMessage => 'Message';

  @override
  String get codexModeDefault => 'Default';

  @override
  String get codexModePlan => 'Plan';

  @override
  String get codexPlanUnsupported => 'not supported';

  @override
  String get codexAskPlaceholder => 'Ask Codex something...';

  @override
  String get codexSend => 'Send';

  @override
  String get codexImplementPlan => 'Implement this plan';

  @override
  String get codexModifyPlan => 'Modify plan';

  @override
  String get codexAbort => 'Abort task';

  @override
  String get codexDecline => 'Decline';

  @override
  String get codexAllowOnce => 'Allow once';

  @override
  String get codexAllowSession => 'Allow for session';

  @override
  String get codexDeclineStop => 'Decline and stop';

  @override
  String get codexInputTitle => 'Codex needs input';

  @override
  String get codexNoReply => 'The task completed without a reply to display.';

  @override
  String get codexEmptyInput => 'Enter a message first';

  @override
  String get codexDisabledTitle => 'Codex app server is disabled';

  @override
  String get codexDisabledHint =>
      'Enable \'Codex app server\' in Settings to use interactive sessions.';

  @override
  String get aboutCheckForUpdates => 'Check for Updates';

  @override
  String get aboutDownloadInstall => 'Download & Install...';

  @override
  String aboutInstall(Object version) {
    return 'Install $version';
  }

  @override
  String get aboutIgnoreVersion => 'Ignore This Version';

  @override
  String get aboutRemindLater => 'Remind Me Later';

  @override
  String aboutUpdateAvailable(Object version) {
    return 'NeurolingsCE $version is available.';
  }

  @override
  String get aboutInstallConfirmTitle => 'Install update?';

  @override
  String aboutInstallConfirmBody(Object version) {
    return 'Version $version is downloaded. Launch the installer now? The app will close.';
  }

  @override
  String get aboutInstallNow => 'Install';
}
