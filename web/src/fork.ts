import { Refused } from "./api.js";
import { failureCode, refusalText } from "./refusals.js";
import { CODES, TEXT } from "./text.js";

/** What the fork does with the switch of the hereditary diseases (Grok G4): it keeps the
 * run's up to its year, or turns it on or off. */
export type Choice = "same" | "on" | "off";

export const FORKED = { format: "simagent.view.forked", version: 1 } as const;

/** The body of `POST /api/runs/{run}/fork` (the addendum G2 of ADR-0021 section 2). */
export function forkBody(run: string, fromYear: number, choice: Choice) {
  const changes = choice === "same" ? [] : [{ "core:hereditary_diseases": choice === "on" }];
  return { run, from_year: fromYear, changes };
}

/** The text of a refused fork (Grok G6): the core's reason of a rejected override, the
 * sentence of a known code, else the code after the general sentence. */
export function forkFailureText(error: unknown): string {
  if (error instanceof Refused && error.code === "override_rejected" && error.reason !== null) {
    return CODES[`reject ${error.reason}`] ?? `${TEXT.fork_refused} ${error.reason}`;
  }
  const code = failureCode(error);
  const texts: Record<string, string> = TEXT;
  return texts[`refused_${code}`] === undefined ? `${TEXT.fork_refused} ${code}` : refusalText(code);
}
