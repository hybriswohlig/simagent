import { getJson } from "./api.js";
import type { WorldAnswer } from "./generated/export.js";
import { TEXT } from "./text.js";

const WORLD = { format: "simagent.view.world", version: 1 } as const;

/** The hunger buttons of the run page: the template each triggers and its label (ADR-0023
 * section 6 and "Texte"). */
export const HUNGERS: readonly (readonly [string, string])[] = [
  ["core:hunger_one_year", TEXT.hunger_one_year],
  ["core:hunger_three_years", TEXT.hunger_three_years],
];

/** The input of a hunger button in the command schema (ADR-0004). */
export function shockInput(shock: string) {
  return { trigger_shock: { shock } };
}

/** The input of the switch button: the switch the other way from where it stands. */
export function switchInput(on: boolean) {
  return { set_world_param: { change: { "core:hereditary_diseases": !on } } };
}

/** The label of the switch button: what pressing it does. */
export function switchLabel(on: boolean): string {
  return on ? TEXT.diseases_switch_off : TEXT.diseases_switch_on;
}

/** Whether the hereditary diseases of `run` are on after its last input (the addendum to
 * ADR-0023 before S2). */
export async function diseasesOn(run: string): Promise<boolean> {
  const answer = (await getJson(`/api/runs/${run}/world`, WORLD)) as unknown as WorldAnswer;
  return answer.hereditary_diseases;
}
