#!/usr/bin/env python3
"""Pack MakeHuman CC0 proxies for the style sample.

Reads the system-assets zip (or the extracted tree), the HM08 base mesh, and
the same morph targets as ``build_body.py``. Writes a small JSON plus a
float buffer the page fits in the browser. Nothing here is committed; the
output stays in the cache.

    python3 art/pack_proxies.py

Coordinates match the glb: OBJ decimeters, Y up, times 0.1, then the same
feet-on-ground shift. MHCLO offsets stay in decimeters and are scaled by the
live distance between the scale-reference vertices, which is about 0.1 on
the unmorphed body.
"""

from __future__ import annotations

import gzip
import json
import struct
import zipfile
from pathlib import Path

CACHE = Path("/Users/dariushk/.cache/simagent-art")
ZIP_PATH = CACHE / "makehuman_system_assets_cc0.zip"
EXTRACT = CACHE / "mh-system"
MPFB = CACHE / "mpfb2"
TARGETS = MPFB / "src/mpfb/data/targets"
BASE_OBJ = MPFB / "src/mpfb/data/3dobjs/base.obj"
BODY_GLB = CACHE / "body.glb"
OUT_DIR = CACHE / "proxies"
BODY_END = 13379

# Same morph list as build_body.py. Age keys are the mean of three races.
RACES = ("african", "asian", "caucasian")


def age_targets(sex: str, age: str) -> list[str]:
    return [f"macrodetails/{race}-{sex}-{age}.target.gz" for race in RACES]


MORPHS = [
    ("age_male_child", age_targets("male", "child"), True),
    ("age_male_young", age_targets("male", "young"), True),
    ("age_male_old", age_targets("male", "old"), True),
    ("age_female_child", age_targets("female", "child"), True),
    ("age_female_young", age_targets("female", "young"), True),
    ("age_female_old", age_targets("female", "old"), True),
    ("fat_male_max", ["macrodetails/universal-male-young-averagemuscle-maxweight.target.gz"], False),
    ("fat_male_min", ["macrodetails/universal-male-young-averagemuscle-minweight.target.gz"], False),
    ("fat_female_max", ["macrodetails/universal-female-young-averagemuscle-maxweight.target.gz"], False),
    ("fat_female_min", ["macrodetails/universal-female-young-averagemuscle-minweight.target.gz"], False),
    ("muscle_male_max", ["macrodetails/universal-male-young-maxmuscle-averageweight.target.gz"], False),
    ("muscle_male_min", ["macrodetails/universal-male-young-minmuscle-averageweight.target.gz"], False),
    ("muscle_female_max", ["macrodetails/universal-female-young-maxmuscle-averageweight.target.gz"], False),
    ("muscle_female_min", ["macrodetails/universal-female-young-minmuscle-averageweight.target.gz"], False),
    ("leg_upper", ["legs/measure-upperleg-height-incr.target.gz"], False),
    ("leg_lower", ["legs/measure-lowerleg-height-incr.target.gz"], False),
    ("head_size", [
        "head/head-scale-horiz-incr.target.gz",
        "head/head-scale-vert-incr.target.gz",
        "head/head-scale-depth-incr.target.gz",
    ], False),
    ("shoulder_width", ["torso/measure-shoulder-dist-incr.target.gz"], False),
    ("chest_depth", ["torso/torso-scale-depth-incr.target.gz"], False),
    ("pelvis_width", ["hip/hip-scale-horiz-incr.target.gz"], False),
    ("fat_central", ["torso/measure-waist-circ-incr.target.gz"], False),
    ("fat_peripheral", [
        "legs/l-upperleg-fat-incr.target.gz",
        "legs/r-upperleg-fat-incr.target.gz",
    ], False),
    ("fat_belly", ["stomach/stomach-pregnant-incr.target.gz"], False),
    ("fat_upperarm", [
        "arms/l-upperarm-fat-incr.target.gz",
        "arms/r-upperarm-fat-incr.target.gz",
    ], False),
    ("muscle_arm", [
        "arms/l-upperarm-muscle-incr.target.gz",
        "arms/r-upperarm-muscle-incr.target.gz",
    ], False),
    ("muscle_shoulder", [
        "arms/l-upperarm-shoulder-muscle-incr.target.gz",
        "arms/r-upperarm-shoulder-muscle-incr.target.gz",
    ], False),
    ("muscle_chest", ["torso/torso-muscle-pectoral-incr.target.gz"], False),
    ("breast_max", ["breast/female-young-averagemuscle-averageweight-maxcup-averagefirmness.target.gz"], False),
    ("breast_min", ["breast/female-young-averagemuscle-averageweight-mincup-averagefirmness.target.gz"], False),
    ("face_age_decr", ["head/head-age-decr.target.gz"], False),
    ("face_age_incr", ["head/head-age-incr.target.gz"], False),
    ("face_axis_01", ["nose/nose-scale-horiz-incr.target.gz"], False),
    ("face_axis_02", ["nose/nose-scale-depth-incr.target.gz"], False),
    ("face_axis_03", ["nose/nose-scale-vert-incr.target.gz"], False),
    ("face_axis_04", ["chin/chin-width-incr.target.gz"], False),
    ("face_axis_05", ["chin/chin-height-incr.target.gz"], False),
    ("face_axis_06", ["chin/chin-prognathism-incr.target.gz"], False),
    ("face_axis_07", ["cheek/l-cheek-volume-incr.target.gz", "cheek/r-cheek-volume-incr.target.gz"], False),
    ("face_axis_08", ["cheek/l-cheek-bones-incr.target.gz", "cheek/r-cheek-bones-incr.target.gz"], False),
    ("face_axis_09", ["eyes/l-eye-scale-incr.target.gz", "eyes/r-eye-scale-incr.target.gz"], False),
    ("face_axis_10", ["eyes/l-eye-trans-out.target.gz", "eyes/r-eye-trans-out.target.gz"], False),
    ("face_axis_11", [
        "mouth/mouth-upperlip-volume-incr.target.gz",
        "mouth/mouth-lowerlip-volume-incr.target.gz",
    ], False),
    ("face_axis_12", ["mouth/mouth-scale-horiz-incr.target.gz"], False),
]

