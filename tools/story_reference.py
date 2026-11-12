#!/usr/bin/env python3
"""Story checker for the unscripted chain of phase 3 (plan section 5, criterion 4;
slice 12 of section 6). Independent of the Rust code and of tools/social_reference.py:
it does not import either. It only reads a run's domain log and follows keys and causes
(ADR-0004 section 7: every entry names its cause; the simulation never reads the log back).

What it implements:

- A two-hop passing: `core:told` of a statement, `core:believed` by the hearer with basis
  `gehört` caused by that told (ADR-0031 sections 2 and 3), then a later `core:told` of the
  same statement by that hearer to a third person.
- A full chain on that passing (criterion 4: said, believed, passed on, avoided, refuted,
  trust lost). The avoidance is a `core:company` whose `avoided` is the doer, after the
  hearer's `core:bond_changed` caused by the `core:believed`, with trust under 400
  (ADR-0030 section 4, ADR-0031 section 5). The refutation is `core:refuted` of that hearer
  for the same statement key, and the trust loss is the hearer's `core:bond_changed` toward
  the source named there, caused by the `core:refuted`.
- Per run: the full chains with their entry ids, the number of two-hop passings, and, when
  no full chain exists, which of those links are missing on the furthest partial story.
- `--canary`: how the three canaries of criterion 4 (passing on, avoidance, refutation
  switched off one at a time) show up as red in this report.
- `--check`: hand-built logs. A full chain, a chain missing each link, a two-hop passing,
  a lie as the origin, and a few logs that must not complete a link.

Input, the on-disk domain log (ADR-0004 sections 7 and 8). Read from
`crates/sim-core/src/log.rs` (the entry, the causes, the payload variants) and from the
writer `crates/sim-cli/src/session.rs` (`drain` appends one JSON object and a newline) via
`crates/sim-cli/src/files.rs` (`events.ndjson`, one object per line). The field shapes those
types embed were confirmed in `crates/sim-core/src/social/belief.rs` (`Statement`, `Basis`),
`crates/sim-core/src/ids.rs`, `crates/sim-core/src/time/mod.rs` and `crates/sim-core/src/hex_u64.rs`:

- A run directory holds `events.ndjson`. A path to that file is the same log. UTF-8, LF,
  one `LogEntry` per line. A last line cut off is ignored.
- `{"id": <number>, "t": <second>, "subjects": [{"sim": <id>}, ...], "causes": [...],
  "payload": {"core:<kind>": {...}}}`. The payload is externally tagged. Its name is the kind.
- Causes are externally tagged and snake_case: `{"event": <id>}`, `{"plan": {"by", "planned_at"}}`,
  `{"input": <seq>}`, `{"rule": "core:..."}`, `{"random": {"tag", "subject", "occasion", "detail"}}`
  with each draw field 16 lowercase hex digits. An event cause is an earlier entry of the same
  handler call.
- A statement is `{"tat": {"doer", "kind", "other", "day"}}` or
  `{"vater_von": {"child", "man"}}`. Basis `gehört` is the heard one. Trust is value index 1
  of `before` / `after` on `core:bond_changed` (affection, trust, respect, attraction,
  familiarity, grudge).

Readings, where the ADR leaves the join to the log:

- The person of the chain is the hearer of the first told: they believe it, they tell it on,
  their bond to the doer is the one caused by that belief, their later choice avoids the doer,
  they are refuted, and they lose trust in their source. Plan section 2 names a third person
  as the one who avoids; the cause the criterion asks for is the `core:believed` of the hearer
  who was told and who passes the sentence on. One lie about that hearer (`lästert` or `lügt`
  toward them) drops a housemate's trust by 80, from 450 under 400. The third person's own
  hearing of the same sentence drops trust by 20 and does not, by itself, cross 400.
- "Caused by the believed" allows one `core:unbelieved` in between. ADR-0031 section 2 writes
  that when another key falls out of a full list, and section 3 makes each following entry of
  the step name the previous one, while section 5 still calls the bond's cause the believed.
- Avoidance counts only when that bond leaves trust strictly under 400 and a later
  `core:company` of the same person names the doer with `avoid` above 0. A drop that stays at
  400 or above does not explain the row.
- The refuted statement matches by key (for `VaterVon` the man afterwards can differ). The
  passing matches the whole statement. The trust-loss bond names the refuted entry directly
  and its `other` is `source` on `core:refuted`. The step of 120 is not re-checked; the
  reference of the formulas owns that number.
- Later in the file is later in the story. The log is append-only. Entry ids are the ones the
  lines carry.

Usage: ~/.cache/simagent-tools-venv/bin/python tools/story_reference.py --check
       ~/.cache/simagent-tools-venv/bin/python tools/story_reference.py --canary
       ~/.cache/simagent-tools-venv/bin/python tools/story_reference.py RUN [RUN ...]
         RUN is `events.ndjson` or a run directory that contains it.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from pathlib import Path

LINKS = ("told", "believed", "told_on", "avoided", "refuted", "trust_loss")
TRUST = 1  # ADR-0030 section 2
AVOID_BELOW = 400  # ADR-0030 section 4, ADR-0031 section 5
HEARD = "gehört"
EVENTS_NAME = "events.ndjson"

CANARY = """\
Criterion 4 is red on a batch of runs when no full chain appears, or when fewer than 16 of
20 seeds have a two-hop passing. This checker does not switch anything off. A canary run is
red because the report loses the link that canary removes:

