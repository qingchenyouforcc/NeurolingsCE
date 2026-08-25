// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Chinese (`zh`).
class AppLocalizationsZh extends AppLocalizations {
  AppLocalizationsZh([String locale = 'zh']) : super(locale);

  @override
  String get appTitle => 'NeurolingsCE — 桌宠管理器';

  @override
  String get navHome => '主页';

  @override
  String get navStore => '商店';

  @override
  String get navCreate => '制作';

  @override
  String get navCombinations => '组合';

  @override
  String get navCodex => 'Codex';

  @override
  String get navSettings => '设置';

  @override
  String get navAbout => '关于';

  @override
  String get closeConfirmTitle => '关闭 NeurolingsCE';

  @override
  String get closeConfirmBody => '确定要关闭 NeurolingsCE 吗？';

  @override
  String get closeKeepOpen => '保持打开';

  @override
  String get closeConfirmClose => '关闭';

  @override
  String get runtimeOnline => '运行时在线';

  @override
  String get runtimeOffline => '运行时离线';

  @override
  String get startRuntime => '启动运行时';

  @override
  String get loadedMascots => '桌宠库';

  @override
  String get runningMascots => '运行中的桌宠';

  @override
  String get spawn => '召唤';

  @override
  String get spawnRandom => '随机生成';

  @override
  String get dismiss => '关闭';

  @override
  String get dismissAll => '全部关闭';

  @override
  String get refresh => '刷新';

  @override
  String get importButton => '导入';

  @override
  String get showFolder => '打开文件夹';

  @override
  String get deleteSelected => '删除所选项';

  @override
  String get noTemplates => '尚未导入桌宠';

  @override
  String get noTemplatesHint => '导入 .mascot 包或 Shimeji 压缩包即可开始使用。';

  @override
  String get importMascot => '导入桌宠...';

  @override
  String get noRunning => '没有运行中的桌宠。';

  @override
  String get version => '版本';

  @override
  String get author => '作者';

  @override
  String get description => '描述';

  @override
  String get license => '许可证';

  @override
  String get error => '错误';

  @override
  String get ok => '确定';

  @override
  String get cancel => '取消';

  @override
  String get save => '保存';

  @override
  String get delete => '删除';

  @override
  String get close => '关闭';

  @override
  String get homeTitle => '桌宠管理器';

  @override
  String get homePageDescription => '浏览桌宠库、启动桌面伙伴并管理已安装的桌宠包。';

  @override
  String get homeSelectTemplate => '未选择桌宠';

  @override
  String statusBar(int mascots, int templates) {
    return '  当前桌宠数量: $mascots  |  桌宠模板数: $templates';
  }

  @override
  String inspectorTitle(Object name) {
    return '检查器 — $name';
  }

  @override
  String get inspectorClose => '关闭检查器';

  @override
  String get combinationsOfflineHint => '请确认运行时已启动。';

  @override
  String get homeImportDone => '导入完成';

  @override
  String homeAndMore(int count) {
    return '等 $count 个';
  }

  @override
  String homeDeleteConfirm(int count) {
    return '删除选中的 $count 个桌宠？';
  }

  @override
  String get createTitle => '制作桌宠包';

  @override
  String get createHint => '把旧版 Shimeji zip 压缩包转换为 .mascot 包。';

  @override
  String get createStep1 => '选择 zip 压缩包并检查内容';

  @override
  String get createStep2 => '勾选桌宠并编辑 info.json';

  @override
  String get createStep3 => '选择输出目录并生成';

  @override
  String get createChooseZip => '选择 Zip...';

  @override
  String get createCheckContent => '检查内容';

  @override
  String get createChooseFolder => '选择文件夹...';

  @override
  String get createGenerate => '生成 .mascot';

  @override
  String get createValidJson => 'JSON 有效。';

  @override
  String createInvalidJson(Object error) {
    return 'JSON 无效：$error';
  }

  @override
  String get createNotConvertible => '不可转换';

  @override
  String get createNoCandidates => '该压缩包中没有找到可转换的桌宠。';

  @override
  String createCreated(Object path) {
    return '已创建：$path';
  }

