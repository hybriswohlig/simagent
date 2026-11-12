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

The navy shader band is gone. Each person wears a MakeHuman proxy suit and a
pair of shoes from the same CC0 system-assets zip as the hair. `pack_proxies.py`
fits them with the `.mhclo` bindings, so the cloth follows the morphed body.
The page recolours the suit from a muted palette and keeps the ambient-occlusion
weave. Printed logos are not shown. Shoes keep their diffuse texture.

Every clothes item in that zip was opened. All twenty carry the same header:
released as CC0 in September 2020 by Data Collection AB, Joel Palmius and
Jonas Hauquier. Source page:
https://static.makehumancommunity.org/assets/assetpacks/makehuman_system_assets.html
Zip:
https://files.makehumancommunity.org/asset_packs/makehuman_system_assets/makehuman_system_assets_cc0.zip

| Key | Path in the zip | Worn as |
|---|---|---|
| `cloth_shirt` | `clothes/male_casualsuit01/` | Button shirt and trousers. |
| `cloth_jacket` | `clothes/male_casualsuit05/` | Jacket over a vest, and trousers. |
| `cloth_tee` | `clothes/male_casualsuit06/` | Packed. Sleeveless, so the wardrobe does not assign it. |
| `cloth_f_tee` | `clothes/female_casualsuit01/` | Short-sleeve t-shirt and trousers. Children use this for trousers. |
| `cloth_shorts` | `clothes/female_casualsuit02/` | T-shirt and shorts. One child outfit. |
| `cloth_skirt` | `clothes/female_elegantsuit01/` | Top and skirt. |
| `shoes_brown` | `clothes/shoes01/` | Brown shoes. |
| `shoes_black` | `clothes/shoes04/` | Black shoes. |
| `shoes_sneaker` | `clothes/shoes06/` | Sneakers. Every child. |

Adults get a t-shirt, a shirt or a jacket, plus trousers or a skirt, plus shoes.
Children of 13 and under get a t-shirt with trousers or shorts, and sneakers.
The choice is a hash of the sim id. The same person keeps the same outfit.

Inspected, CC0, and not worn: `fedora01`, `fedora_cocked`, `female_sportsuit01`,
`male_casualsuit02`, `male_casualsuit03`, `male_casualsuit04`,
`male_elegantsuit01`, `male_worksuit01`, `shoes02`, `shoes03`, `shoes05`.

## Studio light

Style A uses the Poly Haven HDRI `studio_small_03` by Greg Zaal as image-based
lighting only. It is not the room and it is not drawn as the background.

| | |
|---|---|
| Licence | CC0. https://polyhaven.com/license |
| Asset | https://polyhaven.com/a/studio_small_03 |
| File | https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/1k/studio_small_03_1k.hdr |
| Local | `/Users/dariushk/.cache/simagent-art/hdri/studio_small_03_1k.hdr` |
| Size | 1 686 299 bytes |
| MD5 | `74e6ef69ea9024c2cc25b3a7de8ec2f7` |

## Skin

The face and body use the MakeHuman system-assets default light skin. The
material is CC0, released September 2020, same pack as the proxies above.

| | |
|---|---|
| Material | `skins/young_caucasian_female/young_caucasian_female.mhmat` |
| Diffuse | `skins/young_caucasian_female/young_lightskinned_female_diffuse.png` |
| Local file | `/Users/dariushk/.cache/simagent-art/mh-system/skins/young_caucasian_female/young_lightskinned_female_diffuse.png` |

The page divides the texture by its average linear colour
`(0.617, 0.357, 0.236)` and multiplies by the contract skin colour, so
melanin still sets the tone. The mix is partial. Near-black paint in that
albedo (the drawn eyes, brows and nostrils) is lifted, so it does not
blotch the face under the eye and brow proxies. Pores stay. Lips and cheeks
pick up a little extra warmth where the texture is already redder.
Roughness is 0.8, with no metal. The body is a glTF mesh, so this texture
is sampled with v = 0 at the top. The mhmat points at the litsphere
`data/litspheres/lit_standard_skin.png`,
which is not in this pack. `shaderConfig normal` is false, and the directory
has no normal or bump image, so the sample has no skin normal map.

