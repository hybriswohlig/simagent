/**
 * The standing figure on the person card, and the same figure for each member of a
 * household. The body values arrive from the server; the shape keys, colours and
 * clothes come from `figure-map`. The mesh, the proxy bytes and the decoded images
 * are loaded once per page. Each mount makes its own textures from those images and
 * frees them with its renderer. A card that cannot draw rejects, and the page keeps
 * the portrait. A household is one renderer and one scene: browsers cap WebGL contexts.
 *
 * The body faces +Z (the sample's mesh space, Y up). The camera sits on +Z and looks
 * toward the origin, so the face is toward the viewer. A positive yaw swings that face
 * toward world +X.
 */

import { figureBytes, figureImage, figureText } from "../api.js";
import type { BodyParams } from "../generated/body.js";
import {
  ACESFilmicToneMapping,
  BufferAttribute,
  BufferGeometry,
  ClampToEdgeWrapping,
  Color,
  DirectionalLight,
  DoubleSide,
  Float32BufferAttribute,
  FrontSide,
  Group,
  HemisphereLight,
  Material,
  Bone,
  Matrix4,
  Mesh,
  MeshStandardMaterial,
  NoColorSpace,
  Object3D,
  PerspectiveCamera,
  Quaternion,
  Scene,
  Skeleton,
  SkinnedMesh,
  SRGBColorSpace,
  Texture,
  Uint16BufferAttribute,
  Vector3,
  WebGLRenderer,
  MathUtils,
  type Shader,
} from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { clone as cloneSkeleton } from "three/addons/utils/SkeletonUtils.js";

import {
  ageSlices,
  clothes,
  hairColor,
  irisColor,
  morphWeights,
  skinColor,
  wornHair,
  type Outfit,
} from "./figure-map.js";
import {
  FAMILY_HEIGHT_PX,
  FAMILY_LIMIT,
  FAMILY_SPACING_M,
  FRAME_CM,
  FRAME_FILL,
  familyCanvasWidth,
  familyPlaces,
  familyViewWidthM,
  framedBodyM,
} from "./family-layout.js";
import { selfContained } from "./glb.js";
import type { Sex } from "./portrait.js";

export const FIGURE_WIDTH = 260;
export const FIGURE_HEIGHT = 420;

const SKIN_MEAN = new Vector3(0.616986, 0.356523, 0.236129);
const CLEAR = 0xe7e2da;
/** How far a hanging arm and a standing leg lean outward, as sideways per unit down. */
const ARM_OUT = 0.08;
/** A child's arms hang a little further out, clear of the rounder belly. */
const CHILD_ARM_OUT = 0.05;
const LEG_OUT = 0.03;
/** The seated pose: the thighs' turn forward, their turn apart and the knees' bend, in radians. */
const SIT_THIGH = 1.45;
const SIT_SPREAD = 0.12;
const SIT_KNEE = 0.35;
const SIT_ARM = 0.7;
/** How far the seat lies below the hip joints, as a share of the thigh's length. */
const SIT_SEAT = 0.55;
/** The baby shape rests with the thighs splayed; its knees are brought in by this much more. */
const BABY_KNEE_IN = 0.2;
/** How far the eyeballs sit in front of their fitted place, in metres of the mesh. */
const EYE_FORWARD = 0.002;
const GROUND_SHIFT = 0.81676;

const MEASURE_KEYS = new Set([
  "age_male_baby",
  "age_female_baby",
  "age_male_child",
  "age_male_young",
  "age_male_old",
  "age_female_child",
  "age_female_young",
  "age_female_old",
  "fat_male_max",
  "fat_male_min",
  "fat_female_max",
  "fat_female_min",
  "muscle_male_max",
  "muscle_male_min",
  "muscle_female_max",
  "muscle_female_min",
  "fat_belly",
  "fat_upperarm",
  "muscle_arm",
  "muscle_shoulder",
  "muscle_chest",
]);

const HAIR_CAP: Record<string, number> = {
  hair_short: 0.042,
  hair_medium: 0.048,
  hair_long: 0.05,
  hair_tied: 0.036,
  hair_braid: 0.04,
};

const WAIST: Record<string, number> = {
  cloth_shirt: 0.56,
  cloth_jacket: 0.55,
  cloth_f_tee: 0.58,
  cloth_shorts: 0.48,
  cloth_skirt: 0.42,
};

/** Proxies the card draws. The tank top and the unused short cut are not requested. */
const USED = new Set([
  "eyes",
  "brows",
  "lashes",
  "hair_short",
  "hair_medium",
  "hair_long",
  "hair_tied",
  "hair_braid",
  "cloth_shirt",
  "cloth_jacket",
  "cloth_f_tee",
  "cloth_shorts",
  "cloth_skirt",
  "shoes_brown",
  "shoes_black",
  "shoes_sneaker",
]);

const FIGURE = "./figure";

type Ref = { offset: number; count: number };
type Pair = readonly [number, number, number];
type Scale = { x: Pair; y: Pair; z: Pair };

type Morph = { name: string; data: Float32Array };

type Human = { basis: Float32Array; morphs: Morph[] };

type FigureImage = ImageBitmap | HTMLImageElement;

type Proxy = {
  kind: string;
  scale: Scale;
  count: number;
  image: FigureImage;
  bind: Float32Array;
  uv: Float32Array;
  faces: Uint32Array;
  split: number;
  aoImage: FigureImage | null;
  skinIndex: Uint16Array | null;
  skinWeight: Float32Array | null;
  deleteIndices: Uint32Array | null;
};

type Pack = { human: Human; proxies: Record<string, Proxy>; bodyRest: Float32Array; groundShift: number };

type NormalGroups = { groupOf: Int32Array; count: number };

type Assets = {
  template: Group;
  groups: NormalGroups;
  rings: Int32Array[];
  restIndex: Int32Array;
  pack: Pack;
  skin: FigureImage;
};

type Owned = {
  geometries: BufferGeometry[];
  materials: Material[];
  textures: Texture[];
  skeletons: Skeleton[];
  made: Map<FigureImage, Texture>;
};

function freshOwned(): Owned {
  return { geometries: [], materials: [], textures: [], skeletons: [], made: new Map() };
}

/** One texture for this mount. The decoded image stays in the page cache. */
function textureOf(image: FigureImage, flipY: boolean, colorSpace: string, owned: Owned): Texture {
  const found = owned.made.get(image);
  if (found !== undefined) return found;
  const texture = new Texture(image);
  texture.colorSpace = colorSpace;
  texture.flipY = flipY;
  texture.wrapS = ClampToEdgeWrapping;
  texture.wrapT = ClampToEdgeWrapping;
  texture.needsUpdate = true;
  owned.made.set(image, texture);
  owned.textures.push(texture);
  return texture;
}

type Marks = {
  maxY: number;
  crown: [number, number, number];
  chin: [number, number, number];
  jaw: { noseDy: number; noseZ: number };
};

function num(array: ArrayLike<number>, index: number): number {
  return array[index] ?? 0;
}

function fail(message: string): never {
  throw new Error(message);
}

function asRecord(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) fail(`${label} is not an object`);
  return value as Record<string, unknown>;
}

function refOf(value: unknown, label: string): Ref {
  const record = asRecord(value, label);
  if (typeof record["offset"] !== "number" || typeof record["count"] !== "number") fail(`${label} has no span`);
  return { offset: record["offset"], count: record["count"] };
}

function pairOf(value: unknown, label: string): Pair {
  if (!Array.isArray(value) || value.length < 3) fail(`${label} is short`);
  const a = value[0];
  const b = value[1];
  const c = value[2];
  if (typeof a !== "number" || typeof b !== "number" || typeof c !== "number") fail(`${label} is not numbers`);
  return [a, b, c];
}

function viewF32(buffer: ArrayBuffer, ref: Ref): Float32Array {
  return new Float32Array(buffer, ref.offset, ref.count);
}

function viewU32(buffer: ArrayBuffer, ref: Ref): Uint32Array {
  return new Uint32Array(buffer, ref.offset, ref.count);
}

function viewU16(buffer: ArrayBuffer, ref: Ref): Uint16Array {
  return new Uint16Array(buffer, ref.offset, ref.count);
}

async function loadPack(): Promise<Pack> {
  const meta = asRecord(JSON.parse(await figureText(`${FIGURE}/proxies/proxies.json`)), "proxies");
  const buffer = await figureBytes(`${FIGURE}/proxies/proxies.bin`);
  const humanMeta = asRecord(meta["human"], "human");
  const morphList = humanMeta["morphs"];
  if (!Array.isArray(morphList)) fail("human morphs are missing");
  const human: Human = {
    basis: viewF32(buffer, refOf(humanMeta["basis"], "basis")),
    morphs: morphList.map((entry) => {
      const record = asRecord(entry, "morph");
      if (typeof record["name"] !== "string") fail("a morph has no name");
      return { name: record["name"], data: viewF32(buffer, refOf(record, "morph")) };
    }),
  };
  const proxies: Record<string, Proxy> = {};
  const listed = asRecord(meta["proxies"], "proxies");
  for (const key of Object.keys(listed)) {
    if (!USED.has(key)) continue;
    const proxy = asRecord(listed[key], key);
    if (typeof proxy["texture"] !== "string" || typeof proxy["count"] !== "number") fail(`${key} is incomplete`);
    const scale = asRecord(proxy["scale"], `${key} scale`);
    const aoName = proxy["ao"];
    const aoImage = typeof aoName === "string" && aoName !== "" ? await figureImage(`${FIGURE}/proxies/${aoName}`) : null;
    // A suit is dyed over its occlusion map, so its colour texture is not requested.
    const image = aoImage ?? (await figureImage(`${FIGURE}/proxies/${proxy["texture"]}`));
    const skin = proxy["skin_index"];
    proxies[key] = {
      kind: typeof proxy["kind"] === "string" ? proxy["kind"] : "groom",
      scale: {
        x: pairOf(scale["x"], `${key} x`),
        y: pairOf(scale["y"], `${key} y`),
        z: pairOf(scale["z"], `${key} z`),
      },
      count: proxy["count"],
      image,
      bind: viewF32(buffer, refOf(proxy["bind"], `${key} bind`)),
      uv: viewF32(buffer, refOf(proxy["uv"], `${key} uv`)),
      faces: viewU32(buffer, refOf(proxy["faces"], `${key} faces`)),
      split: WAIST[key] ?? (typeof proxy["split"] === "number" ? proxy["split"] : 0),
      aoImage,
      skinIndex: skin === undefined ? null : viewU16(buffer, refOf(skin, `${key} skin`)),
      skinWeight: proxy["skin_weight"] === undefined ? null : viewF32(buffer, refOf(proxy["skin_weight"], `${key} weight`)),
      deleteIndices: proxy["delete"] === undefined ? null : viewU32(buffer, refOf(proxy["delete"], `${key} delete`)),
    };
  }
  const ground = meta["ground_shift_m"];
  return {
    human,
    proxies,
    bodyRest: viewF32(buffer, refOf(meta["body_rest"], "body rest")),
    groundShift: typeof ground === "number" ? ground : GROUND_SHIFT,
  };
}

