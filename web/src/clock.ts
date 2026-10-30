import type { CalendarDate } from "./generated/export.js";
import { TEXT } from "./text.js";

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

/** One step, pressed: asked for unless a request is on its way (never two at a time);
 * while playing, it becomes the step that plays. */
export function press(clock: Clock, step: Step): Next {
  if (clock.busy) {
    return [clock, null];
  }
  const playing = clock.playing === null ? null : step;
  return [{ ...clock, playing, busy: true, refused: null }, step];
}

/** Playing `step`: at once, and after each answer, at the next `tick`, the same step again,
 * until a pause or a refusal. */
export function play(clock: Clock, step: Step): Next {
  const playing = { ...clock, playing: step };
  return clock.busy ? [playing, null] : [{ ...playing, busy: true, refused: null }, step];
}

/** A pause: the request on its way still lands, no further one goes. */
export function pause(clock: Clock): Next {
  return [{ ...clock, playing: null }, null];
}

/** The server's answer to a step: where the run stands now. Playing goes on at the next
 * `tick`, so that the player sees each step. */
export function answered(clock: Clock, closed: number, date: CalendarDate): Next {
  return [{ ...clock, closed, date, busy: false }, null];
}

/** The pace of playing has come round: the playing step again, if it still plays and no
 * request is on its way. */
export function tick(clock: Clock): Next {
  if (clock.playing === null || clock.busy) {
    return [clock, null];
  }
  return [{ ...clock, busy: true, refused: null }, clock.playing];
}

/** A refusal: the clock stands, playing ends, the code is shown. */
export function refused(clock: Clock, code: string): Next {
  return [{ ...clock, busy: false, playing: null, refused: code }, null];
}

/** The server answered a read: a refusal for no answer no longer holds; one the server gave
 * stays until the next step. */
export function heard(clock: Clock): Clock {
  return clock.refused === "network" ? { ...clock, refused: null } : clock;
}

/** The label of the play button: the step it plays (the second playtest). */
export function playText(step: Step): string {
  return TEXT[`play_${step}`];
}
