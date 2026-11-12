import * as THREE from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { RGBELoader } from "three/addons/loaders/RGBELoader.js";
import { clone as cloneSkeleton } from "three/addons/utils/SkeletonUtils.js";
import { EffectComposer } from "three/addons/postprocessing/EffectComposer.js";
import { OutputPass } from "three/addons/postprocessing/OutputPass.js";
import { ShaderPass } from "three/addons/postprocessing/ShaderPass.js";
import { FXAAShader } from "three/addons/shaders/FXAAShader.js";
import { N8AOPass } from "https://cdn.jsdelivr.net/npm/n8ao@2.0.1/dist/N8AO.js";

const SKIN = {
  eumelanin: [0, 0.25, 0.5, 0.75, 1],
  pheomelanin: [0, 0.25, 0.5, 0.75, 1],
  srgb: [
    ["#f7e6dc", "#f6e2d3", "#f5decb", "#f5d9c1", "#f4d5b7"],
    ["#b09588", "#af9282", "#af8f7c", "#ae8c76", "#ad8970"],
    ["#7c5e52", "#7c5c4e", "#7b5b4a", "#7b5947", "#7a5742"],
    ["#563a2f", "#56392d", "#56382a", "#553628", "#553525"],
    ["#3b2219", "#3b2118", "#3b2016", "#3a2014", "#3a1f13"],
  ],
};
const HAIR = {
  eumelanin: [0, 0.25, 0.5, 0.75, 1],
  pheomelanin: [0, 0.25, 0.5, 0.75, 1],
  srgb: [
    ["#e9dfc6", "#e6cfb1", "#e3bd97", "#dfa977", "#ee5a28"],
    ["#8c8171", "#8a7764", "#886c55", "#866041", "#845123"],
    ["#52483e", "#504236", "#4f3b2d", "#4e3421", "#4d2b0f"],
    ["#2d261f", "#2d221b", "#2c1e15", "#2b1a0e", "#2a1405"],
    ["#17110d", "#170f0a", "#160d07", "#160a04", "#150701"],
  ],
};
const IRIS = {
  melanin: [0, 0.25, 0.5, 0.75, 1],
  srgb: ["#8fb3d9", "#7c9f8c", "#9a7a3c", "#6b4423", "#2e1a0e"],
};
const GREY = "#c4c4c4";
const AGE_KEYS = [
  "age_male_child", "age_male_young", "age_male_old",
  "age_female_child", "age_female_young", "age_female_old",
];
const FAT_MUSCLE = [
  "fat_male_max", "fat_male_min", "fat_female_max", "fat_female_min",
  "muscle_male_max", "muscle_male_min", "muscle_female_max", "muscle_female_min",
  "fat_belly", "fat_upperarm", "muscle_arm", "muscle_shoulder", "muscle_chest",
];
const MEASURE_KEYS = new Set([...AGE_KEYS, ...FAT_MUSCLE]);
const HAIR_STYLES = [
  { id: "hair_short", label: "kurz", cap: 0.042 },
  { id: "hair_medium", label: "mittel", cap: 0.048 },
  { id: "hair_long", label: "lang", cap: 0.05 },
  { id: "hair_tied", label: "gebunden", cap: 0.036 },
  { id: "hair_braid", label: "Zopf", cap: 0.04 },
];

const params = new URLSearchParams(location.search);
const editParam = params.get("edit");
const editMode = editParam === "2" ? 2 : editParam === "1" ? 1 : 0;
const state = {
  style: ["A", "B", "C"].includes(params.get("style")) ? params.get("style") : "A",
  close: params.get("close") === "1",
  focus: params.get("focus") || "",
  edit: editMode > 0,
  editMode,
  editIndex: 0,
  editBody: null,
  editBase: null,
  provisional: null,
  editYaw: 0,
  directRegion: "",
  directSide: 1,
  hoverRegion: "",
  cursorPinned: false,
  category: "",
  past: [],
  future: [],
  pickDirty: true,
  hairStyle: "",
  hairDye: "",
  compare: ["fat", "muscle"].includes(params.get("compare")) ? params.get("compare") : "",
  child: params.get("child") === "1",
  still: params.get("still") === "1",
  people: [],
  facePage: Math.max(0, (Number(params.get("page")) || 1) - 1),
  clock: 0,
};
const FACE_COUNT = 5;

const canvas = document.querySelector("#stage");
const labels = document.querySelector("#labels");
const status = document.querySelector("#status");
const caption = document.querySelector("#caption");
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.toneMapping = THREE.NoToneMapping;
renderer.toneMappingExposure = 1;
renderer.shadowMap.type = THREE.PCFSoftShadowMap;
renderer.setClearColor(0xe7e2da, 1);

const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(26, 1, 0.05, 80);
const hemi = new THREE.HemisphereLight(0xfffaf6, 0xe6ddd2, Math.PI * 0.42);
const key = new THREE.DirectionalLight(0xfff6ee, Math.PI * 0.38);
key.position.set(-2.4, 5.2, 4.6);
key.shadow.mapSize.set(2048, 2048);
key.shadow.bias = -0.00025;
key.shadow.normalBias = 0.02;
key.shadow.radius = 4;
key.shadow.camera.near = 0.4;
key.shadow.camera.far = 24;
key.shadow.camera.left = -8;
key.shadow.camera.right = 8;
key.shadow.camera.top = 8;
key.shadow.camera.bottom = -8;
key.shadow.camera.updateProjectionMatrix();
const fill = new THREE.DirectionalLight(0xf6efe6, Math.PI * 0.12);
fill.position.set(4.2, 2.2, 3);
const rim = new THREE.DirectionalLight(0xffd8c2, Math.PI * 0.16);
rim.position.set(0.6, 3.4, -4.4);
scene.add(hemi, key, fill, rim);
const keyDirView = new THREE.Vector3();

let studioMap = null;

function applyStyleLights(style) {
  const grade = style === "A";
  // ACES runs once, in OutputPass. A render target skips the material pass,
  // so styles B and C stay on the direct path with no tone map.
  renderer.toneMapping = grade ? THREE.ACESFilmicToneMapping : THREE.NoToneMapping;
  renderer.shadowMap.enabled = grade;
  key.castShadow = grade;
  ground.receiveShadow = grade;
  scene.environment = grade ? studioMap : null;
  scene.environmentIntensity = grade ? 0.18 : 1;
  const lit = style !== "C";
  hemi.visible = lit;
  key.visible = lit;
  fill.visible = lit;
  rim.visible = style === "A";
  if (style === "A") {
    hemi.color.setHex(0xffe4cc);
    hemi.groundColor.setHex(0xc9d0dc);
    hemi.intensity = Math.PI * 0.32;
    key.color.setHex(0xffc9a0);
    key.intensity = Math.PI * 0.48;
    key.position.set(-2.2, 4.6, 4.0);
    fill.color.setHex(0xb7c7dc);
    fill.intensity = Math.PI * 0.14;
    fill.position.set(4.6, 1.8, 2.4);
    rim.color.setHex(0xffd0b0);
    rim.intensity = Math.PI * 0.16;
    return;
  }
  hemi.color.setHex(0xfffaf6);
  hemi.groundColor.setHex(0xe6ddd2);
  hemi.intensity = Math.PI * 0.42;
  key.color.setHex(0xfff6ee);
  key.intensity = Math.PI * 0.38;
  key.position.set(-2.4, 5.2, 4.6);
  fill.color.setHex(0xf6efe6);
  fill.intensity = Math.PI * 0.12;
  fill.position.set(4.2, 2.2, 3);
}

function updateKeyDir() {
  keyDirView.copy(key.position).normalize().transformDirection(camera.matrixWorldInverse);
}

// Poly Haven maps are metres: wood boards are 1.7 m, plaster is 2 m.
// The wall is tall enough to fill the wide lineup frame. The shelf stands
// in the gap before the palette checks (centers near x=2.94 and x=4.87).
const WOOD_M = 1.7;
const PLASTER_M = 2;
const FLOOR_M = 80;
const WALL_W = 24;
const WALL_H = 7;
const roomLoads = new THREE.LoadingManager();
let roomFailed = false;
roomLoads.onLoad = () => {
  if (!roomFailed) document.body.dataset.room = "1";
};
roomLoads.onError = () => {
  roomFailed = true;
  document.body.dataset.room = "0";
};
const texLoader = new THREE.TextureLoader(roomLoads);

function tiled(url, across, down, colorSpace) {
  const map = texLoader.load(url);
  map.colorSpace = colorSpace;
  map.wrapS = THREE.RepeatWrapping;
  map.wrapT = THREE.RepeatWrapping;
  map.repeat.set(across, down);
  map.anisotropy = 8;
  return map;
}

const woodRepeat = FLOOR_M / WOOD_M;
const woodDiff = tiled("/cache/room/wood_floor_diff_1k.jpg", woodRepeat, woodRepeat, THREE.SRGBColorSpace);
const woodRough = tiled("/cache/room/wood_floor_rough_1k.jpg", woodRepeat, woodRepeat, THREE.NoColorSpace);
const woodNor = tiled("/cache/room/wood_floor_nor_gl_1k.jpg", woodRepeat, woodRepeat, THREE.NoColorSpace);
const plasterDiff = tiled(
  "/cache/room/plastered_wall_diff_1k.jpg",
  WALL_W / PLASTER_M,
  WALL_H / PLASTER_M,
  THREE.SRGBColorSpace,
);

function backdropMaterial(map, extra) {
  if (state.style === "C") return new THREE.MeshBasicMaterial({ map, toneMapped: false });
  return new THREE.MeshStandardMaterial(Object.assign({ map, roughness: 1, metalness: 0 }, extra));
}

const ground = new THREE.Mesh(new THREE.PlaneGeometry(FLOOR_M, FLOOR_M), backdropMaterial(woodDiff, {
  roughnessMap: woodRough,
  normalMap: woodNor,
}));
ground.rotation.x = -Math.PI / 2;
ground.position.y = -0.002;
scene.add(ground);

const wall = new THREE.Mesh(
  new THREE.PlaneGeometry(WALL_W, WALL_H),
  backdropMaterial(plasterDiff, { roughness: 0.94 }),
);
wall.position.set(0, WALL_H / 2, -1.5);
wall.receiveShadow = true;
scene.add(wall);

function assignBackdrop() {
  const previousGround = ground.material;
  ground.material = backdropMaterial(woodDiff, { roughnessMap: woodRough, normalMap: woodNor });
  previousGround.dispose();
  const previousWall = wall.material;
  wall.material = backdropMaterial(plasterDiff, { roughness: 0.94 });
  previousWall.dispose();
}

// Shelf_01 is already in metres (1.00 x 2.08 x 0.26). Back face is local z=0.
new GLTFLoader(roomLoads).load("/cache/room/Shelf_01_1k.gltf", (gltf) => {
  const shelf = gltf.scene;
  shelf.position.set(3.9, 0, -0.55);
  shelf.traverse((node) => {
    if (!node.isMesh) return;
    node.castShadow = true;
    node.receiveShadow = true;
  });
  scene.add(shelf);
});

// SAO stained the cloth and haloed the silhouette. N8AO renders the
// beauty itself, so the composer target stays single-sample. FXAA stands
// in for MSAA on style A. OutputPass applies ACES and sRGB once.
const drawSize = new THREE.Vector2();
const composer = new EffectComposer(renderer);
const n8aoPass = new N8AOPass(scene, camera);
n8aoPass.configuration.aoRadius = 0.1;
n8aoPass.configuration.distanceFalloff = 0.45;
n8aoPass.configuration.intensity = 2.2;
n8aoPass.configuration.gammaCorrection = false;
n8aoPass.configuration.screenSpaceRadius = false;
n8aoPass.setQualityMode("High");
const fxaaPass = new ShaderPass(FXAAShader);
composer.addPass(n8aoPass);
composer.addPass(fxaaPass);
composer.addPass(new OutputPass());

const pmrem = new THREE.PMREMGenerator(renderer);
new RGBELoader().load("/cache/hdri/studio_small_03_1k.hdr", (hdr) => {
  studioMap = pmrem.fromEquirectangular(hdr).texture;
  hdr.dispose();
  pmrem.dispose();
  if (state.style === "A") scene.environment = studioMap;
  document.body.dataset.env = "1";
}, undefined, () => {
  document.body.dataset.env = "0";
  console.error("studio HDR failed to load");
});

const toonGradient = makeGradient();
const clock = new THREE.Clock();

function clamp(value, lo, hi) {
  return Math.min(hi, Math.max(lo, value));
}

function toLinear(channel) {
  return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
}

function toSrgb(linear) {
  const value = linear <= 0.0031308 ? 12.92 * linear : 1.055 * linear ** (1 / 2.4) - 0.055;
  return clamp(value, 0, 1);
}

function linearOf(hex) {
  const byte = (at) => Number.parseInt(hex.slice(at, at + 2), 16) / 255;
  return [toLinear(byte(1)), toLinear(byte(3)), toLinear(byte(5))];
}

function hexOf(channels) {
  const byte = (channel) => Math.floor(toSrgb(channel) * 255 + 0.5);
  return "#" + channels.map((channel) => byte(channel).toString(16).padStart(2, "0")).join("");
}

function mixLinear(a, b, share) {
  const t = clamp(share, 0, 1);
  return a.map((channel, index) => channel * (1 - t) + b[index] * t);
}

function cell(points, value) {
  const last = points.length - 2;
  let index = 0;
  while (index < last && value > points[index + 1]) index += 1;
  const low = points[index];
  const high = points[index + 1];
  return [index, (value - low) / (high - low)];
}

function gridColor(grid, eumelanin, pheomelanin) {
  const [i, t] = cell(grid.eumelanin, clamp(eumelanin, 0, 1));
  const [j, u] = cell(grid.pheomelanin, clamp(pheomelanin, 0, 1));
  const at = (row, col) => linearOf(grid.srgb[row][col]);
  const low = mixLinear(at(i, j), at(i, j + 1), u);
  const high = mixLinear(at(i + 1, j), at(i + 1, j + 1), u);
  return hexOf(mixLinear(low, high, t));
}

function rampColor(ramp, melanin) {
  const [i, t] = cell(ramp.melanin, clamp(melanin, 0, 1));
  return hexOf(mixLinear(linearOf(ramp.srgb[i]), linearOf(ramp.srgb[i + 1]), t));
}

function hairColor(body) {
  const own = gridColor(HAIR, body.hair_eumelanin, body.hair_pheomelanin);
  return hexOf(mixLinear(linearOf(own), linearOf(GREY), body.hair_grey_fraction));
}

