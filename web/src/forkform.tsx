import { useState } from "preact/hooks";

import { postJson } from "./api.js";
import { forkBody, forkFailureText, FORKED, type Choice } from "./fork.js";
import { format, runName } from "./router.js";
import { TEXT } from "./text.js";

const CHOICES: readonly (readonly [Choice, keyof typeof TEXT])[] = [
  ["same", "fork_diseases_same"],
  ["on", "fork_diseases_on"],
  ["off", "fork_diseases_off"],
];

/** Forking the run from a year up to the one it stands in (ADR-0021 section 2 and its
 * addendum G2, texts Grok round 28); on success the page goes to the comparison of the run
 * with its fork. */
export function ForkForm({ run, year }: { run: string; year: number }) {
  const [name, setName] = useState("");
  const [from, setFrom] = useState(String(year));
  const [choice, setChoice] = useState<Choice>("same");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const fromYear = Number(from);
  const valid = runName(name) && Number.isInteger(fromYear) && fromYear >= 1 && fromYear <= year;
  const send = () => {
    setBusy(true);
    setFailed(null);
    postJson(`/api/runs/${run}/fork`, forkBody(name, fromYear, choice), FORKED)
      .then(() => {
        location.hash = format({ page: "diff", a: run, b: name });
      })
      .catch((error: unknown) => {
        setBusy(false);
        setFailed(forkFailureText(error));
      });
  };
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
            onInput={(event) => setFrom(event.currentTarget.value)}
          />
        </label>{" "}
        <label>
          {TEXT.fork_diseases}{" "}
          <select value={choice} disabled={busy} onChange={(event) => setChoice(event.currentTarget.value as Choice)}>
            {CHOICES.map(([id, text]) => (
              <option key={id} value={id}>
                {TEXT[text]}
              </option>
            ))}
          </select>
        </label>{" "}
        <button type="button" disabled={busy || !valid} onClick={send}>
          {TEXT.fork_make}
        </button>
      </p>
      {busy ? <p>{TEXT.fork_busy}</p> : null}
      {failed === null ? null : <p role="alert">{failed}</p>}
    </>
  );
}
