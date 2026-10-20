#!/usr/bin/env python3
"""The goldens of the scenario files of the repository's own data (ADR-0003 6.3), computed
independently of the Rust code by the model in tools/world_reference.py on the repository's
data as the CLI builds them: data/ with map 4, its calibration, the life course data, the
pool data/pool/pool.bin and its frequencies data/pool/pool_frequencies.json
(`world_reference.REPOSITORY`; slice S5 of the switch to map 4, design note sections 5, 6
and 14, I-f). What `simagent run` gives for each scenario file, without a story:

- tests/scenarios/dummy.json (format 5, no pool): tests/golden/dummy.json (the core version,
  the data hash, the year hashes, the log hash) of the run to `until`; tests/golden/
  dummy_fork.json of its fork (the same run to the last second of year 4, the override
  "hereditary diseases on" stamped T_5 − 1 s, then to `until`; ADR-0001 section 6);
  tests/golden/dummy.save, the save of the run on day 100 of year 3; tests/golden/
  dummy_pedigree.json and dummy_fork_pedigree.json, the text of `pedigree.json` of the run and
  of the fork at `until`, compact as serde_json writes it, followed by one newline (`simagent
  export`); tests/golden/dummy_card.json, the genetics card of every person of the run at
  `until` (`card_json`). Its founders draw every locus they do not name (F1, purpose tag 28
  FOUNDER_ALLELE: `q = d_ref / 2` at an additive locus, the pool's frequency at the seven loci
  of the pool), so its story follows the draws; the hand story of this scenario lives on map 3
  (tools/dummy_story.py), where nothing is drawn.
- tests/scenarios/card_hunger.json (the dummy with the nutrition index 0.7 in year 2 and 0.5
  in year 3): tests/golden/card_hunger.json, the height lines of the genetics card of every
  person of its run in the middle of year 2 (`HUNGER_MID`, year 3 still ahead) and at `until`
  (`card_height_json`).
- tests/scenarios/town.json (format 5 with a pool): tests/golden/town.json in the same form,
  and tests/golden/town_card.json, the genetics card of two persons of each case of parents
  in the run (`town_card_json`),
  the run of a town founded from the pool: the CLI fills `pool_hash` and `households` with the
  draw of occasion 0 (`PoolStart::drawn`, `world_reference.pool_start`), the core judges.

Every model load check runs on the save and at the ends (`check_all`). The coverage of
ADR-0003 6.3 is recounted over the dummy run, its fork, the arrival (tests/golden/
arrival.json, tools/arrival_reference.py) and the town: every event kind (event_codes.txt,
codes 1 to 10) must run and every log kind (log_codes.txt, payloads 0 to 13) must appear in
one of them; printed per scenario and for the union (`coverage_rows`).

Usage: .venv/bin/python tools/golden_reference.py           (write the six goldens, print
       the values of the CLI tests)
       .venv/bin/python tools/golden_reference.py --check   (compare the six goldens, run the
       story on map 3 and compare the writer's five dummy files on map 3 with the story's;
       write nothing)
       .venv/bin/python tools/golden_reference.py --story   (the story on map 3 only)
"""

import json
import math
import sys
from pathlib import Path

import mpmath

import data_reference
import rng_reference as rng
import world_reference as wr
from postcard_reference import FEMALE, MALE
from world_reference import MONTH, YEAR, hash64