1. Passing on switched off. The hearer may still believe the sentence. Nobody tells it on,
   so two_hop_passings is 0 on every seed and the bar of 16 fails. The furthest partial is
   missing told_on. full_chains is 0.

2. Avoidance switched off. The passing remains, and the bond_changed caused by the believed
   can still leave trust under 400. No later core:company names that person as avoided.
   two_hop_passings can stay high. The furthest partial is missing avoided. full_chains is 0,
   so the chain bar fails.

3. Refutation switched off. Passing and avoidance still happen. There is no core:refuted and
   no bond_changed caused by one. The furthest partial is missing refuted and trust_loss.
   full_chains is 0, so the chain bar fails.

One seed is short of a chain when full_chains is 0. The passing canary is the one that also
fails the count of two-hop passings.
"""

USAGE = """\
usage: story_reference.py --check
       story_reference.py --canary
       story_reference.py RUN [RUN ...]
RUN is events.ndjson or a run directory that contains it.
"""


@dataclass
class Story:
    told: int | None = None
    believed: int | None = None
    told_on: int | None = None
    lowered: int | None = None
    avoided: int | None = None
    refuted: int | None = None
    trust_loss: int | None = None
    lie: bool = False
    speaker: int | None = None
    hearer: int | None = None
    third: int | None = None
    avoided_person: int | None = None
    source: int | None = None
    origin: int | None = None
    note: str = ""

    def missing(self) -> tuple[str, ...]:
        present = {
            "told": self.told,
            "believed": self.believed,
            "told_on": self.told_on,
            "avoided": self.avoided,
            "refuted": self.refuted,
            "trust_loss": self.trust_loss,
        }
        return tuple(name for name in LINKS if present[name] is None)

    def complete(self) -> bool:
        return self.missing() == ()

    def hop(self) -> bool:
        return None not in (self.told, self.believed, self.told_on)

    def sort_key(self) -> tuple:
        return (
            self.told or 0,
            self.believed or 0,
            self.told_on or 0,
            self.lowered or 0,
            self.avoided or 0,
            self.refuted or 0,
            self.trust_loss or 0,
        )


@dataclass
class Group:
    missing: tuple[str, ...]
    count: int
    example: Story


@dataclass
class Report:
    hops: int
    chains: list
    groups: list
    warnings: list


def kind_of(entry: dict) -> str | None:
    payload = entry.get("payload")
    if not isinstance(payload, dict) or len(payload) != 1:
        return None
    return next(iter(payload))


def body_of(entry: dict) -> dict | None:
    payload = entry.get("payload")
    if not isinstance(payload, dict) or len(payload) != 1:
        return None
    body = next(iter(payload.values()))
    if not isinstance(body, dict):
        return None
    return body


def subject_of(entry: dict) -> int | None:
    subjects = entry.get("subjects")
    if not isinstance(subjects, list) or not subjects:
        return None
    first = subjects[0]
    if isinstance(first, dict) and type(first.get("sim")) is int:
        return first["sim"]
    return None


def event_ids(entry: dict) -> list[int]:
    causes = entry.get("causes")
    if not isinstance(causes, list):
        return []
    found = []
    for cause in causes:
        if isinstance(cause, dict) and type(cause.get("event")) is int:
            found.append(cause["event"])
    return found


def statement_of(entry: dict) -> dict | None:
    body = body_of(entry)
    if body is None:
        return None
    statement = body.get("statement")
    if isinstance(statement, dict):
        return statement
    return None


def sentence(statement: dict) -> tuple | None:
    """The whole statement. A passing carries this, not only the key."""
    if "tat" in statement and isinstance(statement["tat"], dict):
        tat = statement["tat"]
        parts = (tat.get("doer"), tat.get("kind"), tat.get("other"), tat.get("day"))
        if any(type(part) is not int for part in parts):
            return None
        return ("tat",) + parts
    if "vater_von" in statement and isinstance(statement["vater_von"], dict):
        father = statement["vater_von"]
        child, man = father.get("child"), father.get("man")
        if type(child) is not int or type(man) is not int:
            return None
        return ("vater_von", child, man)
    return None


def statement_key(statement: dict) -> tuple | None:
    """ADR-0031 section 1. `VaterVon` keys the child; the man is the content."""
    whole = sentence(statement)
    if whole is None:
        return None
    if whole[0] == "vater_von":
        return ("vater_von", whole[1])
    return whole


def doer_of(statement: dict) -> int | None:
    whole = sentence(statement)
    if whole is None or whole[0] != "tat":
        return None
    return whole[1]


def trust_pair(entry: dict) -> tuple[int, int] | None:
    body = body_of(entry)
    if body is None:
        return None
    before, after = body.get("before"), body.get("after")
    if not isinstance(before, list) or not isinstance(after, list):
        return None
    if len(before) <= TRUST or len(after) <= TRUST:
        return None
    if type(before[TRUST]) is not int or type(after[TRUST]) is not int:
        return None
    return before[TRUST], after[TRUST]


def bond_other(entry: dict) -> int | None:
    body = body_of(entry)
    if body is None or type(body.get("other")) is not int:
        return None
    return body["other"]


def avoids(entry: dict, person: int) -> bool:
    body = body_of(entry)
    if body is None or not isinstance(body.get("objects"), list):
        return False
    for row in body["objects"]:
        if not isinstance(row, dict):
            continue
        if row.get("avoided") == person and type(row.get("avoid")) is int and row["avoid"] > 0:
            return True
    return False


def earlier(index: dict, left_id: int, right_id: int) -> bool:
    return index[left_id] < index[right_id]


def caused_by_believed(bond: dict, believed: dict, by_id: dict, index: dict) -> bool:
    """The bond names the believed, or it names an unbelieved that names the believed."""
    hearer = subject_of(believed)
    for cause_id in event_ids(bond):
        if cause_id not in by_id or not earlier(index, cause_id, bond["id"]):
            continue
        if cause_id == believed["id"]:
            return True
        mid = by_id[cause_id]
        if kind_of(mid) != "core:unbelieved" or subject_of(mid) != hearer:
            continue
        if believed["id"] in event_ids(mid) and earlier(index, believed["id"], cause_id):
            return True
    return False


def parse_events(text: str, name: str) -> tuple[list, list]:
    """One log entry per line. A cut-off last line is a warning, not an entry."""
    warnings = []
    entries = []
    lines = text.splitlines()
    for number, line in enumerate(lines, 1):
        if line.strip() == "":
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError as err:
            if number == len(lines):
                warnings.append(f"{name}:{number}: cut-off last line ignored")
                break
            raise SystemExit(f"{name}:{number}: {err}") from err
        if not isinstance(value, dict) or type(value.get("id")) is not int or "payload" not in value:
            raise SystemExit(f"{name}:{number}: not a log entry")
        entries.append(value)
    seen = set()
    for entry in entries:
        if entry["id"] in seen:
            raise SystemExit(f"{name}: duplicate entry id {entry['id']}")
        seen.add(entry["id"])
    return entries, warnings


def resolve_log(path: Path) -> Path:
    if path.is_dir():
        log = path / EVENTS_NAME
        if not log.is_file():
            raise SystemExit(f"{path}: no {EVENTS_NAME}")
        return log
    if not path.is_file():
        raise SystemExit(f"{path}: not found")
    return path


def _heard_beliefs(told: dict, beliefs: list, index: dict) -> list:
    speaker = subject_of(told)
    body = body_of(told)
    if speaker is None or body is None or type(body.get("to")) is not int:
        return []
    hearer = body["to"]
    if hearer == speaker:
        return []
    wanted = sentence(body["statement"]) if isinstance(body.get("statement"), dict) else None
    if wanted is None:
        return []
    found = []
    for believed in beliefs:
        if subject_of(believed) != hearer:
            continue
        if not earlier(index, told["id"], believed["id"]):
            continue
        if told["id"] not in event_ids(believed):
            continue
        belief = body_of(believed)
        if belief is None or belief.get("basis") != HEARD:
            continue
        if belief.get("source") != speaker:
            continue
        statement = belief.get("statement")
        if not isinstance(statement, dict) or sentence(statement) != wanted:
            continue
        found.append(believed)
    return found


def _passings(told: dict, believed: dict, tolds: list, index: dict) -> list:
    speaker = subject_of(told)
    hearer = subject_of(believed)
    body = body_of(told)
    wanted = sentence(body["statement"]) if body and isinstance(body.get("statement"), dict) else None
    if speaker is None or hearer is None or wanted is None:
        return []
    found = []
    for onward in tolds:
        if not earlier(index, believed["id"], onward["id"]):
            continue
        if subject_of(onward) != hearer:
            continue
        onward_body = body_of(onward)
        if onward_body is None or type(onward_body.get("to")) is not int:
            continue
        third = onward_body["to"]
        if third in (speaker, hearer):
            continue
        statement = onward_body.get("statement")
        if not isinstance(statement, dict) or sentence(statement) != wanted:
            continue
        found.append(onward)
    return found


def _lowering(believed: dict, statement: dict, bonds: list, by_id: dict, index: dict):
    doer = doer_of(statement)
    hearer = subject_of(believed)
    if doer is None or hearer is None:
        return None, ""
    under = []
    short = []
    for bond in bonds:
        if subject_of(bond) != hearer or bond_other(bond) != doer:
            continue
        if not earlier(index, believed["id"], bond["id"]):
            continue
        if not caused_by_believed(bond, believed, by_id, index):
            continue
        pair = trust_pair(bond)
        if pair is None or pair[1] >= pair[0]:
            continue
        if pair[1] < AVOID_BELOW:
            under.append(bond)
        else:
            short.append((bond, pair[1]))
    if under:
        under.sort(key=lambda entry: index[entry["id"]])
        return under[0], ""
    if short:
        short.sort(key=lambda item: index[item[0]["id"]])
        bond, after = short[0]
        return None, f"bond {bond['id']} leaves trust at {after}"
    return None, ""


def _company_after(bond: dict, companies: list, index: dict) -> dict | None:
    person = bond_other(bond)
    who = subject_of(bond)
    if person is None or who is None:
        return None
    found = None
    for company in companies:
        if not earlier(index, bond["id"], company["id"]):
            continue
        if subject_of(company) != who or not avoids(company, person):
            continue
        if found is None or earlier(index, company["id"], found["id"]):
            found = company
    return found


def _refutations(believed: dict, refuteds: list, index: dict) -> list:
    hearer = subject_of(believed)
    belief = body_of(believed)
    if hearer is None or belief is None or not isinstance(belief.get("statement"), dict):
        return []
    key = statement_key(belief["statement"])
    source = belief.get("source")
    found = []
    for refuted in refuteds:
        if subject_of(refuted) != hearer or not earlier(index, believed["id"], refuted["id"]):
            continue
        body = body_of(refuted)
        if body is None or body.get("source") != source:
            continue
        statement = body.get("statement")
        if not isinstance(statement, dict) or statement_key(statement) != key:
            continue
        found.append(refuted)
    found.sort(key=lambda entry: index[entry["id"]])
    return found


def _trust_loss(refuted: dict, bonds: list, index: dict) -> dict | None:
    body = body_of(refuted)
    if body is None or type(body.get("source")) is not int:
        return None
    source = body["source"]
    who = subject_of(refuted)
    found = None
    for bond in bonds:
        if subject_of(bond) != who or bond_other(bond) != source:
            continue
        if refuted["id"] not in event_ids(bond):
            continue
        if not earlier(index, refuted["id"], bond["id"]):
            continue
        pair = trust_pair(bond)
        if pair is None or pair[1] >= pair[0]:
            continue
        if found is None or earlier(index, bond["id"], found["id"]):
            found = bond
    return found


def _story_from(told, believed, told_on, bonds, companies, refuteds, by_id, index) -> Story:
    told_body = body_of(told)
    belief_body = body_of(believed) if believed is not None else None
    statement = told_body["statement"] if told_body else None
    story = Story(
        told=told["id"],
        speaker=subject_of(told),
        lie=bool(told_body and told_body.get("lie")),
    )
    if believed is None or belief_body is None or not isinstance(statement, dict):
        return story
    story.believed = believed["id"]
    story.hearer = subject_of(believed)
    story.source = belief_body.get("source") if type(belief_body.get("source")) is int else None
    story.origin = belief_body.get("origin") if type(belief_body.get("origin")) is int else None
    if told_on is not None:
        onward = body_of(told_on)
        story.told_on = told_on["id"]
        if onward is not None and type(onward.get("to")) is int:
            story.third = onward["to"]
    lowered, note = _lowering(believed, statement, bonds, by_id, index)
    story.note = note
    if lowered is not None:
        story.lowered = lowered["id"]
        story.avoided_person = bond_other(lowered)
        company = _company_after(lowered, companies, index)
        if company is not None:
            story.avoided = company["id"]
    chosen_refuted = None
    chosen_loss = None
    for refuted in _refutations(believed, refuteds, index):
        loss = _trust_loss(refuted, bonds, index)
        if loss is not None and chosen_loss is None:
            chosen_refuted = refuted
            chosen_loss = loss
            break
    if chosen_refuted is None:
        refs = _refutations(believed, refuteds, index)
        if refs:
            chosen_refuted = refs[0]
    if chosen_refuted is not None:
        story.refuted = chosen_refuted["id"]
    if chosen_loss is not None:
        story.trust_loss = chosen_loss["id"]
    return story


def analyze(entries: list, warnings: list | None = None) -> Report:
    warnings = list(warnings or [])
    index = {entry["id"]: place for place, entry in enumerate(entries)}
    by_id = {entry["id"]: entry for entry in entries}
    tolds, beliefs, bonds, companies, refuteds = [], [], [], [], []
    for entry in entries:
        kind = kind_of(entry)
        if kind == "core:told":
            tolds.append(entry)
        elif kind == "core:believed":
            beliefs.append(entry)
        elif kind == "core:bond_changed":
            bonds.append(entry)
        elif kind == "core:company":
            companies.append(entry)
        elif kind == "core:refuted":
            refuteds.append(entry)
    stories = []
    if not tolds:
        stories.append(Story())
    for told in tolds:
        heard = _heard_beliefs(told, beliefs, index)
        if not heard:
            story = Story(told=told["id"], speaker=subject_of(told))
            body = body_of(told)
            story.lie = bool(body and body.get("lie"))
            stories.append(story)
            continue
        for believed in heard:
            onward = _passings(told, believed, tolds, index)
            if not onward:
                stories.append(
                    _story_from(told, believed, None, bonds, companies, refuteds, by_id, index)
                )
                continue
            for told_on in onward:
                stories.append(
                    _story_from(
                        told, believed, told_on, bonds, companies, refuteds, by_id, index
                    )
                )
    chains = [story for story in stories if story.complete()]
    chains.sort(key=lambda story: story.sort_key())
    hops = sum(1 for story in stories if story.hop())
    groups = []
    if not chains:
        partials = [story for story in stories if not story.complete()]
        if not partials:
            partials = [Story()]
        best = min(len(story.missing()) for story in partials)
        buckets: dict[tuple[str, ...], list] = {}
        for story in partials:
            if len(story.missing()) != best:
                continue
            buckets.setdefault(story.missing(), []).append(story)
        for missing in sorted(buckets):
            examples = sorted(buckets[missing], key=lambda story: story.sort_key())
            groups.append(Group(missing=missing, count=len(examples), example=examples[0]))
    return Report(hops=hops, chains=chains, groups=groups, warnings=warnings)


def _ident(value: int | None) -> str:
    if value is None:
        return "-"
    return str(value)


def format_story(story: Story, full: bool) -> str:
    fields = (
        f"told={_ident(story.told)} believed={_ident(story.believed)} "
        f"told_on={_ident(story.told_on)} lowered={_ident(story.lowered)} "
        f"avoided={_ident(story.avoided)} refuted={_ident(story.refuted)} "
        f"trust_loss={_ident(story.trust_loss)}"
    )
    if not full:
        if story.note:
            return fields + f" note={story.note}"
        return fields
    return (
        fields
        + f" lie={str(story.lie).lower()} speaker={_ident(story.speaker)}"
        + f" hearer={_ident(story.hearer)} third={_ident(story.third)}"
        + f" avoided_person={_ident(story.avoided_person)} source={_ident(story.source)}"
        + f" origin={_ident(story.origin)}"
    )


def format_report(name: str, report: Report) -> str:
    lines = [
        f"run: {name}",
        f"two_hop_passings: {report.hops}",
        f"full_chains: {len(report.chains)}",
    ]
    for chain in report.chains:
        lines.append("chain: " + format_story(chain, True))
    if not report.chains:
        if not report.groups:
            lines.append("missing: " + " ".join(LINKS))
        for group in report.groups:
            lines.append("missing: " + " ".join(group.missing))
            lines.append(f"partials: {group.count}")
            lines.append("partial: " + format_story(group.example, False))
    for warning in report.warnings:
        lines.append(f"warning: {warning}")
    return "\n".join(lines)


def check_path(path: Path) -> str:
    log = resolve_log(path)
    text = log.read_text(encoding="utf-8")
    entries, warnings = parse_events(text, str(log))
    return format_report(str(log), analyze(entries, warnings))


# --- hand-built logs for --check -------------------------------------------------------

# Mia (4) gossiped about Lena (2). Tom (1) tells Lena, Lena tells Jonas (3).
SENTENCE = {"tat": {"doer": 4, "kind": 1, "other": 2, "day": 3}}
OTHER = {"tat": {"doer": 4, "kind": 0, "other": 2, "day": 3}}


def _line(entry_id: int, second: int, sim: int, causes: list, payload: dict) -> dict:
    return {
        "id": entry_id,
        "t": second,
        "subjects": [{"sim": sim}],
        "causes": causes,
        "payload": payload,
    }


def _told(entry_id, second, speaker, hearer, statement, lie, cause) -> dict:
    return _line(
        entry_id,
        second,
        speaker,
        cause,
        {"core:told": {"to": hearer, "statement": statement, "certainty": 900, "lie": lie}},
    )


def _believed(entry_id, second, hearer, statement, source, origin, cause, basis=HEARD) -> dict:
    return _line(
        entry_id,
        second,
        hearer,
        cause,
        {
            "core:believed": {
                "statement": statement,
                "certainty": 733,
                "basis": basis,
                "source": source,
                "origin": origin,
                "replaced": False,
            }
        },
    )


def _bond(entry_id, second, sim, other, before_trust, after_trust, cause) -> dict:
    before = [400, before_trust, 400, 0, 200, 0]
    after = [400, after_trust, 400, 0, 200, 0]
    return _line(
        entry_id,
        second,
        sim,
        cause,
        {"core:bond_changed": {"other": other, "before": before, "after": after}},
    )


def _company(entry_id, second, sim, avoided, cause) -> dict:
    row = {"object": 1, "liked": None, "company": 0, "avoided": avoided, "avoid": 8000}
    return _line(entry_id, second, sim, cause, {"core:company": {"objects": [row]}})


def _refuted(entry_id, second, hearer, statement, source) -> dict:
    draw = {
        "tag": "0000000000000025",
        "subject": "0000000000000001",
        "occasion": "0000000000002328",
        "detail": "0000000000000002",
    }
    return _line(
        entry_id,
        second,
        hearer,
        [{"random": draw}],
        {"core:refuted": {"by": 5, "statement": statement, "source": source}},
    )


def full_log(lie: bool = False) -> list:
    """One chain. A decoy company, a decoy bond, a decoy told and a decoy refutation sit beside it."""
    return [
        _company(9, 500, 2, 4, [{"rule": "core:test"}]),
        _told(10, 1000, 1, 2, SENTENCE, lie, [{"plan": {"by": 1, "planned_at": 900}}]),
        _believed(11, 1000, 2, SENTENCE, 1, 1, [{"event": 10}]),
        _bond(14, 1000, 2, 8, 100, 50, [{"event": 11}]),
        _line(12, 1000, 2, [{"event": 11}], {"core:unbelieved": {"statement": OTHER}}),
        _bond(13, 1000, 2, 4, 450, 370, [{"event": 12}]),
        _told(18, 4000, 2, 3, OTHER, False, [{"event": 17}]),
        _told(20, 5000, 2, 3, SENTENCE, False, [{"event": 19}]),
        _company(30, 8000, 2, 4, [{"plan": {"by": 7, "planned_at": 7000}}]),
        _refuted(38, 8500, 2, OTHER, 1),
        _bond(39, 8500, 2, 1, 500, 380, [{"event": 38}]),
        _refuted(40, 9000, 2, SENTENCE, 1),
        _bond(41, 9000, 2, 1, 500, 380, [{"event": 40}]),
    ]


def _loads(entries: list) -> tuple[list, list]:
    text = "".join(json.dumps(entry) + "\n" for entry in entries)
    return parse_events(text, "hand")


def _missing(report: Report) -> tuple[str, ...]:
    if report.chains:
        return ()
    if len(report.groups) != 1:
        raise SystemExit(f"FAIL expected one missing-group, got {report.groups}")
    return report.groups[0].missing


def _expect(name: str, entries: list, hops: int, chains: int, missing: tuple[str, ...]) -> Report:
    parsed, warnings = _loads(entries)
    report = analyze(parsed, warnings)
    label = " ".join(missing) if missing else "-"
    print(
        f"{name}: two_hop_passings={report.hops} full_chains={len(report.chains)} missing={label}"
    )
    problems = []
    if report.warnings:
        problems.append(f"warnings {report.warnings}")
    if report.hops != hops:
        problems.append(f"hops {report.hops} != {hops}")
    if len(report.chains) != chains:
        problems.append(f"chains {len(report.chains)} != {chains}")
    if _missing(report) != missing:
        problems.append(f"missing {_missing(report)} != {missing}")
    if problems:
        raise SystemExit("FAIL " + name + ": " + "; ".join(problems))
    return report


def _expect_ids(report: Report, ids: tuple, lie: bool) -> None:
    chain = report.chains[0]
    got = (
        chain.told,
        chain.believed,
        chain.told_on,
        chain.lowered,
        chain.avoided,
        chain.refuted,
        chain.trust_loss,
    )
    if got != ids or chain.lie is not lie:
        raise SystemExit(f"FAIL ids {got} lie={chain.lie}")
    if chain.avoided_person != 4 or chain.source != 1 or chain.third != 3:
        raise SystemExit(
            f"FAIL people avoided={chain.avoided_person} source={chain.source} third={chain.third}"
        )
    if chain.speaker != 1 or chain.hearer != 2 or chain.origin != 1:
        raise SystemExit(
            f"FAIL roles speaker={chain.speaker} hearer={chain.hearer} origin={chain.origin}"
        )


def check_all() -> None:
    log = full_log()
    print(format_report("full", analyze(*_loads(log))))
    report = _expect("full", log, 1, 1, ())
    _expect_ids(
        report,
        (10, 11, 20, 13, 30, 40, 41),
        False,
    )
    direct = [entry for entry in log if entry["id"] != 12]
    for entry in direct:
        if entry["id"] == 13:
            entry["causes"] = [{"event": 11}]
    report = _expect("direct_cause", direct, 1, 1, ())
    _expect_ids(report, (10, 11, 20, 13, 30, 40, 41), False)

    _expect(
        "missing_told",
        [entry for entry in full_log() if kind_of(entry) != "core:told"],
        0,
        0,
        LINKS,
    )
    _expect(
        "missing_believed",
        [entry for entry in full_log() if entry["id"] != 11],
        0,
        0,
        ("believed", "told_on", "avoided", "refuted", "trust_loss"),
    )
    _expect(
        "missing_told_on",
        [entry for entry in full_log() if entry["id"] != 20],
        0,
        0,
        ("told_on",),
    )
    no_avoid = full_log()
    for entry in no_avoid:
        if entry["id"] == 30:
            entry["payload"]["core:company"]["objects"][0]["avoided"] = None
    _expect("missing_avoided", no_avoid, 1, 0, ("avoided",))
    _expect(
        "missing_refuted",
        [entry for entry in full_log() if entry["id"] not in (40, 41)],
        1,
        0,
        ("refuted", "trust_loss"),
    )
    _expect(
        "missing_trust_loss",
        [entry for entry in full_log() if entry["id"] != 41],
        1,
        0,
        ("trust_loss",),
    )
    _expect(
        "two_hop",
        [entry for entry in full_log() if entry["id"] in (10, 11, 20)],
        1,
        0,
        ("avoided", "refuted", "trust_loss"),
    )

    lied = full_log(lie=True)
    report = _expect("lie_origin", lied, 1, 1, ())
    _expect_ids(report, (10, 11, 20, 13, 30, 40, 41), True)
    if any(subject_of(entry) == 1 and kind_of(entry) == "core:believed" for entry in lied):
        raise SystemExit("FAIL the liar was given a belief")

    above = full_log()
    for entry in above:
        if entry["id"] == 13:
            entry["payload"]["core:bond_changed"]["after"][TRUST] = 430
    report = _expect("trust_stays_above", above, 1, 0, ("avoided",))
    if "430" not in report.groups[0].example.note:
        raise SystemExit(f"FAIL note {report.groups[0].example.note}")

    early = full_log()
    company = next(entry for entry in early if entry["id"] == 30)
    bond = next(entry for entry in early if entry["id"] == 13)
    early.remove(company)
    early.insert(early.index(bond), company)
    _expect("company_before_bond", early, 1, 0, ("avoided",))

    seen = full_log()
    for entry in seen:
        if entry["id"] == 11:
            entry["payload"]["core:believed"]["basis"] = "gesehen"
    _expect(
        "seen_basis",
        seen,
        0,
        0,
        ("believed", "told_on", "avoided", "refuted", "trust_loss"),
    )

    wrong_loss = full_log()
    for entry in wrong_loss:
        if entry["id"] == 41:
            entry["payload"]["core:bond_changed"]["other"] = 4
    _expect("trust_loss_not_in_source", wrong_loss, 1, 0, ("trust_loss",))

    extra = full_log()
    extra.append(_told(21, 6000, 2, 6, SENTENCE, False, [{"event": 19}]))
    report = _expect("two_passings", extra, 2, 2, ())
    onwards = tuple(sorted(chain.told_on for chain in report.chains))
    if onwards != (20, 21):
        raise SystemExit(f"FAIL passings {onwards}")

    for word in ("Passing on", "Avoidance", "Refutation", "told_on", "avoided", "trust_loss"):
        if word not in CANARY:
            raise SystemExit(f"FAIL canary text lacks {word}")
    sample = json.dumps(full_log()[0])
    try:
        parse_events(sample + "\nNOT JSON\n" + sample + "\n", "bad")
    except SystemExit as err:
        if "bad:2" not in str(err):
            raise
    else:
        raise SystemExit("FAIL a broken middle line was accepted")
    parsed, warnings = parse_events(json.dumps(full_log()[0]) + '\n{"id":', "cut")
    if len(parsed) != 1 or not any("cut-off" in warning for warning in warnings):
        raise SystemExit(f"FAIL tail {parsed} {warnings}")
    print("ok")


def main(argv: list[str]) -> int:
    args = argv[1:]
    if "--check" in args:
        check_all()
        return 0
    if "--canary" in args:
        print(CANARY, end="")
        return 0
    paths = [arg for arg in args if not arg.startswith("-")]
    if not paths:
        print(USAGE, end="")
        return 2
    for path in paths:
        print(check_path(Path(path)))
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
