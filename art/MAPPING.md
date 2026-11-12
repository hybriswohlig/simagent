# Render mapping (proposal)

Contract `contracts/body.json` 0.1.1 has 44 parameters. This file is the only place
the face axes get a meaning, and the only place a contract value becomes a
shape-key weight. The glb stores every key at 0. `build_body.py` writes it to
the cache, not the repository.

59 shape keys. Sixteen of them are provisional axes for the direct editor
and are not in the body contract. The skeleton is the MakeHuman
game-engine rig (53 bones).
`height_cm` is not a shape key.

## What was measured

Blender 5.2.1, assets at commit `6ff1eb358f6c1959ced77e79e6dd2406d9a71f49`.
Standing height is the Z span of that shape key after the basis feet sit on
Z = 0. The age keys themselves sit a few millimetres below that ground,
because the shift used the basis.

| Shape key at weight 1 | Standing height (cm) |
|---|---|
| Basis, no morph | 166.59 |
| `age_male_child` | 135.74 |
| `age_male_young` | 173.04 |
| `age_male_old` | 170.00 |
| `age_female_child` | 126.13 |
| `age_female_young` | 158.98 |
| `age_female_old` | 157.65 |

Mesh after helper deletion: 13380 vertices, 13378 polygons. `ball_l` rest
head is about 8 mm above the ground. The glTF exporter keeps the four
strongest joint influences per vertex and normalizes them.

## Age, sex, and the two puberty branches

The files `universal-{male,female}-{child,young,old}-averagemuscle-averageweight`
are empty. They are the neutral corner of the muscle/weight grid and match
the base mesh. Each `age_*` key is the mean of the three race targets
`macrodetails/{african,asian,caucasian}-{sex}-{age}`. The contract has no
origin field, so the three weights are 1/3. MPFB's default is 0.33 each,
which sums to 0.99. Baby is omitted. The sample starts at 8.

These six keys are the androgenic and estrogenic branches. There is no
seventh sex morph. The current core writes the same number into
`body_form_age` and `face_form_age`: biological age, capped at 130
(`crates/sim-core/src/body_contract/mapping.rs`).

Age tent, in years. `child + young + old = 1`.

| `body_form_age` | child | young | old |
|---|---|---|---|
| ≤ 12 | 1 | 0 | 0 |
| 12 to 25 | (25 − age) / 13 | (age − 12) / 13 | 0 |
| 25 to 70 | 0 | (70 − age) / 45 | (age − 25) / 45 |
| > 70 | 0 | 0 | 1 |

Let `A` be `androgenic_development` and `E` be `estrogenic_development`.
For each age slice `s`:

```
weight(age_male_*)   = s * (0.5 + 0.5 * A - 0.5 * E)
weight(age_female_*) = s * (0.5 - 0.5 * A + 0.5 * E)
```

The two weights sum to `s`. Checks: `(A, E) = (1, 0)` is the male key,
`(0, 1)` is the female key, and `(0, 0)`, `(0.5, 0.5)`, and `(1, 1)` are
the average of the two. That is the additive blend in ADR-0005: a neutral
stem plus `A` times the androgenic delta plus `E` times the estrogenic
delta, not a pair of weights forced to sum to 1 beyond that identity.
Before puberty both weights are near 0, so the shape is the stem (the
average of the male and female keys at that age).

## Fat and muscle

The universal weight and muscle files are small deltas on the base mesh,
not a second copy of the young body. Measured on this mesh, weight 1
moves the largest male waist vertices 2.9 cm sideways, and much less on
the shoulders. The muscle file's largest vertices drop the shoulder
about 4 cm. They are added on top of the age blend, on both sexes in
proportion to the male and female shares. Child and old use the same
deltas. The age key is not reduced.

`fu = clamp(body_fat_z / 2, -1, 1)`, `mu = clamp(muscle_z / 2, -1, 1)`,
`du = clamp(fat_distribution_z / 2, -1, 1)`.
`heavy = max(fu, 0)`, `thin = max(-fu, 0)`,
`bulk = max(mu, 0)`, `soft = max(-mu, 0)`,
`belly = max(du, 0)`, `hips = max(-du, 0)`.

Let `s` be the male or female share from the age section.

```
fat_*_max    = s * heavy * 1.5
fat_*_min    = s * thin * 1.5
muscle_*_max = s * bulk * 0.45
muscle_*_min = s * soft * 0.45
```

