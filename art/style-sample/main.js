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
    neck: [0, chinY - 0.006, Math.min(jaw.chinZ, 0.05)],
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
  else material = new THREE.MeshStandardMaterial({ roughness: Math.max(roughness, 0.72), metalness: 0 });
  material.color.setStyle(hex);
  return material;
}

// Post-morph, pre-skin band. The neckline is one height on the morphed torso,
// so breast and age morphs cannot wave it. Arm X is still the A-pose, so
// dropping the arms does not paint them.
const CLOTH_BAND = `
  float ny = vBind.y / uSpan;
  float ax = abs(vBind.x);
  float vertical = smoothstep(0.304, 0.307, ny) * (1.0 - smoothstep(0.748, 0.751, ny));
  float arm = smoothstep(0.18, 0.205, ax) * smoothstep(0.52, 0.55, ny);
  diffuseColor.rgb = mix(diffuseColor.rgb, uCloth, vertical * (1.0 - arm));
`;

function paintCloth(material, span) {
  const uniforms = {
    uCloth: { value: new THREE.Color(CLOTH) },
    uSpan: { value: Math.max(span, 0.2) },
  };
  material.customProgramCacheKey = () => "bodysuit-basis";
  material.onBeforeCompile = (shader) => {
    shader.uniforms.uCloth = uniforms.uCloth;
    shader.uniforms.uSpan = uniforms.uSpan;
    material.userData.shader = shader;
    shader.vertexShader = `varying vec3 vBind;\n${shader.vertexShader.replace(
      "#include <morphtarget_vertex>",
      "#include <morphtarget_vertex>\n\tvBind = transformed;",
    )}`;
    shader.fragmentShader = `varying vec3 vBind;\nuniform vec3 uCloth;\nuniform float uSpan;\n${shader.fragmentShader.replace(
      "#include <color_fragment>",
      `#include <color_fragment>\n${CLOTH_BAND}`,
    )}`;
  };
  material.onBeforeRender = () => {
    const shader = material.userData.shader;
    if (!shader || !shader.uniforms.uSpan) return;
    shader.uniforms.uSpan.value = uniforms.uSpan.value;
  };
  return material;
}

function bodyMaterial(skinHex, style, span) {
  return paintCloth(surfaceMaterial(skinHex, style, 0.58), span);
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
  #ifdef USE_MAP
    cover *= sampledDiffuseColor.a;
    float shade = dot(sampledDiffuseColor.rgb, vec3(0.299, 0.587, 0.114));
    float lift = clamp(shade / 0.16, 0.35, 1.45);
    diffuseColor.rgb = uHair * lift;
    diffuseColor.a = 1.0;
  #endif
  if (cover < uThresh) discard;
`;

const EYE_FRAG = `
  #ifdef USE_MAP
    float blueDisc = sampledDiffuseColor.b - max(sampledDiffuseColor.r, sampledDiffuseColor.g);
    if (blueDisc > 0.015 && sampledDiffuseColor.b > 0.28) {
      sampledDiffuseColor.rgb = vec3(0.83, 0.81, 0.78);
    }
    if (vMapUv.x > 0.62 && vMapUv.y < 0.42) {
      sampledDiffuseColor.rgb = vec3(0.83, 0.81, 0.78);
    }
    float luma = dot(sampledDiffuseColor.rgb, vec3(0.299, 0.587, 0.114));
    float hi = max(sampledDiffuseColor.r, max(sampledDiffuseColor.g, sampledDiffuseColor.b));
    float lo = min(sampledDiffuseColor.r, min(sampledDiffuseColor.g, sampledDiffuseColor.b));
    float iris = smoothstep(0.06, 0.16, hi - lo) * smoothstep(0.04, 0.12, luma) * (1.0 - smoothstep(0.42, 0.62, luma));
    vec3 tinted = uIris * clamp(luma / 0.22, 0.35, 1.65);
    diffuseColor.rgb = mix(sampledDiffuseColor.rgb, tinted, iris);
    diffuseColor.a = 1.0;
  #endif
`;