ROOT = Path(__file__).resolve().parent.parent
GOLDEN = ROOT / "tests" / "golden"
SCENARIOS = ROOT / "tests" / "scenarios"
DAY = 86_400
# The fork point of the dummy goldens (the override acts at T_5) and the second of the save.
FORK_YEAR = 5
SAVE_AT = 3 * YEAR + 100 * DAY
DUMMY_FILES = ("dummy.json", "dummy_fork.json", "dummy.save", "dummy_pedigree.json", "dummy_fork_pedigree.json")
# The export of the run with names (`simagent export`, format 5); not one of the story's files.
EXPORT_FILE = "dummy_export.json"
CARD_FILE = "dummy_card.json"
TOWN_CARD_FILE = "town_card.json"
HUNGER_FILE = "card_hunger.json"
# The last closed second in the middle of year 2: a hunger year under way, the next ahead.
HUNGER_MID = 2 * YEAR + YEAR // 2 - 1
# The traits of the card that read as a percentile (ADR-0021 section 5, Grok G3): height as the
# target height `z_h` of `core:growth`, cognition, curls.
CARD_TRAITS = ("core:growth", "core:cognition", "core:curl")
ADULT_YEARS = 18
NAMES = ROOT / "data" / "names"
# The purpose tags of the names (crates/sim-core/src/rng/tags.rs, ADR-0021 section 3).
FIRST_NAME, SURNAME = 29, 30
TOWN_FILE = "town.json"
# The kinds of ADR-0003 6.3: event kinds 1 (year_end) to 10 (immigration), log payloads 0
# (core:started) to 13 (core:arrived), as tests/golden/event_codes.txt and log_codes.txt
# register them.
EVENT_NAMES = {
    1: "year_end", 2: "birthday", 3: "death", 4: "conception_attempt", 5: "birth", 6: "separation",
    7: "market_round", 8: "leave_home", 9: "emigration", 10: "immigration",
}
LOG_NAMES = {
    0: "core:started", 1: "core:founder_added", 2: "core:birthday", 3: "core:param_changed", 4: "core:death",
    5: "core:partnered", 6: "core:conception", 7: "core:meiosis", 8: "core:birth", 9: "core:separated",
    10: "core:household_formed", 11: "core:moved", 12: "core:emigrated", 13: "core:arrived",
}
# The pairs of the CLI's kinship test (crates/sim-cli/tests/cli.rs) on the dummy run.
KINSHIP_PAIRS = [(1, 8), (8, 10), (1, 2), (3, 3), (13, 13), (2, 13), (10, 13), (8, 7), (8, 4), (14, 1)]


def hexed(value: int) -> str:
    return f"{value:016x}"


def scenario(path: Path) -> dict:
    """A scenario file of format 5 (design note of the switch, section 3) as `World` takes it:
    the founders (born, sex, [(locus, copies)]), the couples, the capacity and the pool
    (None or {"n_start", "immigration"}) as written, the nutrition years as (year, index)."""
    text = json.loads(path.read_text())
    assert text["format_version"] == 5, path
    start = text["start"]
    assert list(start) == ["world_seed", "founder_tag", "world_params", "founders", "couples", "capacity", "pool"], (path, list(start))
    assert list(start["world_params"]) == ["hereditary_diseases", "nutrition"], path
    founders = [
        (founder["born"], FEMALE if founder["sex"] == "female" else MALE, [(allele["locus"], list(allele["copies"])) for allele in founder["alleles"]])
        for founder in start["founders"]
    ]
    return {
        "seed": int(start["world_seed"], 16),
        "tag": start["founder_tag"],
        "switch": start["world_params"]["hereditary_diseases"],
        "nutrition": [(entry["year"], entry["index"]) for entry in start["world_params"]["nutrition"]],
        "founders": founders,
        "couples": [(couple["woman"], couple["man"]) for couple in start["couples"]],
        "capacity": start["capacity"],
        "pool": start["pool"],
        "until": text["until"],
    }


def make_world(s: dict, data: wr.Data) -> wr.World:
    """`simagent run`'s world after `Start`: the scenario's pool resolved on the data as the CLI
    resolves it (`PoolStart::drawn`), then submitted."""
    pool = None if s["pool"] is None else wr.pool_start(data, s["seed"], s["pool"]["n_start"], s["pool"]["immigration"])
    return wr.World(s["seed"], s["tag"], s["switch"], s["founders"], s["couples"], data, capacity=s["capacity"], pool=pool, nutrition=s["nutrition"])


def check_all(world: wr.World) -> None:
    for check in (wr.check_households, wr.check_separations, wr.check_market, wr.check_afflictions, wr.check_emigration, wr.check_pool, wr.check_deaths):
        check(world)


def golden_text(world: wr.World) -> str:
    """The text of tests/golden/<scenario>.json: the core version, the data hash of the
    world's data, the year hashes, the log hash; `json.dumps(indent=2)` and a newline."""
    content = {
        "core_version": wr.CORE_VERSION,
        "data_hash": hexed(world.data.hash),
        "year_hashes": [{"year": year, "hash": hexed(value)} for year, value in enumerate(world.year_hashes)],
        "log_hash": hexed(world.log_hash()),
    }
    return json.dumps(content, indent=2) + "\n"


