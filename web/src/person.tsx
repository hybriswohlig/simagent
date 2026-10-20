import { useEffect, useState } from "preact/hooks";

import { getJson } from "./api.js";
import { added } from "./chronicle.js";
import { readable } from "./contract.js";
import { current, failure, type Loaded } from "./loaded.js";
import type { EventItem, Pedigree, Person, PersonAnswer, Shown } from "./generated/export.js";
import { Chronicle } from "./lines.js";
import { readLog } from "./log.js";
import { Portrait } from "./portrait.js";
import { layout } from "./render/tree.js";
import { format } from "./router.js";
import { TEXT } from "./text.js";
import { hourglass, type Hourglass } from "./tree.js";

/** The format of `pedigree.json` this viewer reads (`simagent export`). */
const PEDIGREE_FORMAT = 4;

/** The pedigree of a run from the server, checked for its format; only an answer for this
 * run (and this `at`, which asks again when the run moved on) counts. */
export function usePedigree(run: string, at = ""): Loaded<Pedigree> {
  const key = `${run}@${at}`;
  const [loaded, setLoaded] = useState<Loaded<Pedigree>>({ state: "loading", key });
  useEffect(() => {
    let live = true;
    setLoaded({ state: "loading", key });
    getJson(`/api/runs/${run}/pedigree`, { format_version: PEDIGREE_FORMAT })
      .then((json) => live && setLoaded({ state: "ready", key, value: json as unknown as Pedigree }))
      .catch((error: unknown) => live && setLoaded({ state: "failed", key, text: failure(error) }));
    return () => {
      live = false;
    };
  }, [key]);
  return current(loaded, key);
}

/** The presentation state of a person from the server; only an answer for this person
 * counts. */
function usePersonView(run: string, sim: string): Loaded<PersonAnswer> {
  const key = `${run}/${sim}`;
  const [loaded, setLoaded] = useState<Loaded<PersonAnswer>>({ state: "loading", key });
  useEffect(() => {
    let live = true;
    setLoaded({ state: "loading", key });
    getJson(`/api/runs/${run}/persons/${sim}`, { format: "simagent.view.person", version: 1 })
      .then((json) => live && setLoaded({ state: "ready", key, value: json as unknown as PersonAnswer }))
      .catch((error: unknown) => live && setLoaded({ state: "failed", key, text: failure(error) }));
    return () => {
      live = false;
    };
  }, [key]);
  return current(loaded, key);
}

/** Where a life stands: born in a year, and now or at its end an age in a year, as the
 * core gives them. */
function Life({ view }: { view: PersonAnswer }) {
  const end = view.ended === "death" ? TEXT.died_in : view.ended === "emigration" ? TEXT.emigrated_in : TEXT.now_in;
  return (
    <p>
      {`${TEXT.born_in} ${view.born.calendar_year} · ${end} ${view.shown.at.calendar_year}, ${view.shown.at.age_years} ${TEXT.years}`}
    </p>
  );
}

/** The portraits at the ages a person reached. */
function Ages({ shown }: { shown: readonly Shown[] }) {
  return (
    <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
      {shown.map((at) => (
        <figure key={at.at.age_years}>
          <Portrait params={at.body} label={`${TEXT.at_age} ${at.at.age_years}`} />
          <figcaption>{`${TEXT.at_age} ${at.at.age_years} (${at.at.calendar_year})`}</figcaption>
        </figure>
      ))}
    </div>
  );
}

/** The link to a person's page. */
export function personHref(run: string, sim: number): string {
  return format({ page: "person", run, sim: String(sim) });
}

function Name({ run, person }: { run: string; person: Person | undefined }) {
  return person === undefined ? <span>{TEXT.unknown}</span> : <a href={personHref(run, person.sim)}>{person.sim}</a>;
}

