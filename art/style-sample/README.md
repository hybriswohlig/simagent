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
`/Users/dariushk/.cache/simagent-art`. The page loads `/cache/body.glb` and
`/cache/family.json`.

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
unlit colour. Skin, hair and iris colours are the contract
`melanin_to_srgb` tables, interpolated in linear light and stored on the
material as sRGB. A shader band colours the torso and hips so the body is
not nude. Hair is a larger cap whose crown shrinks with the balding stage.
Beards are a darker, flatter mix of skin and hair colour on men. There is no hair
dye. Grey is a mix toward neutral grey. Height is a uniform scale of the
root bone after the age, fat and muscle morphs.