PROXIES = [
    ("eyes", "eyes/low-poly/low-poly.mhclo", "eyes/materials/brown_eye.png"),
    ("brows", "eyebrows/eyebrow001/eyebrow001.mhclo", "eyebrows/eyebrow001/eyebrow001.png"),
    ("lashes", "eyelashes/eyelashes01/eyelashes01.mhclo", "eyelashes/eyelashes01/eyelashes01.png"),
    ("hair_short", "hair/short04/short04.mhclo", "hair/short04/short04_diffuse.png"),
    ("hair_medium", "hair/bob01/bob01.mhclo", "hair/bob01/bob01_diffuse.png"),
    ("hair_long", "hair/long01/long01.mhclo", "hair/long01/long01_diffuse.png"),
    ("hair_tied", "hair/ponytail01/ponytail01.mhclo", "hair/ponytail01/ponytail01_diffuse.png"),
    ("hair_braid", "hair/braid01/braid01.mhclo", "hair/braid01/braid01_diffuse.png"),
]


def fail(message: str) -> None:
    raise SystemExit(message)


def read_text(path: Path) -> str:
    if path.is_file():
        return path.read_text(encoding="utf-8", errors="replace")
    rel = path.relative_to(EXTRACT).as_posix()
    with zipfile.ZipFile(ZIP_PATH) as archive:
        return archive.read(rel).decode("utf-8", errors="replace")


def ensure_proxy_files() -> None:
    if EXTRACT.is_dir() and (EXTRACT / "eyes/low-poly/low-poly.mhclo").is_file():
        return
    if not ZIP_PATH.is_file():
        fail(f"Missing {ZIP_PATH}")
    EXTRACT.mkdir(parents=True, exist_ok=True)
    wanted = []
    for _name, mhclo, texture in PROXIES:
        wanted.append(mhclo)
        wanted.append(texture)
        folder = str(Path(mhclo).parent)
        wanted.append(str(Path(folder) / (Path(mhclo).stem + ".obj")))
    with zipfile.ZipFile(ZIP_PATH) as archive:
        for name in archive.namelist():
            if name.endswith("/"):
                continue
            if name in wanted or any(name.startswith(str(Path(item).parent) + "/") and name.endswith(".obj") for item in wanted):
                dest = EXTRACT / name
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(archive.read(name))


