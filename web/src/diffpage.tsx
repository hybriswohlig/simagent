import { Fragment } from "preact";
import { useEffect, useState } from "preact/hooks";

import { getJson } from "./api.js";
import {
  comparedAt,
  diffView,
  home,
  livesDiffering,
  pinnedFor,
  years,
  type Counted,
  type Otherwise,
  type Who,
  type YearPart,
} from "./diff.js";
import type { DiffAnswer, Pedigree } from "./generated/export.js";
import { current, failure, type Loaded } from "./loaded.js";
import { fullName, namesByKey } from "./names.js";
import { TreeView, usePedigree } from "./person.js";
import { browserStorage, pinnedKey } from "./pin.js";
import { format } from "./router.js";
import { useRuns } from "./runs.js";
import { TEXT } from "./text.js";
import { hourglass } from "./tree.js";

const DIFF = { format: "simagent.view.diff", version: 4 } as const;

/** The comparison of two runs from the server; only an answer for these two counts. */
function useDiff(a: string, b: string): Loaded<DiffAnswer> {
  const key = `${a}/${b}`;
  const [loaded, setLoaded] = useState<Loaded<DiffAnswer>>({ state: "loading", key });
  useEffect(() => {
    let live = true;
    setLoaded({ state: "loading", key });
    getJson(`/api/runs/${a}/diff/${b}`, DIFF)
      .then((json) => live && setLoaded({ state: "ready", key, value: json as unknown as DiffAnswer }))
      .catch((error: unknown) => live && setLoaded({ state: "failed", key, text: failure(error) }));
    return () => {
      live = false;
    };
  }, [key]);
  return current(loaded, key);
}

/** A person's name, a link to its page in `run` where it has one there. */
function PersonLink({ run, name, sim }: { run: string; name: string; sim: number | null }) {
  return sim === null ? <>{name}</> : <a href={format({ page: "person", run, sim: String(sim) })}>{name}</a>;
}

function Only({ run, who }: { run: string; who: readonly Who[] }) {
  if (who.length === 0) {
    return null;
  }
  return (
    <>
      <h2>{TEXT.diff_only.replace("{x}", run)}</h2>
      <ul>
        {who.map((person) => (
          <li key={person.key}>
            <PersonLink run={run} name={person.name} sim={person.sim} />
          </li>
        ))}
      </ul>
    </>
  );
}

/** The space around a cell of a life's table, its text at the top. */
const CELL = { padding: "2px 16px 2px 0", verticalAlign: "top", textAlign: "left" } as const;
/** Every table of the page as wide as the page, with the same columns (the third playtest,
 * K8): the labels, then a column per run of half the rest each, at the same place in every
 * table. */
const TABLE = { tableLayout: "fixed", width: "100%" } as const;

function Columns() {
  return (
    <colgroup>
      <col style={{ width: "10em" }} />
      <col />
      <col />
    </colgroup>
  );
}

/** The hereditary diseases counted per run (Grok round 41, V1): only when the core counted
 * one; its notes under the heading, a column per run, each a link to its run. */
