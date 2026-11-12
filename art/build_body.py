#!/usr/bin/env python3
"""Build one glTF body from MakeHuman system assets.

Blender headless::

    /Applications/Blender.app/Contents/MacOS/Blender --background \\
        --python art/build_body.py -- \\
        --assets /Users/dariushk/.cache/simagent-art/mpfb2 \\
        --out /Users/dariushk/.cache/simagent-art/body.glb

The script loads ``base.obj``, the game-engine rig, and the morph targets
named in ``MORPHS``. It does not import the MPFB2 addon (that code is GPL).
The ``.glb`` is written to the cache directory. Shape-key values stay at 0.
How to drive the keys from the body contract is ``MAPPING.md``.

Mesh space after import: OBJ is Y-up, 1 unit = 1 decimeter. The script
bakes vertices to Blender Z-up as ``(x, -z, y)`` and then scales by 0.1
so 1 Blender unit is 1 meter, matching ``default_position`` on the rig.
Helpers (vertex index >= the body range in ``hm08_config.json``) are
deleted only after shape keys and weights exist, so Blender remaps them.
Feet are then shifted to Z = 0, and the same shift is applied to every bone.
"""

from __future__ import annotations

import argparse
import gzip
import json
import struct
import sys
from pathlib import Path

try:
    import bpy
    import numpy as np
    from mathutils import Vector
except ImportError:
    sys.exit(
        "Run this with Blender: "
        "blender --background --python art/build_body.py -- --assets ... --out ..."
    )


DEFAULT_ASSETS = "/Users/dariushk/.cache/simagent-art/mpfb2"
DEFAULT_OUT = "/Users/dariushk/.cache/simagent-art/body.glb"
MORPH_LIMIT = 60

# (shape key, contract parameter, target paths relative to targets/, bidirectional)
# Bilateral files are summed into one key. Incr targets only: a negative
# weight is the opposite, which is an approximation when incr and decr
# were sculpted separately.
# Age keys are the mean of the three race-gender-age targets. The files
# named universal-*-averagemuscle-averageweight are empty gzip (the neutral
# muscle/weight corner, identical to the base mesh) and are not loaded.
RACES = ("african", "asian", "caucasian")


def age_targets(sex: str, age: str) -> list[str]:
    return [f"macrodetails/{race}-{sex}-{age}.target.gz" for race in RACES]


