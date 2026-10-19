/**
 * The layout of an hourglass (SVG geometry, which only `web/src/render/` may compute): one
 * row per generation, the great-grandparents at the top, the grandchildren at the bottom,
 * each row centred; a line from each parent to each of its children in the drawing.
 */

import type { Person } from "../generated/export.js";
import type { Hourglass } from "../tree.js";

export type Placed = { readonly person: Person; readonly x: number; readonly y: number; readonly center: boolean };
export type Edge = { readonly from: number; readonly to: number };
export type Layout = {
  readonly nodes: readonly Placed[];
  readonly edges: readonly Edge[];
  readonly width: number;
  readonly height: number;
};

/** Horizontal distance of two people in a row, vertical distance of two rows. */
const STEP_X = 90;
const STEP_Y = 80;

export function layout(glass: Hourglass): Layout {
  const rows: (readonly Person[])[] = [...glass.up].reverse();
  rows.push([glass.center], ...glass.down);
  const widest = Math.max(...rows.map((row) => row.length));
  const width = Math.max(1, widest) * STEP_X;
  const nodes: Placed[] = [];
  rows.forEach((row, r) => {
    row.forEach((person, i) => {
      const x = width / 2 + (i - (row.length - 1) / 2) * STEP_X;
      nodes.push({ person, x, y: STEP_Y / 2 + r * STEP_Y, center: person.sim === glass.center.sim });
    });
  });
  const drawn = new Set(nodes.map((node) => node.person.sim));
  const edges: Edge[] = [];
  for (const node of nodes) {
    for (const parent of [node.person.mother, node.person.father]) {
      if (parent !== null && drawn.has(parent)) {
        edges.push({ from: parent, to: node.person.sim });
      }
    }
  }
  return { nodes, edges, width, height: rows.length * STEP_Y };
}
