#!/usr/bin/env python3
"""Mutation probes by exact text replacement (docs/testregeln.md, rules 2 and 13).

A probe changes one place of one committed file, runs a command and expects it to fail.
The helper guards against the ways a probe by hand goes wrong:

- The working tree must be clean before and after every probe.
- The old text must occur exactly once in the file. A probe whose text is not there (for
  instance after rustfmt moved a line) is reported as NO-OP/AMBIGUOUS, never as red.
- Only the touched file is restored, with `git checkout -- <file>`.
- With `expect`, the command must fail and its output must contain that text, so a probe
  does not pass because of an unrelated failure (a stale lock file, a compile error).

This is not `cargo mutants`: it only tries the mutants someone wrote down.

Usage: python3 tools/probe.py probes.json [default command ...]

probes.json is a list of objects:
  name    what the mutant does
  file    path relative to the repository root
  old     text to replace, exactly once in the file
  new     replacement
  cmd     optional, the command for this probe as a list (else the default command)
  expect  optional, text that must be in the output of the failing command

Verdicts: RED (the command failed as expected), GREEN! (the mutant survived),
COMPILE-ERROR and RED-OTHER (failed, but not at the guard under test), RED? (failed
without a named test), NO-OP/AMBIGUOUS. The exit status is 1 unless every probe is RED.
The repository is the parent of this directory, or PROBE_REPO.
"""

import json
import os
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(os.environ.get("PROBE_REPO", Path(__file__).resolve().parent.parent))
DEFAULT = ["cargo", "test", "-q", "--workspace", "--no-fail-fast"]


def git(*args: str) -> str:
    done = subprocess.run(["git", *args], cwd=REPO, capture_output=True, text=True, check=True)
    return done.stdout


def clean() -> bool:
    return git("status", "--porcelain").strip() == ""


def run_probe(probe: dict, default: list) -> tuple:
    path = REPO / probe["file"]
    text = path.read_text(encoding="utf-8")
    count = text.count(probe["old"])
    if count != 1:
        return "NO-OP/AMBIGUOUS", [f"the old text occurs {count} times"]
    path.write_text(text.replace(probe["old"], probe["new"], 1), encoding="utf-8")
    try:
        done = subprocess.run(
            probe.get("cmd", default), cwd=REPO, capture_output=True, text=True, check=False
        )
    finally:
        git("checkout", "--", probe["file"])
    out = done.stdout + done.stderr
    failed = done.returncode != 0
    if "expect" in probe:
        hits = [line for line in out.splitlines() if probe["expect"] in line][:2]
        if failed and hits:
            return "RED", hits
        return ("RED-OTHER" if failed else "GREEN!"), out.splitlines()[-3:]
    tests = sorted(set(re.findall(r"^    ([\w:]+)$", out, re.M)))
    if failed and tests:
        return "RED", tests
    if "error[E" in out or "could not compile" in out:
        return "COMPILE-ERROR", out.splitlines()[-3:]
    return ("RED?" if failed else "GREEN!"), []


def main() -> None:
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    with open(sys.argv[1], encoding="utf-8") as file:
        probes = json.load(file)
    default = sys.argv[2:] or DEFAULT
    results = []
    for probe in probes:
        assert clean(), f"the tree is not clean before the probe {probe['name']}"
        results.append((probe["name"], *run_probe(probe, default)))
        assert clean(), f"the tree is not clean after the probe {probe['name']}"
    for name, verdict, detail in results:
        print(f"{verdict:16} {name}: {detail}")
    if any(verdict != "RED" for _, verdict, _ in results):
        sys.exit(1)


if __name__ == "__main__":
    main()