At `body_fat_z = +2` the full-body weight target stays near 1.5, about
4 cm on that waist vertex. A higher weight folded the waist through the
clothes and left a shelf under the arms. The waist, thigh and upper-arm
targets carry the visible mass. `muscle_z = +2` keeps the full-body
muscle target near 0.45. Its largest vertices drop the shoulder about
4 cm at weight 1, and a higher weight left that drop as a shelf over
the sleeve. The arm, shoulder and chest targets carry the bulk.

Distribution shifts where the extra sits. Positive adds waist and a
small belly. Negative adds thigh and a little more upper arm. Every
heavy body still gains some of each, and a thin body pulls the same
regions back. The belly file is a pregnancy sculpt (about 10 cm forward
at weight 1), so its weight stays a small fraction and it is zero unless
the fat is both heavy and central.

```
fat_central     = heavy * (1.6 + 0.8 * belly) - thin * (1.15 + 0.4 * belly)
fat_belly       = heavy * belly * 0.25
fat_peripheral  = heavy * (1.15 + 0.85 * hips) - thin * (0.85 + 0.45 * hips)
fat_upperarm    = heavy * (2.1 + 0.6 * hips) - thin * (1.5 + 0.4 * hips)
muscle_arm      = bulk * 1.8 - soft * 1.1
muscle_shoulder = bulk * 0.55 - soft * 0.35
muscle_chest    = bulk * 1.6 - soft * 0.5
```

## Direct shape keys

On the lineup every key in this table is `z / 4`. `?edit=2` keeps that
quarter for the unlisted axes and uses a stronger weight where ±2 has to
read at full-body distance. ±3 is one and a half times the ±2 weight. A
negative weight mirrors the incr sculpt. That negative side is an
approximation, because the targets are one-sided.

| Key | Contract | Targets, under `targets/` | Weight |
|---|---|---|---|
| `leg_upper` | `leg_ratio_z` | `legs/measure-upperleg-height-incr` | `z / 4`. Positive is longer. |
| `leg_lower` | `leg_ratio_z` | `legs/measure-lowerleg-height-incr` | Same weight as `leg_upper`. |
| `head_size` | `head_size_z` | `head/head-scale-horiz-incr` + `head-scale-vert-incr` + `head-scale-depth-incr` | `z / 4` |
| `shoulder_width` | `shoulder_width_z` | `torso/measure-shoulder-dist-incr` | Lineup `z / 4`. Editor `z`. `torso-vshape-incr` is not used. |
| `chest_depth` | `chest_depth_z` | `torso/torso-scale-depth-incr` | `z / 4` |
| `pelvis_width` | `hip_width_z` | `hip/hip-scale-horiz-incr` | Lineup `z / 4`. Editor `z / 2`. The skirt hem copies this body delta. |
| `fat_central` | `body_fat_z`, `fat_distribution_z` | `torso/measure-waist-circ-incr` | See fat section. Positive distribution adds waist. |
| `fat_peripheral` | `body_fat_z`, `fat_distribution_z` | `legs/l-upperleg-fat-incr` + `r-upperleg-fat-incr` | See fat section. Negative distribution adds thigh. |
| `fat_belly` | `body_fat_z`, `fat_distribution_z` | `stomach/stomach-pregnant-incr` | `heavy * belly * 0.25`. Central fat only. |
| `fat_upperarm` | `body_fat_z`, `fat_distribution_z` | `arms/l-upperarm-fat-incr` + `r-upperarm-fat-incr` | See fat section. |
| `muscle_arm` | `muscle_z` | `arms/l-upperarm-muscle-incr` + `r-upperarm-muscle-incr` | See fat section. |
| `muscle_shoulder` | `muscle_z` | `arms/l-upperarm-shoulder-muscle-incr` + `r-upperarm-shoulder-muscle-incr` | See fat section. |
| `muscle_chest` | `muscle_z` | `torso/torso-muscle-pectoral-incr` | See fat section. |

`hip-scale-horiz-incr` is pelvis width only. It is not reused for fat
distribution. `torso/measure-hips-circ-incr` is unused.

## Gender axis

MakeHuman `macrodetails/Gender` is 0 for female and 1 for male. There is
no separate gender target file. The axis is the male share below, which is
also the weight on the male age key. The recorded sex has to agree with
that share: a male's share stays above 0.5 once androgenic development
passes estrogenic development, and the reverse for a female. While both
developments are near 0, both sexes sit on 0.5. Children of 8 are that
case, so their shapes barely differ.

