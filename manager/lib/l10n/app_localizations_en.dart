// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class AppLocalizationsEn extends AppLocalizations {
  AppLocalizationsEn([String locale = 'en']) : super(locale);

  @override
  String get appTitle => 'NeurolingsCE Manager';

  @override
  String get navHome => 'Home';

  @override
  String get navCreate => 'Create';

  @override
  String get navStore => 'Store';

  @override
  String get navCombinations => 'Combinations';

  @override
  String get navCodex => 'Codex';

  @override
  String get navSettings => 'Settings';

  @override
  String get navAbout => 'About';

  @override
  String get runtimeOnline => 'Runtime online';

  @override
  String get runtimeOffline => 'Runtime offline';

  @override
  String get startRuntime => 'Start runtime';

  @override
  String get loadedMascots => 'Installed mascots';

  @override
  String get runningMascots => 'Running mascots';

  @override
  String get spawn => 'Summon';

  @override
  String get dismiss => 'Close';

  @override
  String get dismissAll => 'Close all';

  @override
  String get refresh => 'Refresh';

  @override
  String get noTemplates =>
      'No mascot templates installed. Import one from the Create page.';

  @override
  String get noRunning => 'No mascots are running.';

  @override
  String get createTitle => 'Import a mascot package';

  @override
  String get createHint =>
      'Select a Shimeji-ee zip archive or a .mascot package to import it into the local storage.';

  @override
  String get pickArchive => 'Choose archive...';

  @override
  String get importing => 'Importing...';

  @override
  String importDone(Object result) {
    return 'Import finished: $result';
  }

  @override
  String get storePlaceholder => 'The mascot store arrives in milestone M7.';

  @override
  String get combinationsPlaceholder =>
      'Mascot combinations arrive in milestone M8.';

  @override
  String get codexPlaceholder => 'Codex integration arrives in milestone M8.';

  @override
  String get settingsLanguage => 'Language';

  @override
  String get settingsRuntime => 'Runtime';

  @override
  String get settingsStorage => 'Mascot storage';

  @override
  String get settingsHttpHint =>
      'The HTTP API listens on 127.0.0.1:32456 when enabled via NEUROLINGSCE_HTTP=1.';

  @override
  String get aboutDescription =>
      'NeurolingsCE is a cross-platform desktop mascot (Shimeji) runner, rewritten in Rust + Flutter.';

  @override
  String get version => 'Version';

  @override
  String get license => 'License';

  @override
  String get error => 'Error';

  @override
  String get ok => 'OK';
}
