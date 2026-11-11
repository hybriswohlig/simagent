# Family sample

`art/family.json` is not in this commit. The file for this sample is `/Users/dariushk/.cache/simagent-art/family.json`. The 50-year run of `data/scenarios/stadt.json` has people at every requested age, and it has no grandparent line that holds all of them together.

`simagent export` writes the pedigree only. Bodies are on `simagent view`: `GET /api/runs/{run}/people` for the living, and `GET /api/runs/{run}/persons/{sim}` for `at_ages`. Fixed preview ages are 2, 8, 12, 16, 25, 40 and 60. Ages 10, 13 and 17 have to be someone's current age. `art/family.py` reads that API and writes one JSON file with the seed, the scenario, the sim ids, the ages, and all 44 body values.

## How to run it

Release build, from this worktree:

```
cargo build -p sim-cli --release
```

The binary is `target/release/simagent`. Fifty years of 364 days is 1572480000 seconds. The scenario seed is `000000000000a001`. The output directory must be missing or empty. This run is already in the cache:

```
target/release/simagent run \
    --scenario data/scenarios/stadt.json \
    --out /Users/dariushk/.cache/simagent-art/runs/stadt \
    --until 1572480000
```

`manifest.json` there has `world_seed` `000000000000a001` and `closed_through` 1572480000. `simagent view` only checks that `--web` contains `index.html`. The stub used here is `/Users/dariushk/.cache/simagent-art/view-web/index.html`. Nothing under `web/` was built.

```
target/release/simagent view \
    /Users/dariushk/.cache/simagent-art/runs \
    --web /Users/dariushk/.cache/simagent-art/view-web \
    --port 8765
python3 art/family.py --base http://127.0.0.1:8765 --run stadt --drive \
    --out /Users/dariushk/.cache/simagent-art/family.json
```

`--drive` is required. It keeps the closest line below and writes each child birthday by resuming `runs/stadt-ages` with `simagent run --resume --until`. Without `--drive` the script exits 1. The first search, which asked for every age alive at once, is the section under this.

## What this run contains

593 people are alive. Counts by whole year of age:

| Age | n | Age | n | Age | n | Age | n |
|---|---|---|---|---|---|---|---|
| 0 | 9 | 25 | 7 | 50 | 7 | 75 | 6 |
| 1 | 11 | 26 | 7 | 51 | 9 | 76 | 5 |
| 2 | 9 | 27 | 5 | 52 | 5 | 77 | 4 |
| 3 | 6 | 28 | 6 | 53 | 8 | 78 | 3 |
| 4 | 12 | 29 | 6 | 54 | 11 | 79 | 4 |
| 5 | 8 | 30 | 7 | 55 | 8 | 80 | 3 |
| 6 | 10 | 31 | 9 | 56 | 7 | 81 | 6 |
| 7 | 10 | 32 | 8 | 57 | 5 | 82 | 4 |
| 8 | 6 | 33 | 5 | 58 | 5 | 83 | 3 |
| 9 | 12 | 34 | 5 | 59 | 10 | 84 | 3 |
| 10 | 7 | 35 | 1 | 60 | 9 | 85 | 1 |
| 11 | 7 | 36 | 5 | 61 | 6 | 86 | 3 |
| 12 | 7 | 37 | 4 | 62 | 6 | 87 | 3 |
| 13 | 11 | 38 | 6 | 63 | 3 | 88 | 1 |
| 14 | 7 | 39 | 9 | 64 | 11 | 90 | 1 |
| 15 | 10 | 40 | 7 | 65 | 4 | 91 | 3 |
| 16 | 9 | 41 | 10 | 66 | 8 | 92 | 1 |
| 17 | 5 | 42 | 10 | 67 | 6 | 94 | 2 |
| 18 | 7 | 43 | 4 | 68 | 5 | 96 | 1 |
| 19 | 9 | 44 | 8 | 69 | 12 | 97 | 1 |
| 20 | 5 | 45 | 7 | 70 | 4 | 100 | 1 |
| 21 | 9 | 46 | 7 | 71 | 3 | | |
| 22 | 8 | 47 | 3 | 72 | 5 | | |
| 23 | 9 | 48 | 5 | 73 | 4 | | |
| 24 | 7 | 49 | 8 | 74 | 4 | | |

Of the 176 people aged 30–55, 89 have both a mother id and a father id in the pedigree. 76 of those have both parents still in town. 29 such couples are both aged 55–80. The town starts as 300 adults aged 18–40, each alone, and later households arrive from the pool, so many adults have no parents inside the town.

None of those 29 lines contains a living 10-year-old, a living boy of 13, a living girl of 13, a living boy of 17, and a living girl of 17. The two closest lines have three of those five:

- Grandparents both 69, one child 47. Descendants include both sexes at 13 and a 17-year-old girl. Living ages in that line include 0, 2, 11, 13, 15, 17, 19, 23, 25 and the parents. There is no 10-year-old and no 17-year-old boy.

That first line is the one this sample uses. The same two children are read again from `runs/stadt-ages`, one deterministic continuation stopped with `--until` on each birthday. Falk is not stopped at 17: that second is after the 50-year run.

| Person | Sim | Snapshots (age at `--until`) |
|---|---|---|
| Ulrich Wolff, grandfather | 214 | 69 at 1572480000 |
| Sonja Heinrich, grandmother | 215 | 69 at 1572480000 |
| Hartmut Wolff, father | 328 | 47 at 1572480000 |
| Regina Haupt, mother | 400 | 39 at 1572480000 |
| Nelli Wolff | 602 | 8 at 1287014400, 10 at 1349913600, 12 at 1412812800, 13 at 1444262400, 17 at 1570060800 |
| Falk Wolff | 646 | 8 at 1400716800, 10 at 1463616000, 12 at 1526515200, 13 at 1557964800 |
- Grandparents 68 and 69, one child 44. Descendants include a 10-year-old and both sexes at 13. There is no 17-year-old.

Five people are 17: two boys and three girls. Each boy sits in a line that does not also hold the other four slots. Ages 8 and 12 are present in the town (6 and 7 people) and were not the cause of the miss. The script asks for them only after the living slots match, from a current age or from `at_ages`.

A later `--until` can be passed to the same script. This snapshot stops here.