```
gender = male share = 0.5 + 0.5 * A - 0.5 * E
```

## Breast

The child female corner is flat-chested. The young and old female corners,
including their fat and muscle keys, carry MakeHuman's average cup.
`breast/female-young-...-mincup` is the delta that removes that one cup.
`breast/female-young-...-maxcup` adds the max cup on top of the average.
Copying a male age delta onto breast vertices tears the torso, because
those vertices also move with the whole sex difference. The cup is
cancelled with the mincup key instead.

The bust is not a function of sex or of `estrogenic_development`. Sex
only picks the body corner. `breast_min` removes the baked average cup.
`breast_max` is the only bust shape: a localized forward delta. `0` leaves
the flat base, about `0.17` is a bud, `0.5` is one average cup (0.34 of
the max-cup target), and `1` is the full max-cup target. A male keeps
both weights at 0.

`youngFemale` is `age_female_young`. `oldFemale` is `age_female_old`.
Fat and muscle are added on top of the age blend, so they are not part
of that sum. The child key is not in it, because that corner is already
flat.

```
cup = clamp(breast_development / 0.5, 0, 1)
weight(breast_min) = youngFemale + oldFemale          # 0 on a male
weight(breast_max) = cup * 0.34 + max(0, breast_development - 0.5) * 1.32
```

`breast_max` is 0.34 at cup 1, and 1 at `breast_development` 1.

## Face maturity

The age keys already include the head. `head/head-age-decr` and
`head/head-age-incr` are only for the case where the two form ages differ.
While `face_form_age == body_form_age`, both weights stay 0. That is every
person the current core emits.

```
face_age_decr = clamp((body_form_age - face_form_age) / 20, 0, 1)
face_age_incr = clamp((face_form_age - body_form_age) / 45, 0, 1)
```

## Face axes

Proposal for this sample. Positive weight is `z / 4`, except nose width
and cheek fullness, which are `z / 2` so ±2 is readable at face zoom.
Bilateral files are one key.

| Axis | Proposal | Targets, under `targets/` |
|---|---|---|
| `face_axis_01` | Nose width. Lineup `z / 4`, editor `z`. | `nose/nose-scale-horiz-incr` |
| `face_axis_02` | Nose projection | `nose/nose-scale-depth-incr` |
| `face_axis_03` | Nose length | `nose/nose-scale-vert-incr` |
| `face_axis_04` | Jaw width | `chin/chin-width-incr` |
| `face_axis_05` | Chin height | `chin/chin-height-incr` |
| `face_axis_06` | Jaw prominence | `chin/chin-prognathism-incr` |
| `face_axis_07` | Cheek fullness. Lineup `z / 4`, editor `z / 2`. | `cheek/l-cheek-volume-incr` + `r-cheek-volume-incr` |
| `face_axis_08` | Cheekbones | `cheek/l-cheek-bones-incr` + `r-cheek-bones-incr` |
| `face_axis_09` | Eye size | `eyes/l-eye-scale-incr` + `r-eye-scale-incr` |
| `face_axis_10` | Eye spacing | `eyes/l-eye-trans-out` + `r-eye-trans-out` |
| `face_axis_11` | Lip fullness | `mouth/mouth-upperlip-volume-incr` + `mouth-lowerlip-volume-incr` |
| `face_axis_12` | Mouth width | `mouth/mouth-scale-horiz-incr` |

This is not the unofficial list in ADR-0005 (width, lower face, jaw angle,
chin, cheekbones, eye spacing, eye size, brow, nose length, nose width,
mouth, ears). Ears are not one of the twelve. The forehead target is a
provisional key below, not one of these twelve.

## Provisional axes

Decision of 2026-11-12 (`docs/entscheidung-koerperachsen-v1.md`): Kimi's
seven axes, plus the nine from the addendum. The core does not store them
yet. `?edit=2` keeps them in page state only. Nothing is written into
`family.json` or the contract. On the lineup they stay at weight 0. In
`?edit=2` the default is still `z / 4`. Buttock volume is `z` (about 6 cm
at ±2). Waist width is `z / 2` on the body; the cloth fit multiplies only
the X delta by 4, because the file is mostly belly depth. Breast volume is
`z * 0.75 * breast_development`. Breast height is
`z * 1.5 * clamp(breast_development / 0.5, 0, 1)`. Nose bridge and nose
tip are `z * 1.35`. A negative weight mirrors the incr sculpt. Every file
is a CC0 MakeHuman target from the pinned mpfb2 commit.

