import { BODY_CONTRACT_VERSION } from "./generated/body.js";

/**
 * Whether this viewer may show body values of contract `version` (ADR-0005 section 6): a
 * reader accepts the same breaking stage (below 1.0.0 the first two numbers, from 1.0.0
 * the first) with the same or an older version; a missing parameter then takes its default
 * (the portrait does that). Anything else it refuses with a message.
 */
export function readable(version: string, own: string = BODY_CONTRACT_VERSION): boolean {
  const parse = (text: string): [number, number, number] | null => {
    const match = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/.exec(text);
    return match === null ? null : [Number(match[1]), Number(match[2]), Number(match[3])];
  };
  const data = parse(version);
  const reader = parse(own);
  if (data === null || reader === null) {
    return false;
  }
  const stage = (v: [number, number, number]) => (v[0] === 0 ? `0.${v[1]}` : `${v[0]}`);
  if (stage(data) !== stage(reader)) {
    return false;
  }
  for (const i of [0, 1, 2] as const) {
    if (data[i] !== reader[i]) {
      return data[i] < reader[i];
    }
  }
  return true;
}