## Eyes, brows, lashes and hair

These proxies come from the MakeHuman community system-assets pack. The zip is
CC0, about 267 MB:

https://files.makehumancommunity.org/asset_packs/makehuman_system_assets/makehuman_system_assets_cc0.zip

The page listing each asset as CC0 is
https://static.makehumancommunity.org/assets/assetpacks/makehuman_system_assets.html

Local zip: `/Users/dariushk/.cache/simagent-art/makehuman_system_assets_cc0.zip`.
`art/pack_proxies.py` reads the files below and writes
`/Users/dariushk/.cache/simagent-art/proxies/proxies.json` and `proxies.bin`.
The page fits each proxy with its `.mhclo` bindings. Textures use alpha test.

| Asset | Path in the zip | Use |
|---|---|---|
| Low-poly eyes | `eyes/low-poly/low-poly.mhclo`, `low-poly.obj` | Both eyeballs. Vertices match the HM08 eye helpers. |
| Brown eye texture | `eyes/materials/brown_eye.png`, `brown.mhmat` | Iris fibre luminance and the pupil position. The page tints that neutral field with the contract iris colour. The sclera is drawn off-white, darker toward the corners. The grey field outside the sclera disc is replaced with the contract skin colour, because that field was showing as a grey socket. |
| eyebrow001 | `eyebrows/eyebrow001/` | Brow cards. Tinted with the hair colour. Opacity follows `eyebrow_density`. |
| eyelashes01 | `eyelashes/eyelashes01/` | Lash cards on the same eye helpers. |
| short04 | `hair/short04/` | Short cut. A man with `balding_stage` of 1 or more is given this cut, so the crown and the temples can open. |
| bob01 | `hair/bob01/` | Medium bob. |
| long01 | `hair/long01/` | Long hair. |
| ponytail01 | `hair/ponytail01/` | Hair tied back. |
| braid01 | `hair/braid01/` | A braid, also tied. |

Length is not a gene and not an age. The page hashes the sim id (FNV-1a)
and picks one of these five. The same person keeps the same cut at every
age. Colour, curl, density, grey and balding still come from the contract.
Curl is stored and is not a strand shape. The editor's Frisur menu and
gefärbt colour replace the view only.

These five were already in the CC0 system-assets zip above. Nothing else
was downloaded. `hair/short02/` was the old child cut and is not used.

Hair colour is the contract hair melanin mixed with `hair_grey_fraction`.
`hair_density` lowers the alpha-test coverage. There is no beard proxy in
this pack, and the system-assets zip has no beard texture. The page paints
one from the neutral CC0 body (`body.glb`) into
`/Users/dariushk/.cache/simagent-art/beard-mask.png`. Weight 1 is the
mustache and the chin. Lower weights are the jaw, then the cheeks.
`beard_coverage` decides how far that shadow spreads, and `beard_density`
sets its opacity. The colour is the hair colour, darkened so grey hair
still reads on pale skin. Lips, the nose and the neck stay 0. No new
downloaded asset.

## Not used

- `rigs/standard/rig.mixamo.json` and `weights.mixamo.json` are in the same directory. This sample does not use Mixamo, and it does not use assets from any game.
- The system-assets zip also contains other skins, teeth and other hair.
  This sample uses the eye, brow, lash and five hair proxies in the table
  above, the young caucasian female skin, and the clothes listed in the
  clothes section.
- The zip does not contain this base mesh or these morph targets. Those stay
  the MPFB2 files at the top of this note.
- `macrodetails/height/*` (144 targets) and the breast/proportion grids are left unused. Height is a bone scale. See `MAPPING.md`.
