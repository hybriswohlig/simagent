const MAGIC = 0x46546c67;
const JSON_CHUNK = 0x4e4f534a;
const BIN_CHUNK = 0x004e4942;

/**
 * Refuses a binary glTF whose buffers or images name a URI: the body must carry everything
 * itself, so the loader never requests a file past the page's API. The whole container is
 * read: exactly one JSON chunk first, then at most one binary chunk, nothing else, and the
 * chunks end where the header says the file ends.
 */
export function selfContained(bytes: ArrayBuffer): void {
  const view = new DataView(bytes);
  if (bytes.byteLength < 20 || view.getUint32(0, true) !== MAGIC) throw new Error("body is not a binary glTF");
  if (view.getUint32(8, true) !== bytes.byteLength) throw new Error("body length does not match its header");
  const chunks: { type: number; start: number; length: number }[] = [];
  let at = 12;
  while (at < bytes.byteLength) {
    if (at + 8 > bytes.byteLength) throw new Error("body chunk header is cut");
    const length = view.getUint32(at, true);
    const type = view.getUint32(at + 4, true);
    if (at + 8 + length > bytes.byteLength) throw new Error("body chunk is cut");
    chunks.push({ type, start: at + 8, length });
    at += 8 + length;
  }
  const [first, ...rest] = chunks;
  if (first === undefined || first.type !== JSON_CHUNK) throw new Error("body is not a binary glTF");
  if (rest.length > 1 || rest.some((chunk) => chunk.type !== BIN_CHUNK)) throw new Error("body has extra chunks");
  const json: unknown = JSON.parse(new TextDecoder().decode(new Uint8Array(bytes, first.start, first.length)));
  const named = (list: unknown): boolean =>
    Array.isArray(list) && list.some((item) => typeof item === "object" && item !== null && "uri" in item);
  const root = (typeof json === "object" && json !== null ? json : {}) as { buffers?: unknown; images?: unknown };
  if (named(root.buffers) || named(root.images)) throw new Error("body names an external file");
}
