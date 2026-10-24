#!/usr/bin/env python3
"""The fork comparison `simagent diff` (ADR-0021 section 3, "Gabelvergleich", and its addenda
of 2026-10-23, "das Format des Gabelvergleichs", and 2026-10-24, "Erkrankungen im Vergleich";
ADR-0001 section 6 with "Grenze": comparisons
never go by SimId), written from the ADRs, independent of the Rust code. The runs come from
tools/world_reference.py on the repository's data as the CLI runs them
(`golden_reference.make_world` on `wr.REPOSITORY`).

`diff(a, b)` is a pure function of two pedigrees (`World.pedigree_view()`, the content of
`pedigree.json` format 4). Per person, by lineage key, a life (`lives`):

- `born`, `died`, `cause_of_death`, `emigrated`: the pedigree's values (seconds, the cause's
  data ID, null where the pedigree has null);
- `partnerships`: per partnership of the person (as woman or man) `{partner, formed, ended,
  ended_by}`, `partner` the other's lineage key, sorted by `formed`, then `partner`;
- `children`: the number of persons of the same pedigree whose mother or father is the
  person. Each parent is the SimId `mother` / `father`; where that is null, the key
  `pool.mother_key` / `pool.father_key` of a person with a pool origin (a child of a pool
  pregnancy, and also an arrival whose pool parents are in the town). A key that names nobody
  of the pedigree counts for nobody;
- `diseases`: the hereditary diseases of the pedigree's person that show (`affected`), in the
  pedigree's order (that of the data), each `{disease, year, age_years}`: its code, the
  calendar year of `B = born + ⌈onset · YEAR⌉` (`wr.onset_second`) and `⌊onset⌋`, the numbers
  of the card and of the entry `core:disease_onset` (ADR-0017 section 3). A disease that
  rests is the same in both runs and no part of a life.

The comparison, format 2: `{"format_version":2,"a_only":[…],"b_only":[…],"changed":[{"lineage_key",
"differs","a","b"}]}`, compact as serde_json writes it (`wr.json_text`). `a_only` and `b_only`
are the keys of one pedigree only, `changed` the keys of both whose lives differ, each
ascending by key (16 lowercase hex digits, so the string order is the u64 order); `differs`
names the differing fields in the order of the life; `a` and `b` are the arguments in order,
not parent and fork. No SimId and no partnership ID reaches the output. Runs on different
data never meet here: `simagent diff` loads both saves with the program's data, and a save of
other data does not load (`save_rejected`, ADR-0003 1.4).

The scenario tests/scenarios/diff_switch.json (format 5, founder tag `test:diff_switch`, seed
1, no capacity, no pool, the switch of the hereditary diseases off at the start; until the last
second of year 34) runs three times: the parent `a` to `until`; the fork `b`, a fresh run to
`T_X − 1 s` with `X = FORK_YEAR` = 5, the override "hereditary diseases on" stamped there (it
acts from `T_5`), then to `until` (as `golden_reference.dummy_worlds`, without the shock); the
fork `b0` without an override (to `T_5 − 1 s`, then to `until`). Checked: the year hashes 0
to 4 and every log entry before `T_5` of `b` and `b0` are those of `a`, the year hash of 5
differs in `b`, `b0` is `a` in full; every founder and every person conceived before `T_5` is
in all three pedigrees; `diff(a, b0)` and `diff(b0, a)` are empty; `diff(b, a)` is `diff(a, b)`
with the sides swapped.

The design of the scenario (by hand, from the draws; `core:disease_ar` is recessive with
penetrance 1, onset 0 and the step hazard 1/900 per year before 18, 0.014322… after). With the
switch off, the founders' alleles at `core:disease_ar` change nothing of run `a` (the disease
only kills, the meiosis draws do not read the alleles), so they can be chosen after the run:

- Founder 2 (a woman of 19, key 935f6cce0d166101) is homozygous; her disease budget is 0.1848,
  the smallest of founders 0 to 9 under this tag. From `T_5` (she is 24) she dies at
  `T_5 + 0.1848 / 0.014322… years`, in year 17. In `a` her second partnership (with founder 3,
  formed in year 5) ends by separation in year 27; in `b` by her death.
- The first child of founders 2 and 1 (key 5e51a4f2e3aba5e9 = `child(founder 2, founder 1, 0,
  0)`, budget 0.0116, the smallest of a first child of any two of the founders 0 to 7; a child's
  key does not depend on time, so it is known before the run) is homozygous because both parents
  are (founder 1, budget 0.855, would die of it only after `until`). Exposed from `T_5` at the age of 4.08, he uses up 0.0116 at 1/900 per year after
  10.45 years and dies at 14.5, before the market age. In `a` he pairs in year 28 and fathers
  fraternal twins (keys only in `a`).
- His partner of `a` (830458…, a daughter of founders 0 and 3) pairs in `b` in the same round
  with his younger brother (c1abb0…) and has one child with him (a key only in `b`); so her
  `partnerships` and `children` differ, and his.
- The second child of founders 0 and 3 (cbd12a…, a son, budget 0.0614) got the allele 1 of
  both carriers (his mother's maternal copy, his father's paternal copy); in `b` he dies at 21,
  in year 23.
- Founder 3's partnership with founder 2 ends differently, so his `partnerships` differ.

Every disease death of `b` is checked against the closed form (`hand_death`): `T_5 +
ceil((x − x0) · YEAR)` with `x0` the age at `T_5` and `x` the age where the budget is used up
from `x0` (3.1).

Usage: $HOME/.cache/simagent-tools-venv/bin/python diff_reference.py [--write]   (from tools/;
with --write the line of `diff(a, b)` goes to tests/golden/diff_switch.json)
"""

