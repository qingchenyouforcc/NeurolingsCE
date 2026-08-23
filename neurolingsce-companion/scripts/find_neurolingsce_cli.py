#!/usr/bin/env python3
"""Find NeurolingsCE-cli and record the path for other helpers."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


CLI_NAMES = ("NeurolingsCE-cli.exe", "NeurolingsCE-cli")


def skill_root() -> Path:
    return Path(__file__).resolve().parents[1]


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def default_record_path() -> Path:
    return skill_root() / "cache" / "neurolingsce-cli-path.json"


def write_json(payload: dict[str, Any], exit_code: int) -> int:
    print(json.dumps(payload, ensure_ascii=False, separators=(",", ":")))
    return exit_code


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
        for name in CLI_NAMES:
            append_candidate(candidates, app_root / name)
            append_candidate(candidates, app_root / "bin" / name)
    return candidates


def common_candidates(explicit: str | None) -> list[Path]:
    candidates: list[Path] = []
    if explicit:
        append_candidate(candidates, Path(explicit).expanduser())

    candidates.extend(common_install_candidates())

    root = repo_root()
    for relative in (
        ("out", "build", "x64-Release", "bin"),
        ("out", "build", "x64-Debug", "bin"),
        ("build", "bin"),
        ("build-release", "bin"),
        ("build-cli-check", "bin"),
        ("publish", "Windows", "release"),
        ("publish", "Windows", "debug"),
    ):
        base = root.joinpath(*relative)
        for name in CLI_NAMES:
            append_candidate(candidates, base / name)

    for name in CLI_NAMES:
        found = shutil.which(name)
        if found:
            append_candidate(candidates, Path(found))

    return candidates


def search_roots(extra_roots: list[str]) -> list[Path]:
    roots: list[Path] = []
    for env_name in ("ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"):
        value = os.environ.get(env_name)
        if value:
            append_candidate(roots, Path(value))
    append_candidate(roots, repo_root())
    for root in extra_roots:
        append_candidate(roots, Path(root).expanduser())
    return roots


def recursive_candidates(roots: list[Path], max_depth: int) -> list[Path]:
    candidates: list[Path] = []
    for root in roots:
        if not root.exists() or not root.is_dir():
            continue
        root_depth = len(root.parts)
        try:
            for current, dirnames, filenames in os.walk(root):
                current_path = Path(current)
                if len(current_path.parts) - root_depth >= max_depth:
                    dirnames[:] = []
                for name in CLI_NAMES:
                    if name in filenames:
                        append_candidate(candidates, current_path / name)
        except OSError:
            continue
    return candidates


def current_contract_ok(details: dict[str, Any]) -> bool:
    payload = details.get("help_payload")
    if not isinstance(payload, dict):
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


def runnable_version(path: Path) -> tuple[bool, dict[str, Any]]:
    try:
        version_completed = subprocess.run(
            [str(path), "--json", "--version"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
            timeout=10,
        )
        help_completed = subprocess.run(
            [str(path), "--json", "--help"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
            timeout=10,
        )
    except OSError as exc:
        return False, {"error": str(exc)}
    except subprocess.TimeoutExpired:
        return False, {"error": "CLI capability check timed out."}

    details: dict[str, Any] = {
        "version_exit_code": version_completed.returncode,
        "version_stdout": version_completed.stdout.strip(),
        "version_stderr": version_completed.stderr.strip(),
        "help_exit_code": help_completed.returncode,
        "help_stdout": help_completed.stdout.strip(),
        "help_stderr": help_completed.stderr.strip(),
    }
    if version_completed.returncode != 0 or help_completed.returncode != 0:
        return False, details
    try:
        parsed = json.loads(details["version_stdout"])
    except json.JSONDecodeError:
        parsed = None
    if isinstance(parsed, dict):
        details["version_payload"] = parsed
    try:
        parsed = json.loads(details["help_stdout"])
    except json.JSONDecodeError:
        parsed = None
    if isinstance(parsed, dict):
        details["help_payload"] = parsed
    ok = current_contract_ok(details)
    if not ok:
        details["error"] = "CLI is runnable but does not support the current command contract."
    return ok, details


def find_cli(explicit: str | None, extra_roots: list[str], max_depth: int) -> tuple[Path | None, list[dict[str, Any]]]:
    checked: list[dict[str, Any]] = []
    candidates = common_candidates(explicit)
    candidates.extend(recursive_candidates(search_roots(extra_roots), max_depth))

    seen: set[Path] = set()
    for candidate in candidates:
        absolute = candidate.resolve() if candidate.exists() else candidate.absolute()
        if absolute in seen:
            continue
        seen.add(absolute)
        entry: dict[str, Any] = {"path": str(absolute), "exists": absolute.is_file()}
        if not absolute.is_file():
            checked.append(entry)
            continue
        ok, details = runnable_version(absolute)
        entry["runnable"] = ok
        entry["details"] = details
        checked.append(entry)
        if ok:
            return absolute, checked
    return None, checked


def write_record(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description="Find NeurolingsCE-cli.exe and record its path."
    )
    parser.add_argument("--cli", help="Explicit NeurolingsCE-cli path to verify first.")
    parser.add_argument(
        "--record",
        default=str(default_record_path()),
        help="JSON file used to record the discovered CLI path.",
    )
    parser.add_argument(
        "--search-root",
        action="append",
        default=[],
        help="Extra directory to recursively search. May be repeated.",
    )
    parser.add_argument(
        "--max-depth",
        type=int,
        default=6,
        help="Maximum recursive search depth from each search root.",
    )
    args = parser.parse_args(argv)

    found, checked = find_cli(args.cli, args.search_root, max(0, args.max_depth))
    record_path = Path(args.record).expanduser()
    payload: dict[str, Any] = {
        "ok": found is not None,
        "cli": str(found) if found else None,
        "record": str(record_path),
        "checked": checked,
        "searched_at": datetime.now(timezone.utc).isoformat(),
    }

    if found is None:
        payload["code"] = "cli_not_found"
        payload["message"] = (
            "NeurolingsCE-cli was not found. Install or build NeurolingsCE, "
            "or pass --cli PATH / --search-root DIR."
        )
        return write_json(payload, 127)

    write_record(record_path, payload)
    return write_json(payload, 0)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