  @override
  String createFailed(Object name, Object error) {
    return '失败：$name - $error';
  }

  @override
  String createConvertedCount(int count) {
    return '已转换 $count 个桌宠。';
  }

  @override
  String get settingsGroupInteraction => '交互';

  @override
  String get settingsGroupCodex => 'Codex';

  @override
  String get settingsGroupDisplay => '显示';

  @override
  String get settingsGroupStartup => '启动';

  @override
  String get settingsGroupUpdates => '更新';

  @override
  String get settingsMultiplication => '允许繁殖';

  @override
  String get settingsMultiplicationHint => '允许桌宠通过交互产生新个体';

  @override
  String get settingsWindowPushing => '允许推挤窗口';

  @override
  String get settingsWindowPushingHint => '桌宠可推移活动窗口';

  @override
  String get settingsSpeechBubble => '语音气泡';

  @override
  String get settingsSpeechBubbleHint => '点击桌宠时显示随机气泡';

  @override
  String get settingsBubbleClicks => '气泡点击次数';

  @override
  String settingsBubbleClicksHint(int count) {
    return '连续点击 $count 次后触发气泡（1-10）';
  }

  @override
  String get settingsCodexEnabled => '启用 Codex 消息气泡';

  @override
  String get settingsCodexEnabledHint => '在 ~/.codex/config.toml 中注入 notify 钩子';

  @override
  String get settingsCodexConfirmTitle => '启用 Codex 消息气泡？';

  @override
  String settingsCodexConfirmBody(Object config) {
    return '将在 $config 中添加以下内容：';
  }

  @override
  String get settingsCodexTemplate => '陪伴模板';

  @override
  String get settingsCodexTemplateHint => '收到 Codex 通知时使用的桌宠';

  @override
  String get settingsCodexTemplateDefault => '默认桌宠';

  @override
  String settingsCodexTemplateMissing(Object name) {
    return '缺失：$name（将使用默认桌宠）';
  }

  @override
  String get settingsCodexTest => '发送测试通知';

  @override
  String get settingsCodexTestHint => '显示一条测试气泡验证集成';

  @override
  String get settingsCodexTestSend => '发送测试';

  @override
  String get settingsCodexAppServer => '启用 Codex app server';

  @override
  String get settingsCodexAppServerHint => '启用 Codex 交互会话（实验性）';

  @override
  String get settingsCodexExecutable => 'Codex 可执行文件';

  @override
  String get settingsCodexExecutableHint => '留空则使用 PATH 中的 codex';

  @override
  String get settingsBrowse => '浏览...';

  @override
  String get settingsApprovalBubble => '审批提醒气泡';

  @override
  String get settingsPlanBubble => '计划与完成气泡';

  @override
  String get settingsDetachSpeed => '脱离速度';

  @override
  String settingsDetachSpeedHint(Object value) {
    return '当前：$value';
  }

  @override
  String get settingsWindowedMode => '窗口化模式';

  @override
  String get settingsWindowedModeHint => '在独立沙盒窗口中运行桌宠（640×480）';

  @override
  String get settingsWindowedBg => '背景色';

  @override
  String get settingsWindowedBgHint => '沙盒画布背景（#RRGGBB）';

  @override
  String get settingsScale => '缩放';

  @override
  String settingsScaleHint(Object value) {
    return '当前：$value';
  }

  @override
  String get settingsColorSaved => '背景色已保存';

  @override
  String get settingsScaleSaved => '缩放已保存';

  @override
  String get settingsDetachSaved => '脱离速度已保存';

  @override
  String get settingsEdit => '编辑...';

  @override
  String get settingsConfigure => '配置...';

  @override
  String get settingsLanguage => '语言';

  @override
  String get settingsLanguageHint => '界面语言（立即生效）';

  @override
  String get settingsAutostart => '开机自启';

  @override
  String get settingsAutostartHint => '登录时自动启动';

  @override
  String get settingsSilent => '静默启动';

  @override
  String get settingsSilentHint => '开机自启时不显示管理器窗口';

  @override
  String get settingsStartupCombo => '启动时恢复组合';