let assetsPromise: Promise<Assets> | null = null;

function findSkinned(root: Object3D): SkinnedMesh {
  let found: SkinnedMesh | null = null;
  root.traverse((node) => {
    if (node instanceof SkinnedMesh) found = node;
  });
  if (found === null) fail("body has no skin");
  return found;
}

function findBone(root: Object3D, name: string): Bone | null {
  let found: Bone | null = null;
  root.traverse((node) => {
    if (node instanceof Bone && node.name === name) found = node;
  });
  return found;
}

function buildNormalGroups(geometry: BufferGeometry): NormalGroups {
  const position = geometry.getAttribute("position") ?? fail("body has no positions");
  const pos = position.array;
  const map = new Map<string, number>();
  const groupOf = new Int32Array(position.count);
  let count = 0;
  for (let i = 0; i < groupOf.length; i += 1) {
    const o = i * 3;
    const key = `${Math.round(num(pos, o) * 1e4)},${Math.round(num(pos, o + 1) * 1e4)},${Math.round(num(pos, o + 2) * 1e4)}`;
    let id = map.get(key);
    if (id === undefined) {
      id = count;
      count += 1;
      map.set(key, id);
    }
    groupOf[i] = id;
  }
  return { groupOf, count };
}

function restKey(x: number, y: number, z: number): string {
  return `${Math.round(x * 1e4)},${Math.round(y * 1e4)},${Math.round(z * 1e4)}`;
}

function buildRestIndex(bodyRest: Float32Array, geometry: BufferGeometry): Int32Array {
  const map = new Map<string, number[]>();
  for (let i = 0; i < bodyRest.length; i += 3) {
    const key = restKey(num(bodyRest, i), num(bodyRest, i + 1), num(bodyRest, i + 2));
    const list = map.get(key);
    if (list !== undefined) list.push(i / 3);
    else map.set(key, [i / 3]);
  }
  const position = geometry.getAttribute("position") ?? fail("body has no positions");
  const pos = position.array;
  const hm = new Int32Array(position.count);
  const nearest = (x: number, y: number, z: number): number => {
    const qx = Math.round(x * 1e4);
    const qy = Math.round(y * 1e4);
    const qz = Math.round(z * 1e4);
    let best = -1;
    let bestD = 4e-8;
    for (let dx = -1; dx <= 1; dx += 1) {
      for (let dy = -1; dy <= 1; dy += 1) {
        for (let dz = -1; dz <= 1; dz += 1) {
          const list = map.get(`${qx + dx},${qy + dy},${qz + dz}`);
          if (list === undefined) continue;
          for (const id of list) {
            const o = id * 3;
            const ex = num(bodyRest, o) - x;
            const ey = num(bodyRest, o + 1) - y;
            const ez = num(bodyRest, o + 2) - z;
            const dist = ex * ex + ey * ey + ez * ez;
            if (dist < bestD) {
              bestD = dist;
              best = id;
            }
          }
        }
      }
    }
    return best;
  };
  for (let i = 0; i < hm.length; i += 1) {
    const x = num(pos, i * 3);
    const y = num(pos, i * 3 + 1);
    const z = num(pos, i * 3 + 2);
    hm[i] = nearest(x, y, z);
  }
  return hm;
}

function loadAssets(): Promise<Assets> {
  if (assetsPromise !== null) return assetsPromise;
  const loading = loadAssetsOnce().then(
    (assets) => assets,
    (error: unknown) => {
      assetsPromise = null;
      throw error;
    },
  );
  assetsPromise = loading;
  return loading;
}

/** Canvases on which a renderer could not be made. three registers its listeners on the
 * canvas before it asks for a context, so trying the same canvas again would only add more;
 * a new card brings a new canvas and tries again. */
const failedCanvases = new WeakSet<HTMLCanvasElement>();

async function loadAssetsOnce(): Promise<Assets> {
  const [bytes, pack, skin] = await Promise.all([
    figureBytes(`${FIGURE}/body.glb`),
    loadPack(),
    figureImage(`${FIGURE}/young_lightskinned_female_diffuse.jpg`),
  ]);
  selfContained(bytes);
  const gltf = await new GLTFLoader().parseAsync(bytes, `${FIGURE}/`);
  const template = gltf.scene;
  const mesh = findSkinned(template);
  return {
    template,
    groups: buildNormalGroups(mesh.geometry),
    rings: jointRings(mesh),
    restIndex: buildRestIndex(pack.bodyRest, mesh.geometry),
    pack,
    skin,
  };
}

/** A vertex belongs to a joint when the bone and its parent each hold this much of it. */
const RING_WEIGHT = 0.15;

/**
 * For each bone of the skeleton, the body vertices around its joint with the parent bone:
 * those both bones pull on. A bone without a parent bone, or without such vertices, has none.
 */
function jointRings(mesh: SkinnedMesh): Int32Array[] {
  const bones = mesh.skeleton.bones;
  const index = mesh.geometry.getAttribute("skinIndex") ?? fail("body has no skin indices");
  const weight = mesh.geometry.getAttribute("skinWeight") ?? fail("body has no skin weights");
  const parentOf = bones.map((bone) => bones.findIndex((other) => other === bone.parent));
  const rings: number[][] = bones.map(() => []);
  for (let vertex = 0; vertex < index.count; vertex += 1) {
    const o = vertex * 4;
    for (let a = 0; a < 4; a += 1) {
      const bone = num(index.array, o + a);
      const parent = parentOf[bone] ?? -1;
      if (parent < 0 || num(weight.array, o + a) < RING_WEIGHT) continue;
      let held = 0;
      for (let b = 0; b < 4; b += 1) {
        if (num(index.array, o + b) === parent) held += num(weight.array, o + b);
      }
      if (held >= RING_WEIGHT) rings[bone]?.push(vertex);
    }
  }
  return rings.map((ring) => Int32Array.from(ring));
}

/**
 * Moves each joint with the skin around it. The skeleton comes with the joints of the
 * unshaped body, and a shape key moves only the skin, so on a child or a heavy body a
 * limb would turn about a point that is no longer its joint. A joint moves by as much as
 * the middle of its ring of vertices moved; a bone without a ring moves with its parent.
 * The bones' inverses follow, so the body at rest is drawn exactly as before.
 */
function fitJoints(root: Object3D, mesh: SkinnedMesh, cloud: { xyz: Float32Array }, rings: readonly Int32Array[]): void {
  root.updateWorldMatrix(true, true);
  const skeleton = mesh.skeleton;
  const bones = skeleton.bones;
  const rest = mesh.geometry.getAttribute("position") ?? fail("body has no positions");
  const bound = bones.map((bone, i) => bone.matrixWorld.clone().multiply(skeleton.boneInverses[i] ?? fail("a bone has no inverse")));
  const places = bones.map((bone) => bone.getWorldPosition(new Vector3()));
  const shifts: Vector3[] = [];
  for (let i = 0; i < bones.length; i += 1) {
    const ring = rings[i];
    const parent = bones.findIndex((other) => other === bones[i]?.parent);
    if (ring === undefined || ring.length < 3) {
      shifts.push((shifts[parent] ?? new Vector3(0, 0, 0)).clone());
      continue;
    }
    const before = new Vector3(0, 0, 0);
    const after = new Vector3(0, 0, 0);
    for (let k = 0; k < ring.length; k += 1) {
      const vertex = num(ring, k);
      before.add(new Vector3(rest.getX(vertex), rest.getY(vertex), rest.getZ(vertex)));
      after.add(new Vector3(num(cloud.xyz, vertex * 3), num(cloud.xyz, vertex * 3 + 1), num(cloud.xyz, vertex * 3 + 2)));
    }
    const toWorld = (bound[i] ?? fail("a bone is missing")).clone().multiply(mesh.bindMatrix);
    before.multiplyScalar(1 / ring.length).applyMatrix4(toWorld);
    after.multiplyScalar(1 / ring.length).applyMatrix4(toWorld);
    shifts.push(after.sub(before));
  }
  // The skeleton lists a parent before its children, so each parent is in place first.
  for (let i = 0; i < bones.length; i += 1) {
    const bone = bones[i];
    const place = places[i];
    const shift = shifts[i];
    if (bone === undefined || place === undefined || shift === undefined || bone.parent === null) continue;
    bone.parent.updateWorldMatrix(true, false);
    bone.position.copy(bone.parent.worldToLocal(place.add(shift)));
  }
  root.updateWorldMatrix(true, true);
  skeleton.boneInverses = bones.map((bone, i) => bone.matrixWorld.clone().invert().multiply(bound[i] ?? fail("a bone is missing")));
}