def dummy_worlds(s: dict, data: wr.Data) -> dict:
    """The run to `until`, the fork (the override "hereditary diseases on" at the last second
    of year 4), the world of the save (day 100 of year 3); every load check on each."""
    run = make_world(s, data)
    run.advance(s["until"])
    check_all(run)
    fork = make_world(s, data)
    fork.advance(FORK_YEAR * YEAR - 1)
    fork.override(True)
    fork.advance(s["until"])
    check_all(fork)
    saved = make_world(s, data)
    saved.advance(SAVE_AT)
    check_all(saved)
    return {"run": run, "fork": fork, "saved": saved, "until": s["until"]}


def export_json(world: wr.World) -> str:
    """`pedigree.json` as `simagent export` writes it, format 5 (ADR-0021 section 3): the
    core's pedigree with `first_name` and `surname` last in each person. The first name is
    the list of the person's sex at `uniform_below` of the first word of the draw (seed,
    FIRST_NAME, its lineage key, 0 female or 1 male, 0); the surname the list of surnames at
    the draw (seed, SURNAME, root, 0, 0), the root being the first person without a father up
    the chain of fathers in the run (a run without a pool here)."""
    view = world.pedigree_view()
    lists = {name: json.loads((NAMES / f"{name}.json").read_text()) for name in ("female", "male", "surnames")}
    persons = {person["sim"]: person for person in view["persons"]}

    def root(person: dict) -> int:
        while person["father"] is not None:
            person = persons[person["father"]]
        assert person["pool"] is None, person["sim"]
        return int(person["lineage_key"], 16)

    def pick(names: list, tag: int, subject: int, occasion: int) -> str:
        word = rng.words(rng.chacha_key(world.seed, tag, subject, occasion, 0))[0]
        return names[rng.uniform_below(word, len(names))]

    for person in view["persons"]:
        female = person["sex"] == "female"
        first = pick(lists["female" if female else "male"], FIRST_NAME, int(person["lineage_key"], 16), 0 if female else 1)
        person["first_name"] = first
        person["surname"] = pick(lists["surnames"], SURNAME, root(person), 0)
    view["format_version"] = 5
    return wr.json_text(view)


def percentile(z: float) -> dict:
    """The percentile of a phenotype against the frozen model reference, where `z` is N(0, 1)
    (Grok G2 and L1): `p = 100 Φ(z)` with mpmath; from the middle up (`z ≥ 0`) "größer als
    `n`" with `n = min(⌊p⌋, 99)`, below "kleiner als `n`" with `n = min(⌊100 − p⌋, 99)`. A
    value whose share lies within 1e-9 of a whole number from 1 to 99 is refused (only
    `z = 0`, exactly 50, is allowed there): the core decides it with the pinned probit, whose
    tolerance this reference does not model."""
    with mpmath.workdps(60):
        phi = mpmath.ncdf(mpmath.mpf(z))
        up = z >= 0.0
        share = 100 * (phi if up else 1 - phi)
        n = int(mpmath.floor(share))
        nearest = int(mpmath.nint(share))
        assert z == 0.0 or not 1 <= nearest <= 99 or abs(share - nearest) > mpmath.mpf("1e-9"), (z, share)
    return {"up": up, "n": min(n, 99)}