  @override
  String get settingsStartupLast => '上次关闭前的组合';

  @override
  String get settingsStartupNone => '不恢复';

  @override
  String get settingsStartupSaved => '指定已保存组合...';

  @override
  String settingsStartupSavedNamed(Object name) {
    return '已保存：$name';
  }

  @override
  String get settingsStartupChoose => '选择启动组合';

  @override
  String get settingsUpdateCheck => '启动时检查更新';

  @override
  String get settingsUpdateCheckHint => '自动检查新版本';

  @override
  String get settingsUpdateProxy => '更新代理';

  @override
  String settingsUpdateProxyHint(Object mode) {
    return '当前：$mode';
  }

  @override
  String get settingsProxySystem => '系统';

  @override
  String get settingsProxyDirect => '直连';

  @override
  String get settingsProxyHttp => 'HTTP';

  @override
  String get settingsProxySocks5 => 'SOCKS5';

  @override
  String get settingsProxyHost => '主机';

  @override
  String get settingsProxyPort => '端口';

  @override
  String get settingsProxyUser => '用户名';

  @override
  String get settingsProxyPass => '密码';

  @override
  String get settingsOfflineHint => '运行时离线 — 部分设置需要运行时在线';

  @override
  String get settingsSaveFailed => '设置保存失败';

  @override
  String get aboutVersion => '版本';

  @override
  String get aboutCurrent => '当前版本';

  @override
  String get aboutLatest => '最新版本';

  @override
  String get aboutLatestNotChecked => '未检查更新';

  @override
  String get aboutCopyVersion => '复制版本信息';

  @override
  String aboutCopyFormat(Object version, Object latest) {
    return 'NeurolingsCE $version (latest: $latest)';
  }

  @override
  String get aboutCopied => '版本信息已复制';

  @override
  String get aboutUpdates => '更新';

  @override
  String get aboutOpenReleasePage => '打开发布页面';

  @override
  String get aboutViewReleaseNotes => '查看发布说明';

  @override
  String get aboutProject => '项目与支持';

  @override
  String get aboutUpstream => '上游';

  @override
  String get aboutQQGroup => 'QQ 群';

  @override
  String get aboutReportIssue => '报告问题';

  @override
  String get aboutLicenses => '查看许可证';

  @override
  String get aboutThirdParty => '第三方';

  @override
  String get storeIndex => '商店索引';

  @override
  String get storeNotConfigured => '（未配置）';

  @override
  String get storeRuntimeOffline => '运行时离线';

  @override
  String get storeRuntimeOfflineHint => '请先在主页启动运行时，再浏览商店。';

  @override
  String get storeUnconfigured => '商店未配置';

  @override
  String get storeUnconfiguredHint =>
      '设置环境变量 NEUROLINGSCE_MASCOT_INDEX_URL 指向商店索引，然后重启运行时。';

  @override
  String get storeLoadFailed => '加载索引失败';

  @override
  String get storeCacheWarning => '缓存警告';

  @override
  String get storeSearchHint => '搜索名称、简介、ID或作者...';

  @override
  String get storeAllTags => '全部标签';

  @override
  String get storeEmpty => '商店暂无桌宠包';

  @override
  String get storeNoMatch => '没有匹配的桌宠';

  @override
  String storeCount(int count) {
    return '$count 个桌宠';
  }

  @override
  String storeTag(Object tag) {
    return '标签: $tag';
  }

  @override
  String get storeFromCache => '来自缓存';

  @override
  String get storeDetails => '详情';

  @override
  String get storeInstall => '安装';

  @override
  String storeAuthors(Object names) {
    return '作者: $names';
  }

  @override
  String storeMinVersion(Object version) {
    return '最低版本: $version';
  }

  @override
  String storeInstallOk(Object name) {
    return '安装成功：$name';
  }

  @override
  String get storeInstallOkHint => '已通过 SHA-256 校验并导入模板库，可在主页召唤。';

  @override
  String storeInstallFailed(Object name) {
    return '安装失败：$name';
  }

  @override
  String get storeCommunity => '社区投稿';