def parse_mhclo(path: Path) -> dict:
    meta = {"name": path.stem, "obj_file": "", "scale": {}}
    binds = []
    in_verts = False
    for raw in read_text(path).splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        words = line.split()
        key = words[0]
        if key == "material" or key.startswith("vertexboneweights"):
            continue
        if key == "verts":
            in_verts = True
            continue
        if key in ("x_scale", "y_scale", "z_scale") and len(words) >= 4:
            meta["scale"][key[0]] = [int(words[1]), int(words[2]), float(words[3])]
            in_verts = False
            continue
        if key == "obj_file":
            meta["obj_file"] = words[1]
            in_verts = False
            continue
        if key in ("name", "uuid", "z_depth", "tag", "basemesh", "delete_verts"):
            if key == "name":
                meta["name"] = words[1]
            if key == "delete_verts":
                in_verts = False
            continue
        if not in_verts:
            continue
        if not words[0].lstrip("-").isdigit():
            in_verts = False
            continue
        if len(words) == 1:
            vertex = int(words[0])
            binds.append((vertex, vertex, vertex, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0))
        elif len(words) >= 9:
            binds.append((
                int(words[0]), int(words[1]), int(words[2]),
                float(words[3]), float(words[4]), float(words[5]),
                float(words[6]), float(words[7]), float(words[8]),
            ))
        else:
            fail(f"{path}: vertex line has {len(words)} fields")
    if not binds:
        fail(f"{path}: no vertex bindings")
    if not meta["obj_file"]:
        fail(f"{path}: no obj_file")
    for axis in ("x", "y", "z"):
        if axis not in meta["scale"]:
            fail(f"{path}: missing {axis}_scale")
    meta["binds"] = binds
    return meta


def parse_proxy_obj(path: Path) -> dict:
    verts = []
    uvs = []
    faces = []
    face_uvs = []
    for raw in read_text(path).splitlines():
        if raw.startswith("v "):
            parts = raw.split()
            verts.append((float(parts[1]), float(parts[2]), float(parts[3])))
        elif raw.startswith("vt "):
            parts = raw.split()
            uvs.append((float(parts[1]), float(parts[2])))
        elif raw.startswith("f "):
            corners = raw.split()[1:]
            ids = []
            tids = []
            for corner in corners:
                bits = corner.split("/")
                ids.append(int(bits[0]))
                tids.append(int(bits[1]) if len(bits) > 1 and bits[1] else 0)
            for a, b, c in ((0, 1, 2), (0, 2, 3)) if len(ids) == 4 else ((0, 1, 2),):
                if c >= len(ids):
                    continue
                faces.append((ids[a], ids[b], ids[c]))
                face_uvs.append((tids[a], tids[b], tids[c]))
    if not verts or not faces:
        fail(f"{path}: empty mesh")
    return {"verts": verts, "uvs": uvs, "faces": faces, "face_uvs": face_uvs}


def load_base_vertices(path: Path) -> list[tuple[float, float, float]]:
    verts = []
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            if line.startswith("v "):
                parts = line.split()
                verts.append((float(parts[1]), float(parts[2]), float(parts[3])))
    if len(verts) != 19158:
        fail(f"base.obj has {len(verts)} vertices, expected 19158")
    return verts


def load_target(path: Path, wanted: set[int]) -> dict[int, list[float]]:
    acc: dict[int, list[float]] = {}
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rt", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if len(parts) < 4:
                continue
            index = int(parts[0])
            if index not in wanted:
                continue
            # OBJ delta to glTF meters. Axes already match the glb.
            delta = acc.setdefault(index, [0.0, 0.0, 0.0])
            delta[0] += float(parts[1]) * 0.1
            delta[1] += float(parts[2]) * 0.1
            delta[2] += float(parts[3]) * 0.1
    return acc


def glb_positions(path: Path) -> list[tuple[float, float, float]]:
    data = path.read_bytes()
    if data[:4] != b"glTF":
        fail(f"{path} is not a glb")
    offset = 12
    length, kind = struct.unpack_from("<I4s", data, offset)
    gltf = json.loads(data[offset + 8:offset + 8 + length])
    offset = offset + 8 + length
    length, kind = struct.unpack_from("<I4s", data, offset)
    if kind != b"BIN\x00":
        fail("glb is missing its binary chunk")
    blob = data[offset + 8:offset + 8 + length]
    primitive = gltf["meshes"][0]["primitives"][0]
    accessor = gltf["accessors"][primitive["attributes"]["POSITION"]]
    view = gltf["bufferViews"][accessor["bufferView"]]
    start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    stride = view.get("byteStride", 12)
    count = accessor["count"]
    out = []
    for index in range(count):
        out.append(struct.unpack_from("<3f", blob, start + index * stride))
    return out