def card_json(world: wr.World, now: int, chosen=None) -> str:
    """tests/golden/dummy_card.json: the genetics card (ADR-0021 section 5, Grok G2 to G5, L1
    to L5) of every person of the run, by SimId, each at its second shown (`now` for the
    living, the second it left for the archived), as a JSON list of `{"sim", "traits"}`.

    Per trait of `CARD_TRAITS`, from `wr.phenotype_parts` at the age then: `z` and its
    `percentile`; `adult` if the age in whole years is at least 18; the parts with `r =
    sqrt(h2)`: `genetic = r · G`, `childhood = sqrt(c2) · C`, `own = sqrt(e2) · E`. With both
    parents in the run (their lineage keys those of sims of the run, living or archived): kind
    `split`, `parents = r · ((G_mother + G_father) / 2)` and `passing = genetic − parents`, and
    `parents_mean` the percentile of `(z_mother + z_father) / 2`, each parent's `z` at its own
    second shown; else kind `inherited` with `inherited = genetic` and `parents_in_town` (0 or
    1), and `parents_mean` null (Grok L4, L5). `chosen`, if given, limits the list to those
    sims."""
    _, calibration, genome_map = data_reference.documents(wr.ROOT / "data")
    stripped = data_reference.strip(calibration)
    traits = [wr.Data.phenotype_trait(name, stripped, genome_map) for name in CARD_TRAITS]
    everyone = sorted(set(world.sims) | set(world.archive))
    by_key = {world.key_of(sim): sim for sim in everyone}

    def facts(sim: int) -> tuple:
        if sim in world.sims:
            born, key, _, person = world.sims[sim][:4]
            return born, key, person, now
        born, left, _, key, _, person = world.archive[sim][:6]
        return born, key, person, left

    def parts(sim: int, trait) -> dict:
        born, key, person, at = facts(sim)
        e_seed, c_seed, parents = world.env_of(sim)
        return wr.phenotype_parts(e_seed, trait, person, key, parents, at - born, c_seed)

    cards = []
    for sim in everyone if chosen is None else sorted(chosen):
        born, _, person, at = facts(sim)
        _, _, parent_keys = world.env_of(sim)
        in_town = [] if parent_keys is None else [by_key[k] for k in parent_keys if k in by_key]
        rows = []
        for name, trait in zip(CARD_TRAITS, traits):
            p = parts(sim, trait)
            root = math.sqrt(p["h2"])
            genetic = root * p["G"]
            childhood = math.sqrt(p["c2"]) * p["C"]
            own = math.sqrt(p["e2"]) * p["E"]
            assert (genetic + childhood) + own == p["z"]
            if len(in_town) == 2:
                mother, father = in_town
                g_mother, g_father = (wr.genetic_value(facts(parent)[2], trait) for parent in (mother, father))
                share = root * ((g_mother + g_father) / 2.0)
                split = {"kind": "split", "parents": share, "passing": genetic - share, "childhood": childhood, "own": own}
                mean = percentile((parts(mother, trait)["z"] + parts(father, trait)["z"]) / 2.0)
            else:
                split = {"kind": "inherited", "inherited": genetic, "childhood": childhood, "own": own, "parents_in_town": len(in_town)}
                mean = None
            rows.append({
                "trait": name,
                "z": p["z"],
                "percentile": percentile(p["z"]),
                "adult": (at - born) // YEAR >= ADULT_YEARS,
                "parts": split,
                "parents_mean": mean,
            })
        cards.append({"sim": sim, "traits": rows})
    return wr.json_text(cards)


def town_card_json(world: wr.World, now: int) -> str:
    """tests/golden/town_card.json: `card_json` of the first two sims (by SimId) of each case of
    parents in the run, each case asserted to occur: a pool person with 0, 1 or 2 parents in
    the run, with 2 also with an archived parent; a town-born child of two pool persons, also
    with an archived parent; of a pool person and a town person; of two town persons (the
    town has one)."""
    everyone = sorted(set(world.sims) | set(world.archive))
    by_key = {world.key_of(sim): sim for sim in everyone}
    cases = {}
    for sim in everyone:
        _, _, keys = world.env_of(sim)
        found = [] if keys is None else [by_key[k] for k in keys if k in by_key]
        archived = any(parent in world.archive for parent in found)
        if world.is_pool_person(sim):
            case = ("pool", len(found), archived)
        elif keys is not None:
            pool_parents = sum(1 for parent in found if world.is_pool_person(parent))
            kind = {0: "town", 1: "mixed", 2: "town of pool"}[pool_parents]
            case = (kind, len(found), archived)
        else:
            continue
        cases.setdefault(case, []).append(sim)
    wanted = [("pool", 0, False), ("pool", 1, False), ("pool", 2, False), ("pool", 2, True),
              ("town of pool", 2, False), ("town of pool", 2, True), ("mixed", 2, False), ("town", 2, False)]
    assert all(case in cases for case in wanted), sorted(cases)
    return card_json(world, now, [sim for case in wanted for sim in cases[case][:2]])


