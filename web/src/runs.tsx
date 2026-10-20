import { useEffect, useState } from "preact/hooks";

import { getJson, postJson } from "./api.js";
import type { RunEntry, RunsAnswer } from "./generated/export.js";
import { current, failure, type Loaded } from "./loaded.js";
import { inputFailureText } from "./refusals.js";
import { format, runName } from "./router.js";
import { TEXT } from "./text.js";

/** The run list of the server; `key` asks again when it changes. */
export function useRuns(key: string): Loaded<RunsAnswer> {
  const [loaded, setLoaded] = useState<Loaded<RunsAnswer>>({ state: "loading", key });
  useEffect(() => {
    let live = true;
    setLoaded({ state: "loading", key });
    getJson("/api/runs", { format: "simagent.view.runs", version: 1 })
      .then((json) => live && setLoaded({ state: "ready", key, value: json as unknown as RunsAnswer }))
      .catch((error: unknown) => live && setLoaded({ state: "failed", key, text: failure(error) }));
    return () => {
      live = false;
    };
  }, [key]);
  return current(loaded, key);
}

/** What a run of the list shows beside its name: its year, or why it is not read. */
function standing(entry: RunEntry): string {
  if (entry.date !== undefined) {
    return `${TEXT.year} ${entry.date.year}`;
  }
  return entry.error === "run_failed" ? TEXT.run_failed : TEXT.run_unreadable;
}

const CREATED = { format: "simagent.view.created", version: 1 } as const;

/** The templates a run is founded from: the server's closed list (ADR-0021 section 2). */
const TEMPLATES: readonly (readonly [string, keyof typeof TEXT])[] = [["stadt", "template_stadt"]];

/** Founding a new run from a template; on success the page goes to it. */
function FoundForm() {
  const [name, setName] = useState("");
  const [template, setTemplate] = useState("stadt");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const send = () => {
    setBusy(true);
    setFailed(null);
    postJson("/api/runs", { template, run: name }, CREATED)
      .then(() => {
        location.hash = format({ page: "run", run: name });
      })
      .catch((error: unknown) => {
        setBusy(false);
        setFailed(inputFailureText(error));
      });
  };
  return (
    <>
      <h2>{TEXT.found}</h2>
      <p>
        <input
          type="text"
          aria-label={TEXT.found_name}
          placeholder={TEXT.found_name}
          value={name}
          onInput={(event) => setName(event.currentTarget.value)}
        />{" "}
        <select aria-label={TEXT.found_template} value={template} onChange={(event) => setTemplate(event.currentTarget.value)}>
          {TEMPLATES.map(([id, text]) => (
            <option key={id} value={id}>
              {TEXT[text]}
            </option>
          ))}
        </select>{" "}
        <button type="button" disabled={busy || !runName(name)} onClick={send}>
          {TEXT.found}
        </button>
      </p>
      {failed === null ? null : <p role="alert">{failed}</p>}
    </>
  );
}

/** The runs of the server's directory, each with its year, linked to its page. */
export function RunsPage() {
  const runs = useRuns("runs");
  if (runs.state !== "ready") {
    return <p>{runs.state === "loading" ? TEXT.loading : TEXT[runs.text]}</p>;
  }
  return (
    <section>
      <h1>{TEXT.runs}</h1>
      {runs.value.runs.length === 0 ? <p>{TEXT.no_runs}</p> : null}
      <ul>
        {runs.value.runs.map((entry) => (
          <li key={entry.run}>
            <a href={format({ page: "run", run: entry.run })}>{entry.run}</a>
            {` · ${standing(entry)}`}
          </li>
        ))}
      </ul>
      <FoundForm />
    </section>
  );
}
