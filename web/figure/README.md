# Figure files

The standing figure of the person card and the house page. `cargo xtask web` copies the
files named in `FIGURE_FILES` (`xtask/src/web.rs`) into `web/dist/figure`, and
`web/test/figure.sha256` holds the SHA-256 of each (`shasum -a 256 -c ../test/figure.sha256`
in this folder).

## Source and licence

Everything here is CC0 1.0 Universal, with no attribution duty.

- `body.glb` and `proxies/proxies.bin`: the MakeHuman base mesh, morph targets, game-engine
  skeleton and skin weights from https://github.com/makehumancommunity/mpfb2 at commit
  `6ff1eb358f6c1959ced77e79e6dd2406d9a71f49` (assets CC0 by its `LICENSE.md` section C;
  none of the add-on's GPL code is part of these files).
- Skin, eyes, brows, lashes, hair, suits and shoes with their textures: the MakeHuman system
  assets, released as CC0 in September 2020 by Data Collection AB, Joel Palmius and Jonas
  Hauquier
  (https://files.makehumancommunity.org/asset_packs/makehuman_system_assets/makehuman_system_assets_cc0.zip).

MakeHuman is a trademark of its owners; this project is not affiliated with them.

## Changes to the textures

Textures are at most 1024 pixels wide. Those the page reads without alpha (skin, eyes,
shoes, the occlusion maps of the suits) are JPEG; the occlusion maps are greyscale. Hair,
brows and lashes keep their alpha as PNG. The colour textures of the suits are left out:
the page dyes a suit over its occlusion map. `proxies/proxies.json` still names them, and
the sleeveless suit `cloth_tee`, which the page does not draw.
