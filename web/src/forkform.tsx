import { useEffect, useRef, useState } from "preact/hooks";

import { postJson } from "./api.js";
import { forkBody, forkFailureText, FORKED, sameLabel, type Choice } from "./fork.js";
import { browserStorage, pinFork } from "./pin.js";
import { format, runName } from "./router.js";
import { TEXT } from "./text.js";

const CHOICES: readonly (readonly [Choice, keyof typeof TEXT])[] = [
  ["same", "fork_diseases_same"],
  ["on", "fork_diseases_on"],
  ["off", "fork_diseases_off"],
];

/** How the run page lets a fork go: whether its clock stands with no write on its way,
 * taking the run's writes for the fork (false if another is on its way) and giving them
 * back. */
export type Writes = { idle: boolean; begin: () => boolean; end: () => void };

/** Forking the run from a year up to the one it stands in (ADR-0021 section 2 and its
 * addendum G2, texts Grok rounds 28 and 40); on success the page goes to the comparison of the
 * run with its fork. The year follows the run's, a typed one until the run steps on, and the
 * choice that keeps the switch names where it stood before that year (`keeps`, from the
 * server); before year 1 a sentence says why there is no fork; while the fork is computed the
 * run takes no other write. */
export function ForkForm({
  run,
  year,
  keeps,
  writes,
}: {
  run: string;
  year: number;
  keeps: readonly boolean[];
  writes: Writes;
}) {
  const [name, setName] = useState("");
  const [chosen, setChosen] = useState<string | null>(null);
  const [choice, setChoice] = useState<Choice>("same");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const shown = useRef(true);
  useEffect(() => {
    shown.current = true;
    return () => {
      shown.current = false;
    };
  }, []);
  // A typed year holds until the run steps on; then the field follows the run again.
  useEffect(() => setChosen(null), [year]);
  const from = chosen ?? String(year);
  const fromYear = Number(from);
  const valid = runName(name) && Number.isInteger(fromYear) && fromYear >= 1 && fromYear <= year;
  const send = () => {
    if (!writes.begin()) {
      return;
    }
    setBusy(true);
    setFailed(null);
    postJson(`/api/runs/${run}/fork`, forkBody(name, fromYear, choice), FORKED)
      .then(() => {
        pinFork(browserStorage(), run, name);
        // Only from the page that asked: a fork that ends after the player left goes nowhere.
        if (shown.current) {
          location.hash = format({ page: "diff", a: run, b: name });
        }
      })
      .catch((error: unknown) => {
        if (shown.current) {
          setBusy(false);
          setFailed(forkFailureText(error));
        }
      })
      .finally(writes.end);
  };
  if (year < 1) {
    return (
      <>
        <h2>{TEXT.fork}</h2>
        <p>{TEXT.fork_not_yet}</p>
      </>
    );
  }
  return (
    <>
      <h2>{TEXT.fork}</h2>
      <p>{TEXT.fork_note}</p>
      <p>
        <input
          type="text"
          aria-label={TEXT.fork_name}
          placeholder={TEXT.fork_name}
          value={name}
          disabled={busy}
          onInput={(event) => setName(event.currentTarget.value)}
        />{" "}
        <label>
          {TEXT.fork_from}{" "}
          <input
            type="number"
            min={1}
            max={year}
            value={from}
            disabled={busy}
            onInput={(event) => setChosen(event.currentTarget.value)}
          />
        </label>{" "}
        <label>
          {TEXT.fork_diseases}{" "}
          <select value={choice} disabled={busy} onChange={(event) => setChoice(event.currentTarget.value as Choice)}>
            {CHOICES.map(([id, text]) => (
              <option key={id} value={id}>
                {id === "same" ? sameLabel(keeps, from, year) : TEXT[text]}
              </option>
            ))}
          </select>
        </label>{" "}
        <button type="button" disabled={busy || !valid || !writes.idle} onClick={send}>
          {TEXT.fork_make}
        </button>
      </p>
      {busy ? <p>{TEXT.fork_busy}</p> : null}
      {failed === null ? null : <p role="alert">{failed}</p>}
    </>
  );
}