def card_height_rows(world: wr.World, now: int) -> list:
    """The height lines of the genetics card (Grok L5, L6) of every person of the run at `now`,
    by SimId, each at its second shown, as a list of `{"sim", "hunger_cm", "age_cm",
    "parents_target_cm"}`: with `x = min(age, 25 years)`, the
    individual curve of `z_h` and `z_p` (`body_reference.individual_curve`), the loss from the
    nutrition years over `[born, born + x)` (`body_reference.nutrition_loss`, G6 for an
    arrival: `census_reference.nutrition_of`), the loss of age at `b = bio_age`
    (`body_reference.old_age_loss`), and with both parents in the run the mean of their target
    heights `h1_i`, `(h1_mother + h1_father) / 2`, else null. The same terms as
    `census_reference.heights`, whose pure height the mapping's raw height equals bit for
    bit."""
    import body_reference as br
    import census_reference as cr

    body = cr.BodyCensusData(world.data, data_reference.documents(wr.ROOT / "data"))
    evaluator = cr.Evaluator(world, body)
    values = body.values
    everyone = sorted(set(world.sims) | set(world.archive))
    by_key = {world.key_of(sim): sim for sim in everyone}

    def curve(sim: int) -> dict:
        parts = evaluator.person_parts(sim)
        return br.individual_curve(values, parts["sex"], parts["z"]["z_h"], parts["z"]["z_p"])

    rows = []
    for sim in everyone:
        parts = evaluator.person_parts(sim)
        at = now if sim in world.sims else world.archive[sim][1]
        age = at - parts["born"]
        x_seconds = min(age, 25 * YEAR)
        own = curve(sim)
        hunger = br.nutrition_loss(values, cr.nutrition_of(world, sim), parts["born"], x_seconds, own["theta_i"])
        b = br.bio_age(wr.as_years(age), parts["delta"], values.bio_age_weight)
        _, _, keys = world.env_of(sim)
        found = [] if keys is None else [by_key[k] for k in keys if k in by_key]
        target = (curve(found[0])["h1_i"] + curve(found[1])["h1_i"]) / 2.0 if len(found) == 2 else None
        rows.append({"sim": sim, "hunger_cm": hunger, "age_cm": br.old_age_loss(values, parts["sex"], b), "parents_target_cm": target})
    return rows


def card_height_json(s: dict, data: wr.Data) -> str:
    """tests/golden/card_hunger.json: `card_height_rows` of the run of `s` at `HUNGER_MID` and
    at `until`, as a JSON list of `{"t", "persons"}`."""
    snapshots = []
    for now in (HUNGER_MID, s["until"]):
        world = make_world(s, data)
        world.advance(now)
        check_all(world)
        snapshots.append({"t": now, "persons": card_height_rows(world, now)})
    return wr.json_text(snapshots)


def dummy_files(w: dict) -> dict:
    return {
        "dummy.json": golden_text(w["run"]).encode(),
        "dummy_fork.json": golden_text(w["fork"]).encode(),
        "dummy.save": w["saved"].save(),
        "dummy_pedigree.json": (w["run"].pedigree_json() + "\n").encode(),
        "dummy_fork_pedigree.json": (w["fork"].pedigree_json() + "\n").encode(),
        EXPORT_FILE: (export_json(w["run"]) + "\n").encode(),
        CARD_FILE: (card_json(w["run"], w["until"]) + "\n").encode(),
    }


def town_world(s: dict, data: wr.Data) -> wr.World:
    world = make_world(s, data)
    check_all(world)
    world.advance(s["until"])
    check_all(world)
    return world


def arrival_world() -> wr.World:
    """The scenario "arrival" of tools/arrival_reference.py to its end (the year end of year
    13), the run of tests/golden/arrival.json."""
    import arrival_reference

    world = arrival_reference.arrival_world()
    world.advance(arrival_reference.ARRIVAL_STOP_YEAR * YEAR - 1)
    return world


def kinds(world: wr.World, after: int = None) -> tuple:
    """(the event kinds the world ran, at or after `after` if given; the log kinds it wrote)."""
    return {kind for t, kind in world.ran if after is None or t >= after}, set(world.entry_kinds)


def coverage_rows(worlds: dict) -> list:
    """ADR-0003 6.3: per scenario the event and log kinds it misses, then the union of all and
    the kinds no scenario covers."""
    rows, events, logs = [], set(), set()
    for name, world in worlds.items():
        ran, wrote = kinds(world)
        events |= ran
        logs |= wrote
        rows.append(
            f"coverage {name} missing events {[EVENT_NAMES[k] for k in sorted(set(EVENT_NAMES) - ran)]}"
            f" missing logs {[LOG_NAMES[k] for k in sorted(set(LOG_NAMES) - wrote)]}"
        )
    if "dummy_fork" in worlds:
        ran, _ = kinds(worlds["dummy_fork"], FORK_YEAR * YEAR)
        rows.append(f"coverage dummy_fork_after_fork_point missing events {[EVENT_NAMES[k] for k in sorted(set(EVENT_NAMES) - ran)]}")
    missing_events = sorted(set(EVENT_NAMES) - events)
    missing_logs = sorted(set(LOG_NAMES) - logs)
    rows.append(
        f"coverage union {' + '.join(worlds)} missing events {[EVENT_NAMES[k] for k in missing_events]}"
        f" missing logs {[LOG_NAMES[k] for k in missing_logs]} {'complete' if not missing_events and not missing_logs else 'INCOMPLETE'}"
    )
    return rows


