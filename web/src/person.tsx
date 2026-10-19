import { useEffect, useState } from "preact/hooks";

import { getJson } from "./api.js";
import type { Pedigree, Person } from "./generated/export.js";
import { layout } from "./render/tree.js";
import { format } from "./router.js";
import { TEXT } from "./text.js";
import { hourglass, type Hourglass } from "./tree.js";

/** The format of `pedigree.json` this viewer reads (`simagent export`). */
const PEDIGREE_FORMAT = 4;

type Loaded =
  | { readonly state: "loading" }
  | { readonly state: "failed"; readonly message: string }
  | { readonly state: "ready"; readonly pedigree: Pedigree };

/** The pedigree of a run from the server, checked for its format. */
function usePedigree(run: string): Loaded {
  const [loaded, setLoaded] = useState<Loaded>({ state: "loading" });
  useEffect(() => {
    let current = true;
    setLoaded({ state: "loading" });
    getJson(`/api/runs/${run}/pedigree`, { format_version: PEDIGREE_FORMAT })
      .then((json) => current && setLoaded({ state: "ready", pedigree: json as unknown as Pedigree }))
      .catch((error: unknown) => current && setLoaded({ state: "failed", message: String(error) }));
    return () => {
      current = false;
    };
  }, [run]);
  return loaded;
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
    <svg viewBox={`0 0 ${width} ${height}`} width={width} height={height} role="img" aria-label={TEXT.tree}>
      {edges.map((edge) => {
        const from = at.get(edge.from);
        const to = at.get(edge.to);
        return from && to ? (
          <line key={`${edge.from}-${edge.to}`} x1={from.x} y1={from.y} x2={to.x} y2={to.y} stroke="#888888" />
        ) : null;
      })}
      {nodes.map((node) => (
        <a key={node.person.sim} href={personHref(run, node.person.sim)} data-sim={node.person.sim}>
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

/** A person of a run: who they are related to, and the tree around them. */
export function PersonPage({ run, sim }: { run: string; sim: string }) {
  const loaded = usePedigree(run);
  if (loaded.state === "loading") {
    return <p>{TEXT.loading}</p>;
  }
  if (loaded.state === "failed") {
    return <p>{`${TEXT.failed} ${loaded.message}`}</p>;
  }
  const glass = hourglass(loaded.pedigree, Number(sim));
  if (glass === null) {
    return <p>{TEXT.unknown_person}</p>;
  }
  const person = glass.center;
  const bySim = new Map(loaded.pedigree.persons.map((p) => [p.sim, p]));
  const parent = (id: number | null) => (id === null ? undefined : bySim.get(id));
  return (
    <section>
      <h1>{`${TEXT.person} ${person.sim}`}</h1>
      <p>{person.sex === "female" ? TEXT.female : TEXT.male}</p>
      <p>
        {TEXT.mother} <Name run={run} person={parent(person.mother)} /> · {TEXT.father}{" "}
        <Name run={run} person={parent(person.father)} />
      </p>
      <TreeView run={run} glass={glass} />
    </section>
  );
}
