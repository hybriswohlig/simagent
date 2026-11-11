#!/usr/bin/env python3
"""Pick one simulated family from a running `simagent view` and write art/family.json.

The core already serves bodies on the view API. Fixed preview ages are
2, 8, 12, 16, 25, 40 and 60 (`SHOWN_AGES`). Ages 10, 13 and 17 are the
current age of a living person. Ages 8 and 12 may be that same person's
`at_ages` entry.

`web/dist` is not required. `simagent view` only checks that `--web` contains
`index.html`, then the JSON routes work. Point `--web` at a cache directory
with a stub page so nothing under `web/` is written.

Example, after a release build of `sim-cli` (binary name `simagent`):

    target/release/simagent run \
        --scenario data/scenarios/stadt.json \
        --out /Users/dariushk/.cache/simagent-art/runs/stadt \
        --until 1572480000
    target/release/simagent view \
        /Users/dariushk/.cache/simagent-art/runs \
        --web /Users/dariushk/.cache/simagent-art/view-web \
        --port 8765
    python3 art/family.py --base http://127.0.0.1:8765 --run stadt \
        --until 1572480000 --out art/family.json

1572480000 seconds is 50 years of 364 days. The scenario seed is
`000000000000a001` (`data/scenarios/stadt.json`). The town starts as 300
adults aged 18 to 40, each alone, so the grandparents and the children
exist only after the run has had time to grow them.
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.request
from collections import defaultdict

YEAR = 364 * 86400
UNTIL_50Y = 50 * YEAR

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

# Exact child ages. 8 and 12 may come from at_ages of an older descendant.
CHILD_SLOTS = [
    ("child_8", 8, None),
    ("child_10", 10, None),
    ("child_12", 12, None),
    ("boy_13", 13, "male"),
    ("girl_13", 13, "female"),
    ("boy_17", 17, "male"),
    ("girl_17", 17, "female"),
]
LIVING_ONLY = {10, 13, 17}


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
    parser = argparse.ArgumentParser(description="Write art/family.json from simagent view.")
    parser.add_argument("--base", required=True, help="http://127.0.0.1:PORT")
    parser.add_argument("--run", required=True, help="Run directory name")
    parser.add_argument("--scenario", default="data/scenarios/stadt.json")
    parser.add_argument("--seed", default="000000000000a001")
    parser.add_argument("--until", type=int, default=UNTIL_50Y)
    parser.add_argument("--out", default="art/family.json")
    parser.add_argument(
        "--runs-dir",
        default="/Users/dariushk/.cache/simagent-art/runs",
    )
    parser.add_argument(
        "--web",
        default="/Users/dariushk/.cache/simagent-art/view-web",
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
        sex = pedigree[sim]["sex"]
        found[sim] = {
            "sim": sim,
            "age": age,
            "sex": sex,
            "shown": person["shown"],
        }
    return found


def age_distance(age: int, target: int) -> int:
    return abs(age - target)


def candidate_families(living: dict, pedigree: dict, children: dict, gp_lo, gp_hi, parent_lo, parent_hi):
    """Yield (score, record) for grandparent couples who cover the child slots."""
    considered = 0
    for parent in living.values():
        if not (parent_lo <= parent["age"] <= parent_hi):
            continue
        ped = pedigree[parent["sim"]]
        mother = ped.get("mother")
        father = ped.get("father")
        if mother is None or father is None:
            continue
        mother = int(mother)
        father = int(father)
        if mother not in living or father not in living:
            continue
        gp_m = living[mother]
        gp_f = living[father]
        if gp_m["sex"] != "female" or gp_f["sex"] != "male":
            continue
        if not (gp_lo <= gp_m["age"] <= gp_hi and gp_lo <= gp_f["age"] <= gp_hi):
            continue
        joint = []
        for child in children.get(mother, ()):
            ped_child = pedigree.get(child)
            if ped_child and int(ped_child.get("father") or -1) == father:
                joint.append(child)
        if parent["sim"] not in joint:
            continue
        pool = descendants(joint, children)
        picks = {}
        used = set()
        ok = True
        for role, age, sex in CHILD_SLOTS:
            if age in LIVING_ONLY:
                options = [
                    sim
                    for sim in pool
                    if sim in living
                    and living[sim]["age"] == age
                    and (sex is None or living[sim]["sex"] == sex)
                    and sim not in used
                ]
                if not options:
                    ok = False
                    break
                choice = min(options)
                used.add(choice)
                picks[role] = ("shown", choice, age)
            # 8 and 12 are filled after the living slots, from at_ages when possible.
        if not ok:
            considered += 1
            continue
        older = [
            sim
            for sim, kind in ((picks[role][1], age) for role, age, _sex in CHILD_SLOTS if age in LIVING_ONLY)
            if living[sim]["age"] >= 12
        ]
        for role, age, _sex in CHILD_SLOTS:
            if age in LIVING_ONLY:
                continue
            shown = [
                sim
                for sim in pool
                if sim in living and living[sim]["age"] == age and sim not in used
            ]
            if shown:
                choice = min(shown)
                used.add(choice)
                picks[role] = ("shown", choice, age)
            elif older:
                donor = min(older)
                picks[role] = ("at_age", donor, age)
            else:
                ok = False
                break
        if not ok:
            continue
        # The other parent of one of the selected children, near 40 and living.
        spouse = None
        for role, age, sex in CHILD_SLOTS:
            if age not in LIVING_ONLY:
                continue
            child = picks[role][1]
            ped_child = pedigree[child]
            pair = [ped_child.get("mother"), ped_child.get("father")]
            pair = [None if item is None else int(item) for item in pair]
            if parent["sim"] not in pair:
                continue
            other = pair[0] if pair[1] == parent["sim"] else pair[1]
            if other is None or other not in living:
                continue
            if parent_lo <= living[other]["age"] <= parent_hi:
                spouse = other
                break
        if spouse is None:
            continue
        score = (
            age_distance(gp_f["age"], 65)
            + age_distance(gp_m["age"], 65)
            + age_distance(parent["age"], 40)
            + age_distance(living[spouse]["age"], 40)
        )
        # Prefer the selected children to be this couple's own children.
        own = 0
        for role, _age, _sex in CHILD_SLOTS:
            if picks[role][0] != "shown":
                continue
            child = picks[role][1]
            ped_child = pedigree[child]
            parents = {ped_child.get("mother"), ped_child.get("father")}
            if parent["sim"] in parents and spouse in parents:
                own += 1
        score -= own
        yield score, {
            "grandmother": mother,
            "grandfather": father,
            "parent_a": parent["sim"],
            "parent_b": spouse,
            "picks": picks,
            "joint_children": joint,
        }
        considered += 1
    return considered


def snapshot(role: str, sim: int, source: str, age: int, living: dict, pedigree: dict, person_view: dict | None) -> dict:
    ped = pedigree[sim]
    if source == "shown":
        shown = living[sim]["shown"]
    else:
        if person_view is None:
            fail(f"missing person view for sim {sim}")
        match = [
            entry
            for entry in person_view["at_ages"]
            if int(entry["at"]["age_years"]) == age
        ]
        if len(match) != 1:
            fail(f"sim {sim} has no single at_ages entry for age {age}")
        shown = match[0]
    if int(shown["at"]["age_years"]) != age:
        fail(f"sim {sim} {source} age is {shown['at']['age_years']}, expected {age}")
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
        "at": shown["at"],
        "mother": None if mother is None else int(mother),
        "father": None if father is None else int(father),
        "body": body_of(shown["body"]),
    }


def adult_snapshot(role: str, sim: int, living: dict, pedigree: dict) -> dict:
    ped = pedigree[sim]
    shown = living[sim]["shown"]
    mother = ped.get("mother")
    father = ped.get("father")
    return {
        "role": role,
        "sim": sim,
        "sex": ped["sex"],
        "first_name": ped.get("first_name"),
        "surname": ped.get("surname"),
        "age_years": living[sim]["age"],
        "source": "shown",
        "at": shown["at"],
        "mother": None if mother is None else int(mother),
        "father": None if father is None else int(father),
        "body": body_of(shown["body"]),
    }


def main(argv: list[str]) -> None:
    args = parse_args(argv)
    base = args.base.rstrip("/")
    run = args.run
    manifest = get_json(f"{base}/api/runs/{run}")
    people = get_json(f"{base}/api/runs/{run}/people")
    pedigree = get_json(f"{base}/api/runs/{run}/pedigree")
    seed = str(manifest.get("world_seed", ""))
    if seed.lower() != args.seed.lower():
        fail(f"run seed {seed} != scenario seed {args.seed}")
    if people.get("contract_version") is None:
        fail("people response has no contract_version")
    ped_index, children = index_pedigree(pedigree)
    living = living_index(people, ped_index)
    windows = [(62, 70, 36, 45), (58, 75, 32, 50), (55, 80, 30, 55)]
    best = None
    best_score = None
    for gp_lo, gp_hi, parent_lo, parent_hi in windows:
        for score, record in candidate_families(
            living, ped_index, children, gp_lo, gp_hi, parent_lo, parent_hi
        ):
            if best_score is None or score < best_score:
                best_score = score
                best = record
        if best is not None:
            break
    if best is None:
        ages = defaultdict(int)
        for person in living.values():
            ages[person["age"]] += 1
        listed = ", ".join(f"{age}:{count}" for age, count in sorted(ages.items()))
        fail(
            "No grandparent couple in this run covers ages 8, 10, 12, 13 and 17 "
            f"with both sexes at 13 and 17. Living ages: {listed}"
        )
    views = {}
    for role, (source, sim, _age) in best["picks"].items():
        if source == "at_age" and sim not in views:
            views[sim] = get_json(f"{base}/api/runs/{run}/persons/{sim}")
    members = [
        adult_snapshot("grandmother", best["grandmother"], living, ped_index),
        adult_snapshot("grandfather", best["grandfather"], living, ped_index),
        adult_snapshot(
            "mother" if living[best["parent_a"]]["sex"] == "female" else "father",
            best["parent_a"],
            living,
            ped_index,
        ),
        adult_snapshot(
            "mother" if living[best["parent_b"]]["sex"] == "female" else "father",
            best["parent_b"],
            living,
            ped_index,
        ),
    ]
    roles = {entry["role"] for entry in members}
    if roles != {"grandmother", "grandfather", "mother", "father"}:
        fail(f"adult roles are {sorted(roles)}")
    for role, _age, _sex in CHILD_SLOTS:
        source, sim, age = best["picks"][role]
        members.append(snapshot(role, sim, source, age, living, ped_index, views.get(sim)))
    document = {
        "contract_version": people["contract_version"],
        "scenario": args.scenario,
        "seed": args.seed,
        "world_seed": seed,
        "until_seconds": args.until,
        "closed_through": manifest.get("closed_through"),
        "run": run,
        "runs_dir": args.runs_dir,
        "members": members,
        "reproduce": {
            "note": (
                "stadt.json starts 300 adults aged 18 to 40, each in their own household. "
                "The family is whoever that run has grown by --until. "
                "simagent view needs a directory with index.html; the stub is not the app."
            ),
            "commands": [
                "cargo build -p sim-cli --release",
                (
                    "target/release/simagent run "
                    f"--scenario {args.scenario} "
                    f"--out {args.runs_dir}/{run} "
                    f"--until {args.until}"
                ),
                (
                    "target/release/simagent view "
                    f"{args.runs_dir} --web {args.web} --port 8765"
                ),
                (
                    "python3 art/family.py "
                    f"--base {base} --run {run} --until {args.until} --out {args.out}"
                ),
            ],
        },
    }
    text = json.dumps(document, indent=2) + "\n"
    with open(args.out, "w", encoding="utf-8") as handle:
        handle.write(text)
    print(
        f"wrote {args.out} members={len(members)} "
        f"grandparents={best['grandfather']},{best['grandmother']} "
        f"score={best_score}"
    )


if __name__ == "__main__":
    main(sys.argv[1:])
