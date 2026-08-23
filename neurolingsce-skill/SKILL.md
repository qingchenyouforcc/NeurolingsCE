---
name: neurolingsce-skill
description: Use NeurolingsCE-Skill for NeurolingsCE control actions such as listing templates, summoning a named mascot, closing mascots, stopping the runtime, or handling NeurolingsCE-cli automation. Trigger this skill when the user asks to 列桌宠, 召唤指定桌宠, 关闭桌宠, close all mascots, stop NeurolingsCE, inspect templates, or run CLI control commands. This skill controls installed mascots only and never creates, draws, generates, imports, or modifies mascot resources.
---

# NeurolingsCE-Skill

## Trigger Guide

Use this skill when the user wants to control or inspect NeurolingsCE, not when they only want emotional company.

Typical trigger wording includes:

- English: `list my mascots`, `show templates`, `summon Hatsune Miku`, `spawn a mascot by name`, `close the mascot`, `close all mascots`, `stop NeurolingsCE`, `NeurolingsCE-cli`.
- Chinese: `列一下桌宠`, `看看有哪些桌宠`, `召唤初音`, `把这个桌宠叫出来`, `关闭桌宠`, `关闭所有桌宠`, `停止 NeurolingsCE`, `列模板`, `桌宠控制`.

If the user simply says `我不开心`, `陪陪我`, `心情不好`, `lonely`, `sad`, or asks for company without a control request, prefer the companion skill instead.

## Quick Workflow

1. Find `NeurolingsCE-cli.exe`.
2. Use `--json`.
3. Run the requested control command.
4. Parse JSON and answer from the result.

## Core Workflow

Use `NeurolingsCE-cli.exe` on Windows so shells and agents can read exit codes reliably. Prefer `--json` for automation.

Runtime control commands auto-start NeurolingsCE when no local runtime is ready, then talk over local IPC. Do not use `--host` or `--port`; they are no longer supported.

Before asking the user to install anything, search for `NeurolingsCE-cli.exe` first. Use the finder helper to check an explicit path, common NeurolingsCE install folders under `Program Files`, common repo build outputs, `PATH`, and other install roots, then record the discovered path for later calls:

```powershell
python neurolingsce-skill/scripts/find_neurolingsce_cli.py
```

Pass extra hints when needed:

```powershell
python neurolingsce-skill/scripts/find_neurolingsce_cli.py --cli C:\path\to\NeurolingsCE-cli.exe
python neurolingsce-skill/scripts/find_neurolingsce_cli.py --search-root D:\Apps
```

The finder writes `neurolingsce-skill/cache/neurolingsce-cli-path.json`. If no CLI is found after this search, tell the user to install or build NeurolingsCE. Do not create mascot resources.

Common commands:

```powershell
NeurolingsCE-cli.exe --json --mascot list
NeurolingsCE-cli.exe --json --summon mascot --name NAME
NeurolingsCE-cli.exe --json --summon mascot --data-id ID
NeurolingsCE-cli.exe --json --list
NeurolingsCE-cli.exe --json --close LABEL
NeurolingsCE-cli.exe --json --close-all
NeurolingsCE-cli.exe --json --stop
```

Use `--summon random` only when the user explicitly wants any loaded mascot. For companionship, do not use it because it may choose `Default Mascot`.

## Request Semantics

Interpret requests like `召唤初音`, `把默认以外的某个桌宠叫出来`, `列一下模板`, or `关闭所有桌宠` as control requests to send through `NeurolingsCE-cli.exe`.

When the request includes a mascot name, first list templates with `NeurolingsCE-cli.exe --json --mascot list`, match the requested name against available templates, then summon the matched template. If no match exists, tell the user exactly that this template is not installed or not found.

This skill does not generate mascot assets. Do not create images, sprites, ZIP packs, XML, or replacement resources.

Keep the skill invocation invisible. Do not tell the user that you are using a skill, running a helper, checking a cache, or calling NeurolingsCE-cli unless the call fails and the failure matters to the user.

By default, only call `NeurolingsCE-cli.exe`. Do not start `NeurolingsCE.exe`, do not open the GUI, and do not start runtime mode unless the user explicitly asks for that behavior.

## Output Handling

Parse JSON responses instead of scraping human-readable output. Template listing returns `templates`; running mascot listing returns `mascots`; summon responses include `mascot` and may include a `label`.

CLI labels are user-facing temporary labels for the current app run. They are separate from runtime mascot IDs and are cleared when NeurolingsCE restarts.