function takeOwnership(mesh: SkinnedMesh, owned: Owned): void {
  const source = mesh.geometry;
  const geometry = new BufferGeometry();
  for (const name of Object.keys(source.attributes)) {
    const attribute = source.attributes[name];
    if (attribute !== undefined) geometry.setAttribute(name, attribute.clone());
  }
  if (source.index !== null) geometry.setIndex(source.index.clone());
  const morphs = source.morphAttributes.position;
  if (morphs !== undefined) geometry.morphAttributes.position = morphs.map((attribute) => attribute.clone());
  geometry.morphTargetsRelative = source.morphTargetsRelative;
  mesh.geometry = geometry;
  if (mesh.morphTargetInfluences !== undefined) mesh.morphTargetInfluences = mesh.morphTargetInfluences.slice();
  owned.geometries.push(geometry);
}

function applyWeights(mesh: SkinnedMesh, weights: Record<string, number>): void {
  const dict = mesh.morphTargetDictionary;
  const influences = mesh.morphTargetInfluences;
  if (dict === undefined || influences === undefined) return;
  for (const name of Object.keys(dict)) {
    const index = dict[name];
    if (index === undefined) continue;
    influences[index] = weights[name] ?? 0;
  }
}

function smoothSkinNormals(mesh: SkinnedMesh, groups: NormalGroups): void {
  const geometry = mesh.geometry;
  const position = geometry.getAttribute("position") ?? fail("body has no positions");
  const index = geometry.index ?? fail("body has no triangles");
  const morphed = new Float32Array(position.count * 3);
  const src = position.array;
  for (let k = 0; k < morphed.length; k += 1) morphed[k] = num(src, k);
  const influences = mesh.morphTargetInfluences ?? [];
  const morphs = geometry.morphAttributes.position ?? [];
  for (let i = 0; i < morphs.length; i += 1) {
    const weight = influences[i] ?? 0;
    const delta = morphs[i];
    if (weight === 0 || delta === undefined) continue;
    const data = delta.array;
    for (let k = 0; k < morphed.length; k += 1) morphed[k] = num(morphed, k) + weight * num(data, k);
  }
  const ax = new Float32Array(groups.count);
  const ay = new Float32Array(groups.count);
  const az = new Float32Array(groups.count);
  const triangles = index.array;
  for (let t = 0; t < triangles.length; t += 3) {
    const a = num(triangles, t);
    const b = num(triangles, t + 1);
    const c = num(triangles, t + 2);
    const a3 = a * 3;
    const b3 = b * 3;
    const c3 = c * 3;
    const x0 = num(morphed, a3);
    const y0 = num(morphed, a3 + 1);
    const z0 = num(morphed, a3 + 2);
    const x1 = num(morphed, b3) - x0;
    const y1 = num(morphed, b3 + 1) - y0;
    const z1 = num(morphed, b3 + 2) - z0;
    const x2 = num(morphed, c3) - x0;
    const y2 = num(morphed, c3 + 1) - y0;
    const z2 = num(morphed, c3 + 2) - z0;
    const nx = y1 * z2 - z1 * y2;
    const ny = z1 * x2 - x1 * z2;
    const nz = x1 * y2 - y1 * x2;
    const g0 = num(groups.groupOf, a);
    const g1 = num(groups.groupOf, b);
    const g2 = num(groups.groupOf, c);
    ax[g0] = num(ax, g0) + nx;
    ay[g0] = num(ay, g0) + ny;
    az[g0] = num(az, g0) + nz;
    ax[g1] = num(ax, g1) + nx;
    ay[g1] = num(ay, g1) + ny;
    az[g1] = num(az, g1) + nz;
    ax[g2] = num(ax, g2) + nx;
    ay[g2] = num(ay, g2) + ny;
    az[g2] = num(az, g2) + nz;
  }
  const normal = new Float32Array(position.count * 3);
  for (let i = 0; i < groups.groupOf.length; i += 1) {
    const g = num(groups.groupOf, i);
    const x = num(ax, g);
    const y = num(ay, g);
    const z = num(az, g);
    const len = Math.hypot(x, y, z) || 1;
    const o = i * 3;
    normal[o] = x / len;
    normal[o + 1] = y / len;
    normal[o + 2] = z / len;
  }
  geometry.setAttribute("normal", new BufferAttribute(normal, 3));
}