def person_of(world: wr.World, sim: int) -> tuple:
    """(born, key, sex, genome, parents, left, cause) of a living or archived sim."""
    if sim in world.sims:
        born, key, sex, person, _, _, _, parents, *_ = world.sims[sim]
        return born, key, sex, person, parents, None, None
    born, left, cause, key, sex, person, _, _, _, parents, *_ = world.archive[sim]
    return born, key, sex, person, parents, left, cause


def relation(world: wr.World, a: int, b: int) -> str:
    """A name for the pedigree relation of two sims (for the CLI's kinship pairs)."""
    known = set(world.sims) | set(world.archive)
    if a not in known or b not in known:
        return "unknown"
    if a == b:
        return "itself"
    def parents(sim: int) -> list:
        value = person_of(world, sim)[4]
        return [None, None] if value is None else list(value[:2])

    pa, pb = parents(a), parents(b)
    if a in pb or b in pa:
        return "parent_child"
    if pa[0] is not None and pa == pb:
        return "full_siblings"
    if any(p is not None and p in pb for p in pa):
        return "half_siblings"
    grand_b = [g for p in pb if p is not None for g in parents(p)]
    grand_a = [g for p in pa if p is not None for g in parents(p)]
    if a in grand_b or b in grand_a:
        return "grandparent"
    if any(p is not None and p in grand_b for p in pa) or any(p is not None and p in grand_a for p in pb):
        return "avuncular"
    partners = [(v[0], v[1]) for v in world.partnerships.values()]
    if (a, b) in partners or (b, a) in partners:
        return "partners"
    return "unrelated" if pa == [None, None] and pb == [None, None] else "other"


