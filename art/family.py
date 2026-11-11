#!/usr/bin/env python3
"""Write one family's body snapshots from `simagent view`.

The line is the closest one in the 50-year stadt run: grandparents who are
both 69, their son near 47, his partner, a daughter who is 17, and a son who
is 13. The adults are read at the run's latest second. The same two children
are read again from one deterministic continuation stopped with `--until` on
the second each of them turns 8, 10, 12, 13 and, for the girl, 17.

`web/dist` is not required. `simagent view` only checks that `--web` contains
`index.html`. The stub is `/Users/dariushk/.cache/simagent-art/view-web`.

    target/release/simagent view \
        /Users/dariushk/.cache/simagent-art/runs \
        --web /Users/dariushk/.cache/simagent-art/view-web \
        --port 8765
    python3 art/family.py --base http://127.0.0.1:8765 --run stadt --drive \
        --out /Users/dariushk/.cache/simagent-art/family.json
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import urllib.error
import urllib.request
from collections import defaultdict
from pathlib import Path

YEAR = 364 * 86400
UNTIL_50Y = 50 * YEAR
CHILD_AGES = (8, 10, 12, 13, 17)
CACHE = "/Users/dariushk/.cache/simagent-art"

BODY_KEYS = [
    "body_form_age",
    "face_form_age",
    "height_cm",
    "leg_ratio_z",
    "head_size_z",
    "body_fat_z",
    "fat_distribution_z",
    "muscle_z",
    "shoulder_width_z",
    "chest_depth_z",
    "hip_width_z",
    "face_axis_01",
    "face_axis_02",
    "face_axis_03",
    "face_axis_04",
    "face_axis_05",
    "face_axis_06",
    "face_axis_07",
    "face_axis_08",
    "face_axis_09",
    "face_axis_10",
    "face_axis_11",
    "face_axis_12",
    "skin_eumelanin",
    "skin_pheomelanin",
    "hair_eumelanin",
    "hair_pheomelanin",
    "iris_melanin",
    "hair_curl",
    "hair_density",
    "balding_stage",
    "hair_grey_fraction",
    "eyebrow_density",
    "beard_density",
    "beard_coverage",
    "body_hair",
    "freckle_density",
    "face_wrinkles",
    "skin_laxity",
    "age_spots",
    "posture_stoop",
    "androgenic_development",
    "estrogenic_development",
    "breast_development",
]


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def get_json(url: str) -> dict:
    request = urllib.request.Request(url, headers={"Accept": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=300) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        fail(f"{url} -> HTTP {error.code} {error.read(400)!r}")
    except urllib.error.URLError as error:
        fail(f"{url} -> {error.reason}")
    raise AssertionError


def body_of(raw: dict) -> dict:
    missing = [key for key in BODY_KEYS if key not in raw]
    if missing:
        fail(f"body is missing {missing}")
    extra = [key for key in raw if key not in BODY_KEYS and key != "contract_version"]
    if extra:
        fail(f"body has keys outside the contract: {extra}")
    out = {}
    for key in BODY_KEYS:
        value = raw[key]
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            fail(f"{key} is not a number: {value!r}")
        out[key] = value
    return out


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Write family.json from simagent view.")
    parser.add_argument("--base", required=True, help="http://127.0.0.1:PORT")
    parser.add_argument("--run", required=True, help="Run directory name of the full town")
    parser.add_argument("--scenario", default="data/scenarios/stadt.json")
    parser.add_argument("--seed", default="000000000000a001")
    parser.add_argument("--until", type=int, default=UNTIL_50Y)
    parser.add_argument("--out", default=f"{CACHE}/family.json")
    parser.add_argument("--runs-dir", default=f"{CACHE}/runs")
    parser.add_argument("--web", default=f"{CACHE}/view-web")
    parser.add_argument("--simagent", default="target/release/simagent")
    parser.add_argument(
        "--ages-run",
        default="stadt-ages",
        help="Run directory stopped at each child birthday in turn",
    )
    parser.add_argument(
        "--drive",
        action="store_true",
        help="Advance --ages-run with simagent run --until and read each stop",
    )
    return parser.parse_args(argv)


def index_pedigree(pedigree: dict) -> tuple[dict, dict]:
    people = {}
    children = defaultdict(list)
    for person in pedigree["persons"]:
        sim = int(person["sim"])
        people[sim] = person
        for parent in (person.get("mother"), person.get("father")):
            if parent is not None:
                children[int(parent)].append(sim)
    return people, children


def descendants(roots: list[int], children: dict) -> set[int]:
    seen = set()
    stack = list(roots)
    while stack:
        sim = stack.pop()
        if sim in seen:
            continue
        seen.add(sim)
        stack.extend(children.get(sim, ()))
    return seen


def living_index(payload: dict, pedigree: dict) -> dict[int, dict]:
    found = {}
    for person in payload["people"]:
        sim = int(person["sim"])
        age = int(person["shown"]["at"]["age_years"])
        found[sim] = {
            "sim": sim,
            "age": age,
            "sex": pedigree[sim]["sex"],
            "shown": person["shown"],
        }
    return found


def joint_children(mother: int, father: int, children: dict, pedigree: dict) -> list[int]:
    found = []
    for child in children.get(mother, ()):
        ped = pedigree.get(child)
        if ped and int(ped.get("father") or -1) == father:
            found.append(child)
    return found


def closest_line(living: dict, pedigree: dict, children: dict) -> dict:
    """Grandparents both 69, and the 17-year-old girl with the 13-year-old boy.

    The two children share a mother and a father. One of those parents is a
    child of the grandparents. Several such pairs would keep the grandparents'
    child closest to 47, then the lowest sim ids.
    """
    best = None
    best_key = None
    seen_couples = set()
    for person in living.values():
        mother = pedigree[person["sim"]].get("mother")
        father = pedigree[person["sim"]].get("father")
        if mother is None or father is None:
            continue
        mother = int(mother)
        father = int(father)
        if (mother, father) in seen_couples:
            continue
        if mother not in living or father not in living:
            continue
        if living[mother]["sex"] != "female" or living[father]["sex"] != "male":
            continue
        if living[mother]["age"] != 69 or living[father]["age"] != 69:
            continue
        seen_couples.add((mother, father))
        joint = joint_children(mother, father, children, pedigree)
        if not joint:
            continue
        pool = descendants(joint, children)
        girls = [
            sim
            for sim in pool
            if sim in living and living[sim]["age"] == 17 and living[sim]["sex"] == "female"
        ]
        boys = [
            sim
            for sim in pool
            if sim in living and living[sim]["age"] == 13 and living[sim]["sex"] == "male"
        ]
        for girl in girls:
            for boy in boys:
                ped_girl = pedigree[girl]
                ped_boy = pedigree[boy]
                if ped_girl.get("mother") != ped_boy.get("mother"):
                    continue
                if ped_girl.get("father") != ped_boy.get("father"):
                    continue
                mom = ped_girl.get("mother")
                dad = ped_girl.get("father")
                if mom is None or dad is None:
                    continue
                mom = int(mom)
                dad = int(dad)
                if mom not in living or dad not in living:
                    continue
                link = None
                if mom in joint:
                    link = mom
                elif dad in joint:
                    link = dad
                if link is None:
                    continue
                key = (abs(living[link]["age"] - 47), girl, boy, mother, father)
                if best_key is None or key < best_key:
                    best_key = key
                    best = {
                        "grandmother": mother,
                        "grandfather": father,
                        "mother": mom,
                        "father": dad,
                        "girl": girl,
                        "boy": boy,
                    }
    if best is None:
        fail(
            "No grandparents both aged 69 have a 17-year-old girl and a "
            "13-year-old boy who share those grandparents and the same parents."
        )
    return best


def member(role: str, sim: int, source: str, pedigree: dict, shown: dict, until_seconds: int) -> dict:
    ped = pedigree[sim]
    age = int(shown["at"]["age_years"])
    mother = ped.get("mother")
    father = ped.get("father")
    return {
        "role": role,
        "sim": sim,
        "sex": ped["sex"],
        "first_name": ped.get("first_name"),
        "surname": ped.get("surname"),
        "age_years": age,
        "source": source,
        "until_seconds": until_seconds,
        "at": shown["at"],
        "mother": None if mother is None else int(mother),
        "father": None if father is None else int(father),
        "body": body_of(shown["body"]),
    }


def birthday(born_t: int, age: int) -> int:
    return born_t + age * YEAR


def simagent(binary: str, args: list[str]) -> None:
    command = [binary, *args]
    print(" ".join(command), flush=True)
    completed = subprocess.run(command, check=False)
    if completed.returncode != 0:
        fail(f"simagent exited {completed.returncode}")


def closed_through(path: Path) -> int | None:
    manifest = path / "manifest.json"
    if not manifest.is_file():
        return None
    payload = json.loads(manifest.read_text(encoding="utf-8"))
    return int(payload["closed_through"])


def advance_ages(args: argparse.Namespace, until: int) -> None:
    out = str(Path(args.runs_dir) / args.ages_run)
    current = closed_through(Path(out))
    if current is None:
        simagent(
            args.simagent,
            ["run", "--scenario", args.scenario, "--out", out, "--until", str(until)],
        )
    elif current < until:
        simagent(
            args.simagent,
            ["run", "--resume", "--out", out, "--until", str(until)],
        )
    elif current > until:
        fail(f"{args.ages_run} is already at {current}, past snapshot {until}")
    seen = closed_through(Path(out))
    if seen != until:
        fail(f"{args.ages_run} closed at {seen}, expected {until}")


def capture_child(base: str, run: str, pedigree: dict, sim: int, age: int, role: str, until: int) -> dict:
    people = get_json(f"{base}/api/runs/{run}/people")
    living = living_index(people, pedigree)
    if sim not in living:
        fail(f"sim {sim} is not living in {run} at {until}")
    shown = living[sim]["shown"]
    got = int(shown["at"]["age_years"])
    if got != age:
        fail(f"sim {sim} is age {got} at {until}, expected {age}")
    if int(shown["at"]["t"]) != until:
        fail(f"sim {sim} is shown at {shown['at']['t']}, expected {until}")
    return member(role, sim, "shown", pedigree, shown, until)


def member_order(entry: dict) -> tuple:
    role = {"grandfather": 0, "grandmother": 1, "father": 2, "mother": 3, "girl_17": 4, "boy_13": 5}
    return (role.get(entry["role"], 9), int(entry["age_years"]), int(entry["sim"]))


def document_of(args, people, seed, manifest, line, adults, child_members, base: str) -> dict:
    return {
        "contract_version": people["contract_version"],
        "scenario": args.scenario,
        "seed": args.seed,
        "world_seed": seed,
        "until_seconds": args.until,
        "closed_through": manifest.get("closed_through"),
        "run": args.run,
        "ages_run": args.ages_run,
        "runs_dir": args.runs_dir,
        "line": {
            "grandfather": line["grandfather"],
            "grandmother": line["grandmother"],
            "father": line["father"],
            "mother": line["mother"],
            "girl_17": line["girl"],
            "boy_13": line["boy"],
        },
        "members": sorted(adults + child_members, key=member_order),
        "reproduce": {
            "note": (
                "The adults are the full stadt run. The children are that same "
                "run stopped with --until on the birthday second born + age * 364 * 86400. "
                "A later birthday is omitted when it falls after the run."
            ),
            "commands": [
                "cargo build -p sim-cli --release",
                (
                    f"{args.simagent} view {args.runs_dir} "
                    f"--web {args.web} --port 8765"
                ),
                (
                    "python3 art/family.py "
                    f"--base {base} --run {args.run} --drive --out {args.out}"
                ),
            ],
        },
    }


def write_document(args: argparse.Namespace, payload: dict) -> None:
    for entry in payload["members"]:
        if len(entry["body"]) != 44:
            fail(f"{entry['role']} sim {entry['sim']} has {len(entry['body'])} body values")
    text = json.dumps(payload, indent=2) + "\n"
    Path(args.out).write_text(text, encoding="utf-8")
    print(f"wrote {args.out} members={len(payload['members'])}", flush=True)


def main(argv: list[str]) -> None:
    args = parse_args(argv)
    base = args.base.rstrip("/")
    manifest = get_json(f"{base}/api/runs/{args.run}")
    people = get_json(f"{base}/api/runs/{args.run}/people")
    pedigree = get_json(f"{base}/api/runs/{args.run}/pedigree")
    seed = str(manifest.get("world_seed", ""))
    if seed.lower() != args.seed.lower():
        fail(f"run seed {seed} != scenario seed {args.seed}")
    if int(manifest.get("closed_through", -1)) != args.until:
        fail(
            f"run {args.run} closed at {manifest.get('closed_through')}, "
            f"expected the latest time {args.until}"
        )
    if people.get("contract_version") is None:
        fail("people response has no contract_version")
    ped_index, children = index_pedigree(pedigree)
    living = living_index(people, ped_index)
    line = closest_line(living, ped_index, children)
    adults = [
        member("grandfather", line["grandfather"], "shown", ped_index, living[line["grandfather"]]["shown"], args.until),
        member("grandmother", line["grandmother"], "shown", ped_index, living[line["grandmother"]]["shown"], args.until),
        member("father", line["father"], "shown", ped_index, living[line["father"]]["shown"], args.until),
        member("mother", line["mother"], "shown", ped_index, living[line["mother"]]["shown"], args.until),
    ]
    births = {}
    schedule = []
    for role, sim in (("girl_17", line["girl"]), ("boy_13", line["boy"])):
        view = get_json(f"{base}/api/runs/{args.run}/persons/{sim}")
        born = int(view["born"]["t"])
        births[sim] = born
        latest_age = living[sim]["age"]
        for age in CHILD_AGES:
            until = birthday(born, age)
            if until > args.until:
                continue
            if age > latest_age:
                continue
            schedule.append((until, role, sim, age))
    schedule.sort()
    if not args.drive:
        fail("pass --drive so each child birthday is a simagent run --until stop")
    previous = Path(args.out)
    saved = {}
    if previous.is_file():
        loaded = json.loads(previous.read_text(encoding="utf-8"))
        for entry in loaded.get("members", []):
            if entry.get("role") in ("girl_17", "boy_13"):
                saved[(entry["sim"], entry["age_years"])] = entry
    child_members = []
    for until, role, sim, age in schedule:
        if (sim, age) in saved:
            child_members.append(saved[(sim, age)])
            continue
        advance_ages(args, until)
        child_members.append(capture_child(base, args.ages_run, ped_index, sim, age, role, until))
        write_document(args, document_of(args, people, seed, manifest, line, adults, child_members, base))
    write_document(args, document_of(args, people, seed, manifest, line, adults, child_members, base))


if __name__ == "__main__":
    main(sys.argv[1:])