function strandMaterial(style, texture, hairHex, threshold, bald, crown, offset) {
  const material = litMaterial(style, 0.62);
  material.map = texture;
  material.color.setHex(0xffffff);
  material.side = THREE.DoubleSide;
  material.polygonOffset = true;
  material.polygonOffsetFactor = offset;
  material.polygonOffsetUnits = offset;
  const uniforms = {
    uHair: { value: new THREE.Color(hairHex) },
    uThresh: { value: threshold },
    uBald: { value: bald },
    uCrown: { value: new THREE.Vector3(crown[0], crown[1], crown[2]) },
  };
  return bindShader(
    material,
    `strand-${style}`,
    uniforms,
    "attribute vec3 aMesh;\nvarying float vKeep;\nvarying vec3 vMesh;\nuniform float uBald;\nuniform vec3 uCrown;",
    HAIR_VERTEX,
    "varying float vKeep;\nvarying vec3 vMesh;\nuniform vec3 uHair;\nuniform float uThresh;",
    STRAND_FRAG,
  );
}

function eyeMaterial(style, texture, irisHex) {
  const material = litMaterial(style, 0.38);
  material.map = texture;
  material.color.setHex(0xffffff);
  material.side = THREE.FrontSide;
  const uniforms = { uIris: { value: new THREE.Color(irisHex) } };
  return bindShader(
    material,
    `eye-${style}`,
    uniforms,
    "",
    "",
    "uniform vec3 uIris;",
    EYE_FRAG,
  );
}

