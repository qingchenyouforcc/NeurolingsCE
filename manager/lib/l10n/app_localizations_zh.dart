// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Chinese (`zh`).
class AppLocalizationsZh extends AppLocalizations {
  AppLocalizationsZh([String locale = 'zh']) : super(locale);

  @override
  String get appTitle => 'NeurolingsCE 管理器';

  @override
  String get navHome => '主页';

  @override
  String get navCreate => '制作';

  @override
  String get navStore => '商店';

  @override
  String get navCombinations => '组合';

  @override
  String get navCodex => 'Codex';

  @override
  String get navSettings => '设置';

  @override
  String get navAbout => '关于';

  @override
  String get runtimeOnline => '运行时在线';

  @override
  String get runtimeOffline => '运行时离线';

  @override
  String get startRuntime => '启动运行时';

  @override
  String get loadedMascots => '已安装的桌宠';

  @override
  String get runningMascots => '运行中的桌宠';

  @override
  String get spawn => '召唤';

  @override
  String get dismiss => '关闭';

  @override
  String get dismissAll => '全部关闭';

  @override
  String get refresh => '刷新';

  @override
  String get noTemplates => '尚未安装桌宠模板，请到制作页导入。';

  @override
  String get noRunning => '当前没有运行中的桌宠。';

  @override
  String get createTitle => '导入桌宠包';

  @override
  String get createHint => '选择 Shimeji-ee zip 压缩包或 .mascot 包，导入到本地存储。';

  @override
  String get pickArchive => '选择压缩包...';

  @override
  String get importing => '导入中...';

  @override
  String importDone(Object result) {
    return '导入完成：$result';
  }

  @override
  String get storePlaceholder => '桌宠商店将在 M7 里程碑提供。';

  @override
  String get combinationsPlaceholder => '桌宠组合将在 M8 里程碑提供。';

  @override
  String get codexPlaceholder => 'Codex 集成将在 M8 里程碑提供。';

  @override
  String get settingsLanguage => '语言';

  @override
  String get settingsRuntime => '运行时';

  @override
  String get settingsStorage => '桌宠存储目录';

  @override
  String get settingsHttpHint =>
      '通过 NEUROLINGSCE_HTTP=1 启用后，HTTP API 监听 127.0.0.1:32456。';

  @override
  String get aboutDescription =>
      'NeurolingsCE 是跨平台桌面看板娘（Shimeji）运行器，已用 Rust + Flutter 重写。';

  @override
  String get version => '版本';

  @override
  String get license => '许可证';

  @override
  String get error => '错误';

  @override
  String get ok => '确定';
}
