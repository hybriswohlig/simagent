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
buttons do the same thing. `?edit=1` opens a preview of one person.
Sliders set the main contract values and the figure updates through the
same mapping. Breast development can move only on the female branch, and
only after puberty. Reset restores that person. A panel lists the current
contract. Nothing is written back. Three extra figures labelled "palette check"
stand past the family and are not part of it. They use the contract melanin
minimum, middle and maximum.

## Looks

A is stylised-realistic: real proportions, simplified skin, and a fixed
draw layer on top of the genes. Eyes are a little larger, brows a little
fuller, and the jaw and cheeks a little softer. Those offsets are constant,
so a gene of zero is still zero. The light is a warm key, a cool fill and a
soft rim. The key casts a soft shadow. A dim studio map fills the shadowed
side (see `../ASSETS.md`). The grade is ACES Filmic, written once to sRGB.
Creases get a short screen-space occlusion from N8AO 2.0.1 (ISC,
https://github.com/N8python/n8ao), loaded from jsDelivr. The occlusion pass
renders the picture itself, so style A is antialiased with FXAA. B and C
render directly, with no tone mapping and no shadow. Skin gets a warm wrap
in the shadows, a little blush on the cheeks
and lips, and low specular. B is toon shading with a dark rim. C is flat
unlit colour. Skin uses the CC0 young-caucasian-female diffuse, divided by
its average and multiplied by the contract skin colour. The darkest painted
eyes and brows in that texture are lifted. Hair and iris colours are the contract
`melanin_to_srgb` tables, interpolated in linear light and stored on the
material as sRGB. Clothes are MakeHuman proxy suits and shoes, fitted to
the morphed body and recoloured from a muted palette. The outfit is a hash
of the sim id. Children wear a t-shirt, trousers or shorts, and sneakers.
Eyes, brows, lashes and hair are
MakeHuman proxies fitted to the morphed body (see `../ASSETS.md`). Hair
length is not a gene and not an age. Each person gets one of five CC0
cuts from a hash of the sim id. A balding man is put on the short cut.
The editor can override the cut and the colour for the view only.
`balding_stage` opens the crown and temples of the short cut. `hair_density`
and `eyebrow_density` raise the alpha-test threshold. A beard is a soft
mask on the upper lip, chin and jaw, spreading onto the cheeks as
`beard_coverage` rises. It is coloured from the hair and it does not cover
the mouth. The arms hang at the sides. In the face framing the body is cut
below the neck. Grey is a mix toward neutral grey. Height
is a uniform scale of the root bone after the age, fat and muscle morphs.
The page also loads `/cache/proxies/proxies.json` and `proxies.bin`.