def story_rows(label: str, w: dict) -> list:
    """What happened in the dummy run and its fork (no hand story: the values the CLI tests
    read, and what changed against map 3)."""
    rows = []
    for name in ("run", "fork"):
        world = w[name]
        left = [(sim, value[1], world.cause_code(value[2]) if value[2] is not None else "emigration") for sim, value in sorted(world.archive.items())]
        births = [(sim, world.born_of(sim) // MONTH) for sim in sorted(set(world.sims) | set(world.archive)) if world.born_of(sim) >= 0]
        partnerships = [
            (n, v[0], v[1], v[2] // MONTH, None if v[3] is None else (v[3][0], wr.ENDED_BY_NAMES[v[3][1]])) for n, v in sorted(world.partnerships.items())
        ]
        rows.append(f"{label} {name} living {len(world.sims)} next_sim {world.next_sim_id} left (sim, second, cause) {left}")
        rows.append(f"{label} {name} born (sim, month) {births}")
        rows.append(f"{label} {name} partnerships (number, woman, man, month formed, ended) {partnerships}")
        rows.append(
            f"{label} {name} emigrations {[(r['year'], [h for h, _ in r['left']]) for r in world.emigrations if r['left']]}"
            f" separations {[(t, number) for t, number, *_ in world.separations]} rounds_with_women {len(world.rounds)} entries {len(world.log)}"
        )
        afflicted = [(sim, [(a.disease.code, a.onset) for a in (world.sims[sim][4] if sim in world.sims else world.archive[sim][6])]) for sim in sorted(set(world.sims) | set(world.archive))]
        rows.append(f"{label} {name} afflictions {[row for row in afflicted if row[1]]}")
        measurement = world.measure()
        text = wr.measurement_json(world.closed, measurement)
        rows.append(
            f"{label} {name} measure traits {len(measurement['traits'])} loci {len(measurement['loci'])} living {measurement['living']}"
            f" json_bytes {len(text.encode())} xxh3 {hexed(hash64(text.encode()))}"
        )
        rows.append(f"{label} {name} measure_json {text}")
    run = w["run"]
    for a, b in KINSHIP_PAIRS:
        phi = run.kinship_of(a, b)
        rows.append(f"{label} kinship sims {a} {b} relation {relation(run, a, b)} phi {phi} json {None if phi is None else wr.json_number(phi)}")
    return rows


def pin_rows(label: str, files: dict) -> list:
    rows = []
    for name, content in files.items():
        extra = ""
        if name.endswith(".json") and "pedigree" not in name and name not in (EXPORT_FILE, CARD_FILE, TOWN_CARD_FILE, HUNGER_FILE):
            golden = json.loads(content)
            extra = (
                f" core_version {golden['core_version']} data_hash {golden['data_hash']} log_hash {golden['log_hash']}"
                f" year_hashes {len(golden['year_hashes'])} first {golden['year_hashes'][0]['hash']} last {golden['year_hashes'][-1]['hash']}"
            )
        rows.append(f"{label} file {name} bytes {len(content)} xxh3 {hexed(hash64(content))}{extra}")
    return rows


def town_rows(world: wr.World) -> list:
    pool = world.data.pool
    start = world.inputs[0]
    persons = sorted(set(world.sims) | set(world.archive))
    rows = [
        f"town config pool_hash {hexed(pool.hash)} pool_seed {hexed(pool.seed)} data_hash {hexed(world.data.hash)} seed {hexed(world.seed)} tag {world.tag}"
        f" n_start {world.pool_start['n_start']} immigration {world.pool_start['immigration']} households_at_start {len(world.pool_start['households'])}",
        f"town start_input bytes {len(start)} xxh3 {hexed(hash64(start))}",
        f"town end living {len(world.sims)} archived {len(world.archive)} persons {len(persons)} arrivals {sum(1 for sim in persons if world.is_arrival(sim))}"
        f" partnerships {len(world.partnerships)} entries {len(world.log)} used {len(world.used)}",
        f"town deaths {sorted({world.cause_code(v[2]) for v in world.archive.values() if v[2] is not None})} emigrated {sum(1 for v in world.archive.values() if v[2] is None)}",
        f"town state_hash {hexed(world.state_hash())} save bytes {len(world.save())} xxh3 {hexed(hash64(world.save()))}",
    ]
    return rows


def story_check() -> list:
    """The story on map 3 (tools/dummy_story.py, its asserts run on import) and the writer's five
    files on map 3 against the story's: the same scenario run two ways."""
    import dummy_story

    story = dummy_story.story_files()
    writer = dummy_files(dummy_worlds(scenario(SCENARIOS / "dummy.json"), wr.MAP3))
    same = {name: writer[name] == story[name] for name in DUMMY_FILES}
    return [f"story map3 data_hash {hexed(wr.MAP3.hash)} the writer's files equal the story's {same}"], all(same.values())


def main(argv) -> int:
    if "--story" in argv:
        import dummy_story

        dummy_story.main()
        return 0
    data = wr.REPOSITORY
    dummy = scenario(SCENARIOS / "dummy.json")
    town = scenario(SCENARIOS / "town.json")
    assert dummy["pool"] is None and town["pool"] is not None and not town["founders"]
    w = dummy_worlds(dummy, data)
    files = dummy_files(w)
    town_run = town_world(town, data)
    hunger = scenario(SCENARIOS / "card_hunger.json")
    files[HUNGER_FILE] = (card_height_json(hunger, data) + "\n").encode()
    files[TOWN_FILE] = golden_text(town_run).encode()
    files[TOWN_CARD_FILE] = (town_card_json(town_run, town["until"]) + "\n").encode()
    rows = [f"data data/ data_hash {hexed(data.hash)} definitions {hexed(data.pool.frozen_hash)} pool {hexed(data.pool.hash)} core_version {wr.CORE_VERSION}"]
    rows += pin_rows("golden", files)
    rows += story_rows("dummy", w)
    rows += town_rows(town_run)
    rows += coverage_rows({"dummy_run": w["run"], "dummy_fork": w["fork"], "arrival": arrival_world(), "town": town_run})
    if "--check" in argv:
        same = {name: (GOLDEN / name).exists() and (GOLDEN / name).read_bytes() == content for name, content in files.items()}
        story, story_ok = story_check()
        for row in rows + story:
            print(row)
        ok = all(same.values()) and story_ok
        print(f"goldens {'match' if ok else 'DIFFER from'} the reference {same}")
        return 0 if ok else 1
    for name, content in files.items():
        (GOLDEN / name).write_bytes(content)
    for row in rows:
        print(row)
    for name in ("run", "fork"):
        print(f"pedigree_json_{name} {w[name].pedigree_json()}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