MORPHS = [
    ("age_male_child", "body_form_age, androgenic_development", age_targets("male", "child"), False),
    ("age_male_young", "body_form_age, androgenic_development", age_targets("male", "young"), False),
    ("age_male_old", "body_form_age, androgenic_development", age_targets("male", "old"), False),
    ("age_female_child", "body_form_age, estrogenic_development", age_targets("female", "child"), False),
    ("age_female_young", "body_form_age, estrogenic_development", age_targets("female", "young"), False),
    ("age_female_old", "body_form_age, estrogenic_development", age_targets("female", "old"), False),
    ("fat_male_max", "body_fat_z, androgenic_development",
     ["macrodetails/universal-male-young-averagemuscle-maxweight.target.gz"], False),
    ("fat_male_min", "body_fat_z, androgenic_development",
     ["macrodetails/universal-male-young-averagemuscle-minweight.target.gz"], False),
    ("fat_female_max", "body_fat_z, estrogenic_development",
     ["macrodetails/universal-female-young-averagemuscle-maxweight.target.gz"], False),
    ("fat_female_min", "body_fat_z, estrogenic_development",
     ["macrodetails/universal-female-young-averagemuscle-minweight.target.gz"], False),
    ("muscle_male_max", "muscle_z, androgenic_development",
     ["macrodetails/universal-male-young-maxmuscle-averageweight.target.gz"], False),
    ("muscle_male_min", "muscle_z, androgenic_development",
     ["macrodetails/universal-male-young-minmuscle-averageweight.target.gz"], False),
    ("muscle_female_max", "muscle_z, estrogenic_development",
     ["macrodetails/universal-female-young-maxmuscle-averageweight.target.gz"], False),
    ("muscle_female_min", "muscle_z, estrogenic_development",
     ["macrodetails/universal-female-young-minmuscle-averageweight.target.gz"], False),
    ("leg_upper", "leg_ratio_z",
     ["legs/measure-upperleg-height-incr.target.gz"], True),
    ("leg_lower", "leg_ratio_z",
     ["legs/measure-lowerleg-height-incr.target.gz"], True),
    ("head_size", "head_size_z",
     [
         "head/head-scale-horiz-incr.target.gz",
         "head/head-scale-vert-incr.target.gz",
         "head/head-scale-depth-incr.target.gz",
     ], True),
    ("shoulder_width", "shoulder_width_z",
     ["torso/measure-shoulder-dist-incr.target.gz"], True),
    ("chest_depth", "chest_depth_z",
     ["torso/torso-scale-depth-incr.target.gz"], True),
    ("pelvis_width", "hip_width_z",
     ["hip/hip-scale-horiz-incr.target.gz"], True),
    ("fat_central", "fat_distribution_z",
     ["torso/measure-waist-circ-incr.target.gz"], True),
    ("fat_peripheral", "fat_distribution_z",
     [
         "legs/l-upperleg-fat-incr.target.gz",
         "legs/r-upperleg-fat-incr.target.gz",
     ], True),
    ("fat_belly", "fat_distribution_z",
     ["stomach/stomach-pregnant-incr.target.gz"], True),
    ("fat_upperarm", "body_fat_z",
     [
         "arms/l-upperarm-fat-incr.target.gz",
         "arms/r-upperarm-fat-incr.target.gz",
     ], True),
    ("muscle_arm", "muscle_z",
     [
         "arms/l-upperarm-muscle-incr.target.gz",
         "arms/r-upperarm-muscle-incr.target.gz",
     ], True),
    ("muscle_shoulder", "muscle_z",
     [
         "arms/l-upperarm-shoulder-muscle-incr.target.gz",
         "arms/r-upperarm-shoulder-muscle-incr.target.gz",
     ], True),
    ("muscle_chest", "muscle_z",
     ["torso/torso-muscle-pectoral-incr.target.gz"], True),
    ("breast_max", "breast_development",
     ["breast/female-young-averagemuscle-averageweight-maxcup-averagefirmness.target.gz"], False),
    ("breast_min", "breast_development",
     ["breast/female-young-averagemuscle-averageweight-mincup-averagefirmness.target.gz"], False),
    ("face_age_decr", "face_form_age",
     ["head/head-age-decr.target.gz"], False),
    ("face_age_incr", "face_form_age",
     ["head/head-age-incr.target.gz"], False),
    ("face_axis_01", "face_axis_01",
     ["nose/nose-scale-horiz-incr.target.gz"], True),
    ("face_axis_02", "face_axis_02",
     ["nose/nose-scale-depth-incr.target.gz"], True),
    ("face_axis_03", "face_axis_03",
     ["nose/nose-scale-vert-incr.target.gz"], True),
    ("face_axis_04", "face_axis_04",
     ["chin/chin-width-incr.target.gz"], True),
    ("face_axis_05", "face_axis_05",
     ["chin/chin-height-incr.target.gz"], True),
    ("face_axis_06", "face_axis_06",
     ["chin/chin-prognathism-incr.target.gz"], True),
    ("face_axis_07", "face_axis_07",
     [
         "cheek/l-cheek-volume-incr.target.gz",
         "cheek/r-cheek-volume-incr.target.gz",
     ], True),
    ("face_axis_08", "face_axis_08",
     [
         "cheek/l-cheek-bones-incr.target.gz",
         "cheek/r-cheek-bones-incr.target.gz",
     ], True),
    ("face_axis_09", "face_axis_09",
     [
         "eyes/l-eye-scale-incr.target.gz",
         "eyes/r-eye-scale-incr.target.gz",
     ], True),
    ("face_axis_10", "face_axis_10",
     [
         "eyes/l-eye-trans-out.target.gz",
         "eyes/r-eye-trans-out.target.gz",
     ], True),
    ("face_axis_11", "face_axis_11",
     [
         "mouth/mouth-upperlip-volume-incr.target.gz",
         "mouth/mouth-lowerlip-volume-incr.target.gz",
     ], True),
    ("face_axis_12", "face_axis_12",
     ["mouth/mouth-scale-horiz-incr.target.gz"], True),
]