  @override
  String get storeCommunityHint => '使用 GitHub 登录后可向注册表投稿桌宠。';

  @override
  String get storeCommunityHintSignedIn => '投稿你自己的桌宠到注册表。';

  @override
  String get storeSignIn => '使用 GitHub 登录';

  @override
  String get storeSignInUnavailable => 'GitHub 登录未配置';

  @override
  String get storeSignInHint => '在 GitHub 上输入此验证码完成登录：';

  @override
  String storeSignInDone(Object login) {
    return '已登录：$login';
  }

  @override
  String get storeSignInFailed => '登录未完成';

  @override
  String get storeSignOut => '退出登录';

  @override
  String get storeCopyCode => '复制验证码';

  @override
  String storeSignedInAs(Object login) {
    return '已登录：$login';
  }

  @override
  String get storeSubmit => '投稿新桌宠...';

  @override
  String get storeSubmitPickPackage => '桌宠包（.mascot）';

  @override
  String get storeSubmitPick => '浏览...';

  @override
  String get storeSubmitName => '名称';

  @override
  String get storeSubmitSummary => '简介';

  @override
  String get storeSubmitMaintainers => '维护者';

  @override
  String get storeSubmitConfirm => '我确认拥有该桌宠的分发权利';

  @override
  String storeSubmitDone(Object url) {
    return '已提交。Pull request：$url';
  }

  @override
  String storeSubmitFailed(Object code, Object error) {
    return '投稿失败（$code）：$error';
  }

  @override
  String get codexSession => '会话';

  @override
  String get codexStatus => '状态';

  @override
  String get codexThread => '线程';

  @override
  String get codexWorkspace => '工作区';

  @override
  String get codexTurn => '回合';

  @override
  String get codexMode => '模式';

  @override
  String get codexConnection => '连接';

  @override
  String get codexConnect => '连接 Codex';

  @override
  String get codexDisconnect => '断开连接';

  @override
  String get codexNewSession => '新建会话';

  @override
  String get codexResume => '恢复最近的会话';

  @override
  String get codexApprovals => '审批';

  @override
  String get codexNoApprovals => '没有待审批项。';

  @override
  String get codexPlan => '计划';

  @override
  String get codexNoPlan => '暂无计划。';

  @override
  String get codexMessage => '消息';

  @override
  String get codexModeDefault => '默认';

  @override
  String get codexModePlan => '计划';

  @override
  String get codexPlanUnsupported => '不支持';

  @override
  String get codexAskPlaceholder => '向 Codex 提问...';

  @override
  String get codexSend => '发送';

  @override
  String get codexImplementPlan => '实施此计划';

  @override
  String get codexModifyPlan => '修改计划';

  @override
  String get codexAbort => '中断任务';

  @override
  String get codexDecline => '拒绝';

  @override
  String get codexAllowOnce => '允许一次';

  @override
  String get codexAllowSession => '本次会话允许';

  @override
  String get codexDeclineStop => '拒绝并停止';

  @override
  String get codexInputTitle => 'Codex 需要输入';

  @override
  String get codexNoReply => '任务已完成，但没有可显示的回复。';

  @override
  String get codexEmptyInput => '请先输入内容';

  @override
  String get codexDisabledTitle => 'Codex app server 未启用';

  @override
  String get codexDisabledHint => '在设置页启用“Codex app server”后才能使用交互会话。';

  @override
  String get aboutCheckForUpdates => '检查更新';

  @override
  String get aboutDownloadInstall => '下载并安装...';

  @override
  String aboutInstall(Object version) {
    return '安装 $version';
  }

  @override
  String get aboutIgnoreVersion => '忽略此版本';

  @override
  String get aboutRemindLater => '稍后提醒';

  @override
  String aboutUpdateAvailable(Object version) {
    return 'NeurolingsCE $version 可用。';
  }

  @override
  String get aboutInstallConfirmTitle => '安装更新？';

  @override
  String aboutInstallConfirmBody(Object version) {
    return '版本 $version 已下载完成。现在启动安装程序吗？应用将关闭。';
  }

  @override
  String get aboutInstallNow => '安装';
}
