import type { CalendarDate } from "./generated/export.js";

/** The one step of a request to move the clock (ADR-0021 section 2). */
export type Step = "days28" | "year" | "decade";

/**
 * The clock of a run page: where the run stands as the server last said, the step that
 * plays, whether a request is on its way, the code of the last refusal. Playing and pausing
 * are no inputs (ADR-0021 section 2): they live here, in the browser.
 */
export type Clock = {
  readonly closed: number;
  readonly date: CalendarDate;
  readonly playing: Step | null;
  readonly busy: boolean;
  readonly refused: string | null;
};

/** What the page does next: the clock, and the step to ask the server for now, if any. */
export type Next = readonly [Clock, Step | null];

/** The clock of a run that stands at `closed`, on `date`. */
export function standing(closed: number, date: CalendarDate): Clock {
  return { closed, date, playing: null, busy: false, refused: null };
}

/** One step, pressed: asked for unless a request is on its way (never two at a time). */
export function press(clock: Clock, step: Step): Next {
  return clock.busy ? [clock, null] : [{ ...clock, busy: true, refused: null }, step];
}

/** Playing `step`: after each answer the same step again, until a pause or a refusal. */
export function play(clock: Clock, step: Step): Next {
  const playing = { ...clock, playing: step };
  return clock.busy ? [playing, null] : [{ ...playing, busy: true, refused: null }, step];
}

/** A pause: the request on its way still lands, no further one goes. */
export function pause(clock: Clock): Next {
  return [{ ...clock, playing: null }, null];
}

/** The server's answer to a step: where the run stands now; the playing step again. */
export function answered(clock: Clock, closed: number, date: CalendarDate): Next {
  const now = { ...clock, closed, date, busy: false };
  return now.playing === null ? [now, null] : [{ ...now, busy: true }, now.playing];
}

/** A refusal: the clock stands, playing ends, the code is shown. */
export function refused(clock: Clock, code: string): Next {
  return [{ ...clock, busy: false, playing: null, refused: code }, null];
}