AGE_HEIGHT_KEYS = [
    "age_male_child",
    "age_male_young",
    "age_male_old",
    "age_female_child",
    "age_female_young",
    "age_female_old",
]


def fail(message: str) -> None:
    raise SystemExit(message)


def parse_args(argv: list[str]) -> argparse.Namespace:
    if "--" in argv:
        argv = argv[argv.index("--") + 1 :]
    else:
        argv = []
    parser = argparse.ArgumentParser(description="Export the style-sample body as glb.")
    parser.add_argument("--assets", default=DEFAULT_ASSETS, help="MPFB2 clone root")
    parser.add_argument("--out", default=DEFAULT_OUT, help="Output .glb path (cache dir)")
    return parser.parse_args(argv)


def data_root(assets: Path) -> Path:
    nested = assets / "src" / "mpfb" / "data"
    if nested.is_dir():
        return nested
    if (assets / "3dobjs" / "base.obj").is_file():
        return assets
    fail(f"No MakeHuman data under {assets}")
    raise AssertionError


def require_cc0(root: Path, data: Path) -> None:
    license_md = (root / "LICENSE.md").read_text(encoding="utf-8")
    marker = "C. The license for the bundled assets"
    parts = license_md.split(marker)
    # The heading is also in the table of contents. The last piece is section C.
    if len(parts) < 2 or "CC0" not in parts[-1][:1200]:
        fail("LICENSE.md section C does not state CC0 for the bundled assets")
    assets_license = root / "LICENSE.ASSETS.md"
    if not assets_license.is_file() or "CC0 1.0" not in assets_license.read_text(encoding="utf-8")[:400]:
        fail("LICENSE.ASSETS.md is missing or is not CC0 1.0")
    weights_path = data / "rigs" / "standard" / "weights.game_engine.json"
    header = json.loads(weights_path.read_text(encoding="utf-8"))
    license_name = str(header.get("license", ""))
    if "CC0" not in license_name.upper():
        fail(f"weights.game_engine.json license is {license_name!r}, expected CC0")


def first_obj_vertex(path: Path) -> tuple[float, float, float]:
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            if line.startswith("v "):
                parts = line.split()
                return (float(parts[1]), float(parts[2]), float(parts[3]))
    fail(f"{path} has no vertex")
    raise AssertionError


def close(a: tuple[float, float, float], b: tuple[float, float, float], tol: float) -> bool:
    return all(abs(a[i] - b[i]) < tol for i in range(3))


def coords_of(mesh_attr) -> np.ndarray:
    count = len(mesh_attr)
    flat = np.empty(count * 3, dtype=np.float32)
    mesh_attr.foreach_get("co", flat)
    return flat


def set_coords(mesh_attr, flat: np.ndarray) -> None:
    mesh_attr.foreach_set("co", flat.astype(np.float32, copy=False))


def scale_flat(flat: np.ndarray, factor: float) -> np.ndarray:
    out = flat.copy()
    out *= factor
    return out


def orient_flat(flat: np.ndarray) -> np.ndarray:
    """OBJ Y-up to Blender Z-up: (x, y, z) -> (x, -z, y)."""
    out = flat.copy()
    x = flat[0::3].copy()
    y = flat[1::3].copy()
    z = flat[2::3].copy()
    out[0::3] = x
    out[1::3] = -z
    out[2::3] = y
    return out