function morphedYSpan(mesh: SkinnedMesh, weights: Record<string, number>): { minY: number; span: number } {
  const position = mesh.geometry.getAttribute("position");
  if (position === undefined) return { minY: 0, span: 1 };
  const morphs = mesh.geometry.morphAttributes.position ?? [];
  const dict = mesh.morphTargetDictionary ?? {};
  let minY = Infinity;
  let maxY = -Infinity;
  for (let vertex = 0; vertex < position.count; vertex += 1) {
    let y = position.getY(vertex);
    for (const name of Object.keys(dict)) {
      if (!MEASURE_KEYS.has(name)) continue;
      const index = dict[name];
      const morph = index === undefined ? undefined : morphs[index];
      const weight = weights[name] ?? 0;
      if (morph !== undefined && weight !== 0) y += weight * morph.getY(vertex);
    }
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  if (minY === Infinity) return { minY: 0, span: 1 };
  return { minY, span: Math.max(maxY - minY, 0.05) };
}

function morphedCloud(mesh: SkinnedMesh, weights: Record<string, number>): { xyz: Float32Array; count: number; maxY: number } {
  const position = mesh.geometry.getAttribute("position") ?? fail("body has no positions");
  const morphs = mesh.geometry.morphAttributes.position ?? [];
  const dict = mesh.morphTargetDictionary ?? {};
  const active: [number, BufferAttribute][] = [];
  for (const name of Object.keys(dict)) {
    const index = dict[name];
    const morph = index === undefined ? undefined : morphs[index];
    const weight = weights[name] ?? 0;
    if (morph !== undefined && weight !== 0) active.push([weight, morph]);
  }
  const count = position.count;
  const xyz = new Float32Array(count * 3);
  let maxY = -Infinity;
  for (let vertex = 0; vertex < count; vertex += 1) {
    let x = position.getX(vertex);
    let y = position.getY(vertex);
    let z = position.getZ(vertex);
    for (const [weight, morph] of active) {
      x += weight * morph.getX(vertex);
      y += weight * morph.getY(vertex);
      z += weight * morph.getZ(vertex);
    }
    xyz[vertex * 3] = x;
    xyz[vertex * 3 + 1] = y;
    xyz[vertex * 3 + 2] = z;
    if (y > maxY) maxY = y;
  }
  return { xyz, count, maxY };
}

function centerRows(cloud: { xyz: Float32Array; count: number; maxY: number }): { dy: number; p90: number }[] {
  const bins = new Map<number, number[]>();
  for (let i = 0; i < cloud.count; i += 1) {
    const x = num(cloud.xyz, i * 3);
    const dy = num(cloud.xyz, i * 3 + 1) - cloud.maxY;
    const z = num(cloud.xyz, i * 3 + 2);
    if (Math.abs(x) > 0.012 || dy < -0.36 || dy > -0.08 || z < 0.02) continue;
    const key = Math.round(dy * 200) / 200;
    const row = bins.get(key);
    if (row !== undefined) row.push(z);
    else bins.set(key, [z]);
  }
  return [...bins.entries()]
    .sort((a, b) => a[0] - b[0])
    .map(([dy, zs]) => {
      zs.sort((a, b) => a - b);
      const at = Math.min(zs.length - 1, Math.floor(0.9 * (zs.length - 1)));
      return { dy, p90: zs[at] ?? 0 };
    });
}

function jawProfile(cloud: { xyz: Float32Array; count: number; maxY: number }): { noseDy: number; noseZ: number } {
  const rows = centerRows(cloud);
  let noseDy = -0.16;
  let noseZ = 0.16;
  let bestNose = -Infinity;
  for (const row of rows) {
    if (row.dy > -0.21 && row.dy < -0.11 && row.p90 > bestNose) {
      bestNose = row.p90;
      noseZ = row.p90;
      noseDy = row.dy;
    }
  }
  return { noseDy, noseZ };
}

function landmarks(cloud: { xyz: Float32Array; count: number; maxY: number }): Marks {
  const crown: [number, number, number] = [0, 0, 0];
  let crownN = 0;
  for (let vertex = 0; vertex < cloud.count; vertex += 1) {
    const x = num(cloud.xyz, vertex * 3);
    const y = num(cloud.xyz, vertex * 3 + 1);
    const z = num(cloud.xyz, vertex * 3 + 2);
    if (y > cloud.maxY - 0.008 && Math.abs(x) < 0.04) {
      crown[0] += x;
      crown[1] += y;
      crown[2] += z;
      crownN += 1;
    }
  }
  if (crownN === 0) {
    crown[0] = 0;
    crown[1] = cloud.maxY;
    crown[2] = 0;
  } else {
    crown[0] /= crownN;
    crown[1] /= crownN;
    crown[2] /= crownN;
  }
  const jaw = jawProfile(cloud);
  const chinY = cloud.maxY + jaw.noseDy - 0.06;
  return { maxY: cloud.maxY, crown, chin: [0, chinY, 0.07], jaw };
}

const SKIN_DETAIL = `
  #ifdef USE_MAP
    vec3 tex = sampledDiffuseColor.rgb;
    vec3 ratio = clamp(tex / uSkinMean, vec3(0.25), vec3(1.5));
    vec3 detail = mix(vec3(1.0), ratio, 0.62);
    detail = max(detail, vec3(0.76));
    float warm = clamp(ratio.r - max(ratio.g, ratio.b), 0.0, 0.35);
    vec3 skinCol = uSkin * detail;
    skinCol += uSkin * vec3(0.07, 0.02, 0.008) * smoothstep(0.03, 0.14, warm);
    diffuseColor.rgb = skinCol;
  #endif
`;

const BLUSH = `
  float cheekBand = smoothstep(uNose.y - 0.04, uNose.y - 0.018, vBind.y)
    * (1.0 - smoothstep(uNose.y - 0.006, uNose.y + 0.012, vBind.y));
  float cheekSide = smoothstep(0.022, 0.04, abs(vBind.x)) * (1.0 - smoothstep(0.055, 0.078, abs(vBind.x)));
  float cheekFwd = smoothstep(uNose.z - 0.035, uNose.z - 0.008, vBind.z);
  float cheek = cheekBand * cheekSide * cheekFwd;
  float lipBand = smoothstep(uChin.y + 0.004, uChin.y + 0.014, vBind.y)
    * (1.0 - smoothstep(uChin.y + 0.02, uChin.y + 0.032, vBind.y));
  float lipSide = 1.0 - smoothstep(0.012, 0.032, abs(vBind.x));
  float lipFwd = smoothstep(uChin.z + 0.004, uChin.z + 0.018, vBind.z);
  float lip = lipBand * lipSide * lipFwd;
  diffuseColor.rgb += uSkin * vec3(0.42, 0.1, 0.06) * cheek * 0.28;
  diffuseColor.rgb += uSkin * vec3(0.34, 0.08, 0.06) * lip * 0.4;
`;

const NECK_CUT = `
  if (uCut > 0.0 && vBind.y < uCut) discard;
  skinMask = 1.0;
`;

const SKIN_WRAP = `
  vec3 keyDir = normalize(uKeyDir);
  float ndl = dot(normal, keyDir);
  float wrap = clamp((ndl + 0.55) / 1.55, 0.0, 1.0);
  float direct = clamp(ndl, 0.0, 1.0);
  totalDiffuse += uSkin * vec3(0.62, 0.24, 0.12) * (wrap - direct) * skinMask * 0.7;
`;

function paintSkin(skinHex: string, skinMap: Texture, marks: Marks, keyDir: Vector3, owned: Owned): Material {
  const material = new MeshStandardMaterial({ roughness: 0.92, metalness: 0 });
  material.color.set("#ffffff");
  material.map = skinMap;
  const uniforms = {
    uSkin: { value: new Color(skinHex) },
    uSkinMean: { value: SKIN_MEAN },
    uCut: { value: -1 },
    uNose: { value: new Vector3(0, marks.maxY + marks.jaw.noseDy, marks.jaw.noseZ) },
    uChin: { value: new Vector3(marks.chin[0], marks.chin[1], marks.chin[2]) },
    uKeyDir: { value: keyDir },
  };
  material.customProgramCacheKey = () => "skin-a";
  material.onBeforeCompile = (shader) => {
    shader.uniforms["uSkin"] = uniforms.uSkin;
    shader.uniforms["uSkinMean"] = uniforms.uSkinMean;
    shader.uniforms["uCut"] = uniforms.uCut;
    shader.uniforms["uNose"] = uniforms.uNose;
    shader.uniforms["uChin"] = uniforms.uChin;
    shader.uniforms["uKeyDir"] = uniforms.uKeyDir;
    shader.vertexShader = `varying vec3 vBind;\n${shader.vertexShader.replace(
      "#include <morphtarget_vertex>",
      "#include <morphtarget_vertex>\n\tvBind = transformed;",
    )}`;
    shader.fragmentShader = `varying vec3 vBind;\nuniform vec3 uSkin;\nuniform vec3 uSkinMean;\nuniform float uCut;\nuniform vec3 uNose;\nuniform vec3 uChin;\nuniform vec3 uKeyDir;\nfloat skinMask;\n${shader.fragmentShader}`;
    shader.fragmentShader = shader.fragmentShader.replace(
      "#include <color_fragment>",
      `#include <color_fragment>\n${SKIN_DETAIL}\n${BLUSH}\n${NECK_CUT}`,
    );
    shader.fragmentShader = shader.fragmentShader.replace(
      "vec3 totalDiffuse = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse;",
      `vec3 totalDiffuse = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse;\n${SKIN_WRAP}`,
    );
  };
  owned.materials.push(material);
  return material;
}

function copyUniform(target: unknown, source: unknown): unknown {
  if (target instanceof Color && source instanceof Color) return target.copy(source);
  if (target instanceof Vector3 && source instanceof Vector3) return target.copy(source);
  return source;
}

function bindShader(
  material: Material,
  key: string,
  uniforms: Record<string, { value: unknown }>,
  vertexDecl: string,
  vertexBody: string,
  fragmentDecl: string,
  fragmentBody: string,
): Material {
  material.customProgramCacheKey = () => key;
  material.onBeforeCompile = (shader: Shader) => {
    for (const name of Object.keys(uniforms)) {
      const uniform = uniforms[name];
      if (uniform !== undefined) shader.uniforms[name] = uniform;
    }
    material.userData["shader"] = shader;
    if (vertexDecl !== "") shader.vertexShader = `${vertexDecl}\n${shader.vertexShader}`;
    if (vertexBody !== "") {
      shader.vertexShader = shader.vertexShader.replace("#include <begin_vertex>", `#include <begin_vertex>\n${vertexBody}`);
    }
    if (fragmentDecl !== "") shader.fragmentShader = `${fragmentDecl}\n${shader.fragmentShader}`;
    if (fragmentBody !== "") {
      shader.fragmentShader = shader.fragmentShader.replace("#include <map_fragment>", `#include <map_fragment>\n${fragmentBody}`);
    }
  };
  material.onBeforeRender = () => {
    const shader = material.userData["shader"] as Shader | undefined;
    if (shader === undefined) return;
    for (const name of Object.keys(uniforms)) {
      const uniform = uniforms[name];
      const slot = shader.uniforms[name];
      if (uniform === undefined || slot === undefined) continue;
      slot.value = copyUniform(slot.value, uniform.value);
    }
  };
  return material;
}

const HAIR_VERTEX = `
  vMesh = aMesh;
  vec3 delta = aMesh - uCrown;
  float radial = length(delta.xz);
  float radius = mix(0.018, 0.072, uBald);
  float crownHole = (1.0 - smoothstep(radius * 0.28, radius, radial))
    * smoothstep(uCrown.y - 0.12, uCrown.y - 0.015, aMesh.y);
  float temple = smoothstep(0.022, 0.055, abs(delta.x))
    * smoothstep(uCrown.z - 0.015, uCrown.z + 0.045, aMesh.z)
    * smoothstep(uCrown.y - 0.14, uCrown.y - 0.045, aMesh.y)
    * (1.0 - smoothstep(uCrown.y - 0.02, uCrown.y + 0.01, aMesh.y));
  float crownW = smoothstep(0.12, 0.55, uBald);
  float templeW = smoothstep(0.18, 0.75, uBald);
  vKeep = 1.0 - clamp(crownW * crownHole + templeW * temple, 0.0, 1.0);
  if (uFill > 0.5) {
    float hemBelow = uCrown.y - aMesh.y;
    float hemSide = abs(aMesh.x - uCrown.x);
    float hemFwd = aMesh.z - uCrown.z;
    float hem = smoothstep(0.012, 0.035, hemBelow) * (1.0 - smoothstep(0.08, 0.115, hemBelow));
    hem *= 1.0 - smoothstep(0.082, 0.108, hemSide);
    hem *= smoothstep(-0.03, 0.01, hemFwd);
    float sideSign = aMesh.x >= uCrown.x ? 1.0 : -1.0;
    transformed.x += sideSign * 0.004 * hem * smoothstep(0.02, 0.055, hemSide);
    transformed.y -= 0.006 * hem;
    transformed.z += 0.003 * hem;
  }
  if (uCurl > 0.01) {
    // Curls stand away from the head in uneven tufts and draw the length up.
    float curlBelow = max(uCrown.y - aMesh.y, 0.0);
    float grow = smoothstep(0.0, 0.05, curlBelow);
    vec3 tuftAt = aMesh * 140.0;
    float bump = sin(tuftAt.x + 1.7 * sin(tuftAt.y * 0.6)) * sin(tuftAt.y * 1.1 + 1.3 * sin(tuftAt.z * 0.7)) * sin(tuftAt.z + 0.5);
    transformed += objectNormal * uCurl * (0.002 + 0.006 * grow + 0.003 * bump * grow);
    transformed.y += uCurl * 0.1 * curlBelow;
  }
`;

const STRAND_FRAG = `
  float cover = vKeep;
  float hairA = 1.0;
  #ifdef USE_MAP
    // A curl bends the strands: the texture is read along a wave that grows with the curl.
    float lock = 3.0 * sin(vMapUv.x * 40.0) + 2.0 * sin(vMapUv.x * 17.0 + 1.3);
    float along = vMapUv.y * (CURL_FREQ + CURL_TIGHT * uCurl) * (1.0 + 0.25 * sin(vMapUv.x * 23.0));
    float wave = 0.75 * sin(along + lock) + 0.25 * sin(along * 2.3 + lock * 1.7);
    vec2 strandUv = vMapUv + vec2(wave * CURL_BEND * uCurl, 0.0);
    vec4 strand = uCurl > 0.01 ? texture(map, strandUv) : sampledDiffuseColor;
    hairA = strand.a;
    float softA = hairA;
    if (uSoft > 0.5) {
      vec2 texel = vec2(0.0045);
      softA = max(softA, texture(map, strandUv + vec2(texel.x, 0.0)).a);
      softA = max(softA, texture(map, strandUv - vec2(texel.x, 0.0)).a);
      softA = max(softA, texture(map, strandUv + vec2(0.0, texel.y)).a);
      softA = max(softA, texture(map, strandUv - vec2(0.0, texel.y)).a);
    }
    cover *= softA;
    float shade = dot(strand.rgb, vec3(0.299, 0.587, 0.114));
    float lift = clamp(shade / uMean, 0.6, 1.3) * (1.0 + CURL_SHADE * uCurl * wave);
    diffuseColor.rgb = uHair * lift;
    diffuseColor.a = 1.0;
  #endif
  if (!gl_FrontFacing) diffuseColor.rgb *= 0.62;
  float below = uCrown.y - vMesh.y;
  float forward = vMesh.z - uCrown.z;
  float fillMid = 1.0 - smoothstep(0.003, 0.01, abs(vMesh.x));
  float clipMid = 1.0 - smoothstep(0.002, 0.007, abs(vMesh.x));
  float tuft = clipMid * step(0.08, below) * step(0.085, forward);
  if (uClip > 0.5 && tuft > 0.5) discard;
  float inMass = step(-0.004, below) * (1.0 - smoothstep(uCap - 0.01, uCap, below));
  float lineSide = abs(vMesh.x - uCrown.x);
  float fringe = smoothstep(0.02, 0.045, below) * (1.0 - smoothstep(0.095, 0.12, below));
  fringe *= 1.0 - smoothstep(0.085, 0.108, lineSide);
  fringe *= smoothstep(0.04, 0.075, forward) * (1.0 - smoothstep(0.115, 0.14, forward));
  float partial = smoothstep(0.05, 0.22, hairA) * (1.0 - smoothstep(0.62, 0.92, hairA));
  float edgeFill = fringe * partial * step(0.5, vKeep);
  float hole = max((1.0 - smoothstep(0.2, 0.45, hairA)) * fillMid * inMass * step(0.5, vKeep), edgeFill);
  if (uFill > 0.5 && hole > 0.4) {
    diffuseColor.rgb = uHair;
    diffuseColor.a = 1.0;
  } else if (uSoft > 0.5) {
    float w = clamp(fwidth(cover), 0.02, 0.08);
    float alpha = smoothstep(uThresh - w, uThresh + w, cover);
    if (alpha < 0.02) discard;
    diffuseColor.a = alpha;
  } else if (cover < uThresh) {
    discard;
  }
`;

const EYE_FRAG = `
  float catchLight = 0.0;
  #ifdef USE_MAP
    vec2 pupilL = vec2(0.2939, 0.2897);
    vec2 pupilR = vec2(0.7099, 0.7019);
    float dL = length(vMapUv - pupilL);
    float dR = length(vMapUv - pupilR);
    float d = min(dL, dR);
    float luma = dot(sampledDiffuseColor.rgb, vec3(0.299, 0.587, 0.114));
    float fiber = clamp(luma / 0.14, 0.62, 1.15);
    float radial = clamp(d / 0.108, 0.0, 1.0);
    float irisLuma = dot(uIris, vec3(0.299, 0.587, 0.114));
    vec3 tint = irisLuma < 0.07 ? uIris * (0.11 / max(irisLuma, 0.02)) : uIris;
    vec3 irisCol = tint * mix(1.05, 0.55, radial) * fiber;
    irisCol = mix(irisCol, irisCol * vec3(0.30, 0.26, 0.24), smoothstep(0.72, 1.0, radial));
    irisCol = mix(irisCol, vec3(0.015, 0.012, 0.010), smoothstep(0.040, 0.022, d));
    float iris = smoothstep(0.116, 0.098, d);
    float inDisc = smoothstep(0.205, 0.175, d);
    float corner = smoothstep(0.145, 0.198, d);
    vec3 scleraCol = mix(vec3(0.96, 0.93, 0.88), vec3(0.62, 0.55, 0.50), corner);
    vec3 eyeCol = mix(scleraCol, irisCol, iris);
    diffuseColor.rgb = mix(uSkin, eyeCol, inDisc);
    diffuseColor.a = 1.0;
    vec2 pupil = dL < dR ? pupilL : pupilR;
    catchLight = smoothstep(0.013, 0.002, length(vMapUv - (pupil + vec2(-0.012, 0.015))));
    diffuseColor.rgb = mix(diffuseColor.rgb, vec3(0.96, 0.97, 1.0), catchLight * iris);
  #endif
`;

function imageOf(texture: Texture): HTMLImageElement | HTMLCanvasElement | ImageBitmap | null {
  const image = texture.image;
  if (image instanceof HTMLImageElement || image instanceof HTMLCanvasElement || image instanceof ImageBitmap) return image;
  return null;
}

const strandMeans = new Map<CanvasImageSource, number>();

function srgbByteToLinear(byte: number): number {
  const value = byte / 255;
  return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
}

/**
 * The mean linear luminance of a strand texture where it is opaque. The shader divides
 * the texture's shade by it, so the strands vary around the hair colour of the body
 * values and a dark texture does not darken a light hair.
 */
function strandMean(texture: Texture): number {
  const image = imageOf(texture);
  if (image === null || image.width === 0) return 0.16;
  const cached = strandMeans.get(image);
  if (cached !== undefined) return cached;
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (context === null) return 0.16;
  context.drawImage(image, 0, 0, 64, 64);
  const data = context.getImageData(0, 0, 64, 64).data;
  let sum = 0;
  let count = 0;
  for (let i = 0; i < data.length; i += 4) {
    if (num(data, i + 3) < 128) continue;
    sum += 0.299 * srgbByteToLinear(num(data, i)) + 0.587 * srgbByteToLinear(num(data, i + 1)) + 0.114 * srgbByteToLinear(num(data, i + 2));
    count += 1;
  }
  const mean = count === 0 ? 0.16 : Math.max(sum / count, 0.01);
  strandMeans.set(image, mean);
  return mean;
}

function strandMaterial(
  texture: Texture,
  hairHex: string,
  threshold: number,
  bald: number,
  crown: readonly [number, number, number],
  offset: number,
  fillPart: boolean,
  softEdge: boolean,
  cap: number,
  curl: number,
  owned: Owned,
): Material {
  const material = new MeshStandardMaterial({ roughness: 0.84, metalness: 0 });
  material.map = texture;
  material.color.set("#ffffff");
  material.side = DoubleSide;
  material.polygonOffset = true;
  material.polygonOffsetFactor = offset;
  material.polygonOffsetUnits = offset;
  if (softEdge) material.alphaToCoverage = true;
  const uniforms = {
    uHair: { value: new Color(hairHex) },
    uThresh: { value: threshold },
    uBald: { value: bald },
    uCrown: { value: new Vector3(crown[0], crown[1], crown[2]) },
    uFill: { value: fillPart ? 1 : 0 },
    uSoft: { value: softEdge ? 1 : 0 },
    uCap: { value: cap },
    uClip: { value: 0 },
    uMean: { value: strandMean(texture) },
    uCurl: { value: curl },
  };
  bindShader(
    material,
    `strand-${fillPart ? "part" : "cut"}-${softEdge ? "soft" : "hard"}`,
    uniforms,
    "attribute vec3 aMesh;\nvarying float vKeep;\nvarying vec3 vMesh;\nuniform float uBald;\nuniform vec3 uCrown;\nuniform float uFill;\nuniform float uCurl;",
    HAIR_VERTEX,
    "varying float vKeep;\nvarying vec3 vMesh;\nuniform vec3 uHair;\nuniform float uThresh;\nuniform float uFill;\nuniform float uSoft;\nuniform float uCap;\nuniform float uClip;\nuniform float uMean;\nuniform float uCurl;\nuniform vec3 uCrown;\nconst float CURL_FREQ = 45.0;\nconst float CURL_TIGHT = 70.0;\nconst float CURL_BEND = 0.007;\nconst float CURL_SHADE = 0.18;",
    STRAND_FRAG,
  );
  owned.materials.push(material);
  return material;
}

function eyeMaterial(texture: Texture, irisHex: string, skinHex: string, owned: Owned): Material {
  const material = new MeshStandardMaterial({ roughness: 0.72, metalness: 0 });
  material.map = texture;
  material.color.set("#ffffff");
  material.side = FrontSide;
  const uniforms = {
    uIris: { value: new Color(irisHex) },
    uSkin: { value: new Color(skinHex) },
  };
  bindShader(material, "eye-a", uniforms, "", "", "uniform vec3 uIris;\nuniform vec3 uSkin;", EYE_FRAG);
  const compile = material.onBeforeCompile;
  material.onBeforeCompile = (shader) => {
    compile(shader);
    shader.fragmentShader = shader.fragmentShader.replace(
      "#include <opaque_fragment>",
      "#include <opaque_fragment>\n  gl_FragColor.rgb += vec3(0.95, 0.97, 1.0) * catchLight;",
    );
  };
  owned.materials.push(material);
  return material;
}

function humanPositions(human: Human, weights: Record<string, number>): Float32Array {
  const pos = new Float32Array(human.basis);
  for (const morph of human.morphs) {
    const weight = weights[morph.name] ?? 0;
    if (weight === 0) continue;
    const delta = morph.data;
    for (let i = 0; i < delta.length; i += 1) pos[i] = num(pos, i) + weight * num(delta, i);
  }
  return pos;
}

function axisScale(humanPos: Float32Array, pair: Pair, component: number): number {
  const rest = pair[2];
  if (rest === 0) return 1;
  const a = Math.trunc(pair[0]) * 3 + component;
  const b = Math.trunc(pair[1]) * 3 + component;
  return Math.abs(num(humanPos, a) - num(humanPos, b)) / rest;
}

function fitProxy(proxy: Proxy, humanPos: Float32Array): Float32Array {
  const sx = axisScale(humanPos, proxy.scale.x, 0);
  const sy = axisScale(humanPos, proxy.scale.y, 1);
  const sz = axisScale(humanPos, proxy.scale.z, 2);
  const bind = proxy.bind;
  const out = new Float32Array(proxy.count * 3);
  for (let i = 0; i < proxy.count; i += 1) {
    const o = i * 9;
    const i0 = Math.trunc(num(bind, o)) * 3;
    const i1 = Math.trunc(num(bind, o + 1)) * 3;
    const i2 = Math.trunc(num(bind, o + 2)) * 3;
    const w0 = num(bind, o + 3);
    const w1 = num(bind, o + 4);
    const w2 = num(bind, o + 5);
    out[i * 3] = w0 * num(humanPos, i0) + w1 * num(humanPos, i1) + w2 * num(humanPos, i2) + num(bind, o + 6) * sx;
    out[i * 3 + 1] = w0 * num(humanPos, i0 + 1) + w1 * num(humanPos, i1 + 1) + w2 * num(humanPos, i2 + 1) + num(bind, o + 7) * sy;
    out[i * 3 + 2] = w0 * num(humanPos, i0 + 2) + w1 * num(humanPos, i1 + 2) + w2 * num(humanPos, i2 + 2) + num(bind, o + 8) * sz;
  }
  return out;
}

function headMatrix(mesh: SkinnedMesh, head: Bone): Matrix4 {
  const index = mesh.skeleton.bones.indexOf(head);
  const inverse = mesh.skeleton.boneInverses[index];
  if (inverse === undefined) fail("the head bone has no inverse");
  return new Matrix4().multiplyMatrices(inverse, mesh.bindMatrix);
}

function toHeadLocal(positions: Float32Array, matrix: Matrix4): Float32Array {
  const out = new Float32Array(positions.length);
  const point = new Vector3();
  for (let i = 0; i < positions.length; i += 3) {
    point.fromArray(positions, i).applyMatrix4(matrix);
    out[i] = point.x;
    out[i + 1] = point.y;
    out[i + 2] = point.z;
  }
  return out;
}

function addFitted(
  mesh: SkinnedMesh,
  head: Bone,
  proxy: Proxy,
  faces: Uint32Array,
  humanPos: Float32Array,
  material: Material,
  order: number,
  nudgeZ: number,
  reshape: ((fitted: Float32Array) => Float32Array) | null,
  owned: Owned,
): void {
  const fitted = reshape === null ? fitProxy(proxy, humanPos) : reshape(fitProxy(proxy, humanPos));
  const placed = nudgeZ === 0 ? fitted : new Float32Array(fitted);
  if (nudgeZ !== 0) {
    for (let i = 2; i < placed.length; i += 3) placed[i] = num(placed, i) + nudgeZ;
  }
  const geometry = new BufferGeometry();
  geometry.setAttribute("position", new BufferAttribute(toHeadLocal(placed, headMatrix(mesh, head)), 3));
  geometry.setAttribute("aMesh", new BufferAttribute(fitted, 3));
  geometry.setAttribute("uv", new BufferAttribute(new Float32Array(proxy.uv), 2));
  geometry.setIndex(new BufferAttribute(new Uint32Array(faces), 1));
  geometry.computeVertexNormals();
  const object = new Mesh(geometry, material);
  object.frustumCulled = false;
  object.renderOrder = order;
  head.add(object);
  owned.geometries.push(geometry);
}

function sortedAt(values: number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = values.slice().sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.max(0, Math.round(p * (sorted.length - 1))));
  return sorted[index] ?? 0;
}

