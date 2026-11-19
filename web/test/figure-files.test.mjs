// The figure files of `web/figure` are the ones `figure.sha256` lists, byte for byte, and
// no other file stands beside them in the build.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";

const figure = new URL("../dist/figure/", import.meta.url);

/** Every file below `dir`, relative to the figure folder. */
function files(dir) {
  return readdirSync(new URL(dir, figure), { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? files(`${dir}${entry.name}/`) : [`${dir}${entry.name}`],
  );
}

test("the figure files have their listed SHA-256", () => {
  const listed = readFileSync(new URL("./figure.sha256", import.meta.url), "utf8")
    .split("\n")
    .filter((line) => line !== "")
    .map((line) => {
      const [, hash, name] = /^([0-9a-f]{64}) {2}(.+)$/.exec(line) ?? assert.fail(`not a list line: ${line}`);
      return [name, hash];
    });
  const found = files("")
    .sort()
    .map((name) => [name, createHash("sha256").update(readFileSync(new URL(name, figure))).digest("hex")]);
  assert.deepEqual(found, listed.slice().sort(([a], [b]) => (a < b ? -1 : 1)));
});
