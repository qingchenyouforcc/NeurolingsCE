# CLI 输出契约（派生自 C++ 源码，待用真实二进制回归验证）

来源：`NeurolingsCE/src/app/cli/{OutputFormatter,CommandLineParser,CommandExecutor}.cc`
与 `src/app/core/commands/MascotApi.cc`。Rust 版 CLI 的 stdout/stderr/退出码必须与此一致。

## 退出码

- `0` 成功；`1` 通用失败（含 validate 无效包）；`2` 参数错误
- `--mascot validate`：`--quiet` 时 0=有效 / 1=无效，无输出
- `--stop` 幂等：运行时已停止也算成功，且不自动启动运行时
- 运行时命令在无运行时可用时自动启动之；`--codex-notify` 永不自动启动

## JSON 对象形状（`--json`，单行 compact，末尾换行）

### mascotInfo（运行中桌宠）
```json
{"id":1,"data_id":"...","name":"...","label":2,"anchor":{"x":0.0,"y":0.0},"active_behavior":"..."}
```
`label`/`active_behavior` 无值时为 JSON null。

### loadedMascotInfo（已加载模板）
```json
{"id":1,"name":"...","version":"...","description":"...","author":"..."}
```

### 各命令成功输出
| 命令 | JSON |
|---|---|
| `--version` | `{"app":"NeurolingsCE","version":"x.y.z"}` |
| `--list` / legacy `list` | `{"mascots":[mascotInfo...]}` |
| legacy `list-loaded` | `{"loaded_mascots":[loadedMascotInfo...]}` |
| `--summon ...` | `{"mascot":mascotInfo,"label":N}`（label 仅在有 CLI 标签时） |
| legacy `spawn`/`alter` | `{"mascot":mascotInfo}` |
| `--stop` | `{"stopped":true}` |
| `--mascot list` / `add` | `{"templates":[loadedMascotInfo...]}` |
| `--mascot remove` | `{"removed":"NAME"}` |
| `--mascot validate` | 见下 |
| `--codex-notify` | `{"handled":bool,"event_type":"...","state":"..."}`（后两者可省） |
| `--help` | helpJson：app/version/global_options/commands[]/legacy_commands[]/label_scope="current_app_run" |

### validate 报告
```json
{"ok":true,"mascot":{"name":"...","version":"...","description":"...","author":"..."},
 "package_version":"...","entry_count":12,"file_count":10,"extracted_bytes":12345,"errors":[]}
```

### 错误输出
```json
{"error":"message","code":"code_string","details":"...","usage":"...","status":400}
```
`details`/`usage` 为空省略；`status` 仅 httpStatus>0 时存在。
非 JSON 模式：stderr 为 `ERROR: <error>` 后接可选 details/usage 行。

## 文本模式要点

- `--version`：`NeurolingsCE x.y.z`
- `--list`：`[label:N|-] [runtime:ID] NAME`
- legacy `list`：`[ID] NAME` + `  Data ID:` / `  Active behavior:` / `  Anchor: {x, y}`
- `--mascot list/add`：`[ID] NAME` + 可选 `  Version:` / `  Author:` 行
- `--mascot remove`：`Removed mascot template NAME`
- validate 有效：`Valid mascot package: NAME vX.Y (N files, B bytes)`
- validate 无效：`Invalid mascot package:` 后逐条 `  - <error>`
- `--close`：`Closed label N`；`--close-all`：`Closed all mascots`；`--stop`：`Stopped NeurolingsCE runtime`

## 解析约束

- 拒绝 `--host` / `--port`
- 全局选项：`--quiet` `--json` `--connect-timeout-ms` `--read-timeout-ms`
- legacy 命令仍支持：`list list-loaded spawn alter dismiss dismiss-all`
- IPC 端点名：`io.github.qingchenyouforcc.NeurolingsCE.cli`（JSON lines）