def load_target(path: Path, count: int) -> np.ndarray:
    """Return oriented deltas, shape (count, 3), still in decimeters."""
    acc = np.zeros((count, 3), dtype=np.float64)
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rt", encoding="utf-8") as handle:
        for line_no, line in enumerate(handle, 1):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if len(parts) < 4:
                fail(f"{path}:{line_no}: expected 'index dx dy dz'")
            idx = int(parts[0])
            if idx < 0 or idx >= count:
                fail(f"{path}:{line_no}: vertex {idx} outside 0..{count - 1}")
            dx, dy, dz = float(parts[1]), float(parts[2]), float(parts[3])
            acc[idx, 0] += dx
            acc[idx, 1] += -dz
            acc[idx, 2] += dy
    if not np.any(acc):
        fail(f"Target {path} has no vertex deltas")
    return acc


def import_base(path: Path):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    kwargs = dict(
        filepath=str(path),
        use_split_objects=False,
        use_split_groups=False,
        validate_meshes=False,
    )
    try:
        bpy.ops.wm.obj_import(
            forward_axis="NEGATIVE_Z",
            up_axis="Y",
            **kwargs,
        )
    except TypeError:
        kwargs.pop("validate_meshes", None)
        bpy.ops.wm.obj_import(**kwargs)
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
    if not meshes:
        fail(f"OBJ import produced no mesh from {path}")
    mesh = max(meshes, key=lambda obj: len(obj.data.vertices))
    for other in meshes:
        if other is not mesh:
            bpy.data.objects.remove(other, do_unlink=True)
    bpy.ops.object.select_all(action="DESELECT")
    mesh.select_set(True)
    bpy.context.view_layer.objects.active = mesh
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    mesh.name = "Human"
    mesh.data.name = "Human"
    return mesh


def fit_import_space(mesh, obj_v0: tuple[float, float, float]) -> str:
    """Put vertices in unscaled Blender dm space: (obj_x, -obj_z, obj_y)."""
    flat = coords_of(mesh.data.vertices)
    mesh_v0 = (float(flat[0]), float(flat[1]), float(flat[2]))
    blender_v0 = (obj_v0[0], -obj_v0[2], obj_v0[1])
    if close(mesh_v0, blender_v0, 1e-3):
        return "importer-z-up"
    if close(mesh_v0, obj_v0, 1e-3):
        set_coords(mesh.data.vertices, orient_flat(flat))
        mesh.data.update()
        return "oriented-y-up-obj"
    scaled_blender = tuple(c * 0.1 for c in blender_v0)
    scaled_obj = tuple(c * 0.1 for c in obj_v0)
    fail(
        "base.obj vertex 0 is not in OBJ decimeters or Blender decimeters. "
        f"file={obj_v0} mesh={mesh_v0} expected_z_up={blender_v0} "
        f"if_already_meters={scaled_blender} or {scaled_obj}"
    )
    raise AssertionError


def add_shape_keys(mesh, targets: Path, delta_scale: float) -> list[str]:
    count = len(mesh.data.vertices)
    if mesh.data.shape_keys is not None:
        fail("Imported mesh already has shape keys")
    basis = mesh.shape_key_add(name="Basis", from_mix=False)
    basis.interpolation = "KEY_LINEAR"
    base = coords_of(mesh.data.vertices).astype(np.float64)
    names = []
    for name, _param, rels, bidirectional in MORPHS:
        if "mixamo" in name.lower():
            fail(f"Refusing Mixamo morph {name}")
        acc = np.zeros((count, 3), dtype=np.float64)
        for rel in rels:
            if "mixamo" in rel.lower():
                fail(f"Refusing Mixamo target {rel}")
            path = targets / rel
            if not path.is_file():
                fail(f"Missing target {path}")
            acc += load_target(path, count)
        # Each race file is a full delta from the base. Mean them so the
        # three weights are 1/3 and sum to 1. Other multi-file keys are
        # bilateral halves and stay summed.
        if name.startswith("age_"):
            if len(rels) != 3:
                fail(f"{name} must blend exactly 3 race targets, got {len(rels)}")
            acc /= 3.0
        acc *= delta_scale
        shaped = base.reshape(count, 3) + acc
        key = mesh.shape_key_add(name=name, from_mix=False)
        set_coords(key.data, shaped.reshape(-1).astype(np.float32))
        key.value = 0.0
        key.interpolation = "KEY_LINEAR"
        if bidirectional:
            key.slider_min = -1.0
            key.slider_max = 1.0
        else:
            key.slider_min = 0.0
            key.slider_max = 1.0
        names.append(name)
    extra = len(mesh.data.shape_keys.key_blocks) - 1
    if extra != len(MORPHS):
        fail(f"Expected {len(MORPHS)} morphs, mesh has {extra}")
    if extra > MORPH_LIMIT:
        fail(f"{extra} morphs exceeds the limit of {MORPH_LIMIT}")
    return names