function Summary({ a, b, rows, notes }: { a: string; b: string; rows: readonly Counted[]; notes: readonly string[] }) {
  if (rows.length === 0) {
    return null;
  }
  return (
    <>
      <h2>{TEXT.diff_diseases}</h2>
      {notes.map((note) => (
        <p key={note}>{note}</p>
      ))}
      <table style={TABLE}>
        <Columns />
        <thead>
          <tr>
            <th scope="col" style={CELL} />
            {[a, b].map((run) => (
              <th scope="col" style={CELL} key={run}>
                <a href={format({ page: "run", run })}>{run}</a>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.name}>
              <th scope="row" style={CELL}>
                {row.name}
              </th>
              <td style={CELL}>{row.a}</td>
              <td style={CELL}>{row.b}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

/** A life that went otherwise: its name, a link to its page, and a row per field that
 * differs, a column per run. */
function Life({ a, b, life }: { a: string; b: string; life: Otherwise }) {
  const lines = (texts: string[]) => texts.map((text, i) => <div key={i}>{text}</div>);
  const page = home(life, a, b);
  return (
    <article>
      <h3 id={`life-${life.key}`}>
        <PersonLink run={page?.run ?? a} name={life.name} sim={page?.sim ?? null} />
      </h3>
      <table aria-labelledby={`life-${life.key}`} style={TABLE}>
        <Columns />
        <thead>
          <tr>
            <td />
            <th scope="col" style={CELL}>
              <PersonLink run={a} name={a} sim={life.a} />
            </th>
            <th scope="col" style={CELL}>
              <PersonLink run={b} name={b} sim={life.b} />
            </th>
          </tr>
        </thead>
        <tbody>
          {life.rows.map((row) => (
            <tr key={row.label}>
              <th scope="row" style={CELL}>
                {row.label}
              </th>
              <td style={CELL}>{lines(row.a)}</td>
              <td style={CELL}>{lines(row.b)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </article>
  );
}

/** The pinned family in both runs side by side (the backlog of the fun test: the family one
 * follows, not hundreds of lives; Grok's second opinion), each its hourglass in that run, or
 * why it is not there; the heading names the run the pin came from when only one pins it. */
function PinnedSide({ a, b, one, other }: { a: string; b: string; one: Pedigree; other: Pedigree }) {
  const storage = browserStorage();
  const pinned = pinnedFor(pinnedKey(storage, a), pinnedKey(storage, b), a, b);
  if (pinned === null) {
    return <p>{TEXT.pinned_none}</p>;
  }
  const columns = [
    { run: a, pedigree: one },
    { run: b, pedigree: other },
  ].map(({ run, pedigree }) => {
    const person = pedigree.persons.find((each) => each.lineage_key === pinned.key);
    return { run, glass: person === undefined ? null : hourglass(pedigree, person.sim) };
  });
  const center = columns.find((column) => column.glass !== null)?.glass?.center;
  if (center === undefined) {
    return <p>{TEXT.pinned_missing}</p>;
  }
  const heading =
    pinned.from === null
      ? TEXT.pinned_family.replace("{name}", fullName(center))
      : TEXT.pinned_family_from.replace("{run}", pinned.from).replace("{name}", fullName(center));
  return (
    <>
      <h2>{heading}</h2>
      <div style={{ display: "flex", flexWrap: "wrap", gap: "16px" }}>
        {columns.map(({ run, glass }) => (
          <div key={run} style={{ flex: "1 1 0", minWidth: "280px" }}>
            <h3>
              <a href={format({ page: "run", run })}>{run}</a>
            </h3>
            {glass === null ? <p>{TEXT.pinned_missing}</p> : <TreeView run={run} glass={glass} />}
          </div>
        ))}
      </div>
    </>
  );
}

/** A sentence with each run in it as a link to its page. */
function Linked({ parts }: { parts: YearPart[] }) {
  return (
    <p>
      {parts.map((part, i) =>
        typeof part === "string" ? (
          part
        ) : (
          <a key={i} href={format({ page: "run", run: part.run })}>
            {part.run}
          </a>
        ),
      )}
    </p>
  );
}

/** The page of two runs compared (ADR-0021 section 3 and its addenda): the pinned family in
 * both, the summary, then folded the persons of one run only and the lives that went
 * otherwise, named from each run's pedigree. */
export function DiffPage({ a, b }: { a: string; b: string }) {
  const diff = useDiff(a, b);
  const one = usePedigree(a);
  const other = usePedigree(b);
  const runs = useRuns(`${a}/${b}`);
  const title = (
    <>
      <p>
        <a href={format({ page: "runs" })}>{TEXT.back}</a>
      </p>
      <h1>{TEXT.diff_title.replace("{a}", a).replace("{b}", b)}</h1>
    </>
  );
  const failed = [diff, one, other].find((loaded) => loaded.state === "failed");
  if (failed?.state === "failed") {
    return (
      <section>
        {title}
        <p>{TEXT[failed.text]}</p>
      </section>
    );
  }
  if (diff.state !== "ready" || one.state !== "ready" || other.state !== "ready") {
    return (
      <section>
        {title}
        <p>{TEXT.loading}</p>
      </section>
    );
  }
  const view = diffView(diff.value, { a: namesByKey(one.value), b: namesByKey(other.value) });
  const none = view.aOnly.length === 0 && view.bOnly.length === 0 && view.changed.length === 0;
  const yearOf = (run: string) =>
    runs.state === "ready" ? runs.value.runs.find((entry) => entry.run === run)?.date?.year : undefined;
  const [x, y] = [yearOf(a), yearOf(b)];
  const at = comparedAt(diff.value);
  return (
    <section>
      {title}
      {x === undefined || y === undefined ? null : <Linked parts={years(a, x, b, y)} />}
      {at === null ? null : <Linked parts={at} />}
      <PinnedSide a={a} b={b} one={one.value} other={other.value} />
      <Summary a={a} b={b} rows={view.summary} notes={view.notes} />
      {none ? (
        <p>{TEXT.diff_none}</p>
      ) : (
        <details>
          <summary>{TEXT.diff_lives.replace("{n}", String(livesDiffering(view)))}</summary>
          <Only run={a} who={view.aOnly} />
          <Only run={b} who={view.bOnly} />
          {view.groups.map((group) => (
            <Fragment key={group.title}>
              <h2>{group.title}</h2>
              {group.lives.map((life) => (
                <Life key={life.key} a={a} b={b} life={life} />
              ))}
            </Fragment>
          ))}
        </details>
      )}
    </section>
  );
}