/** A skirt or trouser follows the body's outer hip when the pelvis key is off zero. */
function conformHips(fitted: Float32Array, rest: Float32Array, human: Human, humanPos: Float32Array, weights: Record<string, number>, skirt: boolean): void {
  if (Math.abs(weights["pelvis_width"] ?? 0) <= 0.02) return;
  const hip = human.morphs.find((morph) => morph.name === "pelvis_width");
  if (hip === undefined) return;
  const basis = human.basis;
  const delta = hip.data;
  const right: number[] = [];
  const left: number[] = [];
  let ySum = 0;
  let n = 0;
  for (let i = 0; i < basis.length; i += 3) {
    const x = num(basis, i);
    const z = num(basis, i + 2);
    if (Math.abs(num(delta, i)) < 0.015 || Math.abs(x) < 0.13 || Math.abs(z) > 0.12) continue;
    const moved = num(humanPos, i) - x;
    if (x >= 0) right.push(moved);
    else left.push(moved);
    ySum += num(basis, i + 1);
    n += 1;
  }
  if (n <= 20 || right.length === 0 || left.length === 0) return;
  const hipY = ySum / n;
  const gainR = sortedAt(right, 0.85);
  const gainL = sortedAt(left, 0.15);
  const count = (fitted.length / 3) | 0;
  for (let i = 0; i < count; i += 1) {
    const x = num(rest, i * 3);
    const y = num(rest, i * 3 + 1);
    if (y > hipY - 0.012) continue;
    const ax = Math.abs(x);
    if (ax < 0.03 || ax > 0.34) continue;
    const drop = hipY - y;
    const follow = skirt ? 1 : Math.min(1, Math.max(0, 1 - Math.max(0, drop - 0.015) / 0.16));
    if (follow <= 0) continue;
    const have = num(fitted, i * 3) - x;
    const want = x >= 0 ? gainR : gainL;
    fitted[i * 3] = num(fitted, i * 3) + (want - have) * follow;
  }
}

