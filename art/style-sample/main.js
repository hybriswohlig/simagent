import * as THREE from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { clone as cloneSkeleton } from "three/addons/utils/SkeletonUtils.js";

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
];
const MEASURE_KEYS = new Set([...AGE_KEYS, ...FAT_MUSCLE]);

const params = new URLSearchParams(location.search);
const state = {
  style: ["A", "B", "C"].includes(params.get("style")) ? params.get("style") : "A",
  close: params.get("close") === "1",
  focus: params.get("focus") || "",
  edit: params.get("edit") === "1",
  editIndex: 0,
  editBody: null,
  editBase: null,
  people: [],
  facePage: Math.max(0, (Number(params.get("page")) || 1) - 1),
  clock: 0,
};
const FACE_COUNT = 5;
const CLOTH = "#314a6e";

const canvas = document.querySelector("#stage");
const labels = document.querySelector("#labels");
const status = document.querySelector("#status");
const caption = document.querySelector("#caption");
const renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.toneMapping = THREE.NoToneMapping;
renderer.setClearColor(0xe7e2da, 1);

const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(26, 1, 0.05, 80);
const hemi = new THREE.HemisphereLight(0xfffaf6, 0xe6ddd2, Math.PI * 0.42);
const key = new THREE.DirectionalLight(0xfff6ee, Math.PI * 0.38);
key.position.set(-2.4, 5.2, 4.6);
const fill = new THREE.DirectionalLight(0xf6efe6, Math.PI * 0.12);
fill.position.set(4.2, 2.2, 3);
const rim = new THREE.DirectionalLight(0xffd8c2, Math.PI * 0.16);
rim.position.set(0.6, 3.4, -4.4);
scene.add(hemi, key, fill, rim);
const keyDirView = new THREE.Vector3();