import math
import sys
import time

import golden_reference as gr
import rng_reference as rng
import world_reference as wr
from world_reference import YEAR

SCENARIO = gr.SCENARIOS / "diff_switch.json"
# The fork year: the override acts at T_X = X · YEAR.
FORK_YEAR = 5
FORMAT_VERSION = 2
LIFE_FIELDS = ("born", "died", "cause_of_death", "emigrated", "partnerships", "children", "diseases")
DISEASE = "core:disease_ar"


def key_value(key: str) -> int:
    """A lineage key of the pedigree (16 lowercase hex digits) as its u64."""
    assert len(key) == 16 and all(char in "0123456789abcdef" for char in key), key
    return int(key, 16)


def lives(view: dict) -> dict:
    """The life of every person of a pedigree, by lineage key (in the order of the persons)."""
    persons = view["persons"]
    key_of = {person["sim"]: person["lineage_key"] for person in persons}
    assert len(set(key_of.values())) == len(key_of), "lineage_key_unique"
    children = {key: 0 for key in key_of.values()}
    for person in persons:
        pool = person["pool"]
        for side in ("mother", "father"):
            parent = key_of[person[side]] if person[side] is not None else None
            if parent is None and pool is not None:
                parent = pool[f"{side}_key"]
            if parent is not None and parent in children:
                children[parent] += 1
    partnerships = {key: [] for key in key_of.values()}
    for partnership in view["partnerships"]:
        woman, man = key_of[partnership["woman"]], key_of[partnership["man"]]
        for own, other in ((woman, man), (man, woman)):
            partnerships[own].append(
                {"partner": other, "formed": partnership["formed"], "ended": partnership["ended"], "ended_by": partnership["ended_by"]}
            )
    out = {}
    for person in persons:
        key = person["lineage_key"]
        out[key] = {
            "born": person["born"],
            "died": person["died"],
            "cause_of_death": person["cause_of_death"],
            "emigrated": person["emigrated"],
            "partnerships": sorted(partnerships[key], key=lambda item: (item["formed"], key_value(item["partner"]))),
            "children": children[key],
            "diseases": [
                {"disease": shown["disease"], "year": wr.onset_second(person["born"], shown["onset"]) // wr.YEAR, "age_years": math.floor(shown["onset"])}
                for shown in person["diseases"]
                if shown["affected"]
            ],
        }
    return out


def diff(a: dict, b: dict) -> dict:
    """The comparison of the pedigrees `a` and `b`, format 2, keys in the order of the format."""
    lives_a, lives_b = lives(a), lives(b)
    changed = []
    for key in sorted(set(lives_a) & set(lives_b), key=key_value):
        differs = [field for field in LIFE_FIELDS if lives_a[key][field] != lives_b[key][field]]
        if differs:
            changed.append({"lineage_key": key, "differs": differs, "a": lives_a[key], "b": lives_b[key]})
    return {
        "format_version": FORMAT_VERSION,
        "a_only": sorted(set(lives_a) - set(lives_b), key=key_value),
        "b_only": sorted(set(lives_b) - set(lives_a), key=key_value),
        "changed": changed,
    }


def diff_runs(a: wr.World, b: wr.World) -> str:
    """The line `simagent diff` prints for two runs of the same data (without its newline)."""
    assert a.data.hash == b.data.hash, "a save of other data does not load"
    return wr.json_text(diff(a.pedigree_view(), b.pedigree_view()))


def runs(s: dict, data: wr.Data, x: int) -> dict:
    """The parent `a`, the fork `b` (override "on" stamped `T_X − 1 s`) and the fork `b0`
    without an override, each to `until`, every load check on each."""
    a = gr.make_world(s, data)
    a.advance(s["until"])
    b = gr.make_world(s, data)
    b.advance(x * YEAR - 1)
    b.override(True)
    b.advance(s["until"])
    b0 = gr.make_world(s, data)
    b0.advance(x * YEAR - 1)
    b0.advance(s["until"])
    for world in (a, b, b0):
        gr.check_all(world)
    return {"a": a, "b": b, "b0": b0}


def entries_before(world: wr.World, t: int) -> list:
    """The log entries with a second before `t`, in log order."""
    return [line for line in world.log if gr.wr_entry_head(line)[1] < t]


def conceived_before(world: wr.World, t: int) -> set:
    """The lineage keys of the children conceived before `t` (a run without a pool)."""
    return {f"{key:016x}" for record in world.multiples if record[0] < t for key, _ in record[6]}


def check_fork(s: dict, w: dict, x: int) -> list:
    """The checks of the fork point (ADR-0001 section 6), as rows."""
    a, b, b0 = w["a"], w["b"], w["b0"]
    t_x = x * YEAR
    assert a.year_hashes[:x] == b.year_hashes[:x] == b0.year_hashes[:x], "fork_diverged_before_fork_point"
    assert a.year_hashes[x] != b.year_hashes[x], "the override changes the year hash of X"
    assert a.year_hashes == b0.year_hashes and a.log_hash() == b0.log_hash(), "a fork without an override is the original"
    rows = [
        f"check year_hashes 0..={x - 1} equal in a, b, b0 (last {gr.hexed(a.year_hashes[x - 1])}); year {x} a {gr.hexed(a.year_hashes[x])}"
        f" b {gr.hexed(b.year_hashes[x])}; b0 equals a (all {len(a.year_hashes)} year hashes, log hash {gr.hexed(a.log_hash())})"
    ]
    before = entries_before(a, t_x)
    assert before == a.log[: len(before)] == b.log[: len(before)] == b0.log[: len(before)]
    assert entries_before(b, t_x) == before == entries_before(b0, t_x)
    rows.append(f"check the {len(before)} log entries before T_{x} = {t_x} are equal in a, b, b0 (bytes)")
    founders = {f"{rng.founder_key(s['tag'], index):016x}" for index in range(len(s["founders"]))}
    early = conceived_before(a, t_x)
    assert conceived_before(b, t_x) == early == conceived_before(b0, t_x)
    keys = {name: {person["lineage_key"] for person in world.pedigree_view()["persons"]} for name, world in w.items()}
    assert all(founders | early <= keys[name] for name in keys), "every founder and every person conceived before T_X is in each run"
    rows.append(f"check the {len(founders)} founders {sorted(founders)} and the {len(early)} persons conceived before T_{x} {sorted(early)} are in a, b and b0")
    return rows


def hand_death(person: dict, data: wr.Data, seed: int, x: int) -> int:
    """The second a homozygous person alive at `T_X` dies of `core:disease_ar` once the switch
    is on from `T_X` (onset 0, nothing used before: the switch was off since the start)."""
    (disease,) = [d for d in data.diseases if d.code == DISEASE]
    t_x = x * YEAR
    x0 = wr.as_years(t_x - person["born"])
    budget = wr.budget_of(seed, key_value(person["lineage_key"]), DISEASE)
    age = wr.disease_age(x0, budget, disease.hazard)
    return t_x + math.ceil((age - x0) * YEAR)


def readable(d: dict) -> list:
    """What a comparison says, one row per key and field."""
    rows = [f"a_only {key}" for key in d["a_only"]] + [f"b_only {key}" for key in d["b_only"]]
    for item in d["changed"]:
        for field in item["differs"]:
            rows.append(f"changed {item['lineage_key']} {field}: a {wr.json_text(item['a'][field])} b {wr.json_text(item['b'][field])}")
    return rows


# The line of `diff(a, b)` with its newline, as `simagent diff` prints it.
GOLDEN = wr.ROOT / "tests" / "golden" / "diff_switch.json"


def main(argv) -> int:
    s = gr.scenario(SCENARIO)
    assert s["pool"] is None and s["capacity"] is None and not s["switch"] and len(s["founders"]) == 4
    mark = time.time()
    data = wr.REPOSITORY
    print(f"time data {time.time() - mark:.1f} s")
    mark = time.time()
    w = runs(s, data, FORK_YEAR)
    print(f"time runs a, b, b0 to until {s['until']} (fork year {FORK_YEAR}) {time.time() - mark:.1f} s")
    for row in check_fork(s, w, FORK_YEAR):
        print(row)
    views = {name: world.pedigree_view() for name, world in w.items()}
    for person in views["a"]["persons"]:
        diseases = [item["disease"] for item in person["diseases"]]
        budget = wr.budget_of(s["seed"], key_value(person["lineage_key"]), DISEASE)
        print(f"person a {person['lineage_key']} sim {person['sim']} {person['sex']} born {person['born']} mother {person['mother']} father {person['father']} diseases {diseases} disease_ar budget {budget!r}")
    for person in views["b"]["persons"]:
        if person["cause_of_death"] == DISEASE:
            hand = hand_death(person, data, s["seed"], FORK_YEAR)
            assert person["died"] == hand, (person["lineage_key"], person["died"], hand)
            print(f"death b {person['lineage_key']} {DISEASE} at {person['died']} (year {person['died'] // YEAR}, age {wr.as_years(person['died'] - person['born'])!r}) equals the closed form")
    ab, ba, ab0 = diff(views["a"], views["b"]), diff(views["b"], views["a"]), diff(views["a"], views["b0"])
    empty = {"format_version": FORMAT_VERSION, "a_only": [], "b_only": [], "changed": []}
    assert ab0 == empty and diff(views["b0"], views["a"]) == empty
    assert ba["a_only"] == ab["b_only"] and ba["b_only"] == ab["a_only"]
    assert ba["changed"] == [{"lineage_key": item["lineage_key"], "differs": item["differs"], "a": item["b"], "b": item["a"]} for item in ab["changed"]]
    assert ab["a_only"] and ab["b_only"] and {"died", "cause_of_death", "partnerships", "children"} <= {f for item in ab["changed"] for f in item["differs"]}
    text = diff_runs(w["a"], w["b"])
    assert text == wr.json_text(ab) and " " not in text
    print(f"diff(a, b) {text}")
    print(f"diff(b, a) {diff_runs(w['b'], w['a'])}")
    print(f"diff(a, b0) {diff_runs(w['a'], w['b0'])}")
    for row in readable(ab):
        print(f"readable diff(a, b) {row}")
    if "--write" in argv[1:]:
        GOLDEN.write_text(text + "\n")
        print(f"wrote {GOLDEN.relative_to(wr.ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
