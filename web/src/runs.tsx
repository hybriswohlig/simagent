import { useEffect, useState } from "preact/hooks";

import { getJson } from "./api.js";
import type { RunEntry, RunsAnswer } from "./generated/export.js";
import { current, failure, type Loaded } from "./loaded.js";
import { format } from "./router.js";
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
    </section>
  );
}