function applyStyleLights(style) {
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
    fill.intensity = Math.PI * 0.22;
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
const ground = new THREE.Mesh(
  new THREE.PlaneGeometry(80, 80),
  new THREE.MeshStandardMaterial({ color: 0xc9c3b8, roughness: 1, metalness: 0 }),
);
ground.rotation.x = -Math.PI / 2;
ground.position.y = -0.002;
scene.add(ground);

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

function morphWeights(body, sex, emphasise) {
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
  const fu = clamp(body.body_fat_z / 4, -1, 1);
  const mu = clamp(body.muscle_z / 4, -1, 1);
  const keep = (1 - Math.abs(fu)) * (1 - Math.abs(mu)) + Math.abs(fu) * Math.abs(mu);
  const fatPos = Math.max(fu, 0) * (1 - Math.abs(mu));
  const fatNeg = Math.max(-fu, 0) * (1 - Math.abs(mu));
  const musPos = Math.max(mu, 0) * (1 - Math.abs(fu));
  const musNeg = Math.max(-mu, 0) * (1 - Math.abs(fu));
  for (const branch of ["male", "female"]) {
    const youngWeight = weights[`age_${branch}_young`];
    weights[`age_${branch}_young`] = youngWeight * keep;
    weights[`fat_${branch}_max`] = youngWeight * fatPos;
    weights[`fat_${branch}_min`] = youngWeight * fatNeg;
    weights[`muscle_${branch}_max`] = youngWeight * musPos;
    weights[`muscle_${branch}_min`] = youngWeight * musNeg;
  }
  const quarter = (value) => value / 4;
  weights.leg_upper = quarter(body.leg_ratio_z);
  weights.leg_lower = quarter(body.leg_ratio_z);
  weights.head_size = quarter(body.head_size_z);
  weights.shoulder_width = quarter(body.shoulder_width_z);
  weights.chest_depth = quarter(body.chest_depth_z);
  weights.pelvis_width = quarter(body.hip_width_z);
  weights.fat_central = Math.max(body.fat_distribution_z, 0) / 4;
  weights.fat_peripheral = Math.max(-body.fat_distribution_z, 0) / 4;
  // Young and old female corners include MakeHuman's average cup. breast_min
  // cancels that cup, so the chest under it is flat. The bust is then only
  // breast_max, the localized max-cup delta: nothing at 0, a bud by about
  // 0.17, one average cup at 0.5, and the full max cup at 1. Sex does not
  // scale it. A male keeps both weights at 0.
  const breast = clamp(body.breast_development, 0, 1);
  const youngFemale =
    (weights.age_female_young || 0) +
    (weights.fat_female_max || 0) + (weights.fat_female_min || 0) +
    (weights.muscle_female_max || 0) + (weights.muscle_female_min || 0);
  const oldFemale = weights.age_female_old || 0;
  const cup = clamp(breast / 0.5, 0, 1);
  weights.breast_min = sex === "male" ? 0 : youngFemale + oldFemale;
  weights.breast_max = sex === "male" ? 0 : cup * 0.34 + Math.max(0, breast - 0.5) * 1.32;
  weights.face_age_decr = clamp((body.body_form_age - body.face_form_age) / 20, 0, 1);
  weights.face_age_incr = clamp((body.face_form_age - body.body_form_age) / 45, 0, 1);
  for (let axis = 1; axis <= 12; axis += 1) {
    const key = `face_axis_${String(axis).padStart(2, "0")}`;
    weights[key] = quarter(body[key]);
  }
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
  #endif
`;

const CLOTH_BAND = `
  if (uCut > 0.0 && vBind.y < uCut) discard;
  float ny = (vBind.y - uFloor) / uSpan;
  float ax = abs(vBind.x);
  float vertical = smoothstep(0.328, 0.336, ny) * (1.0 - smoothstep(0.812, 0.822, ny));
  // Past the shoulder joint (x ≈ 0.17). The A-pose arm is farther out, so the
  // neckline stays a straight cut across the collarbone.
  float arm = smoothstep(0.26, 0.34, ax);
  diffuseColor.rgb = mix(diffuseColor.rgb, uCloth, vertical * (1.0 - arm));
  skinMask = 1.0 - vertical * (1.0 - arm);
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

function paintCloth(material, floor, span, cutY, skinHex, beard, style, marks) {
  const warm = style === "A";
  const uniforms = {
    uCloth: { value: new THREE.Color(CLOTH) },
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
  };
  material.customProgramCacheKey = () => (warm ? "bodysuit-skin-beard-a" : "bodysuit-skin-beard");
  material.onBeforeCompile = (shader) => {
    shader.uniforms.uCloth = uniforms.uCloth;
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
    material.userData.shader = shader;
    shader.vertexShader = `varying vec3 vBind;\n${shader.vertexShader.replace(
      "#include <morphtarget_vertex>",
      "#include <morphtarget_vertex>\n\tvBind = transformed;",
    )}`;
    const decl = warm
      ? "varying vec3 vBind;\nuniform vec3 uCloth;\nuniform vec3 uSkin;\nuniform vec3 uSkinMean;\nuniform vec3 uBeardCol;\nuniform float uBeardDensity;\nuniform float uBeardCoverage;\nuniform sampler2D uBeardMap;\nuniform float uFloor;\nuniform float uSpan;\nuniform float uCut;\nuniform vec3 uNose;\nuniform vec3 uChin;\nuniform vec3 uKeyDir;\nfloat skinMask;\n"
      : "varying vec3 vBind;\nuniform vec3 uCloth;\nuniform vec3 uSkin;\nuniform vec3 uSkinMean;\nuniform vec3 uBeardCol;\nuniform float uBeardDensity;\nuniform float uBeardCoverage;\nuniform sampler2D uBeardMap;\nuniform float uFloor;\nuniform float uSpan;\nuniform float uCut;\nfloat skinMask;\n";
    const color = warm ? `${SKIN_DETAIL}\n${BLUSH}\n${CLOTH_BAND}` : `${SKIN_DETAIL}\n${CLOTH_BAND}`;
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

function bodyMaterial(skinHex, style, floor, span, cutY, beard, marks) {
  const material = surfaceMaterial("#ffffff", style, 0.8);
  material.map = state.skin;
  return paintCloth(material, floor, span, cutY, skinHex, beard, style, marks);
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
`;

const STRAND_FRAG = `
  float cover = vKeep;
  float hairA = 1.0;
  #ifdef USE_MAP
    hairA = sampledDiffuseColor.a;
    // Four texels. This only feathers the cut. It does not decide the part fill.
    float softA = hairA;
    if (uSoft > 0.5) {
      vec2 texel = vec2(0.002);
      softA += texture(map, vMapUv + vec2(texel.x, 0.0)).a;
      softA += texture(map, vMapUv - vec2(texel.x, 0.0)).a;
      softA += texture(map, vMapUv + vec2(0.0, texel.y)).a;
      softA += texture(map, vMapUv - vec2(0.0, texel.y)).a;
      softA *= 0.2;
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
  float hole = (1.0 - smoothstep(0.2, 0.45, hairA)) * fillMid * inMass * step(0.5, vKeep);
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
  // Soft coverage rounds the alpha-test stair. Lashes and brows stay a hard cut.
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
    "attribute vec3 aMesh;\nvarying float vKeep;\nvarying vec3 vMesh;\nuniform float uBald;\nuniform vec3 uCrown;",
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

function humanPositions(weights) {
  const human = state.pack.human;
  const pos = new Float32Array(human.basis);
  for (const morph of human.morphs) {
    const weight = weights[morph.name] || 0;
    if (weight === 0) continue;
    const delta = morph.data;
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

function hairChoice(body) {
  if (body.body_form_age < 16) return "hair_child";
  if (body.sex === "male") return "hair_short";
  return "hair_long";
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

function addGrooming(mesh, head, humanPos, body, hairHex, irisHex, skinHex, style, marks) {
  const pack = state.pack.proxies;
  const hairName = hairChoice(body);
  const density = clamp(body.hair_density, 0, 1);
  if (density > 0.02) {
    const bald = hairName === "hair_short" ? clamp(body.balding_stage, 0, 7) / 7 : 0;
    const threshold = THREE.MathUtils.lerp(0.9, 0.2, density);
    // Caps are the last midline height with opaque hair on both sides.
    const cap = hairName === "hair_child" ? 0.06 : hairName === "hair_short" ? 0.042 : 0.05;
    const crown = hairCrown(pack[hairName], humanPos);
    addFitted(
      mesh, head, pack[hairName], humanPos,
      strandMaterial(style, pack[hairName].texture, hairHex, threshold, bald, crown, -1, true, true, cap, hairName === "hair_child"),
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
  addFitted(
    mesh, head, pack.lashes, humanPos,
    strandMaterial(style, pack.lashes.texture, hexOf(mixLinear(linearOf(hairHex), [0, 0, 0], 0.62)), 0.32, 0, marks.crown, -6),
    4,
    0.0035,
  );
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

function roster() {
  if (state.edit && state.editBody) {
    const member = state.family.members[state.editIndex];
    return [{ ...member, body: state.editBody }];
  }
  let all = fullRoster();
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
  const shaped = morphWeights(member.body, member.sex, emphasise);
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
  const hair = hairColor(member.body);
  const iris = rampColor(IRIS, member.body.iris_melanin);
  const cloth = morphedYSpan(mesh, weights);
  const cutY = state.close ? marks.maxY + marks.jaw.noseDy - 0.098 : -1;
  const bearded = member.sex === "male";
  mesh.material = bodyMaterial(skin, state.style, cloth.minY, cloth.span, cutY, {
    color: hair,
    density: bearded ? clamp(member.body.beard_density, 0, 1) : 0,
    coverage: bearded ? clamp(member.body.beard_coverage, 0, 1) : 0,
  }, marks);
  mesh.frustumCulled = false;
  let outline = null;
  if (state.style === "B") {
    outline = new THREE.SkinnedMesh(mesh.geometry, outlineMaterial(cutY));
    outline.bind(mesh.skeleton, mesh.bindMatrix);
    outline.morphTargetInfluences = mesh.morphTargetInfluences;
    outline.morphTargetDictionary = mesh.morphTargetDictionary;
    outline.frustumCulled = false;
    mesh.parent.add(outline);
  }
  const body = { ...member.body, sex: member.sex };
  const humanPos = humanPositions(weights);
  const groom = addGrooming(mesh, head, humanPos, body, hair, iris, skin, state.style, marks);
  scene.add(wrapper);
  wrapper.updateWorldMatrix(true, true);
  hangArms(root, shaped.slices.child);
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
    anchor: new THREE.Vector3(), member, shaped, skin, hair, iris,
    hairName: groom.hairName, beardTris: 0, arms: root.userData.arms,
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
  const floor = state.style === "C"
    ? new THREE.MeshBasicMaterial({ color: 0xc9c3b8, toneMapped: false })
    : new THREE.MeshStandardMaterial({ color: 0xc9c3b8, roughness: 1, metalness: 0, side: THREE.FrontSide });
  ground.material = floor;
  applyStyleLights(state.style);
  publishReport();
  updateFaceButtons();
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
  } else if (state.focus) {
    const xs = people.map((person) => person.wrapper.position.x);
    const heights = people.map((person) => person.member.body.height_cm / 100);
    const maxH = Math.max(...heights);
    const minX = Math.min(...xs) - 0.2;
    const maxX = Math.max(...xs) + 0.2;
    const midX = (minX + maxX) / 2;
    const width = Math.max(maxX - minX, 0.5);
    const top = maxH * 0.94;
    const bot = maxH * 0.40;
    const midY = (top + bot) / 2;
    const distH = ((top - bot) / 0.88) / (2 * Math.tan(v / 2));
    const distW = width / (2 * Math.tan(h / 2));
    const distance = Math.max(distH, distW);
    camera.position.set(midX, midY, distance);
    camera.lookAt(midX, midY, 0);
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
}

function animate() {
  const delta = clock.getDelta();
  state.clock += delta;
  const breath = Math.sin(state.clock * 1.6);
  const sway = Math.sin(state.clock * 0.7);
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
  renderer.render(scene, camera);
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
  if (state.edit) url.searchParams.set("edit", "1");
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
  const proxies = {};
  for (const [key, proxy] of Object.entries(meta.proxies)) {
    if (!textures[proxy.texture]) {
      const texture = await loader.loadAsync(`/cache/proxies/${proxy.texture}`);
      texture.colorSpace = THREE.SRGBColorSpace;
      texture.flipY = true;
      texture.wrapS = THREE.ClampToEdgeWrapping;
      texture.wrapT = THREE.ClampToEdgeWrapping;
      textures[proxy.texture] = texture;
    }
    proxies[key] = {
      scale: proxy.scale,
      count: proxy.count,
      texture: textures[proxy.texture],
      bind: bufferView(buffer, proxy.bind, Float32Array),
      uv: bufferView(buffer, proxy.uv, Float32Array),
      faces: bufferView(buffer, proxy.faces, Uint32Array),
    };
  }
  return { human, proxies };
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

const FACE_AXIS_LABELS = [
  "Nose width",
  "Nose projection",
  "Nose length",
  "Jaw width",
  "Chin height",
  "Jaw prominence",
  "Cheek fullness",
  "Cheekbones",
  "Eye size",
  "Eye spacing",
  "Lip fullness",
  "Mouth width",
];

const EDIT_SLIDERS = [
  { group: "Body", key: "height_cm", label: "Height", min: 90, max: 210, step: 1 },
  { key: "body_form_age", label: "Body maturity", min: 1, max: 90, step: 1, age: true },
  { key: "body_fat_z", label: "Fat mass", min: -4, max: 4, step: 0.05 },
  { key: "fat_distribution_z", label: "Fat distribution", min: -4, max: 4, step: 0.05 },
  { key: "muscle_z", label: "Muscle", min: -4, max: 4, step: 0.05 },
  { key: "shoulder_width_z", label: "Shoulder width", min: -4, max: 4, step: 0.05 },
  { key: "hip_width_z", label: "Pelvis width", min: -4, max: 4, step: 0.05 },
  { key: "breast_development", label: "Breast development", min: 0, max: 1, step: 0.01, breast: true },
  { key: "leg_ratio_z", label: "Leg length", min: -4, max: 4, step: 0.05 },
  ...FACE_AXIS_LABELS.map((label, index) => ({
    group: index === 0 ? "Face" : undefined,
    key: `face_axis_${String(index + 1).padStart(2, "0")}`,
    label,
    min: -4,
    max: 4,
    step: 0.05,
  })),
  { group: "Colour", key: "skin_eumelanin", label: "Skin eumelanin", min: 0, max: 1, step: 0.01 },
  { key: "skin_pheomelanin", label: "Skin pheomelanin", min: 0, max: 1, step: 0.01 },
  { key: "hair_eumelanin", label: "Hair eumelanin", min: 0, max: 1, step: 0.01 },
  { key: "hair_pheomelanin", label: "Hair pheomelanin", min: 0, max: 1, step: 0.01 },
  { key: "iris_melanin", label: "Iris melanin", min: 0, max: 1, step: 0.01 },
  { key: "balding_stage", label: "Balding stage", min: 0, max: 7, step: 0.1 },
  { key: "beard_density", label: "Beard density", min: 0, max: 1, step: 0.01 },
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

function syncEditSliders() {
  const sex = state.family.members[state.editIndex].sex;
  for (const input of document.querySelectorAll("#edit-sliders input")) {
    const key = input.dataset.key;
    input.value = String(state.editBody[key]);
    const out = input.parentElement.querySelector("output");
    if (out) out.textContent = formatValue(state.editBody[key]);
    if (key === "breast_development") {
      const on = breastSliderOn(state.editBody, sex);
      input.disabled = !on;
      input.closest("label").classList.toggle("is-off", !on);
    }
  }
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

function buildEditSliders() {
  const root = document.querySelector("#edit-sliders");
  root.replaceChildren();
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
    out.textContent = formatValue(state.editBody[spec.key]);
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
      input.title = "Female branch, after puberty";
      if (!on) label.classList.add("is-off");
    }
    input.addEventListener("input", () => {
      const value = Number(input.value);
      state.editBody[spec.key] = value;
      if (spec.age) state.editBody.face_form_age = value;
      out.textContent = formatValue(value);
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
    root.append(label);
  }
}

function loadEditPerson(index) {
  state.editIndex = index;
  const member = state.family.members[index];
  state.editBase = { ...member.body };
  state.editBody = { ...member.body };
  buildEditSliders();
  writeContract();
}

function setupEditor() {
  if (!state.edit) return;
  document.body.classList.add("editing");
  const aside = document.querySelector("#editor");
  aside.hidden = false;
  const select = document.querySelector("#edit-person");
  state.family.members.forEach((member, index) => {
    const option = document.createElement("option");
    option.value = String(index);
    option.textContent = `${member.first_name} ${member.age_years}`;
    select.append(option);
  });
  select.addEventListener("change", () => {
    loadEditPerson(Number(select.value));
    restyle();
    frame();
  });
  document.querySelector("#edit-reset").addEventListener("click", () => {
    state.editBody = { ...state.editBase };
    syncEditSliders();
    writeContract();
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
  }
  select.value = String(start);
  loadEditPerson(start);
}

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
  state.normalGroups = buildNormalGroups(findMesh(gltf.scene).geometry);
  const line = family.line || {};
  caption.textContent = `Seed ${family.seed}. Sims ${line.grandfather}, ${line.grandmother}, ${line.father}, ${line.mother}, girl ${line.girl_17}, boy ${line.boy_13}.`;
  setupEditor();
  resize();
  restyle();
  frame();
  animate();
}

boot().catch((error) => {
  status.textContent = error && error.message ? error.message : String(error);
  console.error(error);
});