/** The hourglass as a drawing; each person a link that centres the tree on them (P11). */
function TreeView({ run, glass }: { run: string; glass: Hourglass }) {
  const { nodes, edges, width, height } = layout(glass);
  const at = new Map(nodes.map((node) => [node.person.sim, node]));
  return (
    <svg viewBox={`0 0 ${width} ${height}`} width={width} height={height} role="group" aria-label={TEXT.tree}>
      {edges.map((edge) => {
        const from = at.get(edge.from);
        const to = at.get(edge.to);
        return from && to ? (
          <line key={`${edge.from}-${edge.to}`} x1={from.x} y1={from.y} x2={to.x} y2={to.y} stroke="#888888" />
        ) : null;
      })}
      {nodes.map((node) => (
        <a
          key={node.person.sim}
          href={personHref(run, node.person.sim)}
          data-sim={node.person.sim}
          aria-label={`${TEXT.person} ${node.person.sim}`}
        >
          <circle
            cx={node.x}
            cy={node.y}
            r={20}
            fill={node.person.sex === "female" ? "#f3d9e4" : "#d9e6f3"}
            stroke={node.center ? "#000000" : "#888888"}
            stroke-width={node.center ? 3 : 1}
          />
          <text x={node.x} y={node.y} text-anchor="middle" dominant-baseline="central">
            {node.person.sim}
          </text>
        </a>
      ))}
    </svg>
  );
}

/** The chronicle of one person: the entries of the log about them, newest first. */
function PersonChronicle({ run, sim }: { run: string; sim: string }) {
  const [retries, setRetries] = useState(0);
  const key = `${run}/${sim}/${retries}`;
  const [loaded, setLoaded] = useState<Loaded<readonly EventItem[]>>({ state: "loading", key });
  useEffect(() => {
    let live = true;
    setLoaded({ state: "loading", key });
    readLog(run, `subject=${sim}`)
      .then((items) => live && setLoaded({ state: "ready", key, value: added([], items) }))
      .catch((error: unknown) => live && setLoaded({ state: "failed", key, text: failure(error) }));
    return () => {
      live = false;
    };
  }, [key]);
  const shown = current(loaded, key);
  return (
    <Chronicle
      run={run}
      items={shown.state === "ready" ? shown.value : []}
      failed={shown.state === "failed" ? TEXT[shown.text] : null}
      retry={() => setRetries((n) => n + 1)}
    />
  );
}

/** A person of a run: who they are related to, the tree around them, and their chronicle. */
export function PersonPage({ run, sim }: { run: string; sim: string }) {
  const loaded = usePedigree(run);
  const view = usePersonView(run, sim);
  if (loaded.state === "loading") {
    return <p>{TEXT.loading}</p>;
  }
  if (loaded.state === "failed") {
    return <p>{TEXT[loaded.text]}</p>;
  }
  const glass = hourglass(loaded.value, Number(sim));
  if (glass === null) {
    return <p>{TEXT.unknown_person}</p>;
  }
  const person = glass.center;
  const bySim = new Map(loaded.value.persons.map((p) => [p.sim, p]));
  const parent = (id: number | null) => (id === null ? undefined : bySim.get(id));
  return (
    <section>
      <h1>{`${TEXT.person} ${person.sim}`}</h1>
      <p>{person.sex === "female" ? TEXT.female : TEXT.male}</p>
      {view.state === "ready" && !readable(view.value.contract_version) ? (
        <p>{TEXT.failed_contract}</p>
      ) : view.state === "ready" ? (
        <div>
          <Portrait params={view.value.shown.body} label={`${TEXT.person} ${person.sim}`} />
          <Life view={view.value} />
          <Ages shown={view.value.at_ages} />
        </div>
      ) : view.state === "failed" ? (
        <p>{TEXT[view.text]}</p>
      ) : (
        <p>{TEXT.loading}</p>
      )}
      <p>
        {TEXT.mother} <Name run={run} person={parent(person.mother)} /> · {TEXT.father}{" "}
        <Name run={run} person={parent(person.father)} />
      </p>
      <TreeView run={run} glass={glass} />
      <PersonChronicle run={run} sim={sim} />
    </section>
  );
}
