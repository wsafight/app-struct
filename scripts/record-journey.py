#!/usr/bin/env python3
"""Opt-in local journey timing; never stores command arguments, output or environment."""
import argparse
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", required=True, choices=["install", "import", "permissions", "first-use", "second-change", "build", "deploy", "restore", "update"])
    parser.add_argument("--record", type=Path, default=Path(".appstruct/journeys.jsonl"))
    parser.add_argument("--help-needed", action="store_true")
    parser.add_argument("--reason", choices=["none", "environment", "schema", "permissions", "build", "deploy", "update", "unknown"], default="none")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("a command after -- is required")
    args.record.parent.mkdir(parents=True, exist_ok=True)
    # Open before running so a recording error cannot silently discard a completed journey.
    flags = os.O_WRONLY | os.O_CREAT | os.O_APPEND | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(args.record, flags, 0o600)
    started_at = datetime.datetime.now(datetime.timezone.utc).isoformat()
    start = time.monotonic()
    code = 130
    try:
        try:
            code = subprocess.call(command)
            if code < 0:
                code = 128 - code
        except OSError as error:
            code = 127
            print(f"Journey command could not start ({type(error).__name__})", file=sys.stderr)
        except KeyboardInterrupt:
            code = 130
    finally:
        event = {
            "schema_version": 1, "stage": args.stage, "started_at": started_at,
            "elapsed_seconds": round(time.monotonic() - start, 3), "exit_code": code,
            "help_needed": args.help_needed,
            "reason": args.reason if args.reason != "none" or code == 0 else "unknown",
        }
        with os.fdopen(descriptor, "a", encoding="utf-8") as output:
            output.write(json.dumps(event, separators=(",", ":")) + "\n")
        print(f"Journey recorded: {args.stage}, exit {code}", file=sys.stderr)
    return code


if __name__ == "__main__":
    sys.exit(main())
