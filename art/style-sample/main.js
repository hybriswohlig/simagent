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
scene.add(hemi, key, fill);
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
  const { male, female, estrogen } = share;
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
  const breast = clamp(body.breast_development, 0, 1);
  weights.breast_max = estrogen * Math.max(0, (breast - 0.5) * 2);
  weights.breast_min = estrogen * Math.max(0, (0.5 - breast) * 2);
  if (sex === "male") {
    weights.breast_max = 0;
    weights.breast_min = Math.max(weights.breast_min, Math.max(0, share.androgen - share.estrogen));
  }
  weights.face_age_decr = clamp((body.body_form_age - body.face_form_age) / 20, 0, 1);
  weights.face_age_incr = clamp((body.face_form_age - body.body_form_age) / 45, 0, 1);
  const feature = emphasise ? 1.2 : 1;
  for (let axis = 1; axis <= 12; axis += 1) {
    const key = `face_axis_${String(axis).padStart(2, "0")}`;
    weights[key] = quarter(body[key]) * feature;
  }
  if (emphasise) weights.head_size *= 1.08;
  return { weights, share, slices: { child, young, old } };
}

function applyWeights(mesh, weights) {
  const dict = mesh.morphTargetDictionary || {};
  const influences = mesh.morphTargetInfluences;
  if (!influences) return;
  for (const [name, index] of Object.entries(dict)) influences[index] = weights[name] || 0;
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

function landmarks(mesh, weights) {
  const position = mesh.geometry.attributes.position;
  const morphs = mesh.geometry.morphAttributes.position || [];
  const dict = mesh.morphTargetDictionary || {};
  const active = [];
  for (const [name, index] of Object.entries(dict)) {
    const weight = weights[name] || 0;
    if (weight !== 0 && morphs[index]) active.push([weight, morphs[index]]);
  }
  let maxY = -Infinity;
  const count = position.count;
  const step = 8;
  const rough = [];
  for (let vertex = 0; vertex < count; vertex += step) {
    let y = position.getY(vertex);
    for (const [weight, morph] of active) y += weight * morph.getY(vertex);
    if (y > maxY) maxY = y;
    rough.push(vertex);
  }
  const bandLow = maxY - 0.28;
  let left = [];
  let right = [];
  let chin = null;
  for (const vertex of rough) {
    let x = position.getX(vertex);
    let y = position.getY(vertex);
    let z = position.getZ(vertex);
    for (const [weight, morph] of active) {
      x += weight * morph.getX(vertex);
      y += weight * morph.getY(vertex);
      z += weight * morph.getZ(vertex);
    }
    if (y < bandLow) continue;
    if (y > maxY - 0.16 && y < maxY - 0.05 && z > 0.02) {
      if (x < -0.015) left.push([x, y, z]);
      if (x > 0.015) right.push([x, y, z]);
    }
    if (Math.abs(x) < 0.045 && y < maxY - 0.18 && y > maxY - 0.26 && (!chin || z > chin[2])) {
      chin = [x, y, z];
    }
  }
  const frontCluster = (rows) => {
    if (!rows.length) return null;
    rows.sort((a, b) => b[2] - a[2]);
    const take = rows.slice(0, Math.max(4, Math.floor(rows.length * 0.25)));
    const mean = [0, 0, 0];
    for (const row of take) {
      mean[0] += row[0];
      mean[1] += row[1];
      mean[2] += row[2];
    }
    return mean.map((value) => value / take.length);
  };
  return {
    maxY,
    leftEye: frontCluster(left),
    rightEye: frontCluster(right),
    chin,
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
  else material = new THREE.MeshStandardMaterial({ roughness: Math.max(roughness, 0.72), metalness: 0 });
  material.color.setStyle(hex);
  return material;
}

// Bind-space band: feet at y=0, crown at 1.666 m. Hips stay wide. Above the
// waist the limit stops at the torso so the A-pose arms stay skin.
const CLOTH_BAND = `
  float ny = vBind.y / 1.666;
  float ax = abs(vBind.x);
  float vertical = smoothstep(0.30, 0.325, ny) * (1.0 - smoothstep(0.74, 0.76, ny));
  float arm = smoothstep(0.18, 0.21, ax) * smoothstep(0.53, 0.56, ny);
  diffuseColor.rgb = mix(diffuseColor.rgb, uCloth, vertical * (1.0 - arm));
`;

function paintCloth(material) {
  material.customProgramCacheKey = () => "bodysuit-basis";
  material.onBeforeCompile = (shader) => {
    shader.uniforms.uCloth = { value: new THREE.Color(CLOTH) };
    shader.vertexShader = `varying vec3 vBind;\n${shader.vertexShader.replace(
      "#include <begin_vertex>",
      "#include <begin_vertex>\n\tvBind = position;",
    )}`;
    shader.fragmentShader = `varying vec3 vBind;\nuniform vec3 uCloth;\n${shader.fragmentShader.replace(
      "#include <color_fragment>",
      `#include <color_fragment>\n${CLOTH_BAND}`,
    )}`;
  };
  return material;
}

function bodyMaterial(skinHex, style) {
  return paintCloth(surfaceMaterial(skinHex, style, 0.58));
}

function hairMaterial(color, opacity, style) {
  const material = style === "C"
    ? new THREE.MeshBasicMaterial({ color, toneMapped: false })
    : style === "B"
      ? new THREE.MeshToonMaterial({ color, gradientMap: toonGradient })
      : new THREE.MeshStandardMaterial({ color, roughness: 0.55, metalness: 0 });
  if (opacity < 0.98) {
    material.transparent = true;
    material.opacity = opacity;
    material.depthWrite = opacity > 0.35;
  }
  return material;
}

function headFit(marks) {
  const chinY = marks.chin ? marks.chin[1] : marks.maxY - 0.23;
  return clamp((marks.maxY - chinY) / 0.24, 0.62, 1.15);
}

function outlineMaterial() {
  const material = new THREE.MeshBasicMaterial({ color: 0x1a1612, side: THREE.BackSide, toneMapped: false });
  material.onBeforeCompile = (shader) => {
    shader.vertexShader = shader.vertexShader.replace(
      "#include <project_vertex>",
      "transformed += normalize(objectNormal) * 0.03;\n#include <project_vertex>",
    );
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

// Landmarks are in mesh space (the face is +Z). inverseBindMatrices map that
// space into the head bone, so a child of `head` follows the Root height scale
// and the spine sway. The same Root scale then sizes the cap.
function mountOnHead(mesh, head, object, x, y, z) {
  const index = mesh.skeleton.bones.indexOf(head);
  const point = new THREE.Vector3(x, y, z).applyMatrix4(mesh.bindMatrix);
  point.applyMatrix4(mesh.skeleton.boneInverses[index]);
  object.position.copy(point);
  object.frustumCulled = false;
  head.add(object);
}

function beardTint(skinHex, hairHex, density) {
  const mixed = mixLinear(linearOf(skinHex), linearOf(hairHex), 0.5 + 0.45 * density);
  return hexOf(mixed.map((channel) => channel * (0.62 - 0.34 * density)));
}

function addCap(mesh, head, color, body, style, marks) {
  const density = clamp(body.hair_density, 0, 1);
  if (density <= 0.02 || !marks || !head) return;
  const stage = clamp(body.balding_stage, 0, 7);
  const cover = 1 - stage / 7;
  const fit = headFit(marks);
  const material = hairMaterial(color, 1, style);
  if (cover > 0.04) {
    const radius = (0.068 + 0.03 * cover) * fit;
    const crown = new THREE.Mesh(
      new THREE.SphereGeometry(
        radius,
        style === "C" ? 14 : 24,
        style === "C" ? 12 : 18,
        0,
        Math.PI * 2,
        0,
        Math.PI * 0.58,
      ),
      material,
    );
    crown.name = "hair";
    crown.scale.set(1.12, 0.82 + 0.2 * cover, 1.05);
    mountOnHead(mesh, head, crown, 0, marks.maxY - radius * 0.62, 0.035 - 0.02 * (1 - cover));
  }
}

function addBeard(mesh, head, color, body, style, marks) {
  if (body.sex !== "male" || !head || !marks.chin) return;
  const density = clamp(body.beard_density, 0, 1);
  if (density <= 0.04) return;
  const coverage = clamp(body.beard_coverage, 0, 1);
  const fit = headFit(marks);
  const material = hairMaterial(color, 0.72 + 0.28 * density, style);
  const beard = new THREE.Mesh(
    new THREE.SphereGeometry(0.032, style === "C" ? 10 : 16, style === "C" ? 8 : 12),
    material,
  );
  beard.name = "beard";
  beard.scale.set((1.5 + 1.3 * coverage) * fit, (0.7 + 0.8 * coverage) * fit, 0.45 * fit);
  mountOnHead(
    mesh,
    head,
    beard,
    0,
    marks.chin[1] - 0.012,
    marks.chin[2] + 0.002,
  );
}

function addEye(mesh, head, point, color, style, fit) {
  if (!point || !head) return;
  const size = fit || 1;
  const sclera = new THREE.Mesh(
    new THREE.SphereGeometry(0.016 * size, 12, 10),
    new THREE.MeshBasicMaterial({ color: 0xf7f4ef, toneMapped: false }),
  );
  mountOnHead(mesh, head, sclera, point[0], point[1], point[2] + 0.012);
  const iris = new THREE.Mesh(
    new THREE.SphereGeometry(0.011 * size, 12, 10),
    style === "A"
      ? new THREE.MeshStandardMaterial({ color, roughness: 0.35, metalness: 0 })
      : new THREE.MeshBasicMaterial({ color, toneMapped: false }),
  );
  mountOnHead(mesh, head, iris, point[0], point[1], point[2] + 0.018);
  const pupil = new THREE.Mesh(
    new THREE.SphereGeometry(0.0032 * size, 8, 8),
    new THREE.MeshBasicMaterial({ color: 0x14110e, toneMapped: false }),
  );
  mountOnHead(mesh, head, pupil, point[0], point[1], point[2] + 0.022);
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
  const all = fullRoster();
  if (!state.close) return all;
  const pages = Math.max(1, Math.ceil(all.length / FACE_COUNT));
  state.facePage = clamp(state.facePage, 0, pages - 1);
  const start = state.facePage * FACE_COUNT;
  return all.slice(start, start + FACE_COUNT);
}

function layoutX(members) {
  const gap = state.close ? 0.5 : 0.8;
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
  const marks = landmarks(mesh, weights);
  const skin = gridColor(SKIN, member.body.skin_eumelanin, member.body.skin_pheomelanin);
  const hair = hairColor(member.body);
  const iris = rampColor(IRIS, member.body.iris_melanin);
  mesh.material = bodyMaterial(skin, state.style);
  mesh.frustumCulled = false;
  let outline = null;
  if (state.style === "B") {
    outline = new THREE.SkinnedMesh(mesh.geometry, outlineMaterial());
    outline.bind(mesh.skeleton, mesh.bindMatrix);
    outline.morphTargetInfluences = mesh.morphTargetInfluences;
    outline.morphTargetDictionary = mesh.morphTargetDictionary;
    outline.frustumCulled = false;
    mesh.parent.add(outline);
  }
  const body = { ...member.body, sex: member.sex };
  addCap(mesh, head, hair, body, state.style, marks);
  addBeard(mesh, head, beardTint(skin, hair, clamp(body.beard_density, 0, 1)), body, state.style, marks);
  const fit = headFit(marks);
  addEye(mesh, head, marks.leftEye, iris, state.style, fit);
  addEye(mesh, head, marks.rightEye, iris, state.style, fit);
  const spines = ["spine_01", "spine_02", "spine_03"].map((name) => findBone(root, name)).filter(Boolean);
  const rest = spines.map((spine) => spine.quaternion.clone());
  scene.add(wrapper);
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
    wrapper, mesh, outline, spines, rest, scale, tag,
    anchor: new THREE.Vector3(), member, shaped, skin, hair, iris,
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
  const lit = state.style !== "C";
  hemi.visible = lit;
  key.visible = lit;
  fill.visible = lit;
  publishReport();
  updateFaceButtons();
}

function frame() {
  const people = state.people;
  if (!people.length) return;
  const xs = people.map((person) => person.wrapper.position.x);
  const heights = people.map((person) => person.member.body.height_cm / 100);
  const maxH = Math.max(...heights);
  const reach = state.close ? 0.22 : 1.05;
  const minX = Math.min(...xs) - reach;
  const maxX = Math.max(...xs) + reach;
  const midX = (minX + maxX) / 2;
  const width = Math.max(maxX - minX, 0.9);
  const headLine = heights.map((height) => height * 0.88);
  const lookY = state.close
    ? (Math.max(...headLine) + Math.min(...headLine)) / 2
    : maxH * 0.5;
  const target = state.close
    ? Math.max(0.46, Math.max(...headLine) - Math.min(...headLine) + 0.36)
    : maxH;
  const fill = state.close ? 0.82 : 0.9;
  camera.fov = 20;
  camera.updateProjectionMatrix();
  const v = THREE.MathUtils.degToRad(camera.fov);
  const aspect = camera.aspect || 1.8;
  const distH = (target / fill) / (2 * Math.tan(v / 2));
  const h = 2 * Math.atan(Math.tan(v / 2) * aspect);
  const distW = width / (2 * Math.tan(h / 2));
  const distance = Math.max(distH, distW);
  const lift = state.close ? target * 0.08 : target * 0.14;
  camera.position.set(midX + distance * 0.1, lookY + lift, distance);
  camera.lookAt(midX, lookY, 0);
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
    person.anchor.set(person.wrapper.position.x, 0.02, 0);
    person.anchor.project(camera);
    const x = (person.anchor.x * 0.5 + 0.5) * canvas.clientWidth;
    const y = (-person.anchor.y * 0.5 + 0.5) * canvas.clientHeight;
    person.tag.style.left = `${x}px`;
    person.tag.style.top = `${y}px`;
  }
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
  if (state.template) restyle();
}

function syncUrl() {
  const url = new URL(location.href);
  url.searchParams.set("style", state.style);
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
  const on = state.close;
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

async function boot() {
  status.textContent = "Loading the body…";
  const [family, gltf] = await Promise.all([
    fetch("/cache/family.json").then((response) => {
      if (!response.ok) throw new Error(`family.json HTTP ${response.status}`);
      return response.json();
    }),
    new Promise((resolve, reject) => {
      new GLTFLoader().load("/cache/body.glb", resolve, undefined, reject);
    }),
  ]);
  state.family = family;
  state.template = gltf.scene;
  const line = family.line || {};
  caption.textContent = `Seed ${family.seed}. Sims ${line.grandfather}, ${line.grandmother}, ${line.father}, ${line.mother}, girl ${line.girl_17}, boy ${line.boy_13}.`;
  resize();
  restyle();
  frame();
  animate();
}

boot().catch((error) => {
  status.textContent = error && error.message ? error.message : String(error);
  console.error(error);
});
