import 'dart:async';

import 'package:fluent_ui/fluent_ui.dart';
import 'package:neurolings_manager/l10n/app_localizations.dart';
import 'package:provider/provider.dart';

import '../state/app_state.dart';

/// Codex 页：app-server 交互全流程（对齐原版 ManagerCodexPage）。
/// 状态卡 / Connection / Approvals / Plan / Message / 诊断 / 用户输入问答。
class CodexPage extends StatefulWidget {
  const CodexPage({super.key});

  @override
  State<CodexPage> createState() => _CodexPageState();
}

class _CodexPageState extends State<CodexPage> {
  Timer? _poll;
  Map<String, dynamic> _status = {};
  final _input = TextEditingController();
  bool _planMode = false;
  String _lastThreadId = '';
  String _lastWorkspace = '';
  // 弹出的用户输入请求 id（避免重复弹窗）。
  String? _activeInputId;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _load());
    _poll = Timer.periodic(const Duration(seconds: 1), (_) => _pollStatus());
  }

  @override
  void dispose() {
    _poll?.cancel();
    _input.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    await _pollStatus();
    await _loadThreadMemory();
  }

  Future<void> _loadThreadMemory() async {
    try {
      final settings =
          await context.read<AppState>().api.command({'command': 'get_settings'});
      if (!mounted) return;
      setState(() {
        _lastThreadId = settings['codex/lastThreadId'] as String? ?? '';
        _lastWorkspace = settings['codex/lastWorkspace'] as String? ?? '';
      });
    } catch (_) {}
  }

  Future<void> _pollStatus() async {
    try {
      final status = await context
          .read<AppState>()
          .api
          .command({'command': 'codex_server_status'});
      if (!mounted) return;
      final threadId = status['thread_id'] as String? ?? '';
      final workspace = status['workspace'] as String? ?? '';
      // threadChanged 持久化时机对齐原版（UI 收到信号时写设置）。
      if (threadId.isNotEmpty && threadId != _lastThreadId) {
        _lastThreadId = threadId;
        _persist('codex/lastThreadId', threadId);
      }
      if (workspace.isNotEmpty && workspace != _lastWorkspace) {
        _lastWorkspace = workspace;
        _persist('codex/lastWorkspace', workspace);
      }
      setState(() => _status = status);
      _maybeShowUserInput();
    } catch (_) {
      // 运行时离线时保持上次状态。
    }
  }

  Future<void> _persist(String key, dynamic value) async {
    try {
      await context
          .read<AppState>()
          .api
          .command({'command': 'set_settings', 'key': key, 'value': value});
    } catch (_) {}
  }

  Future<void> _cmd(Map<String, dynamic> payload) async {
    try {
      await context.read<AppState>().api.command(payload);
    } catch (e) {
      if (!mounted) return;
      displayInfoBar(context, builder: (ctx, close) {
        return InfoBar(
            title: Text(AppLocalizations.of(context).error),
            content: Text(e.toString()),
            severity: InfoBarSeverity.error);
      });
    }
    await _pollStatus();
  }

  bool get _enabled => _status['enabled'] == true;
  String get _state => _status['state'] as String? ?? 'Stopped';
  bool get _running => _state == 'Running';
  bool get _ready => _state == 'Ready';

  Future<void> _connect() async {
    await _cmd({'command': 'codex_server_connect'});
  }

  Future<void> _disconnect() async {
    await _cmd({'command': 'codex_server_disconnect'});
  }

  Future<void> _sendText({bool forcePlan = false}) async {
    final l10n = AppLocalizations.of(context);
    var text = _input.text.trim();
    if (forcePlan) {
      text = 'Please begin implementing the confirmed plan.';
    }
    if (text.isEmpty) {
      displayInfoBar(context, builder: (ctx, close) {
        return InfoBar(
            title: Text(l10n.codexEmptyInput), severity: InfoBarSeverity.warning);
      });
      return;
    }
    final usePlan = forcePlan ? false : _planMode;
    if (_running) {
      await _cmd({'command': 'codex_server_steer', 'text': text});
    } else {
      await _cmd({
        'command': 'codex_server_turn',
        'text': text,
        'plan': usePlan,
      });
    }
  }

  /// 用户输入请求弹出模态问答（≤3 题、每题 ≤3 选项 + isOther 文本）。
  void _maybeShowUserInput() {
    final inputs = _status['user_inputs'];
    if (inputs is! List || inputs.isEmpty || _activeInputId != null) return;
    final input = inputs.first;
    if (input is! Map) return;
    final id = input['id'] as String? ?? '';
    if (id.isEmpty) return;
    _activeInputId = id;
    final l10n = AppLocalizations.of(context);
    unawaited(showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (dialogContext) {
        return _UserInputDialog(
          input: input.cast<String, dynamic>(),
          onSubmit: (answers) async {
            await _cmd({
              'command': 'codex_server_input',
              'id': id,
              'answers': answers,
            });
          },
          onCancel: () async {
            await _cmd({
              'command': 'codex_server_input',
              'id': id,
              'answers': <String, dynamic>{},
            });
          },
          l10n: l10n,
        );
      },
    ).whenComplete(() {
      _activeInputId = null;
    }));
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final threadId = _status['thread_id'] as String? ?? '';
    final turnId = _status['turn_id'] as String? ?? '';
    final workspace = _status['workspace'] as String? ?? '';
    final plan = (_status['plan'] as Map?)?.cast<String, dynamic>() ?? {};
    final planSteps = (plan['steps'] as List?)
            ?.whereType<Map>()
            .map((e) => e.cast<String, dynamic>())
            .toList() ??
        <Map<String, dynamic>>[];
    final planFinal = plan['final'] == true;
    final planText = (plan['final_text'] as String?)?.isNotEmpty == true
        ? plan['final_text'] as String
        : (plan['explanation'] as String? ?? '');
    final finalMessage = _status['final_message'] as String? ?? '';
    final diagnostic = _status['diagnostic'] as String? ?? '';
    final approvals = (_status['approvals'] as List?)
            ?.whereType<Map>()
            .map((e) => e.cast<String, dynamic>())
            .toList() ??
        <Map<String, dynamic>>[];
    final planSupported = _status['plan_supported'] == true;

    return ScaffoldPage.scrollable(
      header: PageHeader(title: Text(l10n.navCodex)),
      children: [
        // 状态卡
        Card(
          margin: const EdgeInsets.symmetric(horizontal: 16),
          child: Padding(
            padding: const EdgeInsets.all(14),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(l10n.codexSession,
                    style: const TextStyle(fontWeight: FontWeight.w600)),
                const SizedBox(height: 8),
                _row(l10n.codexStatus, _state),
                _row(l10n.codexThread,
                    threadId.isEmpty ? '-' : threadId.substring(0, threadId.length.clamp(0, 16))),
                _row(l10n.codexWorkspace, workspace.isEmpty ? '-' : workspace),
                _row(l10n.codexTurn, turnId.isEmpty ? '-' : turnId),
              ],
            ),
          ),
        ),
        const SizedBox(height: 8),

        if (!_enabled)
          // 未启用 app-server 时的降级提示（对齐原版页面降级态）。
          Padding(
            padding: const EdgeInsets.all(16),
            child: InfoBar(
              title: Text(l10n.codexDisabledTitle),
              content: Text(l10n.codexDisabledHint),
              severity: InfoBarSeverity.info,
              isLong: true,
            ),
          )
        else ...[
          // Connection
          Card(
            margin: const EdgeInsets.symmetric(horizontal: 16),
            child: Padding(
              padding: const EdgeInsets.all(14),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(l10n.codexConnection,
                      style: const TextStyle(fontWeight: FontWeight.w600)),
                  const SizedBox(height: 8),
                  Wrap(spacing: 8, runSpacing: 8, children: [
                    if (_state == 'Stopped' || _state == 'Blocked')
                      FilledButton(
                        onPressed: _connect,
                        child: Text(l10n.codexConnect),
                      )
                    else
                      Button(onPressed: _disconnect, child: Text(l10n.codexDisconnect)),
                    Button(
                      onPressed: _ready ? () => _cmd({'command': 'codex_server_new_thread', 'cwd': _lastWorkspace}) : null,
                      child: Text(l10n.codexNewSession),
                    ),
                    Button(
                      onPressed: _ready && _lastThreadId.isNotEmpty
                          ? () => _cmd({
                                'command': 'codex_server_resume',
                                'thread_id': _lastThreadId,
                              })
                          : null,
                      child: Text(l10n.codexResume),
                    ),
                  ]),
                  if (diagnostic.isNotEmpty) ...[
                    const SizedBox(height: 8),
                    Text(diagnostic,
                        style: FluentTheme.of(context).typography.caption),
                  ],
                ],
              ),
            ),
          ),
          const SizedBox(height: 8),

          // Approvals
          Card(
            margin: const EdgeInsets.symmetric(horizontal: 16),
            child: Padding(
              padding: const EdgeInsets.all(14),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('${l10n.codexApprovals} (${approvals.length})',
                      style: const TextStyle(fontWeight: FontWeight.w600)),
                  const SizedBox(height: 8),
                  if (approvals.isEmpty)
                    Text(l10n.codexNoApprovals,
                        style: FluentTheme.of(context).typography.caption)
                  else
                    for (final approval in approvals)
                      _approvalTile(context, approval),
                ],
              ),
            ),
          ),
          const SizedBox(height: 8),

          // Plan
          Card(
            margin: const EdgeInsets.symmetric(horizontal: 16),
            child: Padding(
              padding: const EdgeInsets.all(14),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(l10n.codexPlan,
                      style: const TextStyle(fontWeight: FontWeight.w600)),
                  const SizedBox(height: 8),
                  if (planSteps.isEmpty)
                    Text(l10n.codexNoPlan,
                        style: FluentTheme.of(context).typography.caption)
                  else
                    for (final step in planSteps)
                      Text(
                        '[${step['status']}] ${step['step']}',
                        style: FluentTheme.of(context).typography.body,
                      ),
                  if (planText.isNotEmpty) ...[
                    const SizedBox(height: 8),
                    SelectableText(planText),
                  ],
                ],
              ),
            ),
          ),
          const SizedBox(height: 8),

          // Message
          Card(
            margin: const EdgeInsets.symmetric(horizontal: 16),
            child: Padding(
              padding: const EdgeInsets.all(14),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(l10n.codexMessage,
                      style: const TextStyle(fontWeight: FontWeight.w600)),
                  const SizedBox(height: 8),
                  Row(children: [
                    Text('${l10n.codexMode}: '),
                    ComboBox<bool>(
                      value: _planMode,
                      items: [
                        ComboBoxItem(value: false, child: Text(l10n.codexModeDefault)),
                        ComboBoxItem(
                            value: true,
                            child: planSupported
                                ? Text(l10n.codexModePlan)
                                : Text('${l10n.codexModePlan} (${l10n.codexPlanUnsupported})')),
                      ],
                      onChanged: (v) {
                        if (v == null) return;
                        setState(() => _planMode = v);
                      },
                    ),
                  ]),
                  const SizedBox(height: 8),
                  SizedBox(
                    height: 90,
                    child: TextBox(
                      maxLines: null,
                      controller: _input,
                      placeholder: l10n.codexAskPlaceholder,
                    ),
                  ),
                  const SizedBox(height: 8),
                  Wrap(spacing: 8, runSpacing: 8, children: [
                    FilledButton(
                      onPressed: _ready || _running ? () => _sendText() : null,
                      child: Text(l10n.codexSend),
                    ),
                    Button(
                      onPressed: planFinal && planText.isNotEmpty && _ready
                          ? () => _sendText(forcePlan: true)
                          : null,
                      child: Text(l10n.codexImplementPlan),
                    ),
                    Button(
                      onPressed: _ready ? () => _sendText(forcePlan: true) : null,
                      child: Text(l10n.codexModifyPlan),
                    ),
                    Button(
                      onPressed: _running
                          ? () => _cmd({'command': 'codex_server_interrupt'})
                          : null,
                      child: Text(l10n.codexAbort),
                    ),
                  ]),
                  const SizedBox(height: 8),
                  if (finalMessage.trim().isNotEmpty)
                    SelectableText(
                      finalMessage.trim(),
                      style: FluentTheme.of(context).typography.body,
                    )
                  else
                    Text(l10n.codexNoReply,
                        style: FluentTheme.of(context).typography.caption),
                ],
              ),
            ),
          ),
        ],
      ],
    );
  }

  Widget _row(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        SizedBox(width: 90, child: Text(label)),
        Expanded(child: SelectableText(value)),
      ]),
    );
  }

  Widget _approvalTile(BuildContext context, Map<String, dynamic> approval) {
    final l10n = AppLocalizations.of(context);
    final decisions = (approval['available_decisions'] as List?)
            ?.map((e) => e.toString())
            .toList() ??
        <String>[];
    bool has(String d) => decisions.isEmpty || decisions.contains(d);
    final changes = (approval['changes'] as List?)
            ?.whereType<Map>()
            .map((e) => e.cast<String, dynamic>())
            .toList() ??
        <Map<String, dynamic>>[];
    return Container(
      margin: const EdgeInsets.symmetric(vertical: 4),
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        border: Border.all(color: Colors.grey.withOpacity(0.3)),
        borderRadius: BorderRadius.circular(7),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('${approval['kind'] ?? ''}',
              style: const TextStyle(fontWeight: FontWeight.w600)),
          if ((approval['command'] as String?)?.isNotEmpty == true)
            SelectableText(approval['command'] as String,
                style: const TextStyle(fontFamily: 'Consolas, monospace', fontSize: 11)),
          if ((approval['reason'] as String?)?.isNotEmpty == true)
            Text(approval['reason'] as String,
                style: FluentTheme.of(context).typography.caption),
          for (final change in changes)
            Text('${change['kind']}: ${change['path']}',
                style: FluentTheme.of(context).typography.caption),
          const SizedBox(height: 6),
          Wrap(spacing: 6, runSpacing: 6, children: [
            Button(
              onPressed: has('decline')
                  ? () => _cmd({
                        'command': 'codex_server_resolve',
                        'id': approval['id'],
                        'decision': 'decline',
                      })
                  : null,
              child: Text(l10n.codexDecline),
            ),
            Button(
              onPressed: has('accept')
                  ? () => _cmd({
                        'command': 'codex_server_resolve',
                        'id': approval['id'],
                        'decision': 'accept',
                      })
                  : null,
              child: Text(l10n.codexAllowOnce),
            ),
            Button(
              onPressed: has('acceptForSession')
                  ? () => _cmd({
                        'command': 'codex_server_resolve',
                        'id': approval['id'],
                        'decision': 'acceptForSession',
                      })
                  : null,
              child: Text(l10n.codexAllowSession),
            ),
            Button(
              onPressed: has('cancel')
                  ? () => _cmd({
                        'command': 'codex_server_resolve',
                        'id': approval['id'],
                        'decision': 'cancel',
                      })
                  : null,
              child: Text(l10n.codexDeclineStop),
            ),
          ]),
        ],
      ),
    );
  }
}