def scale_mesh(mesh, factor: float) -> None:
    flat = coords_of(mesh.data.vertices)
    set_coords(mesh.data.vertices, scale_flat(flat, factor))
    for block in mesh.data.shape_keys.key_blocks:
        block_flat = coords_of(block.data)
        set_coords(block.data, scale_flat(block_flat, factor))
    mesh.data.update()


def body_vertex_count(config_path: Path) -> int:
    config = json.loads(config_path.read_text(encoding="utf-8"))
    start, end = config["groups_by_range"]["body"]
    if int(start) != 0:
        fail(f"Body vertex range does not start at 0: {start}..{end}")
    return int(end) + 1


def build_armature(rig_path: Path):
    rig = json.loads(rig_path.read_text(encoding="utf-8"))
    arm_data = bpy.data.armatures.new("game_engine")
    arm = bpy.data.objects.new("game_engine", arm_data)
    bpy.context.scene.collection.objects.link(arm)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    edit = arm.data.edit_bones
    for name, bone in rig.items():
        if "mixamo" in name.lower():
            fail(f"Refusing Mixamo bone {name}")
        created = edit.new(name)
        head = bone["head"]["default_position"]
        tail = bone["tail"]["default_position"]
        created.head = Vector(head)
        created.tail = Vector(tail)
        if (created.tail - created.head).length < 1e-8:
            created.tail = created.head + Vector((0.0, 0.0, 0.001))
        created.roll = float(bone.get("roll", 0.0))
        inherit = bone.get("inherit_scale", "FULL")
        created.inherit_scale = inherit
    for name, bone in rig.items():
        parent_name = bone.get("parent") or ""
        created = edit[name]
        if parent_name:
            if parent_name not in edit:
                fail(f"Bone {name} parent {parent_name} is missing")
            created.parent = edit[parent_name]
        created.use_connect = bool(bone.get("use_connect", False))
        # use_connect snaps the head onto the parent tail. Keep the JSON pose.
        created.head = Vector(bone["head"]["default_position"])
        created.tail = Vector(bone["tail"]["default_position"])
        if (created.tail - created.head).length < 1e-8:
            created.tail = created.head + Vector((0.0, 0.0, 0.001))
        created.roll = float(bone.get("roll", 0.0))
    bpy.ops.object.mode_set(mode="OBJECT")
    return arm


def assign_weights(mesh, weights_path: Path, body_count: int) -> int:
    payload = json.loads(weights_path.read_text(encoding="utf-8"))
    if "CC0" not in str(payload.get("license", "")).upper():
        fail(f"Refusing weights with license {payload.get('license')!r}")
    influences = 0
    vertex_count = len(mesh.data.vertices)
    for bone_name, pairs in payload["weights"].items():
        if "mixamo" in bone_name.lower():
            fail(f"Refusing Mixamo weights for {bone_name}")
        if not pairs:
            continue
        group = mesh.vertex_groups.new(name=bone_name)
        for index, weight in pairs:
            index = int(index)
            if index < 0 or index >= vertex_count:
                fail(f"Weight {bone_name} vertex {index} outside 0..{vertex_count - 1}")
            if index >= body_count:
                continue
            group.add([index], float(weight), "REPLACE")
            influences += 1
    return influences


