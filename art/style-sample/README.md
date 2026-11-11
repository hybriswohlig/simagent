# Style sample

One MakeHuman body, the family in the cache, three looks.

The glb and `family.json` stay in the cache. This page does not install npm.
Three.js is `three@0.170.0` from jsDelivr.

## Open the page

From this worktree, with `body.glb` and `family.json` already in the cache:

```
python3 art/style-sample/serve.py
```

Open http://127.0.0.1:8766/

`serve.py` serves this directory, and `/cache/` is
`/Users/dariushk/.cache/simagent-art`. The page loads `/cache/body.glb`,
`/cache/family.json`, and `/cache/proxies/`.

The same files, without the page, from the cache directory:

```
python3 -m http.server 8766
```

That serves `http://127.0.0.1:8766/body.glb` and `family.json`. The page
expects them on its own origin under `/cache/`, so use `serve.py` to look
at the row.

`?style=A`, `?style=B` or `?style=C` selects a look. `?close=1` starts in
the face framing, five faces at a time. `?page=2` turns the page. The
buttons do the same thing. Three extra figures labelled "palette check"
stand past the family and are not part of it. They use the contract melanin
minimum, middle and maximum.

## Looks

A is stylised-realistic: standard material, light kept under the albedo,
face axes a little stronger. B is toon shading with a dark rim. C is flat
unlit colour. Skin uses the CC0 young-caucasian-female diffuse, divided by
its average and multiplied by the contract skin colour. The darkest painted
eyes and brows in that texture are lifted. Hair and iris colours are the contract
`melanin_to_srgb` tables, interpolated in linear light and stored on the
material as sRGB. A shader band colours the torso and hips navy, with a
straight cut at the base of the neck. Eyes, brows, lashes and hair are
MakeHuman proxies fitted to the morphed body (see `../ASSETS.md`). Hair
uses a short male cut, a longer female cut, or a child cut under 16.
`balding_stage` opens the crown and temples of the short cut. `hair_density`
and `eyebrow_density` raise the alpha-test threshold. A beard is a thin
shell under the nose, coloured from the hair, and it does not cover the
mouth. The arms hang at the sides. In the face framing the body is cut
below the neck. Grey is a mix toward neutral grey. Height
is a uniform scale of the root bone after the age, fat and muscle morphs.
The page also loads `/cache/proxies/proxies.json` and `proxies.bin`.
