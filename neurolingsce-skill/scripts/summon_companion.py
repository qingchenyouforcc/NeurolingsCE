#!/usr/bin/env python3
"""Summon one random non-default NeurolingsCE mascot."""

from __future__ import annotations

import argparse
import json
import os
import random
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any


DEFAULT_NAME = "Default Mascot"


def write_json(payload: dict[str, Any], exit_code: int) -> int:
    print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
    return exit_code


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def skill_root() -> Path:
    return Path(__file__).resolve().parents[1]


def default_record_path() -> Path:
    return skill_root() / "cache" / "neurolingsce-cli-path.json"


def recorded_cli_path(record_path: Path) -> Path | None:
    if not record_path.is_file():
        return None
    try:
        payload = json.loads(record_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    if not isinstance(payload, dict):
        return None
    cli = payload.get("cli")
    if not isinstance(cli, str) or not cli:
        return None
    path = Path(cli)
    return path if path.is_file() else None


def append_candidate(candidates: list[Path], path: Path) -> None:
    if path not in candidates:
        candidates.append(path)


def common_install_candidates() -> list[Path]:
    candidates: list[Path] = []
    install_roots = [
        Path(root).expanduser()
        for env_name in ("ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA")
        for root in [os.environ.get(env_name)]
        if root
    ]
    for root in install_roots:
        app_root = root / "NeurolingsCE"
        append_candidate(candidates, app_root / "NeurolingsCE-cli.exe")
        append_candidate(candidates, app_root / "bin" / "NeurolingsCE-cli.exe")
        append_candidate(candidates, app_root / "NeurolingsCE-cli")
        append_candidate(candidates, app_root / "bin" / "NeurolingsCE-cli")
    return candidates


def candidate_cli_paths(explicit: str | None) -> list[Path]:
    candidates: list[Path] = []
    if explicit:
        append_candidate(candidates, Path(explicit))

    candidates.extend(common_install_candidates())

    root = repo_root()
    for candidate in [
        root / "out" / "build" / "x64-Release" / "bin" / "NeurolingsCE-cli.exe",
        root / "out" / "build" / "x64-Debug" / "bin" / "NeurolingsCE-cli.exe",
        root / "build" / "bin" / "NeurolingsCE-cli.exe",
        root / "build-release" / "bin" / "NeurolingsCE-cli.exe",
        root / "build-cli-check" / "bin" / "NeurolingsCE-cli.exe",
        root / "publish" / "Windows" / "release" / "NeurolingsCE-cli.exe",
        root / "publish" / "Windows" / "debug" / "NeurolingsCE-cli.exe",
    ]:
        append_candidate(candidates, candidate)

    for name in ("NeurolingsCE-cli.exe", "NeurolingsCE-cli"):
        found = shutil.which(name)
        if found:
            append_candidate(candidates, Path(found))
    return candidates


def parse_json_object(stdout: str) -> dict[str, Any] | None:
    try:
        payload = json.loads(stdout)
    except json.JSONDecodeError:
        return None
    return payload if isinstance(payload, dict) else None


def cli_supports_current_contract(cli: Path) -> bool:
    code, stdout, _stderr = run_cli(cli, ["--json", "--help"])
    if code != 0:
        return False
    payload = parse_json_object(stdout)
    if payload is None:
        return False
    commands = payload.get("commands")
    if not isinstance(commands, list):
        return False
    names = {
        item.get("name")
        for item in commands
        if isinstance(item, dict) and isinstance(item.get("name"), str)
    }
    required = {"--summon", "--mascot", "--list", "--stop"}
    return required.issubset(names)


def find_cli(explicit: str | None, record_path: Path) -> tuple[Path | None, list[str]]:
    rejected: list[str] = []

    candidates: list[Path] = []
    recorded = recorded_cli_path(record_path)
    if recorded is not None:
        candidates.append(recorded)
    candidates.extend(candidate_cli_paths(explicit))

    seen: set[Path] = set()
    for candidate in candidates:
        if candidate in seen:
            continue
        seen.add(candidate)
        if not candidate.is_file():
            continue
        if not cli_supports_current_contract(candidate):
            rejected.append(str(candidate))
            continue
        return candidate, rejected
    return None, rejected


def run_cli(cli: Path, args: list[str]) -> tuple[int, str, str]:
    completed = subprocess.run(
        [str(cli), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    return completed.returncode, completed.stdout.strip(), completed.stderr.strip()


def parse_cli_json(stdout: str, command: str) -> tuple[dict[str, Any] | None, dict[str, Any] | None]:
    try:
        value = json.loads(stdout)
    except json.JSONDecodeError as exc:
        return None, {
            "ok": False,
            "code": "invalid_cli_json",
            "message": f"{command} did not return valid JSON.",
            "details": str(exc),
            "stdout": stdout,
        }
    if not isinstance(value, dict):
        return None, {
            "ok": False,
            "code": "invalid_cli_json",
            "message": f"{command} returned JSON that was not an object.",
            "stdout": stdout,
        }
    return value, None


def template_name(template: dict[str, Any]) -> str:
    value = template.get("name", "")
    return value if isinstance(value, str) else ""


def is_non_default_template(template: dict[str, Any]) -> bool:
    name = template_name(template).strip()
    if not name:
        return False
    if name.casefold() == DEFAULT_NAME.casefold():
        return False
    template_id = template.get("id")
    if template_id == 0 or template_id == "0":
        return False
    return True


def loaded_templates(payload: dict[str, Any]) -> list[dict[str, Any]]:
    value = payload.get("templates", payload.get("loaded_mascots", []))
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, dict)]


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description="Summon one random non-default NeurolingsCE mascot."
    )
    parser.add_argument("--cli", help="Path to NeurolingsCE-cli.exe.")
    parser.add_argument(
        "--record",
        default=str(default_record_path()),
        help="JSON record produced by find_neurolingsce_cli.py.",
    )
    parser.add_argument(
        "--label",
        help="Optional user-facing CLI label to pass to --summon.",
    )
    args = parser.parse_args(argv)

    record_path = Path(args.record).expanduser()
    cli, rejected = find_cli(args.cli, record_path)
    if cli is None:
        searched = [str(path) for path in candidate_cli_paths(args.cli)]
        return write_json(
            {
                "ok": False,
                "code": "cli_not_found",
                "message": "Could not find a current NeurolingsCE-cli.exe after checking the recorded path, common NeurolingsCE install folders under Program Files, the explicit path, repo build locations, and PATH. Run find_neurolingsce_cli.py, rebuild NeurolingsCE, or pass --cli PATH.",
                "record": str(record_path),
                "searched": searched,
                "rejected": rejected,
            },
            127,
        )

    code, stdout, stderr = run_cli(cli, ["--json", "--mascot", "list"])
    payload, parse_error = parse_cli_json(stdout, "--mascot list")
    if code != 0:
        return write_json(
            {
                "ok": False,
                "code": "template_list_failed",
                "message": "Could not list mascot templates.",
                "exit_code": code,
                "cli": str(cli),
                "response": payload,
                "parse_error": parse_error,
                "stderr": stderr,
            },
            code,
        )

    if parse_error is not None or payload is None:
        return write_json(parse_error or {}, 1)

    choices = [template for template in loaded_templates(payload) if is_non_default_template(template)]
    if not choices:
        return write_json(
            {
                "ok": False,
                "code": "no_non_default_mascots",
                "message": "No non-default mascot template is installed. Nothing was summoned.",
                "cli": str(cli),
            },
            2,
        )

    chosen = random.choice(choices)
    name = template_name(chosen)
    summon_args = ["--json", "--summon", "mascot", "--name", name]
    if args.label:
        summon_args.append(args.label)

    code, stdout, stderr = run_cli(cli, summon_args)
    summon_payload, parse_error = parse_cli_json(stdout, "--summon mascot")
    if code != 0:
        return write_json(
            {
                "ok": False,
                "code": "summon_failed",
                "message": f"Could not summon mascot {name}.",
                "exit_code": code,
                "cli": str(cli),
                "chosen": chosen,
                "response": summon_payload,
                "parse_error": parse_error,
                "stderr": stderr,
            },
            code,
        )
    if parse_error is not None or summon_payload is None:
        return write_json(parse_error or {}, 1)

    return write_json(
        {
            "ok": True,
            "cli": str(cli),
            "chosen": chosen,
            "summon": summon_payload,
        },
        0,
    )


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
