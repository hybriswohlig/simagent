/**
 * Refuses a binary glTF whose buffers or images name a URI: the body must carry everything
 * itself, so the loader never requests a file past the page's API.
 */
export function selfContained(bytes: ArrayBuffer): void {
  const view = new DataView(bytes);
  const glb = bytes.byteLength >= 20 && view.getUint32(0, true) === 0x46546c67;
  if (!glb || view.getUint32(16, true) !== 0x4e4f534a) throw new Error("body is not a binary glTF");
  const length = view.getUint32(12, true);
  if (20 + length > bytes.byteLength) throw new Error("body JSON chunk is cut");
  const json: unknown = JSON.parse(new TextDecoder().decode(new Uint8Array(bytes, 20, length)));
  const named = (list: unknown): boolean =>
    Array.isArray(list) && list.some((item) => typeof item === "object" && item !== null && "uri" in item);
  const root = (typeof json === "object" && json !== null ? json : {}) as { buffers?: unknown; images?: unknown };
  if (named(root.buffers) || named(root.images)) throw new Error("body names an external file");
}
