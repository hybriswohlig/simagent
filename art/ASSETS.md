# MakeHuman assets

Base mesh, morph targets, and the game-engine skeleton for the style sample.
`build_body.py` reads them from the cache. Nothing in this list is committed.

## Source

| | |
|---|---|
| Repository | https://github.com/makehumancommunity/mpfb2 |
| Local clone | `/Users/dariushk/.cache/simagent-art/mpfb2` |
| Commit | `6ff1eb358f6c1959ced77e79e6dd2406d9a71f49` |
| Commit date | 2026-10-24 14:02:24 +0200 |
| Subject | Merge pull request #459 from makehumancommunity/_placeholder_size |
| Version | No release tag on this commit (`git describe` prints the short hash). The pin is the commit. |

Shallow clone of `master` at that commit:

```
git clone --depth 1 https://github.com/makehumancommunity/mpfb2.git /Users/dariushk/.cache/simagent-art/mpfb2
```

If `master` has moved, check out `6ff1eb358f6c1959ced77e79e6dd2406d9a71f49`.

## License

Every file the sample uses is CC0 1.0 Universal.

| Where | What it says |
|---|---|
| `LICENSE.md` section C | Bundled assets (base mesh and proxies, targets and modifiers, textures, clothes, rigs, poses, expressions, JSON mesh data) are CC0 1.0 Universal. |
| `LICENSE.ASSETS.md` | Full CC0 1.0 legal text for those assets. |
| `weights.game_engine.json` | `"license": "CC0"`, `"version": 110`. |
| MakeHuman community | https://www.makehumancommunity.org/about/license.html — core assets CC0. MPFB code is GPL. MakeHuman code is AGPL. |

The MPFB2 Python addon is GPLv3 (`LICENSE.md` section B, `LICENSE.CODE.md`). The build script does not import it. The addon stays in the cache only because the asset files live in that repository.

## Files used

Paths are relative to `src/mpfb/data/` in the clone.

| File | Role |
|---|---|
| `3dobjs/base.obj` | HM08 base mesh, 19158 vertices. Body vertices are 0..13379. Helpers are removed after shape keys and weights are assigned. |
| `targets/**/*.target.gz` | Morph targets listed in `MAPPING.md`. Text or gzip, `vertex_index dx dy dz`, OBJ space (Y up, decimeters). |

The eight files `macrodetails/universal-{male,female}-{baby,child,young,old}-averagemuscle-averageweight.target.gz` are empty gzip archives (about 76 bytes). They are the neutral corner of the muscle/weight grid and match the base mesh. The build does not load them. Confirmed on this commit and on the raw file at tag `v2.0.18`.

Each `age_*` shape key is the mean of three targets in the same directory, all CC0 in this repository:

- `macrodetails/african-{male,female}-{child,young,old}.target.gz`
- `macrodetails/asian-{male,female}-{child,young,old}.target.gz`
- `macrodetails/caucasian-{male,female}-{child,young,old}.target.gz`

The body contract has no origin field. The three races are weighted equally (1/3 each). MPFB's own default is 0.33, 0.33, 0.33, which sums to 0.99.
| `rigs/standard/rig.game_engine.json` | Skeleton. Rest pose is `default_position` in meters, Z up, before the feet-on-ground shift. |
| `rigs/standard/weights.game_engine.json` | Skin weights, CC0, version 110. |
| `mesh_metadata/hm08_config.json` | `groups_by_range.body` = `[0, 13379]`. |

## Clothes

This run does not download the community system-assets pack. That zip is about 267 MB
(https://files.makehumancommunity.org/asset_packs/makehuman_system_assets/makehuman_system_assets_cc0.zip,
CC0). A fitted proxy would need an `.mhclo` and a second mesh.

The sample instead paints a sleeveless body suit on the CC0 body. A shader band
on the skin material colours the torso and hips `#314a6e` (from mid-thigh to the
top of the chest, stopping before the arms). Head, arms, hands and lower legs
stay skin. Breast shape is still the mesh under that colour. No new asset file
is added.

## Not used

- `rigs/standard/rig.mixamo.json` and `weights.mixamo.json` are in the same directory. This sample does not use Mixamo, and it does not use assets from any game.
- The community system-assets pack is a different download and was not fetched. The pages are https://static.makehumancommunity.org/assets/assetpacks/makehuman_system_assets.html and the zip is https://files.makehumancommunity.org/asset_packs/makehuman_system_assets/makehuman_system_assets_cc0.zip (about 267 MB, CC0). That pack is proxies, eyes, teeth, clothes, skins, and hair. It does not contain this base mesh or these morph targets.
- `macrodetails/height/*` (144 targets) and the breast/proportion grids are left unused. Height is a bone scale. See `MAPPING.md`.