def delete_helpers(mesh, body_count: int) -> None:
    if len(mesh.data.vertices) < body_count:
        fail(f"Mesh has {len(mesh.data.vertices)} verts, body needs {body_count}")
    mesh.active_shape_key_index = 0
    bpy.context.view_layer.objects.active = mesh
    bpy.ops.object.mode_set(mode="OBJECT")
    for vert in mesh.data.vertices:
        vert.select = vert.index >= body_count
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.delete(type="VERT")
    bpy.ops.object.mode_set(mode="OBJECT")
    left = len(mesh.data.vertices)
    if left != body_count:
        fail(f"Helper delete left {left} vertices, expected {body_count}")


def basis_flat(mesh) -> np.ndarray:
    return coords_of(mesh.data.shape_keys.key_blocks["Basis"].data)


def shift_to_ground(mesh, arm) -> float:
    flat = basis_flat(mesh)
    min_z = float(flat[2::3].min())
    dz = -min_z
    set_coords(mesh.data.vertices, _shift_z(coords_of(mesh.data.vertices), dz))
    for block in mesh.data.shape_keys.key_blocks:
        set_coords(block.data, _shift_z(coords_of(block.data), dz))
    mesh.data.update()
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    for bone in arm.data.edit_bones:
        bone.head.z += dz
        bone.tail.z += dz
    bpy.ops.object.mode_set(mode="OBJECT")
    return dz


def _shift_z(flat: np.ndarray, dz: float) -> np.ndarray:
    out = flat.copy()
    out[2::3] += np.float32(dz)
    return out


def z_span_cm(block) -> dict:
    flat = coords_of(block.data)
    zs = flat[2::3]
    min_z = float(zs.min())
    max_z = float(zs.max())
    return {
        "min_z_m": min_z,
        "max_z_m": max_z,
        "height_cm": (max_z - min_z) * 100.0,
    }


def require_age_stature(heights: dict) -> None:
    """Empty age targets copy the basis. A child key must change stature."""
    basis = heights["basis"]["height_cm"]
    male_child = heights["age_male_child"]["height_cm"]
    female_child = heights["age_female_child"]["height_cm"]
    if abs(male_child - basis) < 5.0 or abs(female_child - basis) < 5.0:
        fail(
            "Child age morph did not change standing height enough. "
            f"basis={basis:.2f} male_child={male_child:.2f} "
            f"female_child={female_child:.2f}"
        )


def standing_heights(mesh) -> dict:
    keys = mesh.data.shape_keys.key_blocks
    report = {"basis": z_span_cm(keys["Basis"])}
    saved = {name: keys[name].value for name in AGE_HEIGHT_KEYS}
    for name in AGE_HEIGHT_KEYS:
        for other in AGE_HEIGHT_KEYS:
            keys[other].value = 1.0 if other == name else 0.0
        # Span of the key at weight 1 equals the key's own coordinates,
        # because these keys were built as basis + delta and stored absolute.
        report[name] = z_span_cm(keys[name])
    for name, value in saved.items():
        keys[name].value = value
    return report


def parent_mesh(mesh, arm) -> None:
    mesh.parent = arm
    modifier = mesh.modifiers.new(name="Armature", type="ARMATURE")
    modifier.object = arm
    modifier.use_vertex_groups = True


