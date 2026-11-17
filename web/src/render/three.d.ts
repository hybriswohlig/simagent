/** The surface of Three r170 this viewer uses. The rest of the library stays untyped. */

declare module "three" {
  export class Vector3 {
    constructor(x?: number, y?: number, z?: number);
    x: number;
    y: number;
    z: number;
    set(x: number, y: number, z: number): this;
    copy(v: Vector3): this;
    add(v: Vector3): this;
    sub(v: Vector3): this;
    normalize(): this;
    lengthSq(): number;
    applyMatrix4(m: Matrix4): this;
    applyQuaternion(q: Quaternion): this;
    transformDirection(m: Matrix4): this;
    fromArray(array: ArrayLike<number>, offset?: number): this;
  }

  export class Quaternion {
    copy(q: Quaternion): this;
    invert(): this;
    setFromAxisAngle(axis: Vector3, angle: number): this;
  }

  export class Matrix4 {
    copy(m: Matrix4): this;
    multiplyMatrices(a: Matrix4, b: Matrix4): this;
  }

  export class Euler {
    x: number;
    y: number;
    z: number;
  }

  export class Color {
    constructor(color?: string | number);
    set(color: string): this;
    setHex(hex: number): this;
    setRGB(r: number, g: number, b: number): this;
    copy(color: Color): this;
  }

  export class BufferAttribute {
    constructor(array: ArrayLike<number>, itemSize: number, normalized?: boolean);
    array: ArrayLike<number>;
    count: number;
    itemSize: number;
    needsUpdate: boolean;
    clone(): BufferAttribute;
    getX(index: number): number;
    getY(index: number): number;
    getZ(index: number): number;
  }

  export class Uint16BufferAttribute extends BufferAttribute {}
  export class Float32BufferAttribute extends BufferAttribute {}

  export class BufferGeometry {
    attributes: Record<string, BufferAttribute | undefined>;
    morphAttributes: { position?: BufferAttribute[] };
    morphTargetsRelative: boolean;
    index: BufferAttribute | null;
    setAttribute(name: string, attribute: BufferAttribute): this;
    getAttribute(name: string): BufferAttribute | undefined;
    setIndex(index: BufferAttribute | null): this;
    computeVertexNormals(): void;
    dispose(): void;
  }

  export class Texture {
    colorSpace: string;
    flipY: boolean;
    wrapS: number;
    wrapT: number;
    anisotropy: number;
    image: CanvasImageSource | null;
    dispose(): void;
  }

  export interface Shader {
    uniforms: Record<string, { value: unknown }>;
    vertexShader: string;
    fragmentShader: string;
  }

  export class Material {
    side: number;
    map: Texture | null;
    color: Color;
    polygonOffset: boolean;
    polygonOffsetFactor: number;
    polygonOffsetUnits: number;
    alphaToCoverage: boolean;
    userData: Record<string, unknown>;
    customProgramCacheKey: () => string;
    onBeforeCompile: (shader: Shader) => void;
    onBeforeRender: () => void;
    dispose(): void;
  }

  export class MeshStandardMaterial extends Material {
    constructor(parameters?: { roughness?: number; metalness?: number });
    roughness: number;
    metalness: number;
  }

  export class Object3D {
    name: string;
    parent: Object3D | null;
    children: Object3D[];
    position: Vector3;
    quaternion: Quaternion;
    rotation: Euler;
    scale: Vector3;
    matrixWorld: Matrix4;
    frustumCulled: boolean;
    renderOrder: number;
    visible: boolean;
    userData: Record<string, unknown>;
    traverse(callback: (node: Object3D) => void): void;
    updateWorldMatrix(updateParents: boolean, updateChildren: boolean): void;
    updateMatrixWorld(force?: boolean): void;
    add(...objects: Object3D[]): this;
    getWorldPosition(target: Vector3): Vector3;
    getWorldQuaternion(target: Quaternion): Quaternion;
    rotateZ(angle: number): this;
  }

  export class Group extends Object3D {}
  export class Bone extends Object3D {}

  export class Mesh extends Object3D {
    constructor(geometry?: BufferGeometry, material?: Material);
    geometry: BufferGeometry;
    material: Material;
    castShadow: boolean;
    morphTargetInfluences?: number[];
    morphTargetDictionary?: Record<string, number>;
  }

  export class Skeleton {
    bones: Bone[];
    boneInverses: Matrix4[];
  }

  export class SkinnedMesh extends Mesh {
    constructor(geometry?: BufferGeometry, material?: Material);
    skeleton: Skeleton;
    bindMatrix: Matrix4;
    bindMode: string;
    bind(skeleton: Skeleton, bindMatrix?: Matrix4): void;
  }

  export class Scene extends Object3D {}

  export class Camera extends Object3D {
    matrixWorldInverse: Matrix4;
  }

  export class PerspectiveCamera extends Camera {
    constructor(fov?: number, aspect?: number, near?: number, far?: number);
    fov: number;
    aspect: number;
    near: number;
    far: number;
    updateProjectionMatrix(): void;
    lookAt(x: number, y: number, z: number): void;
  }

  export class Light extends Object3D {
    color: Color;
    intensity: number;
  }

  export class HemisphereLight extends Light {
    constructor(skyColor?: number, groundColor?: number, intensity?: number);
    groundColor: Color;
  }

  export class DirectionalLight extends Light {
    constructor(color?: number, intensity?: number);
  }

  export class WebGLRenderer {
    constructor(parameters?: { canvas?: HTMLCanvasElement; antialias?: boolean });
    domElement: HTMLCanvasElement;
    outputColorSpace: string;
    toneMapping: number;
    toneMappingExposure: number;
    capabilities: { getMaxAnisotropy(): number };
    setPixelRatio(value: number): void;
    setSize(width: number, height: number, updateStyle?: boolean): void;
    setClearColor(color: number, alpha?: number): void;
    render(scene: Scene, camera: Camera): void;
    dispose(): void;
  }

  export class FileLoader {
    setResponseType(value: string): this;
    loadAsync(url: string): Promise<string | ArrayBuffer>;
  }

  export class TextureLoader {
    loadAsync(url: string): Promise<Texture>;
  }

  export const SRGBColorSpace: string;
  export const NoColorSpace: string;
  export const ACESFilmicToneMapping: number;
  export const FrontSide: number;
  export const DoubleSide: number;
  export const ClampToEdgeWrapping: number;

  export const MathUtils: {
    degToRad(degrees: number): number;
    lerp(x: number, y: number, t: number): number;
  };
}

declare module "three/addons/loaders/GLTFLoader.js" {
  import { Group } from "three";
  export class GLTFLoader {
    loadAsync(url: string): Promise<{ scene: Group }>;
  }
}

declare module "three/addons/utils/SkeletonUtils.js" {
  import { Object3D } from "three";
  export function clone(source: Object3D): Object3D;
}