const CLOTH_COLOR = `
  float ao = texture(uAo, vMapUv).r;
  float seam = smoothstep(uSplit - 0.03, uSplit + 0.03, vNy);
  vec3 dye = mix(uBottom, uTop, seam);
  diffuseColor.rgb = dye * mix(0.55, 0.92, ao);
  diffuseColor.a = 1.0;
`;

function ySpan(positions: Float32Array): { minY: number; span: number } {
  let minY = Infinity;
  let maxY = -Infinity;
  for (let i = 1; i < positions.length; i += 3) {
    const y = num(positions, i);
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  if (minY === Infinity) return { minY: 0, span: 0.05 };
  return { minY, span: Math.max(maxY - minY, 0.05) };
}

function clothMap(proxy: Proxy, owned: Owned): Texture {
  if (proxy.aoImage === null) return textureOf(proxy.image, true, SRGBColorSpace, owned);
  return textureOf(proxy.aoImage, true, NoColorSpace, owned);
}

function clothMaterial(proxy: Proxy, outfit: Outfit, floor: number, span: number, owned: Owned): Material {
  if (proxy.kind === "shoes") {
    const material = new MeshStandardMaterial({ roughness: 0.64, metalness: 0 });
    material.map = textureOf(proxy.image, true, SRGBColorSpace, owned);
    material.color.setRGB(0.78, 0.76, 0.72);
    material.side = DoubleSide;
    material.polygonOffset = true;
    material.polygonOffsetFactor = -2;
    material.polygonOffsetUnits = -2;
    owned.materials.push(material);
    return material;
  }
  const map = clothMap(proxy, owned);
  const material = new MeshStandardMaterial({ roughness: 0.88, metalness: 0 });
  material.side = FrontSide;
  material.map = map;
  material.color.set("#ffffff");
  material.polygonOffset = true;
  material.polygonOffsetFactor = -1;
  material.polygonOffsetUnits = -1;
  const uniforms = {
    uTop: { value: new Color(outfit.top) },
    uBottom: { value: new Color(outfit.bottom) },
    uSplit: { value: proxy.split },
    uFloor: { value: floor },
    uSpan: { value: Math.max(span, 0.05) },
    uAo: { value: map },
  };
  bindShader(
    material,
    "cloth-a",
    uniforms,
    "varying float vNy;\nuniform float uFloor;\nuniform float uSpan;",
    "vNy = (position.y - uFloor) / max(uSpan, 1e-4);",
    "varying float vNy;\nuniform vec3 uTop;\nuniform vec3 uBottom;\nuniform float uSplit;\nuniform sampler2D uAo;",
    CLOTH_COLOR,
  );
  owned.materials.push(material);
  return material;
}

function addClothes(mesh: SkinnedMesh, pack: Pack, humanPos: Float32Array, outfit: Outfit, weights: Record<string, number>, owned: Owned): void {
  const human = pack.human;
  const basis = human.basis;
  const parent = mesh.parent;
  if (parent === null) return;
  for (const key of [outfit.suit, outfit.shoes]) {
    const proxy = proxyOf(pack, key);
    if (proxy === null || proxy.skinIndex === null || proxy.skinWeight === null) continue;
    const fitted = fitProxy(proxy, humanPos);
    if (proxy.kind !== "shoes") conformHips(fitted, fitProxy(proxy, basis), human, humanPos, weights, key === "cloth_skirt");
    const span = ySpan(fitted);
    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new BufferAttribute(fitted, 3));
    geometry.setAttribute("uv", new BufferAttribute(new Float32Array(proxy.uv), 2));
    geometry.setAttribute("skinIndex", new Uint16BufferAttribute(proxy.skinIndex, 4));
    geometry.setAttribute("skinWeight", new Float32BufferAttribute(proxy.skinWeight, 4));
    geometry.setIndex(new BufferAttribute(new Uint32Array(proxy.faces), 1));
    geometry.computeVertexNormals();
    const gap = proxy.kind === "shoes" ? 0.003 : 0.006;
    const normals = geometry.getAttribute("normal");
    if (normals !== undefined) {
      const data = normals.array;
      for (let i = 0; i < fitted.length; i += 3) {
        fitted[i] = num(fitted, i) + num(data, i) * gap;
        fitted[i + 1] = num(fitted, i + 1) + num(data, i + 1) * gap;
        fitted[i + 2] = num(fitted, i + 2) + num(data, i + 2) * gap;
      }
      const position = geometry.getAttribute("position");
      if (position !== undefined) position.needsUpdate = true;
    }
    const object = new SkinnedMesh(geometry, clothMaterial(proxy, outfit, span.minY, span.span, owned));
    object.bindMode = "attached";
    object.bind(mesh.skeleton, mesh.bindMatrix);
    object.frustumCulled = false;
    object.renderOrder = proxy.kind === "shoes" ? 2 : 1;
    object.position.copy(mesh.position);
    object.quaternion.copy(mesh.quaternion);
    object.scale.copy(mesh.scale);
    parent.add(object);
    owned.geometries.push(geometry);
  }
}

function proxyOf(pack: Pack, key: string): Proxy | null {
  return pack.proxies[key] ?? null;
}

function browFaces(proxy: Proxy, humanPos: Float32Array): Uint32Array {
  const fitted = fitProxy(proxy, humanPos);
  const faces = proxy.faces;
  const kept: number[] = [];
  for (let t = 0; t < faces.length; t += 3) {
    const a = num(faces, t);
    const b = num(faces, t + 1);
    const c = num(faces, t + 2);
    const cx = (num(fitted, a * 3) + num(fitted, b * 3) + num(fitted, c * 3)) / 3;
    if (Math.abs(cx) < 0.02) continue;
    kept.push(a, b, c);
  }
  return new Uint32Array(kept);
}

function thickenBrows(fitted: Float32Array): Float32Array {
  let leftY = 0;
  let leftN = 0;
  let rightY = 0;
  let rightN = 0;
  for (let i = 0; i < fitted.length; i += 3) {
    if (num(fitted, i) < 0) {
      leftY += num(fitted, i + 1);
      leftN += 1;
    } else {
      rightY += num(fitted, i + 1);
      rightN += 1;
    }
  }
  if (leftN !== 0) leftY /= leftN;
  if (rightN !== 0) rightY /= rightN;
  const out = new Float32Array(fitted);
  for (let i = 0; i < out.length; i += 3) {
    const mid = num(out, i) < 0 ? leftY : rightY;
    out[i + 1] = mid + (num(out, i + 1) - mid) * 1.12;
  }
  return out;
}

