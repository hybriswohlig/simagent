import { Refused, UnknownAnswer } from "./api.js";
import { CODES, TEXT } from "./text.js";

/** The code of a failed write: the server's, `version` for an answer of an unknown kind,
 * `network` for no answer. */
export function failureCode(error: unknown): string {
  if (error instanceof Refused) {
    return error.code;
  }
  return error instanceof UnknownAnswer ? "version" : "network";
}

/** The text of a refusal by its code. */
export function refusalText(code: string): string {
  const texts: Record<string, string> = TEXT;
  return texts[`refused_${code}`] ?? `${TEXT.refused_other} ${code}`;
}

/** The text of a failed input: the core's reason of a rejection, else the refusal. */
export function inputFailureText(error: unknown): string {
  if (error instanceof Refused && error.code === "input_rejected" && error.reason !== null) {
    return CODES[`reject ${error.reason}`] ?? `${TEXT.refused_other} ${error.reason}`;
  }
  return refusalText(failureCode(error));
}