function beardMaterial(style, hairHex, density) {
  const material = litMaterial(style, 0.7);
  material.color.setHex(0xffffff);
  material.side = THREE.DoubleSide;
  material.polygonOffset = true;
  material.polygonOffsetFactor = -2;
  material.polygonOffsetUnits = -2;
  const uniforms = {
    uHair: { value: new THREE.Color(hairHex) },
    uDensity: { value: density },
  };
  return bindShader(
    material,
    `beard-${style}`,
    uniforms,
    "attribute vec3 aMesh;\nvarying vec3 vMesh;",
    "vMesh = aMesh;",
    "varying vec3 vMesh;\nuniform vec3 uHair;\nuniform float uDensity;",
    `
      vec2 cell = floor(vMesh.xy * 80.0);
      float fiber = fract(sin(dot(cell, vec2(127.1, 311.7))) * 43758.5453);
      if (uDensity < 0.82 && fiber > uDensity) discard;
      diffuseColor.rgb = uHair * mix(0.72, 1.05, fiber);
      diffuseColor.a = 1.0;
    `,
  );
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

function addFitted(mesh, head, proxy, humanPos, material, order, nudgeZ) {
  const fitted = fitProxy(proxy, humanPos);
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

function hairChoice(body) {
  if (body.body_form_age < 16) return "hair_child";
  if (body.sex === "male") return "hair_short";
  return "hair_long";
}

function addGrooming(mesh, head, humanPos, body, hairHex, irisHex, style, marks) {
  const pack = state.pack.proxies;
  const hairName = hairChoice(body);
  const density = clamp(body.hair_density, 0, 1);
  if (density > 0.02) {
    const bald = hairName === "hair_short" ? clamp(body.balding_stage, 0, 7) / 7 : 0;
    const threshold = THREE.MathUtils.lerp(0.9, 0.2, density);
    addFitted(
      mesh, head, pack[hairName], humanPos,
      strandMaterial(style, pack[hairName].texture, hairHex, threshold, bald, marks.crown, -1),
      2,
    );
  }
  const brows = clamp(body.eyebrow_density, 0, 1);
  if (brows > 0.02) {
    addFitted(
      mesh, head, pack.brows, humanPos,
      strandMaterial(style, pack.brows.texture, hairHex, THREE.MathUtils.lerp(0.22, 0.02, brows), 0, marks.crown, -8),
      3,
      0.003,
    );
  }
  // The cornea sits behind the lid surface. Face is +Z. Enough to meet the
  // opening, not enough to leave the sphere in front of the lids.
  addFitted(mesh, head, pack.eyes, humanPos, eyeMaterial(style, pack.eyes.texture, irisHex), 1, 0.0075);
  addFitted(
    mesh, head, pack.lashes, humanPos,
    strandMaterial(style, pack.lashes.texture, hexOf(mixLinear(linearOf(hairHex), [0, 0, 0], 0.72)), 0.12, 0, marks.crown, -6),
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

function buildBeard(mesh, head, cloud, body, hairHex, style, marks) {
  if (body.sex !== "male") return 0;
  const density = clamp(body.beard_density, 0, 1);
  if (density <= 0.04) return 0;
  const coverage = clamp(body.beard_coverage, 0, 1);
  const { xyz, maxY } = cloud;
  const { lipLo, noseDy, noseZ, chinDy, chinZ } = marks.jaw;
  // The nose tip is the reliable anchor. The jaw sits a few centimetres
  // below it; measuring down from the lip jump landed on the neck.
  const jawTop = noseDy - 0.036;
  const jawBot = noseDy - 0.058;
  const jawHalf = THREE.MathUtils.lerp(0.018, 0.052, coverage);
  const cheekOut = 0.04 + 0.04 * coverage;
  const mustacheHalf = 0.008 + 0.028 * coverage;
  const lipBot = lipLo - 0.008;
  const lipTop = Math.min(lipLo + 0.05, noseDy - 0.016);
  const mustacheLo = noseDy - 0.008;
  const mustacheHi = noseDy - 0.002;
  const index = mesh.geometry.index;
  const positions = [];
  const meshSpace = [];
  const faces = [];
  const map = new Map();
  const corner = (vertex) => {
    let slot = map.get(vertex);
    if (slot !== undefined) return slot;
    slot = positions.length / 3;
    map.set(vertex, slot);
    positions.push(xyz[vertex * 3], xyz[vertex * 3 + 1], xyz[vertex * 3 + 2]);
    meshSpace.push(xyz[vertex * 3], xyz[vertex * 3 + 1], xyz[vertex * 3 + 2]);
    return slot;
  };
  const normals = [];
  const at = (vertex) => [xyz[vertex * 3], xyz[vertex * 3 + 1], xyz[vertex * 3 + 2]];
  for (let t = 0; t < index.count; t += 3) {
    const a = index.getX(t);
    const b = index.getX(t + 1);
    const c = index.getX(t + 2);
    const pa = at(a);
    const pb = at(b);
    const pc = at(c);
    const abx = pb[0] - pa[0];
    const aby = pb[1] - pa[1];
    const abz = pb[2] - pa[2];
    const acx = pc[0] - pa[0];
    const acy = pc[1] - pa[1];
    const acz = pc[2] - pa[2];
    const bcx = pc[0] - pb[0];
    const bcy = pc[1] - pb[1];
    const bcz = pc[2] - pb[2];
    if (abx * abx + aby * aby + abz * abz > 0.00032) continue;
    if (acx * acx + acy * acy + acz * acz > 0.00032) continue;
    if (bcx * bcx + bcy * bcy + bcz * bcz > 0.00032) continue;
    const nx = aby * acz - abz * acy;
    const ny = abz * acx - abx * acz;
    const nz = abx * acy - aby * acx;
    const length = Math.hypot(nx, ny, nz) || 1;
    if (nz / length < 0.4) continue;
    if (ny / length < -0.45) continue;
    const mx = (pa[0] + pb[0] + pc[0]) / 3;
    const my = (pa[1] + pb[1] + pc[1]) / 3;
    const mz = (pa[2] + pb[2] + pc[2]) / 3;
    const dys = [pa[1] - maxY, pb[1] - maxY, pc[1] - maxY];
    const minDy = Math.min(...dys);
    const maxDy = Math.max(...dys);
    const dy = my - maxY;
    const ax = Math.abs(mx);
    if (dy < jawBot - 0.006 || dy > noseDy) continue;
    if (dy > noseDy - 0.034 && dy < noseDy - 0.014 && ax < 0.042) continue;
    const jaw = dy >= jawBot && dy <= jawTop && ax <= jawHalf && mz > chinZ - 0.02;
    const cheek = coverage > 0.55 && ax >= 0.04 && ax <= cheekOut && dy >= jawBot && dy <= jawTop && mz > chinZ - 0.01;
    const mustache = minDy >= mustacheLo && maxDy <= mustacheHi && ax <= mustacheHalf && mz > noseZ - 0.02;
    if (!jaw && !cheek && !mustache) continue;
    const ia = corner(a);
    const ib = corner(b);
    const ic = corner(c);
    faces.push(ia, ib, ic);
    for (const slot of [ia, ib, ic]) {
      normals[slot * 3] = (normals[slot * 3] || 0) + nx;
      normals[slot * 3 + 1] = (normals[slot * 3 + 1] || 0) + ny;
      normals[slot * 3 + 2] = (normals[slot * 3 + 2] || 0) + nz;
    }
  }
  if (!faces.length) return 0;
  let meanZ = 0;
  for (let i = 2; i < normals.length; i += 3) meanZ += normals[i];
  const flip = meanZ < 0 ? -1 : 1;
  const shell = new Float32Array(positions.length);
  const kept = new Float32Array(meshSpace);
  for (let i = 0; i < positions.length; i += 3) {
    let nx = normals[i] || 0;
    let ny = normals[i + 1] || 0;
    let nz = normals[i + 2] || 0;
    const length = Math.hypot(nx, ny, nz) || 1;
    nx = flip * nx / length;
    ny = flip * ny / length;
    nz = flip * nz / length;
    shell[i] = positions[i] + nx * 0.0015;
    shell[i + 1] = positions[i + 1] + ny * 0.0015;
    shell[i + 2] = positions[i + 2] + nz * 0.0015;
    normals[i] = nx;
    normals[i + 1] = ny;
    normals[i + 2] = nz;
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.BufferAttribute(toHeadLocal(shell, headMatrix(mesh, head)), 3));
  geometry.setAttribute("aMesh", new THREE.BufferAttribute(kept, 3));
  geometry.setIndex(faces);
  geometry.computeVertexNormals();
  const object = new THREE.Mesh(geometry, beardMaterial(style, hairHex, density));
  object.name = "beard";
  object.frustumCulled = false;
  object.renderOrder = 3;
  head.add(object);
  return faces.length / 3;
}

function aimChild(bone, child, direction) {
  bone.updateWorldMatrix(true, false);
  child.updateWorldMatrix(true, false);
  const origin = new THREE.Vector3();
  const tip = new THREE.Vector3();
  bone.getWorldPosition(origin);
  child.getWorldPosition(tip);
  const current = tip.sub(origin);
  if (current.lengthSq() < 1e-8) return;
  current.normalize();
  const delta = new THREE.Quaternion().setFromUnitVectors(current, direction.clone().normalize());
  const parentQ = new THREE.Quaternion();
  bone.parent.getWorldQuaternion(parentQ);
  const worldQ = new THREE.Quaternion();
  bone.getWorldQuaternion(worldQ);
  worldQ.premultiply(delta);
  bone.quaternion.copy(parentQ.invert()).multiply(worldQ);
  bone.updateWorldMatrix(true, true);
}

function hangArms(root) {
  for (const side of ["l", "r"]) {
    const sign = side === "l" ? 1 : -1;
    const clavicle = findBone(root, `clavicle_${side}`);
    const upper = findBone(root, `upperarm_${side}`);
    const lower = findBone(root, `lowerarm_${side}`);
    const hand = findBone(root, `hand_${side}`);
    if (!clavicle || !upper || !lower || !hand) continue;
    // Face is +Z (toward the camera) and character-left is +X. The forearm's
    // small +Z is the elbow bend; a large one swings the hands onto the belly.
    aimChild(clavicle, upper, new THREE.Vector3(sign, -0.25, 0));
    aimChild(upper, lower, new THREE.Vector3(sign * 0.12, -1, 0.06));
    aimChild(lower, hand, new THREE.Vector3(sign * 0.15, -0.96, 0.16));
  }
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
  const gap = state.close ? 0.24 : 0.78;
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
  const cloud = morphedCloud(mesh, weights);
  const marks = landmarks(cloud);
  const skin = gridColor(SKIN, member.body.skin_eumelanin, member.body.skin_pheomelanin);
  const hair = hairColor(member.body);
  const iris = rampColor(IRIS, member.body.iris_melanin);
  const clothSpan = morphedYSpan(mesh, weights).span;
  mesh.material = bodyMaterial(skin, state.style, clothSpan);
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
  const humanPos = humanPositions(weights);
  const groom = addGrooming(mesh, head, humanPos, body, hair, iris, state.style, marks);
  const beardTris = buildBeard(mesh, head, cloud, body, hair, state.style, marks);
  scene.add(wrapper);
  wrapper.updateWorldMatrix(true, true);
  hangArms(root);
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
    hairName: groom.hairName, beardTris,
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
    jaw: person.marks.jaw,
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
  if (state.close) {
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
      minX = Math.min(minX, crown.x - 0.07);
      maxX = Math.max(maxX, crown.x + 0.07);
      sumZ += crown.z;
    }
    const top = maxY + 0.006;
    const bot = minY - 0.012;
    const midY = (top + bot) / 2;
    const midX = (minX + maxX) / 2;
    const targetH = Math.max(top - bot, 0.2);
    const width = Math.max(maxX - minX, 0.2);
    const distH = (targetH / 0.92) / (2 * Math.tan(v / 2));
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

async function boot() {
  status.textContent = "Loading the body…";
  const [family, gltf, pack] = await Promise.all([
    fetch("/cache/family.json").then((response) => {
      if (!response.ok) throw new Error(`family.json HTTP ${response.status}`);
      return response.json();
    }),
    new Promise((resolve, reject) => {
      new GLTFLoader().load("/cache/body.glb", resolve, undefined, reject);
    }),
    loadPack(),
  ]);
  state.family = family;
  state.pack = pack;
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