These are stand-ins, not a finished genetic mapping. `waist_width` reuses
the waist-circumference sculpt that `fat_central` already uses, as its own
key, so a waist drag is not the same number as body fat. `breast_volume`
reuses the max-cup sculpt as its own key. There is no glute-only lift
file, so `buttock_lift` is the hip translation upward. `lip_ratio` adds
the upper-lip volume sculpt and subtracts the lower-lip one.

`breast_volume` and `breast_height` are multiplied by `breast_development`
and forced to 0 for a male, before puberty (`body_form_age` ≤ 12), or when
`breast_development` is below 0.12. The other provisional keys have no sex
or age gate in this sample.

| Key | Axis | Targets, under `targets/` | Positive means |
|---|---|---|---|
| `neck_length` | `neck_length_z` | `neck/measure-neck-height-incr` | longer neck |
| `hand_size` | `hand_size_z` | `hands/l-hand-scale-incr` + `r-hand-scale-incr` | larger hands |
| `foot_size` | `foot_size_z` | `feet/l-foot-scale-incr` + `r-foot-scale-incr` | larger feet |
| `ear_size` | `ear_size_z` | `ears/l-ear-scale-incr` + `r-ear-scale-incr` | larger ears |
| `ear_projection` | `ear_projection_z` | `ears/l-ear-flap-incr` + `r-ear-flap-incr` | ears stick out |
| `eye_roundness` | `eye_roundness_z` | `eyes/l-eye-height2-incr` + `r-eye-height2-incr` | rounder eye opening |
| `forehead_height` | `forehead_height_z` | `forehead/forehead-scale-vert-incr` | taller forehead |
| `buttock_volume` | `buttock_volume_z` | `buttocks/buttocks-volume-incr` | fuller buttocks, editor `z`. The skirt seat follows it. |
| `buttock_lift` | `buttock_lift_z` | `hip/hip-trans-up` | higher buttocks |
| `waist_width` | `waist_width_z` | `torso/measure-waist-circ-incr` | wider waist. Editor body `z / 2`; cloth X is ×4. |
| `breast_volume` | `breast_volume_z` | `breast/female-young-averagemuscle-averageweight-maxcup-averagefirmness` | fuller bust, gated, editor `z * 0.75 * breast` |
| `breast_height` | `breast_height_z` | `breast/breast-trans-up` | higher bust, gated, editor `z * 1.5 * cup gate` |
| `nose_bridge` | `nose_bridge_z` | `nose/nose-hump-incr` | higher nose bridge, editor `z * 1.35` |
| `nose_tip` | `nose_tip_z` | `nose/nose-point-up` | tip turned up, editor `z * 1.35` |
| `brow_arch` | `brow_arch_z` | `eyebrows/eyebrows-angle-up` | more arched brows |
| `lip_ratio` | `lip_ratio_z` | `mouth/mouth-upperlip-volume-incr` minus `mouth-lowerlip-volume-incr` | fuller upper lip |

## Height

`height_cm` scales the skeleton uniformly after the shape keys:

```
scale = height_cm / standing height of the blended mesh
```

The standing height is the Z span of the mesh with the age, fat, and muscle
keys applied, before that scale. The six numbers in the table are the
reference heights of the pure keys. A blend is not the unmorphed 166.59 cm,
and it is not a linear mix of those spans. Leg length and head size stay
morphs on top of the scale. Bones do not follow the morphs. The rig was
fitted to the base mesh.

## Not shape keys

Pigment (`skin_eumelanin`, `skin_pheomelanin`, `hair_eumelanin`,
`hair_pheomelanin`, `iris_melanin`), hair (`hair_curl`, `hair_density`,
`balding_stage`, `hair_grey_fraction`, `eyebrow_density`), beard
(`beard_density`, `beard_coverage`, `body_hair`), `freckle_density`,
`face_wrinkles`, `skin_laxity`, `age_spots`, `posture_stoop`. Wrinkles,
laxity, spots, and stoop are ageing fractions, not the age morphs.
`macrodetails/height/*` (144 targets) is unused.