class Blob:
    def __init__(self) -> None:
        self.parts: list[bytes] = []
        self.size = 0

    def align(self) -> None:
        pad = (-self.size) % 4
        if pad:
            self.parts.append(b"\x00" * pad)
            self.size += pad

    def add_f32(self, values: list[float]) -> dict:
        self.align()
        offset = self.size
        raw = struct.pack(f"<{len(values)}f", *values)
        self.parts.append(raw)
        self.size += len(raw)
        return {"offset": offset, "count": len(values)}

    def add_u32(self, values: list[int]) -> dict:
        self.align()
        offset = self.size
        raw = struct.pack(f"<{len(values)}I", *values)
        self.parts.append(raw)
        self.size += len(raw)
        return {"offset": offset, "count": len(values)}

    def bytes(self) -> bytes:
        return b"".join(self.parts)


def copy_texture(src_rel: str) -> str:
    src = EXTRACT / src_rel
    if not src.is_file():
        fail(f"Missing texture {src}")
    dest_name = src.name
    dest = OUT_DIR / dest_name
    dest.write_bytes(src.read_bytes())
    return dest_name


def main() -> None:
    ensure_proxy_files()
    if not BASE_OBJ.is_file():
        fail(f"Missing {BASE_OBJ}")
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    base = load_base_vertices(BASE_OBJ)
    body_min_y = min(base[index][1] for index in range(BODY_END + 1)) * 0.1
    shift = -body_min_y
    print(f"ground_shift_m={shift:.6f}", flush=True)

    parsed = []
    wanted: set[int] = set()
    for key, mhclo_rel, texture_rel in PROXIES:
        mhclo_path = EXTRACT / mhclo_rel
        meta = parse_mhclo(mhclo_path)
        obj_path = mhclo_path.parent / meta["obj_file"]
        obj = parse_proxy_obj(obj_path)
        if len(obj["verts"]) != len(meta["binds"]):
            fail(
                f"{key}: obj has {len(obj['verts'])} vertices, "
                f"mhclo has {len(meta['binds'])}"
            )
        for bind in meta["binds"]:
            wanted.add(bind[0])
            wanted.add(bind[1])
            wanted.add(bind[2])
        for axis in ("x", "y", "z"):
            wanted.add(meta["scale"][axis][0])
            wanted.add(meta["scale"][axis][1])
        parsed.append((key, meta, obj, texture_rel))
        print(f"{key}: {len(obj['verts'])} verts, {len(obj['faces'])} tris", flush=True)

    order = sorted(wanted)
    compact = {index: slot for slot, index in enumerate(order)}
    basis = []
    for index in order:
        x, y, z = base[index]
        basis.extend((x * 0.1, y * 0.1 + shift, z * 0.1))

    morph_arrays = []
    for name, rels, mean in MORPHS:
        acc = {index: [0.0, 0.0, 0.0] for index in order}
        for rel in rels:
            path = TARGETS / rel
            if not path.is_file():
                fail(f"Missing target {path}")
            for index, delta in load_target(path, wanted).items():
                slot = acc[index]
                slot[0] += delta[0]
                slot[1] += delta[1]
                slot[2] += delta[2]
        if mean:
            count = float(len(rels))
            for slot in acc.values():
                slot[0] /= count
                slot[1] /= count
                slot[2] /= count
        flat = []
        for index in order:
            flat.extend(acc[index])
        morph_arrays.append((name, flat))
        print(f"morph {name}", flush=True)

    if BODY_GLB.is_file():
        # The exporter duplicates corners, so glb indices are not HM08 indices.
        # A body vertex still has to land on some exported corner.
        glb = glb_positions(BODY_GLB)
        worst = 0.0
        for index in (0, 100, 2500, 5000, 8000, 11000, BODY_END):
            src = base[index]
            expected = (src[0] * 0.1, src[1] * 0.1 + shift, src[2] * 0.1)
            best = min(
                max(abs(expected[axis] - got[axis]) for axis in range(3))
                for got in glb
            )
            worst = max(worst, best)
        print(f"glb basis max error {worst:.6f} m over {len(glb)} corners", flush=True)
        if worst > 0.002:
            fail(f"glb basis does not match base.obj (error {worst:.4f} m)")

    blob = Blob()
    human = {
        "count": len(order),
        "indices": blob.add_u32(order),
        "basis": blob.add_f32(basis),
        "morphs": [],
    }
    for name, flat in morph_arrays:
        human["morphs"].append({"name": name, **blob.add_f32(flat)})

    proxies = {}
    for key, meta, obj, texture_rel in parsed:
        binds = []
        for v0, v1, v2, w0, w1, w2, d0, d1, d2 in meta["binds"]:
            binds.extend((
                float(compact[v0]), float(compact[v1]), float(compact[v2]),
                w0, w1, w2, d0, d1, d2,
            ))
        uv_flat = []
        min_u = min_v = 10.0
        max_u = max_v = -10.0
        for (a, b, c), (ta, tb, tc) in zip(obj["faces"], obj["face_uvs"], strict=True):
            for tid in (ta, tb, tc):
                if tid <= 0 or tid > len(obj["uvs"]):
                    uv_flat.extend((0.0, 0.0))
                    continue
                u, v = obj["uvs"][tid - 1]
                uv_flat.extend((u, v))
                min_u, max_u = min(min_u, u), max(max_u, u)
                min_v, max_v = min(min_v, v), max(max_v, v)
        # Drop the per-corner uv walk above and store per-vertex uvs instead.
        # Faces index vertices; a vertex may have one uv in these assets.
        per_vertex = [(0.0, 0.0)] * len(obj["verts"])
        seen = [0] * len(obj["verts"])
        for (a, b, c), (ta, tb, tc) in zip(obj["faces"], obj["face_uvs"], strict=True):
            for vid, tid in ((a, ta), (b, tb), (c, tc)):
                slot = vid - 1
                if slot < 0 or slot >= len(per_vertex):
                    fail(f"{key}: face index {vid} out of range")
                if tid > 0 and tid <= len(obj["uvs"]):
                    per_vertex[slot] = obj["uvs"][tid - 1]
                    seen[slot] += 1
        uv_flat = []
        for u, v in per_vertex:
            uv_flat.extend((u, v))
            min_u, max_u = min(min_u, u), max(max_u, u)
            min_v, max_v = min(min_v, v), max(max_v, v)
        faces = []
        for a, b, c in obj["faces"]:
            faces.extend((a - 1, b - 1, c - 1))
        scale = {}
        for axis in ("x", "y", "z"):
            i0, i1, denom = meta["scale"][axis]
            scale[axis] = [compact[i0], compact[i1], denom]
        proxies[key] = {
            "name": meta["name"],
            "texture": copy_texture(texture_rel),
            "scale": scale,
            "count": len(obj["verts"]),
            "bind": blob.add_f32(binds),
            "uv": blob.add_f32(uv_flat),
            "faces": blob.add_u32(faces),
        }
        print(
            f"{key} uv {min_u:.3f}..{max_u:.3f}, {min_v:.3f}..{max_v:.3f}",
            flush=True,
        )

    # Eyes are exact helper copies. Fitted base positions must match the obj.
    eye_meta = next(meta for key, meta, _obj, _tex in parsed if key == "eyes")
    eye_obj = next(obj for key, _meta, obj, _tex in parsed if key == "eyes")
    worst = 0.0
    for bind, vert in zip(eye_meta["binds"], eye_obj["verts"], strict=True):
        src = base[bind[0]]
        expected = (src[0] * 0.1, src[1] * 0.1 + shift, src[2] * 0.1)
        authored = (vert[0] * 0.1, vert[1] * 0.1 + shift, vert[2] * 0.1)
        err = max(abs(expected[axis] - authored[axis]) for axis in range(3))
        worst = max(worst, err)
    print(f"eye fit max error {worst:.6f} m", flush=True)
    if worst > 0.001:
        fail(f"eye proxy does not sit on the helper vertices (error {worst:.4f} m)")

    meta_out = {
        "license": "CC0-1.0",
        "source": "makehuman_system_assets_cc0",
        "ground_shift_m": shift,
        "human": human,
        "proxies": proxies,
    }
    (OUT_DIR / "proxies.json").write_text(json.dumps(meta_out), encoding="utf-8")
    (OUT_DIR / "proxies.bin").write_bytes(blob.bytes())
    print(
        f"wrote {OUT_DIR / 'proxies.json'} and proxies.bin "
        f"({blob.size} bytes, {len(order)} human verts)",
        flush=True,
    )


if __name__ == "__main__":
    main()