/// 用户输入模态问答（提交 {qid:{answers:[text]}}；取消空对象）。
class _UserInputDialog extends StatefulWidget {
  const _UserInputDialog({
    required this.input,
    required this.onSubmit,
    required this.onCancel,
    required this.l10n,
  });

  final Map<String, dynamic> input;
  final Future<void> Function(Map<String, dynamic>) onSubmit;
  final Future<void> Function() onCancel;
  final AppLocalizations l10n;

  @override
  State<_UserInputDialog> createState() => _UserInputDialogState();
}

class _UserInputDialogState extends State<_UserInputDialog> {
  final Map<String, TextEditingController> _other = {};
  final Map<String, String?> _choice = {};

  @override
  void dispose() {
    for (final controller in _other.values) {
      controller.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = widget.l10n;
    final questions = (widget.input['questions'] as List?)
            ?.whereType<Map>()
            .map((e) => e.cast<String, dynamic>())
            .toList() ??
        <Map<String, dynamic>>[];
    return ContentDialog(
      title: Text(l10n.codexInputTitle),
      content: SizedBox(
        width: 480,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              for (final question in questions)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 6),
                  child: _questionBlock(question),
                ),
            ],
          ),
        ),
      ),
      actions: [
        Button(
          onPressed: () {
            widget.onCancel();
            Navigator.pop(context);
          },
          child: Text(l10n.cancel),
        ),
        FilledButton(
          onPressed: () {
            final answers = <String, dynamic>{};
            for (final question in questions) {
              final qid = question['id'] as String? ?? '';
              final isOther = question['is_other'] == true;
              final text = isOther
                  ? (_other[qid]?.text.trim() ?? '')
                  : (_choice[qid] ?? '');
              answers[qid] = {
                'answers': [text],
              };
            }
            widget.onSubmit(answers);
            Navigator.pop(context);
          },
          child: Text(l10n.ok),
        ),
      ],
    );
  }

  Widget _questionBlock(Map<String, dynamic> question) {
    final qid = question['id'] as String? ?? '';
    final isOther = question['is_other'] == true;
    final isSecret = question['is_secret'] == true;
    final options = (question['options'] as List?)
            ?.whereType<Map>()
            .map((e) => e.cast<String, dynamic>())
            .toList() ??
        <Map<String, dynamic>>[];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if ((question['header'] as String?)?.isNotEmpty == true)
          Text(question['header'] as String,
              style: const TextStyle(fontWeight: FontWeight.w600)),
        Text(question['question'] as String? ?? ''),
        const SizedBox(height: 6),
        if (isOther)
          TextBox(
            controller: _other.putIfAbsent(qid, TextEditingController.new),
            obscureText: isSecret,
          )
        else
          RadioGroup<String>(
            groupValue: _choice[qid],
            onChanged: (value) => setState(() => _choice[qid] = value),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final option in options)
                  RadioButton<String>(
                    value: option['label'] as String? ?? '',
                    content: Text(option['label'] as String? ?? ''),
                  ),
              ],
            ),
          ),
      ],
    );
  }
}