function hairCrown(proxy: Proxy, humanPos: Float32Array): [number, number, number] {
  const fitted = fitProxy(proxy, humanPos);
  let best = 0;
  for (let i = 0; i < fitted.length; i += 3) {
    if (num(fitted, i + 1) > num(fitted, best + 1)) best = i;
  }
  return [num(fitted, best), num(fitted, best + 1), num(fitted, best + 2)];
}

function addGrooming(
  mesh: SkinnedMesh,
  head: Bone,
  pack: Pack,
  humanPos: Float32Array,
  body: BodyParams,
  sex: Sex,
  simId: string,
  hairHex: string,
  irisHex: string,
  skinHex: string,
  marks: Marks,
  owned: Owned,
): void {
  const hairName = wornHair(body, sex, simId);
  const hair = proxyOf(pack, hairName);
  const density = Math.min(1, Math.max(0, body.hair_density));
  if (density > 0.02 && hair !== null) {
    const bald = hairName === "hair_short" ? Math.min(1, Math.max(0, body.balding_stage) / 7) : 0;
    const threshold = MathUtils.lerp(0.9, 0.2, density);
    const crown = hairCrown(hair, humanPos);
    addFitted(
      mesh,
      head,
      hair,
      hair.faces,
      humanPos,
      strandMaterial(textureOf(hair.image, true, SRGBColorSpace, owned), hairHex, threshold, bald, crown, -1, true, true, HAIR_CAP[hairName] ?? 0.04, Math.min(1, Math.max(0, body.hair_curl)), owned),
      2,
      0,
      null,
      owned,
    );
  }
  const brows = proxyOf(pack, "brows");
  const browDensity = Math.min(1, Math.max(0, body.eyebrow_density));
  if (brows !== null && browDensity > 0.02) {
    const browCut = MathUtils.lerp(0.38, 0.15, browDensity);
    addFitted(
      mesh,
      head,
      brows,
      browFaces(brows, humanPos),
      humanPos,
      strandMaterial(textureOf(brows.image, true, SRGBColorSpace, owned), hairHex, browCut, 0, marks.crown, -8, false, false, 0, 0, owned),
      3,
      0.003,
      thickenBrows,
      owned,
    );
  }
  const eyes = proxyOf(pack, "eyes");
  if (eyes !== null) {
    // The eyeballs stay in their sockets, so the lids cover the upper and lower white.
    // Pushed further forward they stand in front of the lids and stare.
    addFitted(mesh, head, eyes, eyes.faces, humanPos, eyeMaterial(textureOf(eyes.image, true, SRGBColorSpace, owned), irisHex, skinHex, owned), 1, EYE_FORWARD, null, owned);
  }
}

function hiddenFaces(geometry: BufferGeometry, hm: Int32Array, deleted: Set<number>): void {
  const index = geometry.index;
  if (index === null) return;
  const src = index.array;
  const kept: number[] = [];
  for (let t = 0; t < src.length; t += 3) {
    const a = num(hm, num(src, t));
    const b = num(hm, num(src, t + 1));
    const c = num(hm, num(src, t + 2));
    if (a >= 0 && b >= 0 && c >= 0 && deleted.has(a) && deleted.has(b) && deleted.has(c)) continue;
    kept.push(num(src, t), num(src, t + 1), num(src, t + 2));
  }
  geometry.setIndex(new BufferAttribute(new Uint32Array(kept), 1));
}

function deleteSet(pieces: (Proxy | null)[]): Set<number> {
  const set = new Set<number>();
  for (const proxy of pieces) {
    if (proxy === null || proxy.deleteIndices === null) continue;
    for (let i = 0; i < proxy.deleteIndices.length; i += 1) set.add(num(proxy.deleteIndices, i));
  }
  return set;
}

const FRONT = new Vector3(0, 0, 1);

/**
 * Turns `bone` about the axis toward the viewer until the line from it to `tip`, seen
 * from the front, points along (`outX`, -1). Returns the angle it turned by.
 */
function swing(bone: Bone, tip: Bone, outX: number): number {
  const origin = bone.getWorldPosition(new Vector3());
  const line = tip.getWorldPosition(new Vector3()).sub(origin);
  line.z = 0;
  if (line.lengthSq() < 1e-8) return 0;
  line.normalize();
  const target = new Vector3(outX, -1, 0).normalize();
  const angle = Math.atan2(line.x * target.y - line.y * target.x, line.x * target.x + line.y * target.y);
  turnFront(bone, angle);
  return angle;
}

/** Turns `bone` by `angle` about the axis toward the viewer, whatever its own axes are. */
function turnFront(bone: Bone, angle: number): void {
  turnAbout(bone, FRONT, angle);
}

/** Turns `bone` by `angle` about `axis` of the scene, whatever its own axes are. */
function turnAbout(bone: Bone, axis: Vector3, angle: number): void {
  const parent = bone.parent;
  if (parent === null) return;
  const world = bone.getWorldQuaternion(new Quaternion()).premultiply(new Quaternion().setFromAxisAngle(axis, angle));
  bone.quaternion.copy(parent.getWorldQuaternion(new Quaternion()).invert().multiply(world));
  bone.updateWorldMatrix(true, true);
}

/**
 * The relaxed stance: the arms hang beside the body with a slight bend at the elbow, and
 * the feet stand under the hips. The mesh rests with arms and legs spread.
 */
function relax(root: Object3D, child: number, baby: number): void {
  const armOut = ARM_OUT + CHILD_ARM_OUT * Math.min(Math.max(child, 0), 1);
  root.updateWorldMatrix(true, true);
  for (const side of ["l", "r"]) {
    const sign = side === "l" ? 1 : -1;
    const upper = findBone(root, `upperarm_${side}`);
    const lower = findBone(root, `lowerarm_${side}`);
    if (upper !== null && lower !== null) {
      swing(upper, lower, sign * armOut);
      lower.quaternion.setFromAxisAngle(new Vector3(1, 0, 0), 0.22);
      upper.updateWorldMatrix(true, true);
    }
    const thigh = findBone(root, `thigh_${side}`);
    const calf = findBone(root, `calf_${side}`);
    const foot = findBone(root, `foot_${side}`);
    if (thigh !== null && calf !== null && foot !== null) {
      // Thigh and lower leg are straightened each (a small child's legs rest bowed), and
      // the foot turns back by both angles, so the sole stays flat on the ground.
      const upperTurn = swing(thigh, calf, sign * (LEG_OUT - BABY_KNEE_IN * baby));
      const lowerTurn = swing(calf, foot, sign * LEG_OUT);
      turnFront(foot, -upperTurn - lowerTurn);
    }
  }
}

const SIDE = new Vector3(1, 0, 0);
const UP = new Vector3(0, 1, 0);

/**
 * Seats a relaxed figure on the ground: the thighs point forward and a little apart,
 * the knees bend so the feet rest, and the whole figure comes down until its seat is on
 * the ground. `wrapper` is the figure's outer group.
 */
function sit(wrapper: Object3D): void {
  let hip = 0;
  let sides = 0;
  for (const side of ["l", "r"]) {
    const sign = side === "l" ? 1 : -1;
    const thigh = findBone(wrapper, `thigh_${side}`);
    const calf = findBone(wrapper, `calf_${side}`);
    const upper = findBone(wrapper, `upperarm_${side}`);
    // The arms come forward, so the hands rest on the thighs and not below the ground.
    if (upper !== null) turnAbout(upper, SIDE, -SIT_ARM);
    if (thigh === null || calf === null) continue;
    turnAbout(thigh, SIDE, -SIT_THIGH);
    turnAbout(thigh, UP, sign * SIT_SPREAD);
    turnAbout(calf, SIDE, SIT_KNEE);
    const place = thigh.getWorldPosition(new Vector3());
    const knee = calf.getWorldPosition(new Vector3());
    hip += place.y - SIT_SEAT * place.sub(knee).length();
    sides += 1;
  }
  if (sides > 0) wrapper.position.y -= hip / sides;
  wrapper.updateWorldMatrix(true, true);
}

/** Every card frames the same height, so a child stands smaller than its parents. */
function frameCamera(camera: PerspectiveCamera, heightCm: number): void {
  const heightM = heightCm / 100;
  const aspect = FIGURE_WIDTH / FIGURE_HEIGHT;
  camera.fov = 26;
  camera.aspect = aspect;
  camera.near = 0.05;
  camera.far = 80;
  const vertical = MathUtils.degToRad(camera.fov);
  const horizontal = 2 * Math.atan(Math.tan(vertical / 2) * aspect);
  const lookY = heightM * 0.48;
  const distH = heightM / FRAME_FILL / (2 * Math.tan(vertical / 2));
  const distW = framedBodyM(heightM) / (2 * Math.tan(horizontal / 2));
  const distance = Math.max(distH, distW);
  camera.position.set(0, lookY, distance);
  camera.lookAt(0, lookY, 0);
  camera.updateProjectionMatrix();
  camera.updateMatrixWorld(true);
}

/**
 * The row frames {@link FRAME_CM} the way the card does, and steps back when the
 * members side by side need more width than that frame already shows.
 */
function frameFamily(camera: PerspectiveCamera, count: number): void {
  const width = familyCanvasWidth(count);
  const height = FAMILY_HEIGHT_PX;
  const viewW = familyViewWidthM(count);
  const aspect = width / height;
  const viewH = viewW / aspect;
  camera.fov = 26;
  camera.aspect = aspect;
  camera.near = 0.05;
  camera.far = 80;
  const tanV = Math.tan(MathUtils.degToRad(camera.fov) / 2);
  const lookY = (FRAME_CM / 100) * 0.48;
  camera.position.set(0, lookY, viewH / (2 * tanV));
  camera.lookAt(0, lookY, 0);
  camera.updateProjectionMatrix();
  camera.updateMatrixWorld(true);
}

