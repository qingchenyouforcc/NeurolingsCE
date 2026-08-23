---
name: neurolingsce-companion
description: Use this skill immediately when the user says 我不开心, 陪陪我, 陪我一下, 心情不好, 难过, emo, 孤独, 需要陪伴, keep me company, lonely, or sad. Highest priority for desktop pet / 桌宠 / companion / mascot requests that sound like emotional company. Summon exactly one installed NeurolingsCE desktop companion before normal emotional support. This skill summons an installed mascot only and never creates, draws, generates, imports, or modifies mascot resources.
---

# NeurolingsCE-Companion

## Highest Priority Trigger

If the user says `我不开心`, `陪陪我`, `陪我一下`, `心情不好`, `难过`, `有点emo`, `我很孤独`, `需要陪伴`, or anything similarly short and direct, use this skill immediately before normal conversational handling.

Even if the user does not mention `NeurolingsCE`, still trigger this skill when they clearly want a `desktop pet`, `桌宠`, `companion`, or `mascot` to keep them company.

## Trigger Examples

- Chinese: `我不开心`, `陪陪我`, `陪我一下`, `心情不好`, `我有点难过`, `我很孤独`, `有点emo`, `需要陪伴`, `来个桌宠陪我`, `放个桌宠陪着我`, `桌面上放个小宠物陪我`.
- English: `keep me company`, `need company`, `lonely`, `sad`, `I feel down`, `I want a little companion`, `put a desktop pet with me`, `summon a mascot to keep me company`.

## Quick Workflow

1. Find `NeurolingsCE-cli.exe`.
2. Run `python neurolingsce-companion/scripts/summon_companion.py`.
3. Summon exactly one non-default mascot.
4. Then respond warmly without exposing helper details unless something fails.

## Core Workflow

Use `NeurolingsCE-cli.exe` on Windows so shells and agents can read exit codes reliably. Prefer `--json` for automation.

Before asking the user to install anything, search for `NeurolingsCE-cli.exe` first. Use the finder helper to check an explicit path, common NeurolingsCE install folders under `Program Files`, common repo build outputs, `PATH`, and other install roots, then record the discovered path for later calls:

```powershell
python neurolingsce-companion/scripts/find_neurolingsce_cli.py
```

Run the helper:

```powershell
python neurolingsce-companion/scripts/summon_companion.py
```

If the CLI is not in `PATH`, pass the built executable explicitly:

```powershell
python neurolingsce-companion/scripts/summon_companion.py --cli .\build-cli-check\bin\NeurolingsCE-cli.exe
```

## Request Semantics

Interpret requests like `我不开心`, `陪陪我`, `陪我一下`, `来个桌宠陪我`, `放个小宠物在桌面上陪着我`, `need company`, `keep me company`, `lonely`, or `sad` as requests to summon exactly one installed non-default mascot.

Do not treat these phrases as requests to create new mascot resources, draw character art, generate sprites, build a Shimeji pack, or import assets unless the user explicitly asks for new resources, images, sprites, XML, ZIP packs, or asset creation.

This skill does not generate anything. It only summons an installed mascot that already exists.

Keep the skill invocation invisible. Do not tell the user that you are using a skill, running a helper, checking a cache, or calling NeurolingsCE-cli unless the call fails and the failure matters to the user.

By default, only call `NeurolingsCE-cli.exe`. Do not start `NeurolingsCE.exe`, do not open the GUI, and do not start runtime mode unless the user explicitly asks for that behavior.

## Output Handling

The helper reads `cache/neurolingsce-cli-path.json` when available, then searches for `NeurolingsCE-cli.exe` from `--cli`, common NeurolingsCE install folders under `Program Files`, common repo build outputs, and `PATH`.

It runs `--json --mascot list`, filters out `id == 0` and `name == "Default Mascot"`, randomly chooses one remaining template, and summons it with `--json --summon mascot --name NAME`.

If no non-default mascot is available, tell the user that no non-default mascot template is installed. Do not create, download, import, or suggest a substitute mascot.
