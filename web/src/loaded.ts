import { UnknownAnswer } from "./api.js";
import type { TEXT } from "./text.js";

/** A value the page waits for, marked with what it was asked for (a run, say). */
export type Loaded<T> =
  | { readonly state: "loading"; readonly key: string }
  | { readonly state: "failed"; readonly key: string; readonly text: keyof typeof TEXT }
  | { readonly state: "ready"; readonly key: string; readonly value: T };

/** `loaded` if it answers `key`, else still loading: a page that switched to another run
 * never shows the old run's value with the new run's links. */
export function current<T>(loaded: Loaded<T>, key: string): Loaded<T> {
  return loaded.key === key ? loaded : { state: "loading", key };
}

/** The text of a failure: an answer of another kind or version, or no answer. */
export function failure(error: unknown): keyof typeof TEXT {
  return error instanceof UnknownAnswer ? "failed_version" : "failed_network";
}