/** One horizontal drag yaws every figure by the same angle, each about its own feet. */
function turn(canvas: HTMLCanvasElement, figures: readonly Object3D[], render: () => void): () => void {
  let yaw = 0;
  let dragging = false;
  let lastX = 0;
  const down = (event: PointerEvent) => {
    dragging = true;
    lastX = event.clientX;
    canvas.setPointerCapture(event.pointerId);
  };
  const move = (event: PointerEvent) => {
    if (!dragging) return;
    const dx = event.clientX - lastX;
    lastX = event.clientX;
    yaw += dx * 0.01;
    for (const figure of figures) figure.rotation.y = yaw;
    render();
  };
  const up = (event: PointerEvent) => {
    dragging = false;
    if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
  };
  canvas.addEventListener("pointerdown", down);
  canvas.addEventListener("pointermove", move);
  canvas.addEventListener("pointerup", up);
  canvas.addEventListener("pointercancel", up);
  return () => {
    canvas.removeEventListener("pointerdown", down);
    canvas.removeEventListener("pointermove", move);
    canvas.removeEventListener("pointerup", up);
    canvas.removeEventListener("pointercancel", up);
  };
}

function buildFigure(assets: Assets, body: BodyParams, sex: Sex, simId: string, keyDir: Vector3, anisotropy: number, owned: Owned): Group {
  const root = cloneSkeleton(assets.template);
  const mesh = findSkinned(root);
  owned.skeletons.push(mesh.skeleton);
  takeOwnership(mesh, owned);
  const weights = morphWeights(body, sex);
  applyWeights(mesh, weights);
  smoothSkinNormals(mesh, assets.groups);
  const cloud = morphedCloud(mesh, weights);
  fitJoints(root, mesh, cloud, assets.rings);
  const measured = morphedYSpan(mesh, weights);
  const standingCm = measured.span * 100;
  const scale = standingCm > 1 ? body.height_cm / standingCm : 1;
  const bone = findBone(root, "Root");
  const head = findBone(root, "head") ?? fail("body has no head");
  const pivot = bone === null ? assets.pack.groundShift : bone.position.y;
  if (bone !== null) bone.scale.set(scale, scale, scale);
  const wrapper = new Group();
  wrapper.add(root);
  wrapper.position.y = -pivot * (1 - scale);
  const marks = landmarks(cloud);
  const skin = skinColor(body);
  const hair = hairColor(body);
  const iris = irisColor(body);
  const skinMap = textureOf(assets.skin, false, SRGBColorSpace, owned);
  skinMap.anisotropy = anisotropy;
  mesh.material = paintSkin(skin, skinMap, marks, keyDir, owned);
  mesh.frustumCulled = false;
  const humanPos = humanPositions(assets.pack.human, weights);
  const outfit = clothes(sex, simId, body.body_form_age);
  const suit = proxyOf(assets.pack, outfit.suit);
  const shoes = proxyOf(assets.pack, outfit.shoes);
  hiddenFaces(mesh.geometry, assets.restIndex, deleteSet([suit, shoes]));
  addGrooming(mesh, head, assets.pack, humanPos, body, sex, simId, hair, iris, skin, marks, owned);
  addClothes(mesh, assets.pack, humanPos, outfit, weights, owned);
  return wrapper;
}

/**
 * Frees this mount's textures and cloned skeletons before the renderer. Uploading a
 * texture attaches a dispose listener that would keep the renderer alive (three r170).
 * `dispose` itself leaves the WebGL context on the canvas. Losing it while the canvas
 * stays mounted would hand the next mount a dead context, because `getContext` returns
 * that one and `dispose` has removed three's restore listener. A canvas that has left
 * the document would keep the context until collection, so lose it then.
 */
function freeMount(owned: Owned, renderer: WebGLRenderer): void {
  for (const texture of owned.textures) texture.dispose();
  for (const skeleton of owned.skeletons) skeleton.dispose();
  for (const geometry of owned.geometries) geometry.dispose();
  for (const material of owned.materials) material.dispose();
  const canvas = renderer.domElement;
  renderer.dispose();
  if (!canvas.isConnected) renderer.forceContextLoss();
}

/** Set `cancelled` to stop a mount before it creates a renderer. */
export type FigureCancel = { cancelled: boolean };

/** One standing person in a mount: the server's body, and the sex and sim the clothes follow. */
export type FigureMember = {
  readonly body: BodyParams;
  readonly sex: Sex;
  readonly simId: string;
};

/**
 * One renderer, one scene, the members in order at `xs` on the ground `buildFigure`
 * already sets. A horizontal drag turns them all. The returned function frees this
 * mount's textures, skeletons, geometry, materials and renderer. Decoded images and
 * the template stay loaded. `onLost(true)` means the WebGL context is gone;
 * `onLost(false)` means a restore has been drawn.
 */
function mountMembers(
  canvas: HTMLCanvasElement,
  members: readonly FigureMember[],
  width: number,
  height: number,
  xs: readonly number[],
  frame: (camera: PerspectiveCamera) => void,
  cancel: FigureCancel,
  onLost: (lost: boolean) => void,
): Promise<() => void> {
  return loadAssets().then((assets) => {
    if (cancel.cancelled) return () => {};
    const owned = freshOwned();
    if (failedCanvases.has(canvas)) throw new Error("no WebGL");
    let renderer: WebGLRenderer;
    try {
      renderer = new WebGLRenderer({ canvas, antialias: true });
    } catch (error) {
      failedCanvases.add(canvas);
      throw error instanceof Error ? error : new Error("no WebGL");
    }
    let undo = () => {};
    let done = false;
    const release = () => {
      if (done) return;
      done = true;
      undo();
      freeMount(owned, renderer);
    };
    try {
      renderer.setPixelRatio(Math.min(devicePixelRatio || 1, 2));
      renderer.setSize(width, height, false);
      renderer.outputColorSpace = SRGBColorSpace;
      renderer.toneMapping = ACESFilmicToneMapping;
      renderer.toneMappingExposure = 1;
      renderer.setClearColor(CLEAR, 1);
      const scene = new Scene();
      const camera = new PerspectiveCamera(26, width / height, 0.05, 80);
      const hemi = new HemisphereLight(0xffe4cc, 0xc9d0dc, Math.PI * 0.32);
      const key = new DirectionalLight(0xffc9a0, Math.PI * 0.48);
      key.position.set(-2.2, 4.6, 4.0);
      const fill = new DirectionalLight(0xb7c7dc, Math.PI * 0.14);
      fill.position.set(4.6, 1.8, 2.4);
      const rim = new DirectionalLight(0xffd0b0, Math.PI * 0.16);
      rim.position.set(0.6, 3.4, -4.4);
      scene.add(hemi, key, fill, rim);
      const keyDir = new Vector3();
      const anisotropy = renderer.capabilities.getMaxAnisotropy();
      const standing: Group[] = [];
      for (let i = 0; i < members.length; i += 1) {
        const member = members[i] ?? fail("a figure is missing");
        const wrapper = buildFigure(assets, member.body, member.sex, member.simId, keyDir, anisotropy, owned);
        wrapper.position.x = xs[i] ?? 0;
        scene.add(wrapper);
        const [baby, child] = ageSlices(member.body.body_form_age);
        relax(wrapper, baby + child, baby);
        // A baby does not stand yet.
        if (baby === 1) sit(wrapper);
        standing.push(wrapper);
      }
      frame(camera);
      keyDir.copy(key.position).normalize().transformDirection(camera.matrixWorldInverse);
      let lost = false;
      const render = () => {
        if (lost) return;
        renderer.render(scene, camera);
      };
      const onContextLost = (event: Event) => {
        event.preventDefault();
        lost = true;
        onLost(true);
      };
      const onContextRestored = () => {
        lost = false;
        render();
        onLost(false);
      };
      canvas.addEventListener("webglcontextlost", onContextLost);
      canvas.addEventListener("webglcontextrestored", onContextRestored);
      const undrag = turn(canvas, standing, render);
      undo = () => {
        undrag();
        canvas.removeEventListener("webglcontextlost", onContextLost);
        canvas.removeEventListener("webglcontextrestored", onContextRestored);
      };
      render();
      return release;
    } catch (error) {
      release();
      throw error instanceof Error ? error : new Error("the figure failed");
    }
  });
}

/**
 * Draws `body` on `canvas` and lets a horizontal drag turn it. The returned function
 * frees this card's textures, skeleton, geometry, materials and renderer. Decoded images
 * and the template stay loaded for the next card. `onLost(true)` means the WebGL context
 * is gone and the portrait should show; `onLost(false)` means a restore has been drawn.
 */
export function mountFigure(
  canvas: HTMLCanvasElement,
  body: BodyParams,
  sex: Sex,
  simId: string,
  cancel: FigureCancel,
  onLost: (lost: boolean) => void,
): Promise<() => void> {
  return mountMembers(
    canvas,
    [{ body, sex, simId }],
    FIGURE_WIDTH,
    FIGURE_HEIGHT,
    [0],
    (camera) => frameCamera(camera, FRAME_CM),
    cancel,
    onLost,
  );
}

/**
 * Draws `members` in order, side by side on one ground, with one renderer. At most
 * eight. An empty row does not take a context. A horizontal drag turns them all.
 */
export function mountFamily(
  canvas: HTMLCanvasElement,
  members: readonly FigureMember[],
  cancel: FigureCancel,
  onLost: (lost: boolean) => void,
): Promise<() => void> {
  const drawn = members.slice(0, FAMILY_LIMIT);
  if (drawn.length === 0) return Promise.resolve(() => {});
  const places = familyPlaces(
    drawn.map((member) => member.body.height_cm),
    FAMILY_SPACING_M,
  );
  return mountMembers(
    canvas,
    drawn,
    familyCanvasWidth(drawn.length),
    FAMILY_HEIGHT_PX,
    places.map((place) => place.x),
    (camera) => frameFamily(camera, drawn.length),
    cancel,
    onLost,
  );
}