def export_glb(path: Path, mesh, arm) -> None:
    import addon_utils

    addon_utils.enable("io_scene_gltf2")
    path.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="DESELECT")
    mesh.select_set(True)
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.export_scene.gltf(
        filepath=str(path),
        export_format="GLB",
        use_selection=True,
        export_morph=True,
        export_skins=True,
        export_animations=False,
        export_morph_normal=False,
        export_yup=True,
    )
    with path.open("rb") as handle:
        magic, version, length = struct.unpack("<4sII", handle.read(12))
    if magic != b"glTF" or version != 2:
        fail(f"Export is not a glTF 2 GLB: magic={magic!r} version={version}")
    if length < 12 or path.stat().st_size != length:
        fail(f"GLB length header {length} does not match file size {path.stat().st_size}")


def main() -> None:
    args = parse_args(sys.argv)
    assets = Path(args.assets)
    out = Path(args.out)
    if assets.name == "mpfb2" or (assets / "LICENSE.md").is_file():
        root = assets
    else:
        root = assets
    data = data_root(assets)
    if (data.parents[2] / "LICENSE.md").is_file():
        root = data.parents[2]
    require_cc0(root, data)

    base_path = data / "3dobjs" / "base.obj"
    rig_path = data / "rigs" / "standard" / "rig.game_engine.json"
    weights_path = data / "rigs" / "standard" / "weights.game_engine.json"
    config_path = data / "mesh_metadata" / "hm08_config.json"
    targets = data / "targets"
    for path in (base_path, rig_path, weights_path, config_path):
        if not path.is_file():
            fail(f"Missing {path}")

    obj_v0 = first_obj_vertex(base_path)
    body_count = body_vertex_count(config_path)
    mesh = import_base(base_path)
    imported = len(mesh.data.vertices)
    if imported != 19158:
        fail(f"Expected 19158 vertices in base.obj, got {imported}")
    space = fit_import_space(mesh, obj_v0)
    print(f"import space={space} vertices={imported}", flush=True)
    names = add_shape_keys(mesh, targets, delta_scale=1.0)
    print(f"shape keys={len(names)}", flush=True)
    scale_mesh(mesh, 0.1)
    arm = build_armature(rig_path)
    print(f"bones={len(arm.data.bones)}", flush=True)
    influences = assign_weights(mesh, weights_path, body_count)
    print(f"weight influences={influences}", flush=True)
    delete_helpers(mesh, body_count)
    ground_dz = shift_to_ground(mesh, arm)
    parent_mesh(mesh, arm)

    flat = basis_flat(mesh)
    zs = flat[2::3]
    ball = arm.data.bones["ball_l"]
    # Bone heads in object mode are the pose bones' armature-local positions.
    ball_z = float(arm.pose.bones["ball_l"].head.z) if arm.pose.bones.get("ball_l") else None
    # Edit-bone rest pose is on the data block after leaving edit mode.
    ball_rest_z = float(arm.data.bones["ball_l"].head_local.z)

    heights = standing_heights(mesh)
    require_age_stature(heights)
    polygons = len(mesh.data.polygons)
    report = {
        "out": str(out),
        "space": space,
        "vertices": len(mesh.data.vertices),
        "polygons": polygons,
        "bones": len(arm.data.bones),
        "morphs": names,
        "morph_count": len(names),
        "weight_influences_on_body": influences,
        "ground_shift_m": ground_dz,
        "basis_min_z_m": float(zs.min()),
        "basis_max_z_m": float(zs.max()),
        "ball_l_head_z_m": ball_rest_z,
        "heights_cm": {key: round(value["height_cm"], 2) for key, value in heights.items()},
        "height_detail": heights,
    }
    export_glb(out, mesh, arm)
    report_path = out.with_name("body_report.json")
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print("REPORT " + json.dumps(report))
    print(
        f"body verts={report['vertices']} polys={polygons} bones={report['bones']} "
        f"morphs={len(names)} ground_shift={ground_dz:.4f} "
        f"basis_z={float(zs.min()):.4f}..{float(zs.max()):.4f} "
        f"ball_l_z={ball_rest_z:.4f} glb={out} bytes={out.stat().st_size}"
    )
    # Silence unused in case a future Blender build drops pose access.
    del ball, ball_z


if __name__ == "__main__":
    main()