function hashSim(id) {
  let hash = 2166136261;
  const text = String(id);
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

function personKey(member) {
  if (member.sim != null) return String(member.sim);
  return `palette:${member.age_years}`;
}

// Length is not a gene and not an age. The same sim id always picks the same
// proxy. A balding man is forced onto the short cut so the crown can open.
function hairStyleOf(body, sim) {
  if (body.sex === "male" && body.balding_stage >= 1) return "hair_short";
  return HAIR_STYLES[hashSim(sim) % HAIR_STYLES.length].id;
}

function hairMeta(id) {
  return HAIR_STYLES.find((style) => style.id === id) || HAIR_STYLES[0];
}

function normalCdf(z) {
  const sign = z < 0 ? -1 : 1;
  const x = Math.abs(z) / Math.SQRT2;
  const a1 = 0.254829592;
  const a2 = -0.284496736;
  const a3 = 1.421413741;
  const a4 = -1.453152027;
  const a5 = 1.061405429;
  const p = 0.3275911;
  const t = 1 / (1 + p * x);
  const erf = 1 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * Math.exp(-x * x);
  return 0.5 * (1 + sign * erf);
}

function zReadout(z, less, more) {
  const pct = Math.round(clamp(normalCdf(z), 0, 1) * 100);
  if (pct === 50) return "durchschnittlich";
  if (pct > 50) return `${more} als ${pct} % der Menschen`;
  return `${less} als ${100 - pct} % der Menschen`;
}

function ageSlices(age) {
  if (age <= 12) return [1, 0, 0];
  if (age <= 25) return [(25 - age) / 13, (age - 12) / 13, 0];
  if (age <= 70) return [0, (70 - age) / 45, (age - 25) / 45];
  return [0, 0, 1];
}

function branchShares(body) {
  const androgen = clamp(body.androgenic_development, 0, 1);
  const estrogen = clamp(body.estrogenic_development, 0, 1);
  const male = 0.5 + 0.5 * androgen - 0.5 * estrogen;
  const female = 0.5 - 0.5 * androgen + 0.5 * estrogen;
  return { androgen, estrogen, male, female, gender: male };
}

function morphWeights(body, sex, emphasise, extra, editor) {
  const age = body.body_form_age;
  const [child, young, old] = ageSlices(age);
  const share = branchShares(body);
  const { male, female } = share;
  const weights = {};
  const slices = [child, young, old];
  ["child", "young", "old"].forEach((name, index) => {
    weights[`age_male_${name}`] = slices[index] * male;
    weights[`age_female_${name}`] = slices[index] * female;
  });
  // The universal weight file moves its largest waist vertices about 2.9 cm
  // at weight 1, and much less on the shoulders. Weight 6 folded that waist
  // through the clothes. 1.5 keeps the peak near 4 cm. The muscle file drops
  // the shoulder about 4 cm at weight 1, so it stays under one half and the visible
  // bulk is the arm, shoulder and chest targets. Both are added on the age blend.
  const fat = clamp(body.body_fat_z / 2, -1, 1);
  const muscle = clamp(body.muscle_z / 2, -1, 1);
  const dist = clamp(body.fat_distribution_z / 2, -1, 1);
  const heavy = Math.max(fat, 0);
  const thin = Math.max(-fat, 0);
  const bulk = Math.max(muscle, 0);
  const soft = Math.max(-muscle, 0);
  const belly = Math.max(dist, 0);
  const hips = Math.max(-dist, 0);
  for (const branch of ["male", "female"]) {
    const branchShare = branch === "male" ? male : female;
    weights[`fat_${branch}_max`] = branchShare * heavy * 1.5;
    weights[`fat_${branch}_min`] = branchShare * thin * 1.5;
    weights[`muscle_${branch}_max`] = branchShare * bulk * 0.45;
    weights[`muscle_${branch}_min`] = branchShare * soft * 0.45;
  }
  const quarter = (value) => value / 4;
  // The lineup stays at a quarter, so the family page does not jump.
  // ?edit=2 raises the axes that were unreadable at z/4. ±2 is the readable
  // step. ±3 is one and a half of that and still a human proportion.
  const hipW = editor ? (value) => value / 2 : quarter;
  const shoulderW = editor ? (value) => value : quarter;
  const cheekW = editor ? (value) => value / 2 : quarter;
  const noseW = editor ? (value) => value : quarter;
  weights.leg_upper = quarter(body.leg_ratio_z);
  weights.leg_lower = quarter(body.leg_ratio_z);
  weights.head_size = quarter(body.head_size_z);
  weights.shoulder_width = shoulderW(body.shoulder_width_z);
  weights.chest_depth = quarter(body.chest_depth_z);
  weights.pelvis_width = hipW(body.hip_width_z);
  // Waist, thigh and upper-arm files carry the mass (peaks about 2.8, 3.3
  // and 1.6 cm at weight 1). Positive distribution adds waist, negative adds
  // thigh and a little arm. The belly file is a pregnancy sculpt, about
  // 10 cm forward at weight 1, so it only adds a small gut when fat is central.
  weights.fat_central = heavy * (1.6 + 0.8 * belly) - thin * (1.15 + 0.4 * belly);
  weights.fat_belly = heavy * belly * 0.25;
  weights.fat_peripheral = heavy * (1.15 + 0.85 * hips) - thin * (0.85 + 0.45 * hips);
  weights.fat_upperarm = heavy * (2.1 + 0.6 * hips) - thin * (1.5 + 0.4 * hips);
  weights.muscle_arm = bulk * 1.8 - soft * 1.1;
  weights.muscle_shoulder = bulk * 0.55 - soft * 0.35;
  weights.muscle_chest = bulk * 1.6 - soft * 0.5;
  // Young and old female corners include MakeHuman's average cup. breast_min
  // cancels that cup, so the chest under it is flat. The bust is then only
  // breast_max, the localized max-cup delta: nothing at 0, a bud by about
  // 0.17, one average cup at 0.5, and the full max cup at 1. Sex does not
  // scale it. A male keeps both weights at 0.
  const breast = clamp(body.breast_development, 0, 1);
  // Fat is no longer a replacement of the young key, so the cup lives only
  // on the female age weights. A child slice keeps both at 0 and stays flat.
  const youngFemale = weights.age_female_young || 0;
  const oldFemale = weights.age_female_old || 0;
  const cup = clamp(breast / 0.5, 0, 1);
  weights.breast_min = sex === "male" ? 0 : youngFemale + oldFemale;
  weights.breast_max = sex === "male" ? 0 : cup * 0.34 + Math.max(0, breast - 0.5) * 1.32;
  weights.face_age_decr = clamp((body.body_form_age - body.face_form_age) / 20, 0, 1);
  weights.face_age_incr = clamp((body.face_form_age - body.body_form_age) / 45, 0, 1);
  for (let axis = 1; axis <= 12; axis += 1) {
    const key = `face_axis_${String(axis).padStart(2, "0")}`;
    const scale = axis === 1 ? noseW : axis === 7 ? cheekW : quarter;
    weights[key] = scale(body[key]);
  }
  // Provisional axes live in the direct editor only. Zero keeps the contract figure.
  const local = extra || {};
  const qz = (key) => quarter(local[key] || 0);
  const ez = (key) => (editor ? (local[key] || 0) : 0);
  weights.neck_length = qz("neck_length_z");
  weights.hand_size = qz("hand_size_z");
  weights.foot_size = qz("foot_size_z");
  weights.ear_size = qz("ear_size_z");
  weights.ear_projection = qz("ear_projection_z");
  weights.eye_roundness = qz("eye_roundness_z");
  weights.forehead_height = qz("forehead_height_z");
  // Buttock volume at weight 1 is about 3 cm. z = ±2 needs about twice that
  // before the seat reads under a skirt at full-body distance.
  weights.buttock_volume = ez("buttock_volume_z");
  weights.buttock_lift = qz("buttock_lift_z");
  // Depth stays at half. Width is multiplied later, in the cloth fit only.
  weights.waist_width = ez("waist_width_z") / 2;
  // Bridge and tip peak near 0.5 cm. 1.35 puts ±2 near 1.2 cm at face zoom.
  weights.nose_bridge = ez("nose_bridge_z") * 1.35;
  weights.nose_tip = ez("nose_tip_z") * 1.35;
  weights.brow_arch = qz("brow_arch_z");
  weights.lip_ratio = qz("lip_ratio_z");
  const breastOn = sex === "female" && body.body_form_age > 12 && breast >= 0.12;
  weights.breast_volume = breastOn ? ez("breast_volume_z") * 0.75 * breast : 0;
  weights.breast_height = breastOn ? ez("breast_height_z") * 1.5 * clamp(breast / 0.5, 0, 1) : 0;
  // Style A draws the same genes a little softer. These are constant offsets,
  // not a scale of the z-score: a gene of 0 stays a gene of 0 in the contract.
  // Eye weight 1 is about +17% radius, so 0.58 is about +10%.
  if (emphasise) {
    weights.face_axis_04 -= 0.12;
    weights.face_axis_06 -= 0.1;
    weights.face_axis_07 += 0.16;
    weights.face_axis_09 += 0.58;
  }
  return { weights, share, slices: { child, young, old } };
}

function applyWeights(mesh, weights) {
  const dict = mesh.morphTargetDictionary || {};
  const influences = mesh.morphTargetInfluences;
  if (!influences) return;
  for (const [name, index] of Object.entries(dict)) influences[index] = weights[name] || 0;
}

// The glTF splits every corner, so the exported normals are face normals.
// Group those corners by base position and average the morphed face normals.
function buildNormalGroups(geometry) {
  const pos = geometry.attributes.position.array;
  const map = new Map();
  const groupOf = new Int32Array(pos.length / 3);
  let count = 0;
  for (let i = 0; i < groupOf.length; i += 1) {
    const o = i * 3;
    const key = `${Math.round(pos[o] * 1e4)},${Math.round(pos[o + 1] * 1e4)},${Math.round(pos[o + 2] * 1e4)}`;
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

function smoothSkinNormals(mesh) {
  const source = mesh.geometry;
  const geometry = new THREE.BufferGeometry();
  for (const name of Object.keys(source.attributes)) {
    geometry.setAttribute(name, source.attributes[name]);
  }
  geometry.setIndex(source.index);
  geometry.morphAttributes.position = source.morphAttributes.position;
  geometry.morphTargetsRelative = source.morphTargetsRelative;
  geometry.setAttribute("normal", source.attributes.normal.clone());
  mesh.geometry = geometry;

  const { groupOf, count } = state.normalGroups;
  const morphed = new Float32Array(geometry.attributes.position.array);
  const influences = mesh.morphTargetInfluences;
  const morphs = geometry.morphAttributes.position || [];
  for (let i = 0; i < morphs.length; i += 1) {
    const weight = influences[i];
    if (!weight) continue;
    const delta = morphs[i].array;
    for (let k = 0; k < morphed.length; k += 1) morphed[k] += weight * delta[k];
  }
  const index = geometry.index.array;
  const ax = new Float32Array(count);
  const ay = new Float32Array(count);
  const az = new Float32Array(count);
  for (let t = 0; t < index.length; t += 3) {
    const a = index[t];
    const b = index[t + 1];
    const c = index[t + 2];
    const a3 = a * 3;
    const b3 = b * 3;
    const c3 = c * 3;
    const x0 = morphed[a3];
    const y0 = morphed[a3 + 1];
    const z0 = morphed[a3 + 2];
    const x1 = morphed[b3] - x0;
    const y1 = morphed[b3 + 1] - y0;
    const z1 = morphed[b3 + 2] - z0;
    const x2 = morphed[c3] - x0;
    const y2 = morphed[c3 + 1] - y0;
    const z2 = morphed[c3 + 2] - z0;
    const nx = y1 * z2 - z1 * y2;
    const ny = z1 * x2 - x1 * z2;
    const nz = x1 * y2 - y1 * x2;
    const g0 = groupOf[a];
    const g1 = groupOf[b];
    const g2 = groupOf[c];
    ax[g0] += nx;
    ay[g0] += ny;
    az[g0] += nz;
    ax[g1] += nx;
    ay[g1] += ny;
    az[g1] += nz;
    ax[g2] += nx;
    ay[g2] += ny;
    az[g2] += nz;
  }
  const normal = geometry.attributes.normal.array;
  for (let i = 0; i < groupOf.length; i += 1) {
    const g = groupOf[i];
    const x = ax[g];
    const y = ay[g];
    const z = az[g];
    const len = Math.hypot(x, y, z) || 1;
    const o = i * 3;
    normal[o] = x / len;
    normal[o + 1] = y / len;
    normal[o + 2] = z / len;
  }
  geometry.attributes.normal.needsUpdate = true;
}

function morphedYSpan(mesh, weights) {
  const position = mesh.geometry.attributes.position;
  const morphs = mesh.geometry.morphAttributes.position || [];
  const dict = mesh.morphTargetDictionary || {};
  let minY = Infinity;
  let maxY = -Infinity;
  const count = position.count;
  for (let vertex = 0; vertex < count; vertex += 1) {
    let y = position.getY(vertex);
    for (const [name, index] of Object.entries(dict)) {
      const weight = weights[name] || 0;
      if (weight !== 0 && morphs[index]) y += weight * morphs[index].getY(vertex);
    }
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  return { minY, maxY, span: maxY - minY };
}

function morphedCloud(mesh, weights) {
  const position = mesh.geometry.attributes.position;
  const morphs = mesh.geometry.morphAttributes.position || [];
  const dict = mesh.morphTargetDictionary || {};
  const active = [];
  for (const [name, index] of Object.entries(dict)) {
    const weight = weights[name] || 0;
    if (weight !== 0 && morphs[index]) active.push([weight, morphs[index]]);
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

function landmarks(cloud) {
  const { xyz, count, maxY } = cloud;
  const crown = [0, 0, 0];
  let crownN = 0;
  for (let vertex = 0; vertex < count; vertex += 1) {
    const x = xyz[vertex * 3];
    const y = xyz[vertex * 3 + 1];
    const z = xyz[vertex * 3 + 2];
    if (y > maxY - 0.008 && Math.abs(x) < 0.04) {
      crown[0] += x;
      crown[1] += y;
      crown[2] += z;
      crownN += 1;
    }
  }
  if (!crownN) crown.splice(0, 3, 0, maxY, 0);
  else {
    crown[0] /= crownN;
    crown[1] /= crownN;
    crown[2] /= crownN;
  }
  const jaw = jawProfile(cloud);
  const chinY = maxY + jaw.chinDy;
  return {
    maxY,
    crown,
    chin: [0, chinY, jaw.chinZ],
    neck: [0, chinY - 0.012, Math.min(jaw.chinZ, 0.05)],
    jaw,
  };
}

function makeGradient() {
  const data = new Uint8Array([70, 70, 70, 255, 150, 150, 150, 255, 245, 245, 245, 255]);
  const texture = new THREE.DataTexture(data, 3, 1);
  texture.magFilter = THREE.NearestFilter;
  texture.minFilter = THREE.NearestFilter;
  texture.needsUpdate = true;
  texture.colorSpace = THREE.NoColorSpace;
  return texture;
}

function surfaceMaterial(hex, style, roughness) {
  let material;
  if (style === "C") material = new THREE.MeshBasicMaterial({ toneMapped: false });
  else if (style === "B") material = new THREE.MeshToonMaterial({ gradientMap: toonGradient });
  else material = new THREE.MeshStandardMaterial({ roughness: style === "A" ? 0.92 : Math.max(roughness, 0.72), metalness: 0 });
  material.color.setStyle(hex);
  return material;
}

// Post-morph, pre-skin band. ny is 0 at the lowest vertex and 1 at the crown.
// The cut is the base of the neck, above the chest, down to mid-thigh.
// Arm X is still the A-pose, outside the ribcage, so the hanging arm stays bare.
// Linear mean of young_lightskinned_female_diffuse.png, pixels above luma 0.08.
// Dividing by this leaves pores and lip colour, and the contract skin sets the tone.
const SKIN_MEAN = new THREE.Vector3(0.616986, 0.356523, 0.236129);

const SKIN_DETAIL = `
  #ifdef USE_MAP
    vec3 tex = sampledDiffuseColor.rgb;
    vec3 ratio = clamp(tex / uSkinMean, vec3(0.25), vec3(1.5));
    // The albedo also paints eyes, brows and nostril cavities. At full strength
    // those read as a dark blotch under the eye and brow proxies. Keep the
    // pore grain and the lip redness, and lift only that near-black paint.
    vec3 detail = mix(vec3(1.0), ratio, 0.62);
    detail = max(detail, vec3(0.76));
    float warm = clamp(ratio.r - max(ratio.g, ratio.b), 0.0, 0.35);
    vec3 skinCol = uSkin * detail;
    skinCol += uSkin * vec3(0.07, 0.02, 0.008) * smoothstep(0.03, 0.14, warm);
    diffuseColor.rgb = skinCol;
    // Beard mask: 1 is mustache and chin, lower values are jaw then cheeks.
    // Lips, the nose and the neck are 0. Low coverage keeps the core only.
    float beardM = texture(uBeardMap, vMapUv).r;
    float edge = mix(0.50, 0.06, clamp(uBeardCoverage, 0.0, 1.0));
    float grow = smoothstep(edge, edge + 0.18, beardM);
    // Stubble is darker than the hair colour, or grey hair vanishes on pale skin.
    vec3 beardCol = uBeardCol * vec3(0.28, 0.24, 0.22);
    float ink = clamp(grow * uBeardDensity * 1.4, 0.0, 0.8);
    diffuseColor.rgb = mix(diffuseColor.rgb, beardCol, ink);
    // Per fragment, so a crack in the middle of a big triangle still matches
    // the hair. A box down the face painted the forehead and the eyes.
    float scalp = 0.0;
    for (int i = 0; i < 80; i++) {
      float dist = distance(vBind, uRim[i]);
      scalp = max(scalp, 1.0 - smoothstep(0.010, 0.020, dist));
    }
    float hairGate = smoothstep(0.02, 0.22, uHairDensity);
    // Match the dark strand, not a lighter brown. A pale rim reads as a tear.
    vec3 scalpCol = uBeardCol * vec3(0.36, 0.32, 0.28);
    diffuseColor.rgb = mix(diffuseColor.rgb, scalpCol, scalp * 0.96 * hairGate);
  #endif
`;

const NECK_CUT = `
  if (uCut > 0.0 && vBind.y < uCut) discard;
  skinMask = 1.0;
`;

// Cheeks sit beside the nose, between nose and mouth. Lips sit just above the chin.
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

const SKIN_WRAP = `
  vec3 keyDir = normalize(uKeyDir);
  float ndl = dot(normal, keyDir);
  float wrap = clamp((ndl + 0.55) / 1.55, 0.0, 1.0);
  float direct = clamp(ndl, 0.0, 1.0);
  totalDiffuse += uSkin * vec3(0.62, 0.24, 0.12) * (wrap - direct) * skinMask * 0.7;
`;

function paintCloth(material, floor, span, cutY, skinHex, beard, style, marks, scalp) {
  const warm = style === "A";
  const uniforms = {
    uSkin: { value: new THREE.Color(skinHex) },
    uSkinMean: { value: SKIN_MEAN },
    uBeardCol: { value: new THREE.Color(beard.color) },
    uBeardDensity: { value: beard.density },
    uBeardCoverage: { value: beard.coverage },
    uBeardMap: { value: state.beard },
    uFloor: { value: floor },
    uSpan: { value: Math.max(span, 0.2) },
    uCut: { value: cutY },
    uNose: { value: new THREE.Vector3(0, marks.maxY + marks.jaw.noseDy, marks.jaw.noseZ) },
    uChin: { value: new THREE.Vector3(marks.chin[0], marks.chin[1], marks.chin[2]) },
    uKeyDir: { value: keyDirView },
    uHairDensity: { value: scalp ? scalp.density : 0 },
    uRim: { value: Array.from({ length: 80 }, () => new THREE.Vector3(0, -8, 0)) },
  };
  material.userData.rim = uniforms.uRim.value;
  material.customProgramCacheKey = () => (warm ? "skin-beard-a6" : "skin-beard-6");
  material.onBeforeCompile = (shader) => {
    shader.uniforms.uSkin = uniforms.uSkin;
    shader.uniforms.uSkinMean = uniforms.uSkinMean;
    shader.uniforms.uBeardCol = uniforms.uBeardCol;
    shader.uniforms.uBeardDensity = uniforms.uBeardDensity;
    shader.uniforms.uBeardCoverage = uniforms.uBeardCoverage;
    shader.uniforms.uBeardMap = uniforms.uBeardMap;
    shader.uniforms.uSpan = uniforms.uSpan;
    shader.uniforms.uFloor = uniforms.uFloor;
    shader.uniforms.uCut = uniforms.uCut;
    shader.uniforms.uNose = uniforms.uNose;
    shader.uniforms.uChin = uniforms.uChin;
    shader.uniforms.uKeyDir = uniforms.uKeyDir;
    shader.uniforms.uHairDensity = uniforms.uHairDensity;
    shader.uniforms.uRim = uniforms.uRim;
    material.userData.shader = shader;
    shader.vertexShader = `varying vec3 vBind;\n${shader.vertexShader.replace(
      "#include <morphtarget_vertex>",
      "#include <morphtarget_vertex>\n\tvBind = transformed;",
    )}`;
    const scalpDecl = "uniform float uHairDensity;\nuniform vec3 uRim[80];\n";
    const decl = warm
      ? `varying vec3 vBind;\nuniform vec3 uSkin;\nuniform vec3 uSkinMean;\nuniform vec3 uBeardCol;\nuniform float uBeardDensity;\nuniform float uBeardCoverage;\nuniform sampler2D uBeardMap;\nuniform float uFloor;\nuniform float uSpan;\nuniform float uCut;\nuniform vec3 uNose;\nuniform vec3 uChin;\nuniform vec3 uKeyDir;\n${scalpDecl}float skinMask;\n`
      : `varying vec3 vBind;\nuniform vec3 uSkin;\nuniform vec3 uSkinMean;\nuniform vec3 uBeardCol;\nuniform float uBeardDensity;\nuniform float uBeardCoverage;\nuniform sampler2D uBeardMap;\nuniform float uFloor;\nuniform float uSpan;\nuniform float uCut;\n${scalpDecl}float skinMask;\n`;
    const color = warm ? `${SKIN_DETAIL}\n${BLUSH}\n${NECK_CUT}` : `${SKIN_DETAIL}\n${NECK_CUT}`;
    shader.fragmentShader = `${decl}${shader.fragmentShader.replace(
      "#include <color_fragment>",
      `#include <color_fragment>\n${color}`,
    )}`;
    if (warm) {
      shader.fragmentShader = shader.fragmentShader.replace(
        "vec3 totalDiffuse = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse;",
        `vec3 totalDiffuse = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse;\n${SKIN_WRAP}`,
      );
    }
  };
  material.onBeforeRender = () => {
    const shader = material.userData.shader;
    if (!shader || !shader.uniforms.uSpan) return;
    shader.uniforms.uFloor.value = uniforms.uFloor.value;
    shader.uniforms.uSpan.value = uniforms.uSpan.value;
    shader.uniforms.uCut.value = uniforms.uCut.value;
  };
  return material;
}

function bodyMaterial(skinHex, style, floor, span, cutY, beard, marks, scalp) {
  const material = surfaceMaterial("#ffffff", style, 0.8);
  material.map = state.skin;
  return paintCloth(material, floor, span, cutY, skinHex, beard, style, marks, scalp);
}

function litMaterial(style, roughness) {
  if (style === "C") return new THREE.MeshBasicMaterial({ toneMapped: false });
  if (style === "B") return new THREE.MeshToonMaterial({ gradientMap: toonGradient });
  return new THREE.MeshStandardMaterial({ roughness, metalness: 0 });
}

function copyUniform(target, source) {
  if (target && target.isColor && source && source.isColor) target.copy(source);
  else if (target && target.isVector3 && source && source.isVector3) target.copy(source);
  else return source;
  return target;
}

function bindShader(material, key, uniforms, vertexDecl, vertexBody, fragmentDecl, fragmentBody) {
  material.customProgramCacheKey = () => key;
  material.onBeforeCompile = (shader) => {
    for (const [name, uniform] of Object.entries(uniforms)) shader.uniforms[name] = uniform;
    material.userData.shader = shader;
    if (vertexDecl) shader.vertexShader = `${vertexDecl}\n${shader.vertexShader}`;
    if (vertexBody) {
      shader.vertexShader = shader.vertexShader.replace(
        "#include <begin_vertex>",
        `#include <begin_vertex>\n${vertexBody}`,
      );
    }
    if (fragmentDecl) shader.fragmentShader = `${fragmentDecl}\n${shader.fragmentShader}`;
    if (fragmentBody) {
      shader.fragmentShader = shader.fragmentShader.replace(
        "#include <map_fragment>",
        `#include <map_fragment>\n${fragmentBody}`,
      );
    }
  };
  material.onBeforeRender = () => {
    const shader = material.userData.shader;
    if (!shader) return;
    for (const [name, uniform] of Object.entries(uniforms)) {
      const slot = shader.uniforms[name];
      if (!slot) continue;
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
  // Slide the hairline hem down over the seam. Lashes and brows leave uFill at 0.
  // aMesh stays put for the fragment test; transformed is the drawn shell.
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
`;

const STRAND_FRAG = `
  float cover = vKeep;
  float hairA = 1.0;
  #ifdef USE_MAP
    hairA = sampledDiffuseColor.a;
    // Four texels. This only feathers the cut. It does not decide the part fill.
    float softA = hairA;
    if (uSoft > 0.5) {
      // Grow into the transparent teeth. An average with the empty texels
      // pulled the cut inward and left a pale rim.
      vec2 texel = vec2(0.0045);
      softA = max(softA, texture(map, vMapUv + vec2(texel.x, 0.0)).a);
      softA = max(softA, texture(map, vMapUv - vec2(texel.x, 0.0)).a);
      softA = max(softA, texture(map, vMapUv + vec2(0.0, texel.y)).a);
      softA = max(softA, texture(map, vMapUv - vec2(0.0, texel.y)).a);
    }
    cover *= softA;
    float shade = dot(sampledDiffuseColor.rgb, vec3(0.299, 0.587, 0.114));
    float lift = clamp(shade / 0.16, 0.35, 1.45);
    diffuseColor.rgb = uHair * lift;
    diffuseColor.a = 1.0;
  #endif
  if (!gl_FrontFacing) diffuseColor.rgb *= 0.62;
  float below = uCrown.y - vMesh.y;
  float forward = vMesh.z - uCrown.z;
  float fillMid = 1.0 - smoothstep(0.003, 0.01, abs(vMesh.x));
  // Only the narrow tuft past the bangs. A wider cut opens a notch in the fringe.
  float clipMid = 1.0 - smoothstep(0.002, 0.007, abs(vMesh.x));
  float tuft = clipMid * step(0.08, below) * step(0.085, forward);
  if (uClip > 0.5 && tuft > 0.5) discard;
  // uCap is metres below the hair crown. Opaque hair still flanks the midline
  // here. Lower than the cap, a fill becomes a bar on the forehead.
  float inMass = step(-0.004, below) * (1.0 - smoothstep(uCap - 0.01, uCap, below));
  // Partial alpha along the hairline. Alpha 0 stays a hole so the forehead
  // between cards is not painted into a helmet. The crown is behind the scalp,
  // so this ring sits 4–13 cm forward of it.
  float lineSide = abs(vMesh.x - uCrown.x);
  float fringe = smoothstep(0.02, 0.045, below) * (1.0 - smoothstep(0.095, 0.12, below));
  fringe *= 1.0 - smoothstep(0.085, 0.108, lineSide);
  fringe *= smoothstep(0.04, 0.075, forward) * (1.0 - smoothstep(0.115, 0.14, forward));
  float partial = smoothstep(0.05, 0.22, hairA) * (1.0 - smoothstep(0.62, 0.92, hairA));
  float edgeFill = fringe * partial * step(0.5, vKeep);
  float hole = max((1.0 - smoothstep(0.2, 0.45, hairA)) * fillMid * inMass * step(0.5, vKeep), edgeFill);
  if (uFill > 0.5 && hole > 0.4) {
    diffuseColor.rgb = uHair * uFillLift;
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
    // Pupil centres on brown_eye.png, measured in the mesh UV (v up).
    vec2 pupilL = vec2(0.2939, 0.2897);
    vec2 pupilR = vec2(0.7099, 0.7019);
    float dL = length(vMapUv - pupilL);
    float dR = length(vMapUv - pupilR);
    float d = min(dL, dR);
    // The packed iris is brown. Its colour must not be multiplied into the tint:
    // brown times blue stays red. Keep only the fibre luminance.
    float luma = dot(sampledDiffuseColor.rgb, vec3(0.299, 0.587, 0.114));
    float fiber = clamp(luma / 0.14, 0.62, 1.15);
    float radial = clamp(d / 0.108, 0.0, 1.0);
    // Near-black contract browns still need a readable iris, same hue.
    float irisLuma = dot(uIris, vec3(0.299, 0.587, 0.114));
    vec3 tint = irisLuma < 0.07 ? uIris * (0.11 / max(irisLuma, 0.02)) : uIris;
    vec3 irisCol = tint * mix(1.05, 0.55, radial) * fiber;
    irisCol = mix(irisCol, irisCol * vec3(0.30, 0.26, 0.24), smoothstep(0.72, 1.0, radial));
    irisCol = mix(irisCol, vec3(0.015, 0.012, 0.010), smoothstep(0.040, 0.022, d));
    float iris = smoothstep(0.116, 0.098, d);
    // The texture around the iris is a grey field, and that field is what shows
    // in the socket. The sclera is the disc; anything past it is lid skin.
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

const fillLifts = new Map();

function srgbByteToLinear(byte) {
  const value = byte / 255;
  return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
}

// Same lift the strand shader uses: texture shade over 0.16, clamped.
// The part has no opaque texel, so it takes the mean of this texture.
function hairFillLift(texture) {
  const image = texture && texture.image;
  if (!image || !image.width) return 0.55;
  if (fillLifts.has(image)) return fillLifts.get(image);
  const canvas = document.createElement("canvas");
  const width = 64;
  const height = 64;
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  context.drawImage(image, 0, 0, width, height);
  const data = context.getImageData(0, 0, width, height).data;
  let sum = 0;
  let count = 0;
  for (let i = 0; i < data.length; i += 4) {
    if (data[i + 3] < 128) continue;
    const shade = 0.299 * srgbByteToLinear(data[i])
      + 0.587 * srgbByteToLinear(data[i + 1])
      + 0.114 * srgbByteToLinear(data[i + 2]);
    sum += shade;
    count += 1;
  }
  const mean = count ? sum / count : 0.16;
  const lift = Math.min(1.45, Math.max(0.35, mean / 0.16));
  fillLifts.set(image, lift);
  return lift;
}

function strandMaterial(style, texture, hairHex, threshold, bald, crown, offset, fillPart, softEdge, cap, clip) {
  const material = litMaterial(style, style === "A" ? 0.84 : 0.62);
  material.map = texture;
  material.color.setHex(0xffffff);
  material.side = THREE.DoubleSide;
  material.polygonOffset = true;
  material.polygonOffsetFactor = offset;
  material.polygonOffsetUnits = offset;
  // Soft coverage rounds the alpha-test stair. Brows stay a hard cut.
  // Style A lashes opt in, so the row does not read as stitches.
  if (softEdge) material.alphaToCoverage = true;
  const uniforms = {
    uHair: { value: new THREE.Color(hairHex) },
    uThresh: { value: threshold },
    uBald: { value: bald },
    uCrown: { value: new THREE.Vector3(crown[0], crown[1], crown[2]) },
    uFill: { value: fillPart ? 1 : 0 },
    uSoft: { value: softEdge ? 1 : 0 },
    uCap: { value: cap == null ? 0 : cap },
    uClip: { value: clip ? 1 : 0 },
    uFillLift: { value: hairFillLift(texture) },
  };
  const kind = `${fillPart ? "part" : "cut"}-${softEdge ? "soft" : "hard"}`;
  return bindShader(
    material,
    `strand-${style}-${kind}`,
    uniforms,
    "attribute vec3 aMesh;\nvarying float vKeep;\nvarying vec3 vMesh;\nuniform float uBald;\nuniform vec3 uCrown;\nuniform float uFill;",
    HAIR_VERTEX,
    "varying float vKeep;\nvarying vec3 vMesh;\nuniform vec3 uHair;\nuniform float uThresh;\nuniform float uFill;\nuniform float uSoft;\nuniform float uCap;\nuniform float uClip;\nuniform float uFillLift;\nuniform vec3 uCrown;",
    STRAND_FRAG,
  );
}

function eyeMaterial(style, texture, irisHex, skinHex) {
  const material = litMaterial(style, style === "A" ? 0.72 : 0.42);
  material.map = texture;
  material.color.setHex(0xffffff);
  material.side = THREE.FrontSide;
  const uniforms = {
    uIris: { value: new THREE.Color(irisHex) },
    uSkin: { value: new THREE.Color(skinHex) },
  };
  bindShader(
    material,
    `eye-${style}`,
    uniforms,
    "",
    "",
    "uniform vec3 uIris;\nuniform vec3 uSkin;",
    EYE_FRAG,
  );
  const compile = material.onBeforeCompile;
  material.onBeforeCompile = (shader) => {
    compile(shader);
    shader.fragmentShader = shader.fragmentShader.replace(
      "#include <opaque_fragment>",
      "#include <opaque_fragment>\n  gl_FragColor.rgb += vec3(0.95, 0.97, 1.0) * catchLight;",
    );
  };
  return material;
}

function outlineMaterial(cutY) {
  const material = new THREE.MeshBasicMaterial({ color: 0x1a1612, side: THREE.BackSide, toneMapped: false });
  material.onBeforeCompile = (shader) => {
    shader.uniforms.uCut = { value: cutY };
    shader.vertexShader = `varying vec3 vBind;\n${shader.vertexShader.replace(
      "#include <morphtarget_vertex>",
      "#include <morphtarget_vertex>\n\tvBind = transformed;",
    ).replace(
      "#include <project_vertex>",
      "transformed += normalize(objectNormal) * 0.006;\n#include <project_vertex>",
    )}`;
    shader.fragmentShader = `varying vec3 vBind;\nuniform float uCut;\n${shader.fragmentShader.replace(
      "#include <color_fragment>",
      "if (uCut > 0.0 && vBind.y < uCut) discard;\n#include <color_fragment>",
    )}`;
  };
  return material;
}

function findMesh(root) {
  let found = null;
  root.traverse((node) => {
    if (node.isSkinnedMesh) found = node;
  });
  return found;
}

function findBone(root, name) {
  let found = null;
  root.traverse((node) => {
    if (node.isBone && node.name === name) found = node;
  });
  return found;
}

// Mesh space is Y up, face +Z. inverseBindMatrices map that into the head bone,
// so a child of `head` follows the Root height scale and the spine sway.
function headMatrix(mesh, head) {
  const index = mesh.skeleton.bones.indexOf(head);
  return new THREE.Matrix4().multiplyMatrices(mesh.skeleton.boneInverses[index], mesh.bindMatrix);
}

function humanPositions(weights, editor) {
  const human = state.pack.human;
  const pos = new Float32Array(human.basis);
  for (const morph of human.morphs) {
    const weight = weights[morph.name] || 0;
    if (weight === 0) continue;
    const delta = morph.data;
    // measure-waist-circ-incr is about 1 cm of width and 3 cm of belly.
    // The editor multiplies X only, and only for the cloth fit. The glTF
    // influence stays at the half weight, so the covered belly does not grow.
    if (editor && morph.name === "waist_width") {
      for (let i = 0; i < delta.length; i += 3) {
        pos[i] += weight * 4 * delta[i];
        pos[i + 1] += weight * delta[i + 1];
        pos[i + 2] += weight * delta[i + 2];
      }
      continue;
    }
    for (let i = 0; i < delta.length; i += 1) pos[i] += weight * delta[i];
  }
  return pos;
}

function fitProxy(proxy, humanPos) {
  const size = (axis, component) => {
    const pair = proxy.scale[axis];
    return Math.abs(humanPos[pair[0] * 3 + component] - humanPos[pair[1] * 3 + component]) / pair[2];
  };
  const sx = size("x", 0);
  const sy = size("y", 1);
  const sz = size("z", 2);
  const bind = proxy.bind;
  const out = new Float32Array(proxy.count * 3);
  for (let i = 0; i < proxy.count; i += 1) {
    const o = i * 9;
    const i0 = bind[o] * 3;
    const i1 = bind[o + 1] * 3;
    const i2 = bind[o + 2] * 3;
    const w0 = bind[o + 3];
    const w1 = bind[o + 4];
    const w2 = bind[o + 5];
    out[i * 3] = w0 * humanPos[i0] + w1 * humanPos[i1] + w2 * humanPos[i2] + bind[o + 6] * sx;
    out[i * 3 + 1] = w0 * humanPos[i0 + 1] + w1 * humanPos[i1 + 1] + w2 * humanPos[i2 + 1] + bind[o + 7] * sy;
    out[i * 3 + 2] = w0 * humanPos[i0 + 2] + w1 * humanPos[i1 + 2] + w2 * humanPos[i2 + 2] + bind[o + 8] * sz;
  }
  return out;
}

function toHeadLocal(positions, matrix) {
  const out = new Float32Array(positions.length);
  const point = new THREE.Vector3();
  for (let i = 0; i < positions.length; i += 3) {
    point.fromArray(positions, i).applyMatrix4(matrix);
    point.toArray(out, i);
  }
  return out;
}

function addFitted(mesh, head, proxy, humanPos, material, order, nudgeZ, reshape) {
  const fitted = reshape ? reshape(fitProxy(proxy, humanPos)) : fitProxy(proxy, humanPos);
  const placed = nudgeZ ? new Float32Array(fitted) : fitted;
  if (nudgeZ) {
    for (let i = 2; i < placed.length; i += 3) placed[i] += nudgeZ;
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(toHeadLocal(placed, headMatrix(mesh, head)), 3));
  geometry.setAttribute("aMesh", new THREE.BufferAttribute(fitted, 3));
  geometry.setAttribute("uv", new THREE.BufferAttribute(new Float32Array(proxy.uv), 2));
  geometry.setIndex(new THREE.BufferAttribute(new Uint32Array(proxy.faces), 1));
  geometry.computeVertexNormals();
  const object = new THREE.Mesh(geometry, material);
  object.frustumCulled = false;
  object.renderOrder = order;
  head.add(object);
  return object;
}

const DYES = [
  ["#6d7c8a", "#2c333c"],
  ["#8d735c", "#3a342c"],
  ["#5e7268", "#2e3832"],
  ["#8a8070", "#24312e"],
  ["#7a624e", "#343c4c"],
  ["#6a6258", "#4a433c"],
];
const CHILD_DYES = [
  ["#5f746c", "#3a4450"],
  ["#6e7f8c", "#35423e"],
  ["#8a6e58", "#4a3e36"],
  ["#5c6574", "#3a4250"],
];
// Fraction of the fitted suit where the jeans or skirt take over.
// A lower cut painted the crotch in the shirt colour.
const WAIST = {
  cloth_shirt: 0.56,
  cloth_jacket: 0.55,
  cloth_tee: 0.58,
  cloth_f_tee: 0.58,
  cloth_shorts: 0.48,
  cloth_skirt: 0.42,
};

function outfitOf(member) {
  const h = hashSim(personKey(member));
  const years = Number(member.age_years);
  const child = Number.isFinite(years) && years <= 13;
  const female = member.sex === "female";
  let suit;
  let shoes;
  if (child) {
    const trousers = (h % 2) === 1;
    // The male casual suit is a tank. Children wear the sleeved t-shirt.
    suit = trousers ? "cloth_f_tee" : "cloth_shorts";
    shoes = "shoes_sneaker";
  } else if (female) {
    const slot = h % 3;
    suit = ["cloth_f_tee", "cloth_skirt", "cloth_shirt"][slot];
    shoes = ["shoes_sneaker", "shoes_black", "shoes_brown"][slot];
  } else {
    const slot = h % 3;
    suit = ["cloth_f_tee", "cloth_shirt", "cloth_jacket"][slot];
    shoes = ["shoes_sneaker", "shoes_brown", "shoes_black"][slot];
  }
  const dyes = (child ? CHILD_DYES : DYES)[(h >>> 4) % (child ? CHILD_DYES.length : DYES.length)];
  return { suit, shoes, top: dyes[0], bottom: dyes[1], child };
}

function restKey(x, y, z) {
  return `${Math.round(x * 1e4)},${Math.round(y * 1e4)},${Math.round(z * 1e4)}`;
}

function buildRestIndex(bodyRest, geometry) {
  // 0.1 mm cells. A glb corner and its base vertex can sit on opposite
  // sides of a cell wall, so a miss also checks the neighbouring cells.
  const map = new Map();
  for (let i = 0; i < bodyRest.length; i += 3) {
    const key = restKey(bodyRest[i], bodyRest[i + 1], bodyRest[i + 2]);
    const list = map.get(key);
    if (list) list.push(i / 3);
    else map.set(key, [i / 3]);
  }
  const pos = geometry.attributes.position.array;
  const hm = new Int32Array(pos.length / 3);
  let missed = 0;
  const nearest = (x, y, z) => {
    const qx = Math.round(x * 1e4);
    const qy = Math.round(y * 1e4);
    const qz = Math.round(z * 1e4);
    let best = -1;
    let bestD = 4e-8;
    for (let dx = -1; dx <= 1; dx += 1) {
      for (let dy = -1; dy <= 1; dy += 1) {
        for (let dz = -1; dz <= 1; dz += 1) {
          const list = map.get(`${qx + dx},${qy + dy},${qz + dz}`);
          if (!list) continue;
          for (let n = 0; n < list.length; n += 1) {
            const id = list[n];
            const o = id * 3;
            const ex = bodyRest[o] - x;
            const ey = bodyRest[o + 1] - y;
            const ez = bodyRest[o + 2] - z;
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
    const x = pos[i * 3];
    const y = pos[i * 3 + 1];
    const z = pos[i * 3 + 2];
    const id = nearest(x, y, z);
    if (id < 0) {
      hm[i] = -1;
      missed += 1;
    } else {
      hm[i] = id;
    }
  }
  return { hm, missed, corners: hm.length };
}

function hiddenFaces(geometry, hm, deleted) {
  const src = geometry.index.array;
  const kept = [];
  let dropped = 0;
  for (let t = 0; t < src.length; t += 3) {
    const a = hm[src[t]];
    const b = hm[src[t + 1]];
    const c = hm[src[t + 2]];
    if (a >= 0 && b >= 0 && c >= 0 && deleted.has(a) && deleted.has(b) && deleted.has(c)) {
      dropped += 1;
      continue;
    }
    kept.push(src[t], src[t + 1], src[t + 2]);
  }
  return { index: new Uint32Array(kept), dropped, total: src.length / 3 };
}

function deleteSet(pieces) {
  const set = new Set();
  for (const proxy of pieces) {
    if (!proxy || !proxy.delete) continue;
    for (let i = 0; i < proxy.delete.length; i += 1) set.add(proxy.delete[i]);
  }
  return set;
}

const CLOTH_COLOR = `
  float ao = texture(uAo, vMapUv).r;
  float seam = smoothstep(uSplit - 0.03, uSplit + 0.03, vNy);
  vec3 dye = mix(uBottom, uTop, seam);
  diffuseColor.rgb = dye * mix(0.55, 0.92, ao);
  diffuseColor.a = 1.0;
`;

function clothMaterial(style, proxy, topHex, bottomHex, floor, span) {
  if (proxy.kind === "shoes") {
    const material = litMaterial(style, 0.64);
    material.map = proxy.texture;
    material.color.setRGB(0.78, 0.76, 0.72);
    // Cuffs and collars are open shells. One winding faces inward.
    material.side = THREE.DoubleSide;
    material.polygonOffset = true;
    material.polygonOffsetFactor = -2;
    material.polygonOffsetUnits = -2;
    return material;
  }
  const material = litMaterial(style, 0.88);
  material.side = THREE.FrontSide;
  material.map = proxy.ao || proxy.texture;
  material.color.setHex(0xffffff);
  material.polygonOffset = true;
  material.polygonOffsetFactor = -1;
  material.polygonOffsetUnits = -1;
  const uniforms = {
    uTop: { value: new THREE.Color(topHex) },
    uBottom: { value: new THREE.Color(bottomHex) },
    uSplit: { value: proxy.split },
    uFloor: { value: floor },
    uSpan: { value: Math.max(span, 0.05) },
    uAo: { value: proxy.ao || proxy.texture },
  };
  return bindShader(
    material,
    `cloth-${style}`,
    uniforms,
    "varying float vNy;\nuniform float uFloor;\nuniform float uSpan;",
    "vNy = (position.y - uFloor) / max(uSpan, 1e-4);",
    "varying float vNy;\nuniform vec3 uTop;\nuniform vec3 uBottom;\nuniform float uSplit;\nuniform sampler2D uAo;",
    CLOTH_COLOR,
  );
}

function ySpan(positions) {
  let minY = Infinity;
  let maxY = -Infinity;
  for (let i = 1; i < positions.length; i += 3) {
    minY = Math.min(minY, positions[i]);
    maxY = Math.max(maxY, positions[i]);
  }
  return { minY, span: Math.max(maxY - minY, 0.05) };
}

function sortedAt(values, p) {
  if (!values.length) return 0;
  const sorted = values.slice().sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.max(0, Math.round(p * (sorted.length - 1))));
  return sorted[index];
}

// The hip sculpt stops on the hip line. A skirt hem is bound to the thigh,
// and the body under the cloth is deleted, so an unmoved hem hides the width.
// The gain is the body's own outer-hip delta, not the cloth band: the cloth
// band's high percentile over-widened the plus side and missed the minus side.
// A skirt keeps that gain to the hem. Trousers fade it out over 16 cm.
// Seat and bust panels move only when those sliders are off zero, so a lineup
// shirt is not restyled. The normal gap after this keeps the shell outside.
function conformCloth(fitted, rest, key, humanPos, weights) {
  const basis = state.pack.human.basis;
  const count = (fitted.length / 3) | 0;
  const skirt = key === "cloth_skirt";
  const used = weights || {};
  let minY = Infinity;
  let maxY = -Infinity;
  for (let i = 1; i < rest.length; i += 3) {
    if (rest[i] < minY) minY = rest[i];
    if (rest[i] > maxY) maxY = rest[i];
  }
  const mid = (minY + maxY) * 0.5;
  if (Math.abs(used.pelvis_width || 0) > 0.02) {
    const hip = state.pack.human.morphs.find((morph) => morph.name === "pelvis_width");
    const delta = hip ? hip.data : null;
    const right = [];
    const left = [];
    let ySum = 0;
    let n = 0;
    if (delta) {
      for (let i = 0; i < basis.length; i += 3) {
        const x = basis[i];
        const z = basis[i + 2];
        if (Math.abs(delta[i]) < 0.015 || Math.abs(x) < 0.13 || Math.abs(z) > 0.12) continue;
        const moved = humanPos[i] - x;
        if (x >= 0) right.push(moved);
        else left.push(moved);
        ySum += basis[i + 1];
        n += 1;
      }
    }
    if (n > 20 && right.length && left.length) {
      const hipY = ySum / n;
      const gainR = sortedAt(right, 0.85);
      const gainL = sortedAt(left, 0.15);
      for (let i = 0; i < count; i += 1) {
        const x = rest[i * 3];
        const y = rest[i * 3 + 1];
        if (y > hipY - 0.012) continue;
        const ax = Math.abs(x);
        if (ax < 0.03 || ax > 0.34) continue;
        const drop = hipY - y;
        const follow = skirt ? 1 : clamp(1 - Math.max(0, drop - 0.015) / 0.16, 0, 1);
        if (follow <= 0) continue;
        const have = fitted[i * 3] - x;
        const want = x >= 0 ? gainR : gainL;
        fitted[i * 3] += (want - have) * follow;
      }
    }
  }

  const bodyDeltas = (pick) => {
    const values = [];
    for (let i = 0; i < basis.length; i += 3) {
      if (!pick(basis[i], basis[i + 1], basis[i + 2])) continue;
      values.push(humanPos[i + 2] - basis[i + 2]);
    }
    values.sort((a, b) => a - b);
    return values;
  };
  if (Math.abs(used.buttock_volume || 0) > 0.02) {
    const fuller = used.buttock_volume >= 0;
    const seatBody = bodyDeltas((x, y, z) => (
      Math.abs(x) < 0.18 && z < 0.02 && y < mid + 0.05 && y > minY + 0.02
    ));
    if (seatBody.length > 12) {
      const target = sortedAt(seatBody, fuller ? 0.12 : 0.88);
      const gate = Math.max(0.004, Math.abs(target) * 0.25);
      if (Math.abs(target) > 0.003) {
        for (let i = 0; i < count; i += 1) {
          const x = rest[i * 3];
          const y = rest[i * 3 + 1];
          const z = rest[i * 3 + 2];
          if (Math.abs(x) > 0.2 || z > 0.04 || y > mid + 0.06 || y < minY) continue;
          const dz = fitted[i * 3 + 2] - z;
          const lag = fuller ? dz - target : target - dz;
          if (lag < gate) continue;
          fitted[i * 3 + 2] += (target - dz) * 0.9;
        }
      }
    }
  }
  const pullChest = (side) => {
    const chestBody = bodyDeltas((x, y, z) => {
      if (y < mid - 0.02 || z < 0.025) return false;
      return side < 0 ? x < -0.03 && x > -0.14 : x > 0.03 && x < 0.14;
    });
    if (chestBody.length < 12) return;
    const target = sortedAt(chestBody, 0.84);
    if (target < 0.003) return;
    const gate = Math.max(0.005, target * 0.35);
    for (let i = 0; i < count; i += 1) {
      const x = rest[i * 3];
      const y = rest[i * 3 + 1];
      const z = rest[i * 3 + 2];
      if (y < mid - 0.04 || z < 0.015) continue;
      if (side < 0) {
        if (x > -0.02 || x < -0.16) continue;
      } else if (x < 0.02 || x > 0.16) continue;
      const dz = fitted[i * 3 + 2] - z;
      if (target - dz < gate) continue;
      fitted[i * 3 + 2] += (target - dz) * 0.8;
    }
  };
  if (Math.abs(used.breast_volume || 0) > 0.02 || Math.abs(used.breast_height || 0) > 0.02) {
    pullChest(-1);
    pullChest(1);
  }
}

function addClothes(mesh, humanPos, outfit, style, weights) {
  const worn = [];
  const basis = state.pack.human.basis;
  for (const key of [outfit.suit, outfit.shoes]) {
    const proxy = state.pack.proxies[key];
    if (!proxy || !proxy.skinIndex) continue;
    const fitted = fitProxy(proxy, humanPos);
    if (proxy.kind !== "shoes") conformCloth(fitted, fitProxy(proxy, basis), key, humanPos, weights);
    const span = ySpan(fitted);
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(fitted, 3));
    geometry.setAttribute("uv", new THREE.BufferAttribute(new Float32Array(proxy.uv), 2));
    geometry.setAttribute("skinIndex", new THREE.Uint16BufferAttribute(proxy.skinIndex, 4));
    geometry.setAttribute("skinWeight", new THREE.Float32BufferAttribute(proxy.skinWeight, 4));
    geometry.setIndex(new THREE.BufferAttribute(new Uint32Array(proxy.faces), 1));
    geometry.computeVertexNormals();
    // The garment is fitted to the same morphs as the body, but a pec or a
    // hip still breaks through where the authored offset is only a few
    // millimetres. A small step along the normal keeps the cloth outside.
    const gap = proxy.kind === "shoes" ? 0.003 : 0.006;
    const normals = geometry.attributes.normal.array;
    for (let i = 0; i < fitted.length; i += 3) {
      fitted[i] += normals[i] * gap;
      fitted[i + 1] += normals[i + 1] * gap;
      fitted[i + 2] += normals[i + 2] * gap;
    }
    geometry.attributes.position.needsUpdate = true;
    const object = new THREE.SkinnedMesh(
      geometry,
      clothMaterial(style, proxy, outfit.top, outfit.bottom, span.minY, span.span),
    );
    // Bone matrices are in world space. Attached mode, the same mode as the
    // body, rebuilds bindMatrixInverse from this mesh's world matrix. A
    // detached inverse stayed at identity and the placement was applied twice.
    object.bindMode = "attached";
    object.bind(mesh.skeleton, mesh.bindMatrix);
    object.castShadow = true;
    object.frustumCulled = false;
    object.renderOrder = proxy.kind === "shoes" ? 2 : 1;
    object.position.copy(mesh.position);
    object.quaternion.copy(mesh.quaternion);
    object.scale.copy(mesh.scale);
    mesh.parent.add(object);
    worn.push(object);
  }
  return worn;
}

// eyebrow001's inner ends sit between the eyes, on the nose, and the hair
// strokes there read as a dark grid. The arcs over the eyes stay.
function browFaces(proxy, humanPos) {
  const fitted = fitProxy(proxy, humanPos);
  const faces = proxy.faces;
  const kept = [];
  for (let t = 0; t < faces.length; t += 3) {
    const a = faces[t];
    const b = faces[t + 1];
    const c = faces[t + 2];
    const cx = (fitted[a * 3] + fitted[b * 3] + fitted[c * 3]) / 3;
    if (Math.abs(cx) < 0.02) continue;
    kept.push(a, b, c);
  }
  return kept;
}

function thickenBrows(fitted) {
  let leftY = 0;
  let leftN = 0;
  let rightY = 0;
  let rightN = 0;
  for (let i = 0; i < fitted.length; i += 3) {
    if (fitted[i] < 0) {
      leftY += fitted[i + 1];
      leftN += 1;
    } else {
      rightY += fitted[i + 1];
      rightN += 1;
    }
  }
  if (leftN) leftY /= leftN;
  if (rightN) rightY /= rightN;
  const out = new Float32Array(fitted);
  for (let i = 0; i < out.length; i += 3) {
    const mid = out[i] < 0 ? leftY : rightY;
    out[i + 1] = mid + (out[i + 1] - mid) * 1.12;
  }
  return out;
}

function chosenHair(body, sim) {
  if (state.edit && state.hairStyle) return state.hairStyle;
  return hairStyleOf(body, sim);
}

function smooth01(edge0, edge1, x) {
  const t = Math.min(1, Math.max(0, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

const hairAlphaMaps = new Map();

function hairAlphaMap(texture) {
  const image = texture && texture.image;
  if (!image || !image.width) return null;
  if (hairAlphaMaps.has(image)) return hairAlphaMaps.get(image);
  const canvas = document.createElement("canvas");
  canvas.width = image.width;
  canvas.height = image.height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  context.drawImage(image, 0, 0);
  const map = {
    data: context.getImageData(0, 0, image.width, image.height).data,
    width: image.width,
    height: image.height,
  };
  hairAlphaMaps.set(image, map);
  return map;
}

// Proxy textures are OBJ uvs with flipY, so v = 0 is the bottom row.
function hairTexelAlpha(map, u, v) {
  if (!map) return 1;
  const x = Math.min(map.width - 1, Math.max(0, Math.round(u * (map.width - 1))));
  const y = Math.min(map.height - 1, Math.max(0, Math.round((1 - v) * (map.height - 1))));
  return map.data[(y * map.width + x) * 4 + 3] / 255;
}

// Same discard as the hair vertex shader. A bald crown has no cards, so the
// skin there stays the skin colour.
function hairKept(point, crown, bald) {
  if (bald < 0.12) return true;
  const dx = point[0] - crown[0];
  const dz = point[2] - crown[2];
  const radial = Math.hypot(dx, dz);
  const radius = 0.018 + 0.054 * bald;
  const crownHole = (1 - smooth01(radius * 0.28, radius, radial))
    * smooth01(crown[1] - 0.12, crown[1] - 0.015, point[1]);
  const temple = smooth01(0.022, 0.055, Math.abs(dx))
    * smooth01(crown[2] - 0.015, crown[2] + 0.045, point[2])
    * smooth01(crown[1] - 0.14, crown[1] - 0.045, point[1])
    * (1 - smooth01(crown[1] - 0.02, crown[1] + 0.01, point[1]));
  const keep = 1 - Math.min(
    1,
    smooth01(0.12, 0.55, bald) * crownHole + smooth01(0.18, 0.75, bald) * temple,
  );
  return keep > 0.5;
}

// One point per 1.6 cm cell, at most 80. The fragment shader measures to these.
// Empty slots stay under the floor so they never tint the skin.
function packRim(slots, points) {
  for (let i = 0; i < slots.length; i += 1) slots[i].set(0, -8, 0);
  if (!points.length) return;
  const cell = 0.016;
  const bins = new Map();
  for (let i = 0; i < points.length; i += 1) {
    const p = points[i];
    const key = `${Math.floor(p[0] / cell)},${Math.floor(p[1] / cell)},${Math.floor(p[2] / cell)}`;
    if (!bins.has(key)) bins.set(key, p);
  }
  let list = [...bins.values()];
  if (list.length > slots.length) {
    list.sort((a, b) => Math.atan2(a[0], a[2]) - Math.atan2(b[0], b[2]) || b[1] - a[1]);
    const step = list.length / slots.length;
    const kept = [];
    for (let i = 0; i < slots.length; i += 1) {
      kept.push(list[Math.min(list.length - 1, Math.floor(i * step))]);
    }
    list = kept;
  }
  for (let i = 0; i < list.length; i += 1) slots[i].set(list[i][0], list[i][1], list[i][2]);
}

function opaqueHairPoints(proxy, humanPos, threshold, bald) {
  const fitted = fitProxy(proxy, humanPos);
  let crownI = 0;
  for (let i = 3; i < fitted.length; i += 3) {
    if (fitted[i + 1] > fitted[crownI + 1]) crownI = i;
  }
  const crown = [fitted[crownI], fitted[crownI + 1], fitted[crownI + 2]];
  const map = hairAlphaMap(proxy.texture);
  const uv = proxy.uv;
  const points = [];
  for (let i = 0; i < proxy.count; i += 1) {
    if (hairTexelAlpha(map, uv[i * 2], uv[i * 2 + 1]) < threshold) continue;
    const point = [fitted[i * 3], fitted[i * 3 + 1], fitted[i * 3 + 2]];
    if (!hairKept(point, crown, bald)) continue;
    points.push(point);
  }
  return points;
}

function paintScalp(mesh, humanPos, body, sim) {
  const slots = mesh.material.userData.rim;
  if (!slots) return;
  const hairName = chosenHair(body, sim);
  const density = clamp(body.hair_density, 0, 1);
  const pack = state.pack.proxies[hairName];
  if (!(density > 0.02 && pack)) {
    packRim(slots, []);
    return;
  }
  const bald = hairName === "hair_short" ? clamp(body.balding_stage, 0, 7) / 7 : 0;
  const threshold = THREE.MathUtils.lerp(0.9, 0.2, density);
  packRim(slots, opaqueHairPoints(pack, humanPos, threshold, bald));
}

// Top of the fitted hair. The scalp crown sits under it, and a cap measured
// from the scalp paints the part down onto the forehead.
function hairCrown(proxy, humanPos) {
  const fitted = fitProxy(proxy, humanPos);
  let best = 0;
  for (let i = 0; i < fitted.length; i += 3) {
    if (fitted[i + 1] > fitted[best + 1]) best = i;
  }
  return [fitted[best], fitted[best + 1], fitted[best + 2]];
}

function addGrooming(mesh, head, humanPos, body, hairHex, irisHex, skinHex, style, marks, sim) {
  const pack = state.pack.proxies;
  const hairName = chosenHair(body, sim);
  const density = clamp(body.hair_density, 0, 1);
  if (density > 0.02 && pack[hairName]) {
    const bald = hairName === "hair_short" ? clamp(body.balding_stage, 0, 7) / 7 : 0;
    const threshold = THREE.MathUtils.lerp(0.9, 0.2, density);
    const cap = hairMeta(hairName).cap;
    const crown = hairCrown(pack[hairName], humanPos);
    addFitted(
      mesh, head, pack[hairName], humanPos,
      strandMaterial(style, pack[hairName].texture, hairHex, threshold, bald, crown, -1, true, true, cap, false),
      2,
    );
  }
  const brows = clamp(body.eyebrow_density, 0, 1);
  if (brows > 0.02) {
    const browCut = style === "A" ? THREE.MathUtils.lerp(0.38, 0.15, brows) : THREE.MathUtils.lerp(0.45, 0.18, brows);
    addFitted(
      mesh, head, { ...pack.brows, faces: browFaces(pack.brows, humanPos) }, humanPos,
      // The card's empty area is not true zero: alpha 1–30 is compression fringe.
      // A threshold near 0.02 turns that fringe into a dark grid on the nose.
      strandMaterial(style, pack.brows.texture, hairHex, browCut, 0, marks.crown, -8),
      3,
      0.003,
      style === "A" ? thickenBrows : null,
    );
  }
  // The cornea sits behind the lid surface. Face is +Z. Enough to meet the
  // opening, not enough to leave the sphere in front of the lids.
  addFitted(mesh, head, pack.eyes, humanPos, eyeMaterial(style, pack.eyes.texture, irisHex, skinHex), 1, 0.0075);
  // Style A keeps a thin dark fringe. The hard cut read as a row of stitches,
  // and the back faces doubled it.
  // Style A: darker and thinner than the stitch row, still visible as lashes.
  const lashDark = style === "A" ? 0.85 : 0.62;
  const lashCut = style === "A" ? 0.5 : 0.32;
  const lashMat = strandMaterial(
    style, pack.lashes.texture,
    hexOf(mixLinear(linearOf(hairHex), [0, 0, 0], lashDark)),
    lashCut, 0, marks.crown, -6, false, style === "A",
  );
  if (style === "A") lashMat.side = THREE.FrontSide;
  addFitted(mesh, head, pack.lashes, humanPos, lashMat, 4, style === "A" ? 0.0022 : 0.0035);
  return { hairName };
}

function centerRows(cloud) {
  const { xyz, count, maxY } = cloud;
  const bins = new Map();
  for (let i = 0; i < count; i += 1) {
    const x = xyz[i * 3];
    const dy = xyz[i * 3 + 1] - maxY;
    const z = xyz[i * 3 + 2];
    if (Math.abs(x) > 0.012 || dy < -0.36 || dy > -0.08 || z < 0.02) continue;
    const key = Math.round(dy * 200) / 200;
    const row = bins.get(key);
    if (row) row.push(z);
    else bins.set(key, [z]);
  }
  return [...bins.entries()].sort((a, b) => a[0] - b[0]).map(([dy, zs]) => {
    zs.sort((a, b) => a - b);
    return { dy, p90: zs[Math.min(zs.length - 1, Math.floor(0.9 * (zs.length - 1)))] };
  });
}

// Lower lip is the sharp forward step on the midline. The chin tip is the
// most posterior point of that midline below the lip, before the neck swings
// forward again. dy is metres below the crown.
function jawProfile(cloud) {
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
  let lipLo = null;
  for (let i = rows.length - 1; i >= 1; i -= 1) {
    const row = rows[i];
    if (row.dy > noseDy - 0.025) continue;
    if (row.dy < noseDy - 0.1) break;
    if (row.p90 - rows[i - 1].p90 > 0.03) {
      lipLo = row.dy;
      break;
    }
  }
  if (lipLo === null) lipLo = noseDy - 0.06;
  // The chin tip is only a few centimetres below the lower lip. Walking
  // further down follows the neck, which swings forward and was painted as a bib.
  let chinDy = lipLo - 0.035;
  let chinZ = Infinity;
  for (let i = rows.length - 1; i >= 0; i -= 1) {
    const row = rows[i];
    if (row.dy > lipLo - 0.012) continue;
    if (row.dy < lipLo - 0.042) break;
    if (row.p90 < chinZ) {
      chinZ = row.p90;
      chinDy = row.dy;
    }
  }
  if (chinZ === Infinity) chinZ = 0.07;
  return { lipLo, noseDy, noseZ, chinDy, chinZ };
}

// Rest pose (body.glb): upper-arm local +Y is the bone, local +Z is world
// forward. The elbow is a +X bend of the forearm, so the hand sits forward.
// setFromUnitVectors rolls that bend behind the body. A local-Z turn swings
// the arm in the coronal plane and leaves the palm facing inward.
function hangArms(root, child) {
  const origin = new THREE.Vector3();
  const tip = new THREE.Vector3();
  const target = new THREE.Vector3();
  const pose = {};
  const out = 0.14 + 0.32 * Math.min(Math.max(child || 0, 0), 1);
  root.updateWorldMatrix(true, true);
  for (const side of ["l", "r"]) {
    const sign = side === "l" ? 1 : -1;
    const upper = findBone(root, `upperarm_${side}`);
    const lower = findBone(root, `lowerarm_${side}`);
    const hand = findBone(root, `hand_${side}`);
    if (!upper || !lower || !hand) continue;
    upper.getWorldPosition(origin);
    lower.getWorldPosition(tip);
    tip.sub(origin);
    tip.z = 0;
    if (tip.lengthSq() < 1e-8) continue;
    tip.normalize();
    // Children are rounder than the skeleton, so the same hang sits inside the torso.
    target.set(sign * out, -1, 0).normalize();
    const angle = Math.atan2(
      tip.x * target.y - tip.y * target.x,
      tip.x * target.x + tip.y * target.y,
    );
    upper.rotateZ(angle);
    lower.quaternion.setFromAxisAngle(new THREE.Vector3(1, 0, 0), 0.22);
    upper.updateWorldMatrix(true, true);
    const dir = (bone, child) => {
      bone.getWorldPosition(origin);
      child.getWorldPosition(tip);
      tip.sub(origin).normalize();
      return [tip.x, tip.y, tip.z].map((value) => Math.round(value * 100) / 100);
    };
    const palm = new THREE.Vector3(0, 0, 1).applyQuaternion(hand.getWorldQuaternion(new THREE.Quaternion()));
    pose[side] = {
      upper: dir(upper, lower),
      forearm: dir(lower, hand),
      palm: [palm.x, palm.y, palm.z].map((value) => Math.round(value * 100) / 100),
    };
  }
  root.userData.arms = pose;
}

// Local +Y runs along the bone. A negative thigh X and a positive calf X
// bring the knee forward and the heel back. The foot X undoes most of that
// pitch so the sole stays down. Left is +X, so a positive pelvis Z lifts it.
function idleStance(root, member) {
  const hash = hashSim(personKey(member));
  const weightLeft = (hash & 1) === 0;
  const vary = 0.82 + ((hash >>> 8) % 7) * 0.03;
  const young = typeof member.age_years === "number" && member.age_years <= 13 ? 0.85 : 1;
  const amount = vary * young;
  const sign = weightLeft ? 1 : -1;
  const turn = findBone(root, "pelvis");
  if (turn) turn.rotateZ(0.09 * amount * sign);
  [0.04, 0.028, 0.012].forEach((roll, index) => {
    const spine = findBone(root, `spine_0${index + 1}`);
    if (spine) spine.rotateZ(-roll * amount * sign);
  });
  const mid = findBone(root, "spine_02");
  if (mid) mid.rotateY(0.018 * amount * ((hash >>> 4) & 1 ? 1 : -1));
  const drop = 0.07 * amount;
  const clavL = findBone(root, "clavicle_l");
  const clavR = findBone(root, "clavicle_r");
  if (clavL) clavL.rotateZ(-drop - (weightLeft ? 0.018 * amount : 0));
  if (clavR) clavR.rotateZ(drop + (weightLeft ? 0 : 0.018 * amount));
  bendLeg(root, weightLeft ? "r" : "l", 0.22 * amount, 0.55 * amount);
  bendLeg(root, weightLeft ? "l" : "r", 0.05 * amount, 0.1 * amount);
  const arm = young < 1 ? 0.45 : 1;
  relaxArm(root, weightLeft ? "r" : "l", 0.1 * amount * arm);
  relaxArm(root, weightLeft ? "l" : "r", 0.03 * amount * arm);
  curlHand(root, "l", amount * (0.85 + ((hash >>> 12) % 4) * 0.04));
  curlHand(root, "r", amount * (0.72 + ((hash >>> 16) % 4) * 0.04));
  root.updateWorldMatrix(true, true);
}

function bendLeg(root, side, thighPitch, knee) {
  const thigh = findBone(root, `thigh_${side}`);
  const calf = findBone(root, `calf_${side}`);
  const foot = findBone(root, `foot_${side}`);
  if (!thigh || !calf || !foot) return;
  thigh.rotateX(-thighPitch);
  calf.rotateX(knee);
  foot.rotateX(-(knee - thighPitch) * 0.85);
}

function relaxArm(root, side, extra) {
  const lower = findBone(root, `lowerarm_${side}`);
  if (lower) lower.rotateX(extra);
}

function curlHand(root, side, scale) {
  const hand = findBone(root, `hand_${side}`);
  if (!hand) return;
  root.updateWorldMatrix(true, true);
  [
    ["index", 0.38],
    ["middle", 0.44],
    ["ring", 0.48],
    ["pinky", 0.42],
  ].forEach(([name, base]) => {
    [1, 2, 3].forEach((seg) => {
      const bone = findBone(root, `${name}_0${seg}_${side}`);
      if (!bone) return;
      const bend = base * scale * (seg === 2 ? 1.1 : seg === 3 ? 0.65 : 0.8);
      curlTowardPalm(bone, hand, bend);
    });
  });
  [0.15, 0.18, 0.08].forEach((angle, index) => {
    const bone = findBone(root, `thumb_0${index + 1}_${side}`);
    if (bone) curlTowardPalm(bone, hand, angle * scale);
  });
}

function curlTowardPalm(bone, hand, angle) {
  const handQ = hand.getWorldQuaternion(new THREE.Quaternion());
  const boneQ = bone.getWorldQuaternion(new THREE.Quaternion());
  const palm = new THREE.Vector3(0, 0, 1).applyQuaternion(handQ);
  const along = new THREE.Vector3(0, 1, 0).applyQuaternion(boneQ);
  const axis = new THREE.Vector3().crossVectors(along, palm);
  if (axis.lengthSq() < 1e-8) return;
  axis.normalize().applyQuaternion(boneQ.invert());
  bone.rotateOnAxis(axis, angle);
  bone.updateWorldMatrix(true, true);
}

function neutralBody(eumelanin, pheomelanin) {
  const body = {
    body_form_age: 30,
    face_form_age: 30,
    height_cm: 170,
    leg_ratio_z: 0,
    head_size_z: 0,
    body_fat_z: 0,
    fat_distribution_z: 0,
    muscle_z: 0,
    shoulder_width_z: 0,
    chest_depth_z: 0,
    hip_width_z: 0,
    skin_eumelanin: eumelanin,
    skin_pheomelanin: pheomelanin,
    hair_eumelanin: eumelanin,
    hair_pheomelanin: pheomelanin,
    iris_melanin: eumelanin,
    hair_curl: 0,
    hair_density: 1,
    balding_stage: 0,
    hair_grey_fraction: 0,
    eyebrow_density: 0.4,
    beard_density: 0,
    beard_coverage: 0,
    body_hair: 0,
    androgenic_development: 0.5,
    estrogenic_development: 0.5,
    breast_development: 0.5,
  };
  for (let axis = 1; axis <= 12; axis += 1) {
    body[`face_axis_${String(axis).padStart(2, "0")}`] = 0;
  }
  return body;
}

function paletteMembers() {
  return [
    ["min", 0, 0],
    ["mid", 0.5, 0.5],
    ["max", 1, 1],
  ].map(([label, eumelanin, pheomelanin]) => ({
    role: "palette",
    palette: true,
    sex: "female",
    first_name: "palette check",
    surname: "",
    age_years: label,
    body: neutralBody(eumelanin, pheomelanin),
  }));
}

function fullRoster() {
  return state.family.members.concat(paletteMembers());
}

function compareRoster() {
  const adult = state.family.members.find((member) => member.role === "father")
    || state.family.members.find((member) => member.age_years >= 25);
  const word = state.compare === "muscle" ? "Muskeln" : "Fett";
  return [-2, 0, 2].map((value) => ({
    ...adult,
    first_name: `${word} ${value > 0 ? "+" : ""}${value}`,
    surname: "",
    palette: false,
    body: {
      ...adult.body,
      body_fat_z: state.compare === "fat" ? value : 0,
      muscle_z: state.compare === "muscle" ? value : 0,
    },
  }));
}

function roster() {
  if (state.edit && state.editBody) {
    const member = state.family.members[state.editIndex];
    return [{ ...member, body: state.editBody }];
  }
  if (state.compare) return compareRoster();
  let all = fullRoster();
  if (state.child) all = all.filter((member) => member.age_years <= 13);
  if (state.focus) {
    const name = state.focus.toLowerCase();
    all = all.filter((member) => (member.first_name || "").toLowerCase() === name);
  }
  if (!state.close) return all;
  const pages = Math.max(1, Math.ceil(all.length / FACE_COUNT));
  state.facePage = clamp(state.facePage, 0, pages - 1);
  const start = state.facePage * FACE_COUNT;
  return all.slice(start, start + FACE_COUNT);
}

function layoutX(members) {
  const gap = state.close ? 0.175 : 0.78;
  const breakGap = state.close ? 0.28 : 1.15;
  let cursor = 0;
  const xs = members.map((member, index) => {
    if (index > 0) {
      const split = member.palette && !members[index - 1].palette;
      cursor += split ? gap + breakGap : gap;
    }
    return cursor;
  });
  const mid = xs.length ? (xs[0] + xs[xs.length - 1]) / 2 : 0;
  return xs.map((x) => x - mid);
}

function buildPerson(template, member, x) {
  const root = cloneSkeleton(template);
  const mesh = findMesh(root);
  const emphasise = state.style === "A" && !member.palette;
  const editor = state.editMode === 2;
  const extra = editor ? state.provisional : null;
  const shaped = morphWeights(member.body, member.sex, emphasise, extra, editor);
  const weights = shaped.weights;
  applyWeights(mesh, weights);
  smoothSkinNormals(mesh);
  const measured = morphedYSpan(mesh, Object.fromEntries(
    Object.entries(weights).filter(([name]) => MEASURE_KEYS.has(name)),
  ));
  const standingCm = measured.span * 100;
  const scale = standingCm > 1 ? member.body.height_cm / standingCm : 1;
  const bone = findBone(root, "Root");
  const head = findBone(root, "head");
  const pivot = bone ? bone.position.y : 0.81676;
  if (bone) bone.scale.setScalar(scale);
  const wrapper = new THREE.Group();
  wrapper.add(root);
  // Root scale lifts the feet by pivot * (scale - 1). Drop the wrapper so they stay on Y=0.
  wrapper.position.y = -pivot * (1 - scale);
  wrapper.position.x = x;
  const cloud = morphedCloud(mesh, weights);
  const marks = landmarks(cloud);
  const skin = gridColor(SKIN, member.body.skin_eumelanin, member.body.skin_pheomelanin);
  const geneticHair = hairColor(member.body);
  const hair = state.edit && state.hairDye ? state.hairDye : geneticHair;
  const iris = rampColor(IRIS, member.body.iris_melanin);
  const cloth = morphedYSpan(mesh, weights);
  const cutY = state.close ? marks.maxY + marks.jaw.noseDy - 0.098 : -1;
  const bearded = member.sex === "male";
  mesh.material = bodyMaterial(skin, state.style, cloth.minY, cloth.span, cutY, {
    color: hair,
    density: bearded ? clamp(member.body.beard_density, 0, 1) : 0,
    coverage: bearded ? clamp(member.body.beard_coverage, 0, 1) : 0,
  }, marks, {
    density: clamp(member.body.hair_density, 0, 1),
  });
  mesh.castShadow = true;
  mesh.frustumCulled = false;
  const body = { ...member.body, sex: member.sex };
  const humanPos = humanPositions(weights, editor);
  paintScalp(mesh, humanPos, body, personKey(member));
  const outfit = outfitOf(member);
  const suitProxy = state.pack.proxies[outfit.suit];
  const shoeProxy = state.pack.proxies[outfit.shoes];
  let covered = null;
  if (state.bodyHm && suitProxy) {
    covered = hiddenFaces(mesh.geometry, state.bodyHm.hm, deleteSet([suitProxy, shoeProxy]));
    mesh.geometry.setIndex(new THREE.BufferAttribute(covered.index, 1));
  }
  let outline = null;
  if (state.style === "B") {
    outline = new THREE.SkinnedMesh(mesh.geometry, outlineMaterial(cutY));
    outline.bind(mesh.skeleton, mesh.bindMatrix);
    outline.morphTargetInfluences = mesh.morphTargetInfluences;
    outline.morphTargetDictionary = mesh.morphTargetDictionary;
    outline.frustumCulled = false;
    mesh.parent.add(outline);
  }
  const groom = addGrooming(mesh, head, humanPos, body, hair, iris, skin, state.style, marks, personKey(member));
  const clothes = addClothes(mesh, humanPos, outfit, state.style, weights);
  if (state.close) {
    for (const piece of clothes) piece.visible = false;
  }
  scene.add(wrapper);
  wrapper.updateWorldMatrix(true, true);
  // hangArms reads the arm in world XY and drops world Z. A yaw already on
  // the wrapper turns that bone onto Z, the hang is skipped, and the side
  // view stays in the T-pose. Pose first, then turn the figure.
  hangArms(root, shaped.slices.child);
  idleStance(root, member);
  if (state.editMode === 2) {
    wrapper.rotation.y = state.editYaw || 0;
    wrapper.updateWorldMatrix(true, true);
  }
  const spines = ["spine_01", "spine_02", "spine_03"].map((name) => findBone(root, name)).filter(Boolean);
  const rest = spines.map((spine) => spine.quaternion.clone());
  const tag = document.createElement("div");
  tag.className = "tag";
  const name = document.createElement("b");
  name.textContent = member.palette
    ? "palette check"
    : `${member.first_name || ""} ${member.surname || ""}`.trim();
  const line = document.createElement("span");
  line.textContent = member.palette ? String(member.age_years) : `${member.age_years} · ${member.sex}`;
  tag.append(name, line);
  labels.append(tag);
  return {
    wrapper, mesh, head, outline, spines, rest, scale, tag, marks,
    anchor: new THREE.Vector3(), member, shaped, skin, hair, geneticHair, iris,
    hairName: groom.hairName, beardTris: 0, arms: root.userData.arms,
    outfit, covered: covered ? covered.dropped : 0, clothes,
  };
}

function weightRow(person) {
  const { weights, share, slices } = person.shaped;
  const sum = (prefix) => ["male", "female"].reduce((total, branch) => (
    total + (weights[`${prefix}_${branch}_max`] || 0) + (weights[`${prefix}_${branch}_min`] || 0)
  ), 0);
  return {
    name: person.member.palette ? `palette ${person.member.age_years}` : `${person.member.first_name} ${person.member.age_years}`,
    sex: person.member.sex,
    age: person.member.body.body_form_age,
    gender: share.gender,
    male: share.male,
    female: share.female,
    child: slices.child,
    young: slices.young,
    old: slices.old,
    breast: person.member.body.breast_development,
    breast_max: weights.breast_max,
    breast_min: weights.breast_min,
    fat: sum("fat"),
    muscle: sum("muscle"),
    skin: person.skin,
    hair: person.hair,
    iris: person.iris,
    skin_eumelanin: person.member.body.skin_eumelanin,
    skin_pheomelanin: person.member.body.skin_pheomelanin,
    hair_eumelanin: person.member.body.hair_eumelanin,
    hair_pheomelanin: person.member.body.hair_pheomelanin,
    hair_grey_fraction: person.member.body.hair_grey_fraction,
    iris_melanin: person.member.body.iris_melanin,
    hair_proxy: person.hairName,
    suit: person.outfit.suit,
    shoes: person.outfit.shoes,
    covered: person.covered,
    beard_tris: person.beardTris,
    balding_stage: person.member.body.balding_stage,
    beard_density: person.member.body.beard_density,
    beard_coverage: person.member.body.beard_coverage,
  };
}

function publishReport() {
  window.__styleReport = state.people.map(weightRow);
}

function restyle() {
  for (const person of state.people) {
    scene.remove(person.wrapper);
    person.tag.remove();
  }
  state.people = [];
  const members = roster();
  const xs = layoutX(members);
  members.forEach((member, index) => {
    state.people.push(buildPerson(state.template, member, xs[index]));
  });
  assignBackdrop();
  applyStyleLights(state.style);
  publishReport();
  updateFaceButtons();
  state.pickDirty = true;
}

function landmarkWorld(person, point) {
  const mesh = person.mesh;
  const head = person.head;
  const index = mesh.skeleton.bones.indexOf(head);
  const placed = new THREE.Vector3(point[0], point[1], point[2]);
  placed.applyMatrix4(mesh.bindMatrix);
  placed.applyMatrix4(mesh.skeleton.boneInverses[index]);
  placed.applyMatrix4(head.matrixWorld);
  return placed;
}

function frame() {
  const people = state.people;
  if (!people.length) return;
  scene.updateMatrixWorld(true);
  camera.fov = 20;
  camera.updateProjectionMatrix();
  const v = THREE.MathUtils.degToRad(camera.fov);
  const aspect = camera.aspect || 1.8;
  const h = 2 * Math.atan(Math.tan(v / 2) * aspect);
  if (state.close && !state.edit) {
    scene.updateMatrixWorld(true);
    const crownY = people.map((person) => landmarkWorld(person, person.marks.crown).y);
    const tallest = Math.max(...crownY);
    people.forEach((person, index) => {
      person.wrapper.position.y += tallest - crownY[index];
    });
    scene.updateMatrixWorld(true);
    let minY = Infinity;
    let maxY = -Infinity;
    let minX = Infinity;
    let maxX = -Infinity;
    let sumZ = 0;
    for (const person of people) {
      const crown = landmarkWorld(person, person.marks.crown);
      const neck = landmarkWorld(person, person.marks.neck);
      minY = Math.min(minY, neck.y);
      maxY = Math.max(maxY, crown.y);
      minX = Math.min(minX, crown.x - 0.085);
      maxX = Math.max(maxX, crown.x + 0.085);
      sumZ += crown.z;
    }
    const top = maxY + 0.008;
    const bot = minY - 0.02;
    const midY = (top + bot) / 2;
    const midX = (minX + maxX) / 2;
    const targetH = Math.max(top - bot, 0.2);
    const width = Math.max(maxX - minX, 0.2);
    const distH = (targetH / 0.98) / (2 * Math.tan(v / 2));
    const distW = width / (2 * Math.tan(h / 2));
    const distance = Math.max(distH, distW);
    const faceZ = sumZ / people.length;
    camera.position.set(midX, midY, faceZ + distance);
    camera.lookAt(midX, midY, faceZ);
  } else if (state.editMode === 2) {
    const person = people[0];
    const spec = regionSpec(state.directRegion);
    if (spec && spec.band === "face") {
      scene.updateMatrixWorld(true);
      const crown = landmarkWorld(person, person.marks.crown);
      const chin = landmarkWorld(person, person.marks.chin);
      const faceH = Math.max(crown.y - chin.y, 0.16) * 1.55;
      const midY = (crown.y + chin.y) / 2;
      const midX = crown.x - 0.02;
      const dist = (faceH / 0.78) / (2 * Math.tan(v / 2));
      camera.position.set(midX, midY, chin.z + dist);
      camera.lookAt(midX, midY, chin.z);
    } else {
      const maxH = person.member.body.height_cm / 100;
      const midX = person.wrapper.position.x;
      const lookY = maxH * 0.48;
      const bias = maxH * 0.16;
      const distH = (maxH / 0.82) / (2 * Math.tan(v / 2));
      const distW = (maxH * 0.7) / (2 * Math.tan(h / 2));
      camera.position.set(midX + bias, lookY, Math.max(distH, distW));
      camera.lookAt(midX + bias * 0.2, lookY, 0);
    }
  } else if (state.focus) {
    const xs = people.map((person) => person.wrapper.position.x);
    const heights = people.map((person) => person.member.body.height_cm / 100);
    const maxH = Math.max(...heights);
    const minX = Math.min(...xs) - 0.35;
    const maxX = Math.max(...xs) + 0.35;
    const midX = (minX + maxX) / 2;
    const width = Math.max(maxX - minX, 0.7);
    const lookY = maxH * 0.46;
    const distH = (maxH / 0.82) / (2 * Math.tan(v / 2));
    const distW = width / (2 * Math.tan(h / 2));
    const distance = Math.max(distH, distW);
    camera.position.set(midX, lookY + maxH * 0.02, distance);
    camera.lookAt(midX, lookY, 0);
  } else {
    const xs = people.map((person) => person.wrapper.position.x);
    const heights = people.map((person) => person.member.body.height_cm / 100);
    const maxH = Math.max(...heights);
    const reach = 0.36;
    const minX = Math.min(...xs) - reach;
    const maxX = Math.max(...xs) + reach;
    const midX = (minX + maxX) / 2;
    const width = Math.max(maxX - minX, 0.9);
    const lookY = maxH * 0.48;
    const distH = (maxH / 0.9) / (2 * Math.tan(v / 2));
    const distW = width / (2 * Math.tan(h / 2));
    const distance = Math.max(distH, distW);
    camera.position.set(midX + distance * 0.08, lookY + maxH * 0.08, distance);
    camera.lookAt(midX, lookY, 0);
  }
  camera.updateProjectionMatrix();
}

function resize() {
  const view = document.querySelector("#view");
  const width = view.clientWidth;
  const height = view.clientHeight;
  renderer.setSize(width, height, false);
  camera.aspect = width / Math.max(height, 1);
  camera.updateProjectionMatrix();
  composer.setSize(width, height);
  renderer.getDrawingBufferSize(drawSize);
  fxaaPass.material.uniforms.resolution.value.set(1 / Math.max(drawSize.x, 1), 1 / Math.max(drawSize.y, 1));
}

function animate() {
  const delta = clock.getDelta();
  state.clock += delta;
  const breath = state.still ? 0 : Math.sin(state.clock * 1.6);
  const sway = state.still ? 0 : Math.sin(state.clock * 0.7);
  for (const person of state.people) {
    person.spines.forEach((spine, index) => {
      const extra = new THREE.Quaternion().setFromEuler(new THREE.Euler(
        breath * 0.03 * (index + 1) * 0.45,
        sway * 0.012,
        0,
      ));
      spine.quaternion.copy(person.rest[index]).multiply(extra);
    });
    if (state.close) {
      const neck = landmarkWorld(person, person.marks.neck);
      person.anchor.set(neck.x, neck.y, neck.z);
    } else {
      person.anchor.set(person.wrapper.position.x, 0.02, 0);
    }
    person.anchor.project(camera);
    const x = (person.anchor.x * 0.5 + 0.5) * canvas.clientWidth;
    const y = (-person.anchor.y * 0.5 + 0.5) * canvas.clientHeight;
    person.tag.style.left = `${x}px`;
    person.tag.style.top = `${y}px`;
  }
  updateKeyDir();
  if (state.editMode === 2) placeDirectChrome();
  if (state.style === "A") composer.render();
  else renderer.render(scene, camera);
  if (!state.ready) {
    state.ready = true;
    document.body.dataset.ready = "1";
    window.__styleReady = true;
    status.textContent = "";
  }
  requestAnimationFrame(animate);
}

function setStyle(style) {
  state.style = style;
  for (const id of ["a", "b", "c"]) {
    document.querySelector(`#style-${id}`).setAttribute("aria-pressed", style === id.toUpperCase() ? "true" : "false");
  }
  syncUrl();
  if (state.template) {
    restyle();
    frame();
  }
}

function syncUrl() {
  const url = new URL(location.href);
  url.searchParams.set("style", state.style);
  if (state.editMode === 2) url.searchParams.set("edit", "2");
  else if (state.editMode === 1) url.searchParams.set("edit", "1");
  else url.searchParams.delete("edit");
  if (state.close) url.searchParams.set("close", "1");
  else url.searchParams.delete("close");
  if (state.close && state.facePage > 0) url.searchParams.set("page", String(state.facePage + 1));
  else url.searchParams.delete("page");
  history.replaceState(null, "", url);
}

function updateFaceButtons() {
  const prev = document.querySelector("#faces-prev");
  const next = document.querySelector("#faces-next");
  const label = document.querySelector("#faces-page");
  const total = state.family ? fullRoster().length : 0;
  const pages = Math.max(1, Math.ceil(total / FACE_COUNT));
  const on = state.close && !state.edit;
  prev.hidden = !on;
  next.hidden = !on;
  label.hidden = !on;
  if (!on || !total) return;
  state.facePage = clamp(state.facePage, 0, pages - 1);
  const start = state.facePage * FACE_COUNT + 1;
  const end = Math.min(total, start + FACE_COUNT - 1);
  label.textContent = `${start}–${end} of ${total}`;
  prev.disabled = state.facePage <= 0;
  next.disabled = state.facePage >= pages - 1;
}

function setClose(close) {
  state.close = close;
  document.querySelector("#close").setAttribute("aria-pressed", close ? "true" : "false");
  syncUrl();
  if (state.template) {
    restyle();
    frame();
  }
}

function setFacePage(page) {
  state.facePage = page;
  syncUrl();
  if (state.template && state.close) {
    restyle();
    frame();
  }
}

document.querySelector("#style-a").addEventListener("click", () => setStyle("A"));
document.querySelector("#style-b").addEventListener("click", () => setStyle("B"));
document.querySelector("#style-c").addEventListener("click", () => setStyle("C"));
document.querySelector("#close").addEventListener("click", () => setClose(!state.close));
document.querySelector("#faces-prev").addEventListener("click", () => setFacePage(state.facePage - 1));
document.querySelector("#faces-next").addEventListener("click", () => setFacePage(state.facePage + 1));
document.querySelector("#close").setAttribute("aria-pressed", state.close ? "true" : "false");
for (const id of ["a", "b", "c"]) {
  const style = id.toUpperCase();
  document.querySelector(`#style-${id}`).setAttribute("aria-pressed", state.style === style ? "true" : "false");
}
addEventListener("resize", () => {
  resize();
  frame();
});

function bufferView(buffer, ref, Typed) {
  return new Typed(buffer, ref.offset, ref.count);
}

async function loadPack() {
  const meta = await fetch("/cache/proxies/proxies.json").then((response) => {
    if (!response.ok) throw new Error(`proxies.json HTTP ${response.status}`);
    return response.json();
  });
  const buffer = await fetch("/cache/proxies/proxies.bin").then((response) => {
    if (!response.ok) throw new Error(`proxies.bin HTTP ${response.status}`);
    return response.arrayBuffer();
  });
  const human = {
    count: meta.human.count,
    basis: bufferView(buffer, meta.human.basis, Float32Array),
    morphs: meta.human.morphs.map((morph) => ({
      name: morph.name,
      data: bufferView(buffer, morph, Float32Array),
    })),
  };
  const loader = new THREE.TextureLoader();
  const textures = {};
  const aoTextures = {};
  const colorTexture = async (file) => {
    if (!textures[file]) {
      const texture = await loader.loadAsync(`/cache/proxies/${file}`);
      texture.colorSpace = THREE.SRGBColorSpace;
      texture.flipY = true;
      texture.wrapS = THREE.ClampToEdgeWrapping;
      texture.wrapT = THREE.ClampToEdgeWrapping;
      textures[file] = texture;
    }
    return textures[file];
  };
  const aoTexture = async (file) => {
    if (!file) return null;
    if (!aoTextures[file]) {
      const texture = await loader.loadAsync(`/cache/proxies/${file}`);
      // Weave, not colour. OBJ uvs, same flip as the diffuse.
      texture.colorSpace = THREE.NoColorSpace;
      texture.flipY = true;
      texture.wrapS = THREE.ClampToEdgeWrapping;
      texture.wrapT = THREE.ClampToEdgeWrapping;
      aoTextures[file] = texture;
    }
    return aoTextures[file];
  };
  const proxies = {};
  for (const [key, proxy] of Object.entries(meta.proxies)) {
    const entry = {
      kind: proxy.kind || "groom",
      scale: proxy.scale,
      count: proxy.count,
      texture: await colorTexture(proxy.texture),
      bind: bufferView(buffer, proxy.bind, Float32Array),
      uv: bufferView(buffer, proxy.uv, Float32Array),
      faces: bufferView(buffer, proxy.faces, Uint32Array),
    };
    if (proxy.skin_index) {
      entry.split = WAIST[key] ?? (proxy.split || 0);
      entry.ao = await aoTexture(proxy.ao);
      entry.skinIndex = bufferView(buffer, proxy.skin_index, Uint16Array);
      entry.skinWeight = bufferView(buffer, proxy.skin_weight, Float32Array);
      entry.delete = bufferView(buffer, proxy.delete, Uint32Array);
    }
    proxies[key] = entry;
  }
  const bodyRest = meta.body_rest ? bufferView(buffer, meta.body_rest, Float32Array) : null;
  return { human, proxies, bodyRest };
}

async function loadSkin() {
  const texture = await new THREE.TextureLoader().loadAsync(
    "/cache/mh-system/skins/young_caucasian_female/young_lightskinned_female_diffuse.png",
  );
  texture.colorSpace = THREE.SRGBColorSpace;
  // body.glb is a glTF export: v = 0 is the top of the image. The proxy
  // textures are raw OBJ uvs and stay flipped the other way.
  texture.flipY = false;
  texture.wrapS = THREE.ClampToEdgeWrapping;
  texture.wrapT = THREE.ClampToEdgeWrapping;
  texture.anisotropy = renderer.capabilities.getMaxAnisotropy();
  return texture;
}

async function loadBeard() {
  const texture = await new THREE.TextureLoader().loadAsync("/cache/beard-mask.png");
  // Data, not colour. Same glTF uv orientation as the skin.
  texture.colorSpace = THREE.NoColorSpace;
  texture.flipY = false;
  texture.wrapS = THREE.ClampToEdgeWrapping;
  texture.wrapT = THREE.ClampToEdgeWrapping;
  texture.generateMipmaps = false;
  texture.minFilter = THREE.LinearFilter;
  texture.magFilter = THREE.LinearFilter;
  return texture;
}

// Same order as BODY_KEYS in art/family.py. The editor lists every one.
const BODY_KEYS = [
  "body_form_age",
  "face_form_age",
  "height_cm",
  "leg_ratio_z",
  "head_size_z",
  "body_fat_z",
  "fat_distribution_z",
  "muscle_z",
  "shoulder_width_z",
  "chest_depth_z",
  "hip_width_z",
  "face_axis_01",
  "face_axis_02",
  "face_axis_03",
  "face_axis_04",
  "face_axis_05",
  "face_axis_06",
  "face_axis_07",
  "face_axis_08",
  "face_axis_09",
  "face_axis_10",
  "face_axis_11",
  "face_axis_12",
  "skin_eumelanin",
  "skin_pheomelanin",
  "hair_eumelanin",
  "hair_pheomelanin",
  "iris_melanin",
  "hair_curl",
  "hair_density",
  "balding_stage",
  "hair_grey_fraction",
  "eyebrow_density",
  "beard_density",
  "beard_coverage",
  "body_hair",
  "freckle_density",
  "face_wrinkles",
  "skin_laxity",
  "age_spots",
  "posture_stoop",
  "androgenic_development",
  "estrogenic_development",
  "breast_development",
];

const FACE_AXES = [
  ["Nasenbreite", "schmaler", "breiter"],
  ["Nasenvorsprung", "flacher", "vorspringender"],
  ["Nasenlänge", "kürzer", "länger"],
  ["Kieferbreite", "schmaler", "breiter"],
  ["Kinnhöhe", "niedriger", "höher"],
  ["Kiefervorsprung", "flacher", "vorspringender"],
  ["Wangenfülle", "flacher", "voller"],
  ["Wangenknochen", "weicher", "markanter"],
  ["Augengröße", "kleiner", "größer"],
  ["Augenabstand", "enger", "weiter"],
  ["Lippenfülle", "schmaler", "voller"],
  ["Mundbreite", "schmaler", "breiter"],
];

const EDIT_SLIDERS = [
  { group: "Körper", key: "height_cm", label: "Größe", min: 90, max: 210, step: 1 },
  { key: "body_form_age", label: "Körperreife", min: 1, max: 90, step: 1, age: true },
  { key: "body_fat_z", label: "Fettmasse", min: -3, max: 3, step: 0.05, z: true, less: "dünner", more: "schwerer" },
  { key: "fat_distribution_z", label: "Fettverteilung (Bauch ↔ Hüfte)", min: -3, max: 3, step: 0.05, z: true, less: "mehr Hüfte", more: "mehr Bauch" },
  { key: "muscle_z", label: "Muskeln", min: -3, max: 3, step: 0.05, z: true, less: "schwächer", more: "muskulöser" },
  { key: "shoulder_width_z", label: "Schulterbreite", min: -3, max: 3, step: 0.05, z: true, less: "schmaler", more: "breiter" },
  { key: "hip_width_z", label: "Beckenbreite", min: -3, max: 3, step: 0.05, z: true, less: "schmaler", more: "breiter" },
  { key: "breast_development", label: "Brust", min: 0, max: 1, step: 0.01, breast: true },
  { key: "leg_ratio_z", label: "Beinlänge", min: -3, max: 3, step: 0.05, z: true, less: "kürzer", more: "länger" },
  ...FACE_AXES.map(([label, less, more], index) => ({
    group: index === 0 ? "Gesicht" : undefined,
    key: `face_axis_${String(index + 1).padStart(2, "0")}`,
    label,
    min: -3,
    max: 3,
    step: 0.05,
    z: true,
    less,
    more,
  })),
  { group: "Farbe", key: "skin_eumelanin", label: "Hautfarbe, Eumelanin", min: 0, max: 1, step: 0.01 },
  { key: "skin_pheomelanin", label: "Hautfarbe, Phäomelanin", min: 0, max: 1, step: 0.01 },
  { key: "hair_eumelanin", label: "Haarfarbe, Eumelanin", min: 0, max: 1, step: 0.01, hair: true },
  { key: "hair_pheomelanin", label: "Haarfarbe, Phäomelanin", min: 0, max: 1, step: 0.01, hair: true },
  { key: "iris_melanin", label: "Augenfarbe", min: 0, max: 1, step: 0.01 },
  { key: "balding_stage", label: "Glatze", min: 0, max: 7, step: 0.1 },
  { key: "beard_density", label: "Bart", min: 0, max: 1, step: 0.01 },
];

let editDraw = null;

function formatValue(value) {
  if (!Number.isFinite(value)) return String(value);
  const abs = Math.abs(value);
  if (abs !== 0 && abs < 0.0001) return value.toExponential(2);
  return String(Math.round(value * 10000) / 10000);
}

function breastSliderOn(body, sex) {
  return sex === "female" && body.body_form_age > 12;
}

function writeContract() {
  const list = document.querySelector("#edit-contract");
  list.replaceChildren();
  for (const key of BODY_KEYS) {
    const term = document.createElement("dt");
    term.textContent = key;
    const value = document.createElement("dd");
    value.textContent = formatValue(state.editBody[key]);
    list.append(term, value);
  }
}

function sliderSpec(key) {
  return EDIT_SLIDERS.find((spec) => spec.key === key);
}

function sliderReadout(spec, value) {
  if (!spec) return formatValue(value);
  if (spec.z) return zReadout(value, spec.less, spec.more);
  // The body contract stores centimetres only. It has no mean or spread
  // by age and sex, so the slider cannot give a height percentile.
  if (spec.key === "height_cm") return `${Math.round(value)} cm`;
  if (spec.key === "body_form_age") return `${Math.round(value)} Jahre`;
  if (spec.key === "breast_development") {
    if (value < 0.05) return "flach";
    return `${Math.round(value * 100)} %`;
  }
  if (spec.key === "balding_stage") return `Stufe ${formatValue(value)}`;
  return formatValue(value);
}

function paintGeneticHair() {
  const chip = document.querySelector("#genetic-hair");
  if (chip && state.editBody) chip.style.background = hairColor(state.editBody);
}

function syncEditSliders() {
  const sex = state.family.members[state.editIndex].sex;
  for (const input of document.querySelectorAll("#edit-sliders input[data-key]")) {
    const key = input.dataset.key;
    const spec = sliderSpec(key);
    input.value = String(state.editBody[key]);
    const out = input.parentElement.querySelector("output");
    if (out) out.textContent = sliderReadout(spec, state.editBody[key]);
    if (key === "breast_development") {
      const on = breastSliderOn(state.editBody, sex);
      input.disabled = !on;
      input.closest("label").classList.toggle("is-off", !on);
    }
  }
  const hair = document.querySelector("#edit-hair");
  if (hair) hair.value = state.hairStyle;
  const dyeOn = document.querySelector("#edit-dye");
  const dyeColor = document.querySelector("#edit-dye-color");
  if (dyeOn) dyeOn.checked = Boolean(state.hairDye);
  if (dyeColor) {
    dyeColor.disabled = !state.hairDye;
    if (state.hairDye) dyeColor.value = state.hairDye;
  }
  paintGeneticHair();
}

function drawEdit() {
  if (editDraw !== null) return;
  editDraw = requestAnimationFrame(() => {
    editDraw = null;
    if (!state.template) return;
    restyle();
    frame();
  });
}

function buildHairControls(root) {
  const heading = document.createElement("h3");
  heading.textContent = "Haar";
  const note = document.createElement("p");
  note.className = "hair-note";
  note.textContent = "Frisur und Färbung ändern nur die Ansicht. Die genetische Farbe bleibt daneben.";
  root.append(heading, note);

  const row = document.createElement("label");
  row.className = "hair-line";
  const name = document.createElement("span");
  name.textContent = "Frisur";
  const select = document.createElement("select");
  select.id = "edit-hair";
  const auto = document.createElement("option");
  auto.value = "";
  auto.textContent = "aus der Person";
  select.append(auto);
  for (const style of HAIR_STYLES) {
    const option = document.createElement("option");
    option.value = style.id;
    option.textContent = style.label;
    select.append(option);
  }
  select.value = state.hairStyle;
  select.addEventListener("change", () => {
    state.hairStyle = select.value;
    drawEdit();
  });
  row.append(name, select);
  root.append(row);

  const gene = document.createElement("div");
  gene.className = "hair-line";
  const geneName = document.createElement("span");
  geneName.textContent = "genetisch";
  const chip = document.createElement("i");
  chip.id = "genetic-hair";
  chip.style.background = hairColor(state.editBody);
  gene.append(geneName, chip);
  root.append(gene);

  const dye = document.createElement("label");
  dye.className = "hair-line hair-dye";
  const dyeName = document.createElement("span");
  dyeName.textContent = "gefärbt";
  const toggle = document.createElement("input");
  toggle.type = "checkbox";
  toggle.id = "edit-dye";
  toggle.checked = Boolean(state.hairDye);
  const color = document.createElement("input");
  color.type = "color";
  color.id = "edit-dye-color";
  color.value = state.hairDye || "#7f6d5b";
  color.disabled = !state.hairDye;
  toggle.addEventListener("change", () => {
    state.hairDye = toggle.checked ? color.value : "";
    color.disabled = !toggle.checked;
    drawEdit();
  });
  color.addEventListener("input", () => {
    if (!toggle.checked) return;
    state.hairDye = color.value;
    drawEdit();
  });
  dye.append(dyeName, toggle, color);
  root.append(dye);
}

function buildEditSliders() {
  const root = document.querySelector("#edit-sliders");
  root.replaceChildren();
  buildHairControls(root);
  const sex = state.family.members[state.editIndex].sex;
  let group = "";
  for (const spec of EDIT_SLIDERS) {
    if (spec.group && spec.group !== group) {
      group = spec.group;
      const heading = document.createElement("h3");
      heading.textContent = group;
      root.append(heading);
    }
    const label = document.createElement("label");
    label.className = "slider";
    const name = document.createElement("span");
    name.textContent = spec.label;
    const out = document.createElement("output");
    out.textContent = sliderReadout(spec, state.editBody[spec.key]);
    const input = document.createElement("input");
    input.type = "range";
    input.min = String(spec.min);
    input.max = String(spec.max);
    input.step = String(spec.step);
    input.dataset.key = spec.key;
    input.value = String(state.editBody[spec.key]);
    if (spec.breast) {
      const on = breastSliderOn(state.editBody, sex);
      input.disabled = !on;
      input.title = "Weiblicher Zweig, nach der Pubertät";
      if (!on) label.classList.add("is-off");
    }
    input.addEventListener("input", () => {
      const value = Number(input.value);
      state.editBody[spec.key] = value;
      if (spec.age) state.editBody.face_form_age = value;
      out.textContent = sliderReadout(spec, value);
      if (spec.hair) paintGeneticHair();
      if (spec.age) {
        const breast = root.querySelector('input[data-key="breast_development"]');
        if (breast) {
          const on = breastSliderOn(state.editBody, sex);
          breast.disabled = !on;
          breast.closest("label").classList.toggle("is-off", !on);
        }
      }
      writeContract();
      drawEdit();
    });
    label.append(name, out, input);
    if (spec.z) {
      const tick = document.createElement("span");
      tick.className = "tick";
      const mark = document.createElement("i");
      const word = document.createElement("b");
      word.textContent = "Durchschnitt";
      tick.append(mark, word);
      label.append(tick);
    }
    root.append(label);
  }
}

function loadEditPerson(index) {
  state.editIndex = index;
  const member = state.family.members[index];
  state.editBase = { ...member.body };
  state.editBody = { ...member.body };
  state.hairStyle = "";
  state.hairDye = "";
  state.provisional = emptyProvisional();
  state.directRegion = "";
  state.hoverRegion = "";
  state.past = [];
  state.future = [];
  const quick = document.querySelector("#quick-person");
  if (quick) quick.value = String(index);
  if (state.editMode === 2) {
    syncDirect();
    return;
  }
  buildEditSliders();
  writeContract();
}

function defaultAdultIndex() {
  const members = state.family.members;
  let best = -1;
  members.forEach((member, index) => {
    if (!(member.age_years >= 18)) return;
    if (best < 0) {
      best = index;
      return;
    }
    const current = members[best];
    const preferFemale = member.sex === "female" && current.sex !== "female";
    const youngerSame = member.sex === current.sex && member.age_years < current.age_years;
    if (preferFemale || youngerSame) best = index;
  });
  return best >= 0 ? best : 0;
}

function setupEditor() {
  if (!state.edit) return;
  document.body.classList.add("editing");
  if (state.editMode === 2) {
    document.body.classList.add("editing-direct");
    const note = document.querySelector(".editor-note");
    if (note) note.textContent = "Vorschau. Nichts wird zurückgeschrieben.";
    document.querySelector("#edit-direct").hidden = false;
  }
  const aside = document.querySelector("#editor");
  aside.hidden = false;
  const select = document.querySelector("#edit-person");
  const quick = document.querySelector("#quick-person");
  state.family.members.forEach((member, index) => {
    const option = document.createElement("option");
    option.value = String(index);
    option.textContent = `${member.first_name} ${member.age_years}`;
    select.append(option);
    if (quick) quick.append(option.cloneNode(true));
  });
  if (quick) {
    quick.addEventListener("change", () => {
      select.value = quick.value;
      select.dispatchEvent(new Event("change"));
    });
  }
  select.addEventListener("change", () => {
    loadEditPerson(Number(select.value));
    restyle();
    frame();
  });
  document.querySelector("#edit-reset").addEventListener("click", () => {
    if (state.editMode === 2) beginChange();
    state.editBody = { ...state.editBase };
    state.hairStyle = "";
    state.hairDye = "";
    state.provisional = emptyProvisional();
    state.directRegion = "";
    state.hoverRegion = "";
    if (state.editMode === 2) {
      commitChange();
      syncDirect();
    } else {
      syncEditSliders();
      writeContract();
    }
    restyle();
    frame();
  });
  let start = 0;
  if (state.focus) {
    const name = state.focus.toLowerCase();
    const found = state.family.members.findIndex((member) => (
      (member.first_name || "").toLowerCase() === name
    ));
    if (found >= 0) start = found;
  } else if (state.editMode === 2) {
    start = defaultAdultIndex();
  }
  select.value = String(start);
  loadEditPerson(start);
  if (state.editMode === 2) bindDirectEditor();
}

const PROVISIONAL_KEYS = [
  "neck_length_z", "hand_size_z", "foot_size_z", "ear_size_z", "ear_projection_z",
  "eye_roundness_z", "forehead_height_z", "buttock_volume_z", "buttock_lift_z",
  "waist_width_z", "breast_volume_z", "breast_height_z", "nose_bridge_z",
  "nose_tip_z", "brow_arch_z", "lip_ratio_z",
];

function emptyProvisional() {
  const out = {};
  for (const key of PROVISIONAL_KEYS) out[key] = 0;
  return out;
}

function axisOf(key, store, less, more) {
  return { key, store, less, more };
}

// Front view edits widths. A side view edits front-back projection.
// There is no on-screen handle: the cursor shows the allowed directions.
const hipWidth = axisOf("hip_width_z", "body", "schmaler", "breiter");
const shoulderWidth = axisOf("shoulder_width_z", "body", "schmaler", "breiter");
const waistWidth = axisOf("waist_width_z", "provisional", "schmaler", "breiter");
const buttockVolume = axisOf("buttock_volume_z", "provisional", "flacher", "voller");
const buttockLift = axisOf("buttock_lift_z", "provisional", "tiefer", "höher");
const breastVolume = axisOf("breast_volume_z", "provisional", "flacher", "voller");
const breastHeight = axisOf("breast_height_z", "provisional", "tiefer", "höher");
const breastProjection = axisOf("breast_volume_z", "provisional", "flacher", "vorspringender");
const bellySpread = axisOf("fat_distribution_z", "body", "mehr an der Hüfte", "mehr am Bauch");
const bellyDepth = axisOf("fat_distribution_z", "body", "flacher", "vortretender");
const DIRECT_REGIONS = {
  hips: { label: "Hüfte", band: "body", radius: 0.1, frontX: hipWidth },
  thighs: { label: "Oberschenkel", band: "body", radius: 0.11, frontX: hipWidth },
  buttocks: { label: "Gesäß", band: "body", radius: 0.12, sideX: buttockVolume, sideY: buttockLift, backX: buttockVolume, backY: buttockLift },
  waist: { label: "Taille", band: "body", radius: 0.09, frontX: waistWidth },
  chest: { label: "Brust", band: "body", radius: 0.12, breast: true, frontX: breastVolume, frontY: breastHeight, sideX: breastProjection, sideY: breastHeight },
  shoulders: { label: "Schultern", band: "body", radius: 0.14, frontX: shoulderWidth },
  belly: { label: "Bauch", band: "body", radius: 0.1, frontX: bellySpread, sideX: bellyDepth },
  cheeks: {
    label: "Wangen", band: "face", radius: 0.05,
    frontX: axisOf("face_axis_07", "body", "flacher", "voller"),
    frontY: axisOf("face_axis_08", "body", "weicher", "markanter"),
    sideX: axisOf("face_axis_07", "body", "flacher", "voller"),
  },
  nose_bridge: {
    label: "Nasenrücken", band: "face", radius: 0.024,
    frontX: axisOf("face_axis_01", "body", "schmaler", "breiter"),
    frontY: axisOf("nose_bridge_z", "provisional", "flacher", "höher"),
    sideX: axisOf("face_axis_02", "body", "flacher", "vorspringender"),
    sideY: axisOf("nose_bridge_z", "provisional", "flacher", "höher"),
  },
  nose_tip: {
    label: "Nasenspitze", band: "face", radius: 0.03,
    frontY: axisOf("nose_tip_z", "provisional", "tiefer", "höher"),
    sideX: axisOf("face_axis_02", "body", "flacher", "vorspringender"),
    sideY: axisOf("nose_tip_z", "provisional", "tiefer", "höher"),
  },
  chin: {
    label: "Kinn", band: "face", radius: 0.045,
    frontX: axisOf("face_axis_04", "body", "schmaler", "breiter"),
    frontY: axisOf("face_axis_05", "body", "niedriger", "höher"),
    sideX: axisOf("face_axis_06", "body", "flacher", "vorspringender"),
    sideY: axisOf("face_axis_05", "body", "niedriger", "höher"),
  },
  eyes: {
    label: "Augen", band: "face", radius: 0.04,
    frontX: axisOf("face_axis_09", "body", "kleiner", "größer"),
    frontY: axisOf("eye_roundness_z", "provisional", "schmaler", "runder"),
    sideX: axisOf("face_axis_09", "body", "kleiner", "größer"),
    sideY: axisOf("eye_roundness_z", "provisional", "schmaler", "runder"),
  },
  brow: {
    label: "Brauen", band: "face", radius: 0.04,
    frontY: axisOf("brow_arch_z", "provisional", "gerader", "geschwungener"),
    sideY: axisOf("brow_arch_z", "provisional", "gerader", "geschwungener"),
  },
  lips: {
    label: "Lippen", band: "face", radius: 0.035,
    frontX: axisOf("face_axis_11", "body", "schmaler", "voller"),
    frontY: axisOf("lip_ratio_z", "provisional", "mehr Unterlippe", "mehr Oberlippe"),
    sideX: axisOf("face_axis_11", "body", "schmaler", "voller"),
    sideY: axisOf("lip_ratio_z", "provisional", "mehr Unterlippe", "mehr Oberlippe"),
  },
  ears: {
    label: "Ohren", band: "face", radius: 0.04,
    frontX: axisOf("ear_size_z", "provisional", "kleiner", "größer"),
    sideX: axisOf("ear_projection_z", "provisional", "anliegender", "abstehender"),
    sideY: axisOf("ear_size_z", "provisional", "kleiner", "größer"),
  },
  neck: {
    label: "Hals", band: "body", radius: 0.06,
    frontY: axisOf("neck_length_z", "provisional", "kürzer", "länger"),
    sideY: axisOf("neck_length_z", "provisional", "kürzer", "länger"),
  },
  hands: {
    label: "Hände", band: "body", radius: 0.07,
    frontX: axisOf("hand_size_z", "provisional", "kleiner", "größer"),
    sideX: axisOf("hand_size_z", "provisional", "kleiner", "größer"),
  },
  feet: {
    label: "Füße", band: "body", radius: 0.07,
    frontX: axisOf("foot_size_z", "provisional", "kleiner", "größer"),
    sideX: axisOf("foot_size_z", "provisional", "kleiner", "größer"),
  },
  forehead: {
    label: "Stirn", band: "face", radius: 0.05,
    frontY: axisOf("forehead_height_z", "provisional", "niedriger", "höher"),
    sideY: axisOf("forehead_height_z", "provisional", "niedriger", "höher"),
  },
};

function quantizeZ(value) {
  return Math.round(clamp(value, -3, 3) * 10) / 10;
}

function zPhrase(z, less, more) {
  const mag = Math.abs(z);
  const pct = clamp(Math.round(normalCdf(z) * 100), 1, 99);
  if (mag < 0.5) return { degree: "durchschnittlich", rank: "" };
  const word = z > 0 ? more : less;
  const tail = z > 0 ? pct : 100 - pct;
  let degree = "etwas";
  if (mag >= 2) degree = "sehr";
  else if (mag >= 1) degree = "deutlich";
  return { degree: `${degree} ${word}`, rank: `${word} als ${tail} %` };
}

function editSex() {
  const member = state.family && state.family.members[state.editIndex];
  return member ? member.sex : "female";
}

function breastDragOn(body, sex) {
  return Boolean(body) && sex === "female" && body.body_form_age > 12 && body.breast_development >= 0.12;
}

function regionSpec(id) {
  const spec = DIRECT_REGIONS[id];
  if (!spec) return null;
  if (!spec.breast || breastDragOn(state.editBody, editSex())) return spec;
  return {
    label: "Brustkorb",
    band: "body",
    radius: spec.radius,
    sideX: axisOf("chest_depth_z", "body", "flacher", "tiefer"),
    blocked: "Brust erst im weiblichen Zweig, nach der Pubertät.",
  };
}

function facing() {
  const yaw = state.editYaw || 0;
  const s = Math.sin(yaw);
  const c = Math.cos(yaw);
  if (Math.abs(s) >= Math.abs(c)) return "side";
  return c >= 0 ? "front" : "back";
}

function axesFor(id) {
  const spec = regionSpec(id);
  if (!spec) return { x: null, y: null };
  const face = facing();
  if (face === "side") return { x: spec.sideX || null, y: spec.sideY || null };
  if (face === "back") return { x: spec.backX || spec.frontX || null, y: spec.backY || spec.frontY || null };
  return { x: spec.frontX || null, y: spec.frontY || null };
}

function cursorDir(id) {
  const axes = axesFor(id);
  if (axes.x && axes.y) return "move";
  if (axes.x) return "ew";
  if (axes.y) return "ns";
  return "";
}

function cssCursor(id) {
  const dir = cursorDir(id);
  if (dir === "move") return "move";
  if (dir === "ew") return "ew-resize";
  if (dir === "ns") return "ns-resize";
  return "default";
}

function regionHint(id) {
  const spec = regionSpec(id);
  if (!spec) return "Waagerecht zieht die Breite. Leere Fläche dreht die Figur.";
  const axes = axesFor(id);
  if (spec.blocked && !axes.x && !axes.y) return spec.blocked;
  if (!axes.x && !axes.y) {
    return facing() === "front"
      ? "Diese Stelle von der Seite ziehen."
      : "Diese Stelle von vorn ziehen.";
  }
  const bits = [];
  if (axes.x) bits.push(`Waagerecht: ${axes.x.less} oder ${axes.x.more}`);
  if (axes.y) bits.push(`senkrecht: ${axes.y.less} oder ${axes.y.more}`);
  return `${bits.join(", ")}.`;
}

function axisBag(axis) {
  return axis.store === "provisional" ? state.provisional : state.editBody;
}

function axisValue(axis) {
  if (!axis) return 0;
  const bag = axisBag(axis);
  return bag && Number.isFinite(bag[axis.key]) ? bag[axis.key] : 0;
}

function setAxis(axis, value) {
  if (!axis) return false;
  const bag = axisBag(axis);
  if (!bag) return false;
  const next = quantizeZ(value);
  if (bag[axis.key] === next) return false;
  bag[axis.key] = next;
  return true;
}

function syncDirect() {
  if (state.editMode !== 2 || !state.editBody) return;
  const id = state.hoverRegion || state.directRegion;
  const spec = regionSpec(id);
  const name = document.querySelector("#region-name");
  const degree = document.querySelector("#region-degree");
  const rank = document.querySelector("#region-rank");
  const note = document.querySelector("#region-note");
  if (!spec) {
    name.textContent = "Keine Stelle";
    degree.textContent = "Eine Stelle an der Figur wählen.";
    rank.textContent = "";
    note.textContent = "Waagerecht zieht die Breite. Leere Fläche dreht die Figur.";
  } else {
    const axes = axesFor(id);
    const parts = [axes.x, axes.y].filter(Boolean).map((axis) => ({
      axis,
      value: axisValue(axis),
      phrase: zPhrase(axisValue(axis), axis.less, axis.more),
    }));
    name.textContent = spec.label;
    const spoken = parts.filter((part) => part.phrase.degree !== "durchschnittlich");
    const shown = spoken.length ? spoken : parts;
    degree.textContent = shown.length
      ? shown.map((part) => part.phrase.degree).join(" · ")
      : "in dieser Ansicht ohne Zug";
    rank.textContent = shown.map((part) => part.phrase.rank).filter(Boolean).join(" · ");
    note.textContent = regionHint(id);
    document.querySelector("#direct-readout").title = parts
      .map((part) => `${part.axis.key} ${part.value.toFixed(1)}`)
      .join(", ");
  }
  const fat = zPhrase(state.editBody.body_fat_z, "dünner", "schwerer");
  const muscle = zPhrase(state.editBody.muscle_z, "schwächer", "muskulöser");
  const fatLine = fat.rank || fat.degree;
  const muscleLine = muscle.rank || muscle.degree;
  const pad = document.querySelector("#pad-readout");
  if (pad) pad.textContent = `Fett ${fatLine}. Muskeln ${muscleLine}.`;
  const handle = document.querySelector("#shape-handle");
  const u = (clamp(state.editBody.body_fat_z, -3, 3) + 3) / 6;
  const v = (clamp(state.editBody.muscle_z, -3, 3) + 3) / 6;
  if (handle) {
    handle.style.left = `${u * 100}%`;
    handle.style.bottom = `${v * 100}%`;
  }
  const dist = document.querySelector("#edit-dist");
  if (dist) dist.value = String(state.editBody.fat_distribution_z);
  const distPhrase = zPhrase(state.editBody.fat_distribution_z, "mehr an der Hüfte", "mehr am Bauch");
  const distRead = document.querySelector("#dist-readout");
  if (distRead) distRead.textContent = distPhrase.rank || distPhrase.degree;
  const fatSlider = document.querySelector("#fat-slider");
  const muscleSlider = document.querySelector("#muscle-slider");
  if (fatSlider) fatSlider.value = String(state.editBody.body_fat_z);
  if (muscleSlider) muscleSlider.value = String(state.editBody.muscle_z);
  placeMarker("#skin-marker", state.editBody.skin_pheomelanin, state.editBody.skin_eumelanin);
  placeMarker("#hair-marker", state.editBody.hair_pheomelanin, state.editBody.hair_eumelanin);
  placeMarker("#iris-marker", state.editBody.iris_melanin, 0.5);
  const skinRead = document.querySelector("#skin-readout");
  if (skinRead) {
    skinRead.textContent = `Eumelanin ${state.editBody.skin_eumelanin.toFixed(2)}, Phäomelanin ${state.editBody.skin_pheomelanin.toFixed(2)}`;
  }
  const hairRead = document.querySelector("#hair-readout");
  if (hairRead) {
    const grey = state.editBody.hair_grey_fraction;
    hairRead.textContent = grey > 0.05
      ? `Eumelanin ${state.editBody.hair_eumelanin.toFixed(2)}, Phäomelanin ${state.editBody.hair_pheomelanin.toFixed(2)}. Grau mischt sich darüber.`
      : `Eumelanin ${state.editBody.hair_eumelanin.toFixed(2)}, Phäomelanin ${state.editBody.hair_pheomelanin.toFixed(2)}`;
  }
  const hair = document.querySelector("#direct-hair");
  if (hair && hair.value !== state.hairStyle) hair.value = state.hairStyle;
  const dye = document.querySelector("#direct-dye");
  const dyeColor = document.querySelector("#direct-dye-color");
  if (dye) dye.checked = Boolean(state.hairDye);
  if (dyeColor) {
    dyeColor.disabled = !state.hairDye;
    if (state.hairDye) dyeColor.value = state.hairDye;
  }
  const cloth = document.querySelector("#cloth-line");
  if (cloth && state.family) {
    const outfit = outfitOf(state.family.members[state.editIndex]);
    cloth.textContent = `${CLOTH_NAMES[outfit.suit] || outfit.suit}, ${CLOTH_NAMES[outfit.shoes] || outfit.shoes}`;
  }
  syncHistory();
  document.body.dataset.region = state.directRegion || "";
  document.body.dataset.hover = state.hoverRegion || "";
  document.body.dataset.facing = facing();
}

const _skinM = new THREE.Matrix4();
const _skinBase = new THREE.Vector3();
const _skinTmp = new THREE.Vector3();
const _skinOut = new THREE.Vector3();
const _skinAcc = new THREE.Vector3();
const directRay = new THREE.Raycaster();

function appendPosed(mesh, positions, indices) {
  const geom = mesh.geometry;
  const pos = geom.attributes.position;
  const index = geom.index;
  const skinIndex = geom.attributes.skinIndex;
  const skinWeight = geom.attributes.skinWeight;
  if (!pos || !index || !skinIndex || !mesh.skeleton) return;
  mesh.skeleton.update();
  const morphs = geom.morphAttributes.position || [];
  const influences = mesh.morphTargetInfluences;
  const base = positions.length / 3;
  for (let i = 0; i < pos.count; i += 1) {
    _skinOut.fromBufferAttribute(pos, i);
    if (influences) {
      for (let m = 0; m < influences.length; m += 1) {
        const weight = influences[m];
        const morph = morphs[m];
        if (!weight || !morph) continue;
        _skinOut.x += weight * morph.getX(i);
        _skinOut.y += weight * morph.getY(i);
        _skinOut.z += weight * morph.getZ(i);
      }
    }
    _skinBase.copy(_skinOut).applyMatrix4(mesh.bindMatrix);
    _skinAcc.set(0, 0, 0);
    for (let k = 0; k < 4; k += 1) {
      const bone = skinIndex.getComponent(i, k);
      const weight = skinWeight.getComponent(i, k);
      if (!weight) continue;
      _skinM.fromArray(mesh.skeleton.boneMatrices, bone * 16);
      _skinTmp.copy(_skinBase).applyMatrix4(_skinM);
      _skinAcc.addScaledVector(_skinTmp, weight);
    }
    _skinAcc.applyMatrix4(mesh.bindMatrixInverse).applyMatrix4(mesh.matrixWorld);
    positions.push(_skinAcc.x, _skinAcc.y, _skinAcc.z);
  }
  for (let i = 0; i < index.count; i += 1) indices.push(index.getX(i) + base);
}

function rebuildPick(person) {
  person.wrapper.updateWorldMatrix(true, true);
  const positions = [];
  const indices = [];
  appendPosed(person.mesh, positions, indices);
  for (const cloth of person.clothes || []) appendPosed(cloth, positions, indices);
  if (positions.length < 9) return;
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setIndex(indices);
  geometry.computeBoundingSphere();
  if (!state.directPick) {
    state.directPick = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial());
    state.directPick.frustumCulled = false;
    state.directPick.matrixAutoUpdate = false;
  } else {
    state.directPick.geometry.dispose();
    state.directPick.geometry = geometry;
  }
  state.directPick.matrix.identity();
  state.directPick.updateMatrixWorld(true);
}

function boneLocal(person, name) {
  const bone = findBone(person.wrapper, name);
  const point = new THREE.Vector3();
  if (!bone) return point;
  bone.getWorldPosition(point);
  return person.wrapper.worldToLocal(point);
}

function markLocal(person, tuple) {
  return person.wrapper.worldToLocal(landmarkWorld(person, tuple));
}

function classifyRegion(person, worldPoint) {
  const p = person.wrapper.worldToLocal(worldPoint.clone());
  if (nearBone(p, person, "hand_l", 0.1) || nearBone(p, person, "hand_r", 0.1)) return "hands";
  if (nearBone(p, person, "foot_l", 0.11) || nearBone(p, person, "foot_r", 0.11)) return "feet";
  const head = boneLocal(person, "head");
  const neck = boneLocal(person, "neck_01");
  const pelvis = boneLocal(person, "pelvis");
  const spine2 = boneLocal(person, "spine_02");
  const spine3 = boneLocal(person, "spine_03");
  const crown = markLocal(person, person.marks.crown);
  const chin = markLocal(person, person.marks.chin);
  const jaw = person.marks.jaw;
  const nose = markLocal(person, [0, person.marks.maxY + jaw.noseDy, jaw.noseZ]);
  const side = Math.abs(p.x - head.x);
  const span = Math.max(crown.y - chin.y, 0.08);
  const t = (p.y - chin.y) / span;
  if (p.y >= chin.y - 0.012 && p.y <= crown.y + 0.02 && side < 0.11) {
    if (side > 0.055 && t > 0.18 && t < 0.78) return "ears";
    if (t > 0.82) return "forehead";
    if (t > 0.66 && t <= 0.84 && side < 0.055) return "brow";
    if (t > 0.48 && t <= 0.7 && side < 0.02) return p.y >= nose.y ? "nose_bridge" : "nose_tip";
    if (t > 0.52 && t <= 0.74 && side < 0.055) return "eyes";
    if (t > 0.3 && t < 0.58 && side < 0.022) return p.y >= nose.y ? "nose_bridge" : "nose_tip";
    if (t > 0.12 && t <= 0.34 && side < 0.032) return "lips";
    if (t <= 0.18) return "chin";
    return "cheeks";
  }
  if (p.y < chin.y - 0.008 && p.y > spine3.y - 0.01 && Math.abs(p.x) < 0.05 && Math.abs(p.z - neck.z) < 0.07) {
    return "neck";
  }
  if (p.y > spine3.y - 0.04 && p.y < chin.y && Math.abs(p.x) > 0.08 && Math.abs(p.x) < 0.26) return "shoulders";
  // The hanging arm is not the waist. A missing bone must not count.
  if (nearBone(p, person, "upperarm_l", 0.09) || nearBone(p, person, "upperarm_r", 0.09)) return "";
  if (nearBone(p, person, "lowerarm_l", 0.075) || nearBone(p, person, "lowerarm_r", 0.075)) return "";
  const front = p.z > pelvis.z + 0.02;
  const back = p.z < pelvis.z - 0.025;
  const sideView = facing() === "side";
  const waistY = pelvis.y + (spine2.y - pelvis.y) * 0.62;
  if (p.y > spine2.y - 0.02 && p.y < spine3.y + 0.02 && Math.abs(p.x) < 0.12 && (front || (sideView && p.z > pelvis.z - 0.01))) {
    return "chest";
  }
  if (Math.abs(p.y - waistY) < 0.045 && Math.abs(p.x) < 0.11 && Math.abs(p.z - pelvis.z) < 0.11) return "waist";
  if (p.y < waistY - 0.03 && p.y > pelvis.y - 0.02 && Math.abs(p.x) < 0.09 && (front || (sideView && p.z > pelvis.z))) {
    return "belly";
  }
  if ((back || (sideView && p.z < pelvis.z - 0.01)) && p.y > pelvis.y - 0.1 && p.y < pelvis.y + 0.045 && Math.abs(p.x) < 0.16) {
    return "buttocks";
  }
  // The pelvis bone sits at the waistband. The hip is the skirt flare below the hands.
  const hipY = pelvis.y - 0.12;
  if (Math.abs(p.y - hipY) < 0.05 && Math.abs(p.x) > 0.035 && Math.abs(p.x) < 0.16 && !back) return "hips";
  const kneeY = pelvis.y * 0.55;
  if (p.y < hipY - 0.05 && p.y > kneeY && Math.abs(p.x) > 0.04 && Math.abs(p.x) < 0.18) return "thighs";
  return "";
}

function nearBone(point, person, name, radius) {
  const bone = findBone(person.wrapper, name);
  if (!bone) return false;
  const at = new THREE.Vector3();
  bone.getWorldPosition(at);
  return point.distanceTo(person.wrapper.worldToLocal(at)) < radius;
}

function regionLocal(person, id) {
  const pelvis = boneLocal(person, "pelvis");
  const spine2 = boneLocal(person, "spine_02");
  const spine3 = boneLocal(person, "spine_03");
  const neck = boneLocal(person, "neck_01");
  const side = state.directSide < 0 ? -1 : 1;
  if (id === "hands") return boneLocal(person, side < 0 ? "hand_r" : "hand_l");
  if (id === "feet") {
    const foot = boneLocal(person, side < 0 ? "foot_r" : "foot_l");
    return foot.clone().setY(foot.y - 0.02);
  }
  if (id === "neck") return neck;
  if (id === "shoulders") return new THREE.Vector3(side * 0.12, spine3.y + 0.01, spine3.z);
  if (id === "chest") return new THREE.Vector3(0, (spine2.y + spine3.y) * 0.5, spine3.z + 0.05);
  if (id === "belly") return new THREE.Vector3(0, (pelvis.y + spine2.y) * 0.48, pelvis.z + 0.06);
  if (id === "waist") return new THREE.Vector3(0, pelvis.y + (spine2.y - pelvis.y) * 0.62, pelvis.z);
  if (id === "hips") return new THREE.Vector3(side * 0.06, pelvis.y - 0.12, pelvis.z + 0.03);
  if (id === "thighs") return new THREE.Vector3(side * 0.09, pelvis.y - 0.16, pelvis.z);
  if (id === "buttocks") return new THREE.Vector3(0, pelvis.y - 0.02, pelvis.z - 0.07);
  const jaw = person.marks.jaw;
  const point = {
    forehead: person.marks.crown,
    brow: [0, person.marks.maxY + jaw.noseDy + 0.035, jaw.noseZ - 0.01],
    eyes: [side * 0.03, person.marks.maxY + jaw.noseDy + 0.02, jaw.noseZ],
    nose_bridge: [0, person.marks.maxY + jaw.noseDy + 0.004, jaw.noseZ],
    nose_tip: [0, person.marks.maxY + jaw.noseDy, jaw.noseZ],
    cheeks: [side * 0.045, person.marks.maxY + jaw.noseDy - 0.005, jaw.noseZ - 0.02],
    lips: [0, person.marks.maxY + jaw.lipLo, jaw.noseZ],
    chin: person.marks.chin,
    ears: [side * 0.075, person.marks.maxY + jaw.noseDy + 0.01, 0.02],
  }[id];
  if (!point) return boneLocal(person, "head");
  return markLocal(person, point);
}

function ensurePick(person) {
  if (!state.pickDirty && state.directPick && state.pickFor === person) return;
  rebuildPick(person);
  state.pickFor = person;
  state.pickDirty = false;
}

function projectPoint(world) {
  const screen = world.clone().project(camera);
  const view = document.querySelector("#view");
  return {
    x: (screen.x * 0.5 + 0.5) * view.clientWidth,
    y: (-screen.y * 0.5 + 0.5) * view.clientHeight,
    behind: screen.z > 1,
  };
}

function placeDirectChrome() {
  const person = state.people[0];
  const glow = document.querySelector("#region-glow");
  const cursor = document.querySelector("#hover-cursor");
  if (!glow || !cursor) return;
  const showId = state.hoverRegion || state.directRegion;
  const spec = person && regionSpec(showId);
  if (!person || !spec) {
    glow.hidden = true;
    cursor.hidden = true;
    return;
  }
  person.wrapper.updateWorldMatrix(true, true);
  const world = person.wrapper.localToWorld(regionLocal(person, showId).clone());
  const center = projectPoint(world);
  const radius = (spec.radius || 0.1) * (person.scale || 1);
  const edge = projectPoint(world.clone().setX(world.x + radius));
  const size = Math.max(22, Math.abs(edge.x - center.x) * 2);
  glow.hidden = center.behind;
  glow.style.left = `${center.x}px`;
  glow.style.top = `${center.y}px`;
  glow.style.width = `${size}px`;
  glow.style.height = `${size * 0.7}px`;
  const dir = cursorDir(showId);
  const showCursor = Boolean(dir) && (Boolean(state.hoverRegion) || state.cursorPinned);
  cursor.hidden = !showCursor;
  if (!showCursor) return;
  cursor.dataset.dir = dir;
  const at = projectPoint(world);
  cursor.style.left = `${at.x}px`;
  cursor.style.top = `${at.y}px`;
}

function pickHit(event) {
  const person = state.people[0];
  if (!person) return null;
  ensurePick(person);
  const rect = canvas.getBoundingClientRect();
  const x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
  const y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
  directRay.setFromCamera(new THREE.Vector2(x, y), camera);
  const hits = state.directPick ? directRay.intersectObject(state.directPick, false) : [];
  const extras = [];
  person.wrapper.traverse((node) => {
    if (node.isMesh && !node.isSkinnedMesh) extras.push(node);
  });
  hits.push(...directRay.intersectObjects(extras, false));
  hits.sort((a, b) => a.distance - b.distance);
  return hits[0] || null;
}

const DRAG_SLOP = 5;
let directDrag = null;
let changeBase = null;

function snapshot() {
  return {
    body: { ...state.editBody },
    provisional: { ...state.provisional },
    hairStyle: state.hairStyle,
    hairDye: state.hairDye,
    region: state.directRegion,
  };
}

function beginChange() {
  if (changeBase || !state.editBody) return;
  changeBase = snapshot();
}

function commitChange() {
  if (!changeBase) return;
  const next = snapshot();
  if (JSON.stringify(next) !== JSON.stringify(changeBase)) {
    state.past.push(changeBase);
    state.future = [];
  }
  changeBase = null;
  syncHistory();
}

function restoreShot(shot) {
  state.editBody = { ...shot.body };
  state.provisional = { ...shot.provisional };
  state.hairStyle = shot.hairStyle;
  state.hairDye = shot.hairDye;
  state.directRegion = shot.region;
  state.hoverRegion = "";
  syncDirect();
  drawEdit();
  frame();
}

function undoEdit() {
  if (!state.past.length || changeBase) return;
  state.future.push(snapshot());
  restoreShot(state.past.pop());
}

function redoEdit() {
  if (!state.future.length || changeBase) return;
  state.past.push(snapshot());
  restoreShot(state.future.pop());
}

function syncHistory() {
  const undo = document.querySelector("#edit-undo");
  const redo = document.querySelector("#edit-redo");
  if (undo) undo.disabled = state.past.length === 0;
  if (redo) redo.disabled = state.future.length === 0;
}

function turnBy(delta) {
  state.editYaw += delta;
  const person = state.people[0];
  if (person) person.wrapper.rotation.y = state.editYaw;
  syncDirect();
  if (regionSpec(state.directRegion) && regionSpec(state.directRegion).band === "face") frame();
}

function applyDirectDrag(dx, dy) {
  if (directDrag.mode === "orbit") {
    state.editYaw = directDrag.yaw + dx * 0.01;
    const person = state.people[0];
    if (person) person.wrapper.rotation.y = state.editYaw;
    if (regionSpec(state.directRegion) && regionSpec(state.directRegion).band === "face") frame();
    syncDirect();
    return;
  }
  const axes = axesFor(state.directRegion);
  let changed = false;
  if (axes.x) changed = setAxis(axes.x, directDrag.originX + dx / 72) || changed;
  if (axes.y) changed = setAxis(axes.y, directDrag.originY - dy / 72) || changed;
  if (state.directRegion === "belly" && facing() === "side") {
    const nextFat = quantizeZ(directDrag.fat + (dx / 72) * 0.5);
    if (state.editBody.body_fat_z !== nextFat) {
      state.editBody.body_fat_z = nextFat;
      changed = true;
    }
  }
  if (!changed) return;
  syncDirect();
  drawEdit();
}

function updateHover(event) {
  const hit = pickHit(event);
  const person = state.people[0];
  let next = "";
  if (hit && person) {
    state.directSide = person.wrapper.worldToLocal(hit.point.clone()).x;
    next = classifyRegion(person, hit.point);
  }
  state.cursorPinned = false;
  const changed = next !== state.hoverRegion;
  state.hoverRegion = next;
  canvas.style.cursor = next ? cssCursor(next) : "grab";
  if (changed) syncDirect();
}

function onCanvasDown(event) {
  if (state.editMode !== 2 || event.button !== 0) return;
  const hit = pickHit(event);
  const person = state.people[0];
  if (!hit || !person) {
    directDrag = {
      mode: "orbit",
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      yaw: state.editYaw,
      moved: false,
    };
    canvas.style.cursor = "grabbing";
    return;
  }
  state.directSide = person.wrapper.worldToLocal(hit.point.clone()).x;
  const id = classifyRegion(person, hit.point);
  if (!id) return;
  const prevBand = regionSpec(state.directRegion) && regionSpec(state.directRegion).band;
  state.directRegion = id;
  state.hoverRegion = id;
  state.cursorPinned = false;
  const axes = axesFor(id);
  directDrag = {
    mode: "region",
    pointerId: event.pointerId,
    x: event.clientX,
    y: event.clientY,
    originX: axes.x ? axisValue(axes.x) : 0,
    originY: axes.y ? axisValue(axes.y) : 0,
    fat: state.editBody.body_fat_z,
    moved: false,
  };
  canvas.style.cursor = cssCursor(id);
  syncDirect();
  const nextBand = regionSpec(id) && regionSpec(id).band;
  if (prevBand !== nextBand) frame();
}

function onPointerMove(event) {
  if (directDrag && directDrag.pointerId === event.pointerId) {
    const dx = event.clientX - directDrag.x;
    const dy = event.clientY - directDrag.y;
    if (!directDrag.moved && Math.hypot(dx, dy) < DRAG_SLOP) return;
    if (!directDrag.moved && directDrag.mode === "region") beginChange();
    directDrag.moved = true;
    applyDirectDrag(dx, dy);
    return;
  }
  if (!directDrag && event.target === canvas) updateHover(event);
}

function endDrag(event) {
  if (!directDrag) return;
  if (event && directDrag.pointerId !== event.pointerId) return;
  directDrag = null;
  commitChange();
  canvas.style.cursor = state.hoverRegion ? cssCursor(state.hoverRegion) : "grab";
}

const CLOTH_NAMES = {
  cloth_f_tee: "T-Shirt",
  cloth_skirt: "Rock",
  cloth_shirt: "Hemd",
  cloth_jacket: "Jacke",
  cloth_shorts: "kurze Hose",
  shoes_sneaker: "Turnschuhe",
  shoes_black: "schwarze Schuhe",
  shoes_brown: "braune Schuhe",
};

function placeMarker(selector, x, y) {
  const marker = document.querySelector(selector);
  if (!marker) return;
  marker.style.left = `${clamp(x, 0, 1) * 100}%`;
  marker.style.top = `${clamp(y, 0, 1) * 100}%`;
}

function paintGrid(target, sample) {
  const w = target.width;
  const h = target.height;
  const ctx = target.getContext("2d");
  const image = ctx.createImageData(w, h);
  const data = image.data;
  for (let y = 0; y < h; y += 1) {
    const down = h === 1 ? 0 : y / (h - 1);
    for (let x = 0; x < w; x += 1) {
      const across = w === 1 ? 0 : x / (w - 1);
      const hex = sample(across, down);
      const i = (y * w + x) * 4;
      data[i] = Number.parseInt(hex.slice(1, 3), 16);
      data[i + 1] = Number.parseInt(hex.slice(3, 5), 16);
      data[i + 2] = Number.parseInt(hex.slice(5, 7), 16);
      data[i + 3] = 255;
    }
  }
  ctx.putImageData(image, 0, 0);
}

let skinPainted = false;
let hairPainted = false;
let irisPainted = false;

function paintSkinField() {
  if (skinPainted) return;
  skinPainted = true;
  paintGrid(document.querySelector("#skin-field"), (_x, down) => gridColor(SKIN, down, _x));
}

function paintHairField() {
  if (hairPainted) return;
  hairPainted = true;
  paintGrid(document.querySelector("#hair-field"), (_x, down) => gridColor(HAIR, down, _x));
}

function paintIrisField() {
  if (irisPainted) return;
  irisPainted = true;
  paintGrid(document.querySelector("#iris-field"), (across) => rampColor(IRIS, across));
}

function fieldPoint(event) {
  const rect = event.currentTarget.getBoundingClientRect();
  return {
    x: clamp((event.clientX - rect.left) / rect.width, 0, 1),
    y: clamp((event.clientY - rect.top) / rect.height, 0, 1),
  };
}

function bindField(target, pick) {
  let active = 0;
  target.addEventListener("pointerdown", (event) => {
    active = event.pointerId;
    beginChange();
    pick(event);
  });
  target.addEventListener("pointermove", (event) => {
    if (active === event.pointerId) pick(event);
  });
  const stop = (event) => {
    if (active !== event.pointerId) return;
    active = 0;
    commitChange();
  };
  target.addEventListener("pointerup", stop);
  target.addEventListener("pointercancel", stop);
}

function setCategory(id, force) {
  let next = id || "";
  if (!force && state.category === next) next = "";
  state.category = next;
  document.body.classList.toggle("panel-open", Boolean(next));
  for (const name of ["koerper", "gesicht", "haare", "kleidung", "haut"]) {
    document.querySelector(`#panel-${name}`).hidden = name !== next;
  }
  for (const button of document.querySelectorAll("#categories button")) {
    button.setAttribute("aria-pressed", button.dataset.cat === next ? "true" : "false");
  }
  if (next === "haut") paintSkinField();
  if (next === "haare") {
    paintHairField();
    paintIrisField();
  }
  requestAnimationFrame(() => {
    resize();
    if (state.template && state.people.length) frame();
  });
}

function logistic(unit) {
  return 1 / (1 + Math.exp(-unit));
}

function gaussian() {
  let u = 0;
  let v = 0;
  while (u === 0) u = Math.random();
  while (v === 0) v = Math.random();
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(Math.PI * 2 * v);
}

function round01(value) {
  return Math.round(clamp(value, 0, 1) * 100) / 100;
}

// Editor-only stand-in for a founder. Z-scores are the population normal.
// Pigment uses the northern-European logits in data/calibration/genetics.json
// (10.4), not a second genome. Adult height uses the European mean and sd.
function drawFounder() {
  const sex = editSex();
  const age = state.editBody.body_form_age;
  const body = state.editBody;
  for (const key of BODY_KEYS) {
    if (key.endsWith("_z") || key.startsWith("face_axis_")) body[key] = quantizeZ(gaussian());
  }
  if (age >= 18) {
    const mean = sex === "male" ? 180.7 : 167.3;
    const sd = sex === "male" ? 7.01 : 6.67;
    body.height_cm = Math.round(clamp(mean + sd * gaussian(), 145, 210));
  }
  const pigment = gaussian();
  const hairDark = gaussian();
  const red = Math.random() < 0.04;
  const blueEyes = Math.random() < 0.5;
  let skinEu = logistic(-2.7 + 0.6 * pigment);
  let skinPheo = logistic(-1.5 - 0.25 * pigment);
  let hairEu = logistic(-0.8 + 0.55 * pigment + 0.9 * hairDark);
  let hairPheo = logistic(-1.1 - 0.25 * pigment);
  let iris = logistic(-0.35 + 0.9 * pigment);
  if (red) {
    skinEu *= 0.55;
    skinPheo = 0.45;
    hairEu *= 0.08;
    hairPheo = 1;
  }
  if (blueEyes) iris *= 0.15;
  body.skin_eumelanin = round01(skinEu);
  body.skin_pheomelanin = round01(skinPheo);
  body.hair_eumelanin = round01(hairEu);
  body.hair_pheomelanin = round01(hairPheo);
  body.iris_melanin = round01(iris);
  const share = logistic((age - 13) / 1.15);
  const ampA = logistic(1.5 + gaussian());
  const ampE = logistic(1.5 + gaussian());
  if (sex === "male") {
    body.androgenic_development = round01(share * ampA);
    body.estrogenic_development = round01(share * 0.1 * ampE);
    body.breast_development = 0;
  } else {
    body.androgenic_development = round01(share * 0.15 * ampA);
    body.estrogenic_development = round01(share * ampE);
    body.breast_development = round01(share * logistic(gaussian()));
  }
  for (const key of PROVISIONAL_KEYS) state.provisional[key] = quantizeZ(gaussian());
  if (!breastDragOn(body, sex)) {
    state.provisional.breast_volume_z = 0;
    state.provisional.breast_height_z = 0;
  }
}

function bindZSlider(input, key) {
  input.addEventListener("pointerdown", () => beginChange());
  input.addEventListener("input", () => {
    state.editBody[key] = quantizeZ(Number(input.value));
    syncDirect();
    drawEdit();
  });
  input.addEventListener("pointerup", () => commitChange());
  input.addEventListener("change", () => commitChange());
}

function bindDirectEditor() {
  if (state.directBound) return;
  state.directBound = true;
  canvas.style.cursor = "grab";
  const hair = document.querySelector("#direct-hair");
  const auto = document.createElement("option");
  auto.value = "";
  auto.textContent = "aus der Person";
  hair.append(auto);
  for (const style of HAIR_STYLES) {
    const option = document.createElement("option");
    option.value = style.id;
    option.textContent = style.label;
    hair.append(option);
  }
  hair.addEventListener("change", () => {
    beginChange();
    state.hairStyle = hair.value;
    commitChange();
    drawEdit();
  });
  const dye = document.querySelector("#direct-dye");
  const dyeColor = document.querySelector("#direct-dye-color");
  dye.addEventListener("change", () => {
    beginChange();
    state.hairDye = dye.checked ? dyeColor.value : "";
    commitChange();
    syncDirect();
    drawEdit();
  });
  dyeColor.addEventListener("pointerdown", () => beginChange());
  dyeColor.addEventListener("input", () => {
    if (!dye.checked) return;
    state.hairDye = dyeColor.value;
    drawEdit();
  });
  dyeColor.addEventListener("change", () => commitChange());
  document.querySelector("#categories").addEventListener("click", (event) => {
    const button = event.target.closest("button");
    if (button) setCategory(button.dataset.cat);
  });
  document.querySelector("#turn-left").addEventListener("click", () => turnBy(-Math.PI / 4));
  document.querySelector("#turn-right").addEventListener("click", () => turnBy(Math.PI / 4));
  document.querySelector("#edit-back").addEventListener("click", () => {
    document.querySelector("#edit-reset").click();
  });
  document.querySelector("#edit-random").addEventListener("click", () => {
    beginChange();
    drawFounder();
    commitChange();
    syncDirect();
    drawEdit();
  });
  document.querySelector("#edit-undo").addEventListener("click", undoEdit);
  document.querySelector("#edit-redo").addEventListener("click", redoEdit);
  bindZSlider(document.querySelector("#fat-slider"), "body_fat_z");
  bindZSlider(document.querySelector("#muscle-slider"), "muscle_z");
  bindZSlider(document.querySelector("#edit-dist"), "fat_distribution_z");
  const pad = document.querySelector("#shape-pad");
  const movePad = (event) => {
    const rect = pad.getBoundingClientRect();
    const u = clamp((event.clientX - rect.left) / rect.width, 0, 1);
    const v = clamp(1 - (event.clientY - rect.top) / rect.height, 0, 1);
    state.editBody.body_fat_z = quantizeZ(u * 6 - 3);
    state.editBody.muscle_z = quantizeZ(v * 6 - 3);
    syncDirect();
    drawEdit();
  };
  let padDrag = 0;
  pad.addEventListener("pointerdown", (event) => {
    padDrag = event.pointerId;
    beginChange();
    movePad(event);
  });
  pad.addEventListener("pointermove", (event) => {
    if (padDrag === event.pointerId) movePad(event);
  });
  pad.addEventListener("pointerup", (event) => {
    if (padDrag !== event.pointerId) return;
    padDrag = 0;
    commitChange();
  });
  bindField(document.querySelector("#skin-field"), (event) => {
    const point = fieldPoint(event);
    state.editBody.skin_pheomelanin = round01(point.x);
    state.editBody.skin_eumelanin = round01(point.y);
    syncDirect();
    drawEdit();
  });
  bindField(document.querySelector("#hair-field"), (event) => {
    const point = fieldPoint(event);
    state.editBody.hair_pheomelanin = round01(point.x);
    state.editBody.hair_eumelanin = round01(point.y);
    syncDirect();
    drawEdit();
  });
  bindField(document.querySelector("#iris-field"), (event) => {
    state.editBody.iris_melanin = round01(fieldPoint(event).x);
    syncDirect();
    drawEdit();
  });
  canvas.addEventListener("pointerdown", onCanvasDown);
  canvas.addEventListener("pointerleave", () => {
    if (directDrag || state.cursorPinned) return;
    if (!state.hoverRegion) return;
    state.hoverRegion = "";
    canvas.style.cursor = "grab";
    syncDirect();
  });
  window.addEventListener("pointermove", onPointerMove);
  window.addEventListener("pointerup", endDrag);
  window.addEventListener("pointercancel", endDrag);
  syncDirect();
}

function applySimulate() {
  const raw = params.get("simulate");
  if (!raw || state.editMode !== 2 || !state.editBody) return;
  for (const part of raw.split(",")) {
    const cut = part.indexOf(":");
    if (cut < 0) continue;
    const op = part.slice(0, cut);
    const arg = part.slice(cut + 1);
    if (op === "yaw") state.editYaw = Number(arg);
    else if (op === "hover") {
      state.hoverRegion = arg;
      state.cursorPinned = true;
    } else if (op === "panel") setCategory(arg, true);
    else if (op === "region") {
      if (DIRECT_REGIONS[arg]) state.directRegion = arg;
    } else if (op === "axis") {
      const cut2 = arg.indexOf(":");
      if (cut2 < 0) continue;
      const key = arg.slice(0, cut2);
      const bag = PROVISIONAL_KEYS.includes(key) ? state.provisional : state.editBody;
      if (bag && Object.prototype.hasOwnProperty.call(bag, key)) bag[key] = quantizeZ(Number(arg.slice(cut2 + 1)));
    } else if (DIRECT_REGIONS[op]) {
      state.directRegion = op;
      const value = Number(arg);
      const axes = axesFor(op);
      if (axes.x) setAxis(axes.x, value);
      if (op === "nose_bridge" && axes.y) setAxis(axes.y, value);
      else if (!axes.x && axes.y) setAxis(axes.y, value);
    }
  }
  syncDirect();
  restyle();
  frame();
}

window.__directState = () => ({
  region: state.directRegion,
  hover: state.hoverRegion,
  yaw: state.editYaw,
  facing: facing(),
  hip: state.editBody && state.editBody.hip_width_z,
  butt: state.provisional && state.provisional.buttock_volume_z,
  nose: state.editBody && state.editBody.face_axis_01,
  bridge: state.provisional && state.provisional.nose_bridge_z,
  skin: state.editBody && [state.editBody.skin_eumelanin, state.editBody.skin_pheomelanin],
  category: state.category,
});

async function boot() {
  status.textContent = "Loading the body…";
  const [family, gltf, pack, skin, beard] = await Promise.all([
    fetch("/cache/family.json").then((response) => {
      if (!response.ok) throw new Error(`family.json HTTP ${response.status}`);
      return response.json();
    }),
    new Promise((resolve, reject) => {
      new GLTFLoader().load("/cache/body.glb", resolve, undefined, reject);
    }),
    loadPack(),
    loadSkin(),
    loadBeard(),
  ]);
  state.family = family;
  state.pack = pack;
  state.skin = skin;
  state.beard = beard;
  state.template = gltf.scene;
  const templateMesh = findMesh(gltf.scene);
  state.normalGroups = buildNormalGroups(templateMesh.geometry);
  if (pack.bodyRest) {
    state.bodyHm = buildRestIndex(pack.bodyRest, templateMesh.geometry);
    console.log(`body rest match missed ${state.bodyHm.missed} of ${state.bodyHm.corners}`);
  }
  const line = family.line || {};
  caption.textContent = `Seed ${family.seed}. Sims ${line.grandfather}, ${line.grandmother}, ${line.father}, ${line.mother}, girl ${line.girl_17}, boy ${line.boy_13}.`;
  setupEditor();
  resize();
  restyle();
  frame();
  applySimulate();
  animate();
}

boot().catch((error) => {
  status.textContent = error && error.message ? error.message : String(error);
  console.error(error);
});
