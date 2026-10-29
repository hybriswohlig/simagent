/**
 * The layout of an hourglass (SVG geometry, which only `web/src/render/` may compute): one
 * row per generation, the great-grandparents at the top, the grandchildren at the bottom,
 * each row centred, without empty generations above and below; a line from each parent to
 * each of its children in the drawing, from below the parent's name to the child's circle.
 */

import type { Person } from "../generated/export.js";
import type { Hourglass } from "../tree.js";

export type Placed = { readonly person: Person; readonly x: number; readonly y: number; readonly center: boolean };
/** A line from a parent to its child, and where it is drawn. */
export type Edge = {
  readonly from: number;
  readonly to: number;
  readonly x1: number;
  readonly y1: number;
  readonly x2: number;
  readonly y2: number;
};
export type Layout = {
  readonly nodes: readonly Placed[];
  readonly edges: readonly Edge[];
  readonly width: number;
  readonly height: number;
};

/** Horizontal distance of two people in a row, vertical distance of two rows. */
const STEP_X = 90;
const STEP_Y = 100;
/** The radius of a person's circle. */
export const RADIUS = 20;
/** Offsets under a circle's centre of the first name and of the mark below it. */
export const NAME_DY = 32;
export const MARK_DY = 44;
/** The bottom of a person's name and mark, under its circle's centre: the descenders of the
 * mark's font end above it. */
const LABELS = MARK_DY + 6;
/** Room below the last row, so that the descenders of the mark stay in the drawing. */
const BELOW = LABELS - STEP_Y / 2;

export function layout(glass: Hourglass): Layout {
  const all: (readonly Person[])[] = [...glass.up].reverse();
  all.push([glass.center], ...glass.down);
  // Empty generations at the top and the bottom take no room.
  const first = all.findIndex((row) => row.length > 0);
  const last = all.length - 1 - [...all].reverse().findIndex((row) => row.length > 0);
  const rows = all.slice(first, last + 1);
  const widest = Math.max(...rows.map((row) => row.length));
  const width = Math.max(1, widest) * STEP_X;
  const nodes: Placed[] = [];
  rows.forEach((row, r) => {
    row.forEach((person, i) => {
      const x = width / 2 + (i - (row.length - 1) / 2) * STEP_X;
      nodes.push({ person, x, y: STEP_Y / 2 + r * STEP_Y, center: person.sim === glass.center.sim });
    });
  });
  const at = new Map(nodes.map((node) => [node.person.sim, node]));
  const edges: Edge[] = [];
  for (const node of nodes) {
    for (const [parent, child] of glass.links) {
      const from = child === node.person.sim ? at.get(parent) : undefined;
      // Only links that go up: a parent cut off by the limit of the rows is not drawn
      // beside or below its child.
      if (from !== undefined && from.y < node.y) {
        // Below the parent's name and mark to the top of the child's circle, so that a line
        // runs through no one's circle, name or mark between two rows (the third playtest).
        edges.push({ from: parent, to: child, x1: from.x, y1: from.y + LABELS, x2: node.x, y2: node.y - RADIUS });
      }
    }
  }
  return { nodes, edges, width, height: rows.length * STEP_Y + BELOW };
}
