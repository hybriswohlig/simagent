import type { PreviewChild } from "./generated/export.js";
import { LOOK_WORDS, TEXT } from "./text.js";

/** The words under a possible child (Grok's round 44, K4): hair, eyes, skin and height, as
 * the core sends them. */
export function childWords(child: PreviewChild): string {
  const looks = child.looks;
  return [
    `${TEXT.look_hair} ${LOOK_WORDS.hair[looks.hair]}`,
    `${TEXT.look_eyes} ${LOOK_WORDS.eyes[looks.eyes]}`,
    `${TEXT.look_skin} ${LOOK_WORDS.skin[looks.skin]}`,
    `${TEXT.look_height} ${LOOK_WORDS.height[looks.height]}`,
  ].join(" · ");
}

/** The caption of a possible child: "Tochter" or "Sohn" (K5). */
export function childName(child: PreviewChild): string {
  return child.sex === "female" ? TEXT.child_daughter : TEXT.child_son;
}

/** The heading over the possible children of a couple (K5). */
export function childrenHeading(mother: string, father: string): string {
  return TEXT.children_heading.replace("{mother}", mother).replace("{father}", father);
}
