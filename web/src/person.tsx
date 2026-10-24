import { useEffect, useRef, useState } from "preact/hooks";

import { getJson, postJson } from "./api.js";
import { added } from "./chronicle.js";
import { readable } from "./contract.js";
import { current, failure, type Loaded } from "./loaded.js";
import type { EventItem, Pedigree, Person, PersonAnswer, Shown } from "./generated/export.js";
import { CardView } from "./cardview.js";
import { agesOf, kinChoices, offerLabel } from "./choices.js";
import { kinshipText } from "./kin.js";
import { Chronicle } from "./lines.js";
import { fullName, namesOf } from "./names.js";
import { readLog } from "./log.js";
import { arrangement, candidates, pairingShown, type Pairing } from "./pairing.js";
import { browserStorage, pin, pinnedKey, unpin } from "./pin.js";
import { inputFailureText } from "./refusals.js";
import { Portrait } from "./portrait.js";
import { layout, MARK_DY, NAME_DY } from "./render/tree.js";
import { format } from "./router.js";
import { TEXT } from "./text.js";
import { currentPartner, hourglass, nodeMark, parentsOf, type Hourglass } from "./tree.js";

/** The format of `pedigree.json` this viewer reads (`simagent export`). */
const PEDIGREE_FORMAT = 5;

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
    getJson(`/api/runs/${run}/persons/${sim}`, { format: "simagent.view.person", version: 2 })
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
  return person === undefined ? <span>{TEXT.unknown}</span> : <a href={personHref(run, person.sim)}>{fullName(person)}</a>;
}

/** The hourglass as a drawing; each person a link that centres the tree on them (P11). */
export function TreeView({ run, glass }: { run: string; glass: Hourglass }) {
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
          aria-label={nodeMark(node.person).spoken}
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
          <text x={node.x} y={node.y} dy={NAME_DY} text-anchor="middle" font-size="11">
            {node.person.first_name}
          </text>
          {nodeMark(node.person).shown === null ? null : (
            <text x={node.x} y={node.y} dy={MARK_DY} text-anchor="middle" font-size="10">
              {nodeMark(node.person).shown}
            </text>
          )}
        </a>
      ))}
    </svg>
  );
}

/** The chronicle of one person: the entries of the log about them, newest first. */
function PersonChronicle({ run, sim, names }: { run: string; sim: string; names: ReadonlyMap<number, string> }) {
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
      names={names}
      failed={shown.state === "failed" ? TEXT[shown.text] : null}
      retry={() => setRetries((n) => n + 1)}
    />
  );
}

/** What another person of the run is to this one (ADR-0021 section 3): the core's kinship
 * word, foster word and `r`, for a person chosen from the pedigree. Only the answer for the
 * chosen person counts. */
function KinForm({
  run,
  sim,
  pedigree,
  ages,
}: {
  run: string;
  sim: string;
  pedigree: Pedigree;
  ages: ReadonlyMap<number, number>;
}) {
  const [other, setOther] = useState("");
  const key = `${run}/${sim}?to=${other}`;
  const [loaded, setLoaded] = useState<Loaded<PersonAnswer>>({ state: "loading", key });
  useEffect(() => {
    if (other === "") {
      return;
    }
    let live = true;
    setLoaded({ state: "loading", key });
    getJson(`/api/runs/${run}/persons/${sim}?to=${other}`, { format: "simagent.view.person", version: 2 })
      .then((json) => live && setLoaded({ state: "ready", key, value: json as unknown as PersonAnswer }))
      .catch((error: unknown) => live && setLoaded({ state: "failed", key, text: failure(error) }));
    return () => {
      live = false;
    };
  }, [key]);
  const shown = current(loaded, key);
  const kinship = shown.state === "ready" ? shown.value.kinship : undefined;
  const others = kinChoices(pedigree, ages, Number(sim));
  return (
    <>
      <h2>{TEXT.kin_with}</h2>
      <p>
        <select aria-label={TEXT.kin_with} value={other} onChange={(event) => setOther(event.currentTarget.value)}>
          <option value="">{TEXT.pair_with}</option>
          {others.map((choice) => (
            <option key={choice.sim} value={String(choice.sim)}>
              {choice.label}
            </option>
          ))}
        </select>{" "}
        {other === "" ? null : kinship !== undefined ? (
          <span role="status">{kinshipText(kinship)}</span>
        ) : shown.state === "failed" ? (
          <span role="alert">{TEXT[shown.text]}</span>
        ) : (
          <span>{TEXT.loading}</span>
        )}
      </p>
    </>
  );
}

const INPUT = { format: "simagent.view.input", version: 1 } as const;

/** Pairing a person with one of the partners the core offers (ADR-0021 section 6, "ein
 * Paar stiften"); the core judges age and kinship. */
function PairForm({
  run,
  pedigree,
  person,
  offered,
  ages,
  paired,
}: {
  run: string;
  pedigree: Pedigree;
  person: Person;
  offered: readonly number[];
  ages: ReadonlyMap<number, number>;
  paired: () => void;
}) {
  const options = candidates(pedigree, offered);
  const [chosen, setChosen] = useState("");
  const [pairing, setPairing] = useState<Pairing>({ state: "idle" });
  const failure = pairing.state === "failed" ? <p role="alert">{pairing.text}</p> : null;
  const shown = pairingShown(options.length, pairing.state);
  if (shown === "paired") {
    return <p role="status">{TEXT.paired}</p>;
  }
  if (shown === "nothing") {
    return failure;
  }
  const other = options.find((candidate) => String(candidate.sim) === chosen);
  const input = other === undefined ? null : arrangement(person, other);
  const send = () => {
    if (input === null) {
      return;
    }
    setPairing({ state: "busy" });
    postJson(`/api/runs/${run}/input`, input, INPUT)
      .then(() => {
        setPairing({ state: "paired" });
        paired();
      })
      .catch((error: unknown) => setPairing({ state: "failed", text: inputFailureText(error) }));
  };
  return (
    <>
      <h2>{TEXT.pair}</h2>
      <p>
        <select aria-label={TEXT.pair_with} value={chosen} onChange={(event) => setChosen(event.currentTarget.value)}>
          <option value="">{TEXT.pair_with}</option>
          {options.map((candidate) => (
            <option key={candidate.sim} value={String(candidate.sim)}>
              {offerLabel(candidate, ages)}
            </option>
          ))}
        </select>{" "}
        <button type="button" disabled={input === null || pairing.state === "busy"} onClick={send}>
          {TEXT.pair}
        </button>
      </p>
      {failure}
    </>
  );
}

/** A person of a run: who they are related to, the tree around them, pairing, and their
 * chronicle. */
/** Pinning this person's family to the run page, or taking it off again (ADR-0021 section 6,
 * Grok A1, A2); without a storage in the browser there is no button. */
function PinButton({ run, person }: { run: string; person: Person }) {
  const storage = browserStorage();
  const [pinned, setPinned] = useState(() => pinnedKey(storage, run));
  if (storage === null) {
    return null;
  }
  const here = pinned === person.lineage_key;
  const toggle = () => {
    if (here) {
      unpin(storage, run);
    } else {
      pin(storage, run, person.lineage_key);
    }
    setPinned(pinnedKey(storage, run));
  };
  return (
    <p>
      <button type="button" onClick={toggle}>
        {here ? TEXT.unpin_family : TEXT.pin_family}
      </button>
    </p>
  );
}

export function PersonPage({ run, sim }: { run: string; sim: string }) {
  const [changed, setChanged] = useState(0);
  const loaded = usePedigree(run, String(changed));
  const view = usePersonView(run, sim);
  // The last pedigree of this run stays while a newer one loads after a change, so the
  // page (and what pairing reports) does not go away in between.
  const last = useRef<{ run: string; pedigree: Pedigree } | null>(null);
  if (loaded.state === "ready") {
    last.current = { run, pedigree: loaded.value };
  }
  const kept = last.current?.run === run ? last.current.pedigree : null;
  const pedigree = loaded.state === "ready" ? loaded.value : kept;
  // A person page that cannot be shown still leads to its run.
  const toRun = (text: string) => (
    <p>
      {text} <a href={format({ page: "run", run })}>{TEXT.to_run.replace("{run}", run)}</a>
    </p>
  );
  if (pedigree === null) {
    return toRun(loaded.state === "failed" ? TEXT[loaded.text] : TEXT.loading);
  }
  // A reload that failed leaves the last pedigree on the page, with why.
  const reloadFailed = loaded.state === "failed" ? <p role="alert">{TEXT[loaded.text]}</p> : null;
  const glass = hourglass(pedigree, Number(sim));
  if (glass === null) {
    return toRun(TEXT.unknown_person);
  }
  const person = glass.center;
  const [mother, father] = parentsOf(pedigree, person);
  const partner = currentPartner(pedigree, person);
  const ages = agesOf(view.state === "ready" ? view.value.ages : []);
  return (
    <section>
      <p>
        <a href={format({ page: "run", run })}>{TEXT.to_run.replace("{run}", run)}</a>
      </p>
      <h1>{fullName(person)}</h1>
      {reloadFailed}
      <p>{person.sex === "female" ? TEXT.female : TEXT.male}</p>
      {view.state === "ready" && !readable(view.value.contract_version) ? (
        <p>{TEXT.failed_contract}</p>
      ) : view.state === "ready" ? (
        <div>
          <Portrait params={view.value.shown.body} label={fullName(person)} />
          <Life view={view.value} />
          <Ages shown={view.value.at_ages} />
          <CardView
            card={view.value.card}
            sex={person.sex}
            nowCm={view.value.shown.body.height_cm}
            names={namesOf(pedigree)}
          />
        </div>
      ) : view.state === "failed" ? (
        <p>{TEXT[view.text]}</p>
      ) : (
        <p>{TEXT.loading}</p>
      )}
      <p>
        {TEXT.mother} <Name run={run} person={mother} /> · {TEXT.father}{" "}
        <Name run={run} person={father} />
      </p>
      {partner === undefined ? null : (
        <p>
          {partner.sex === "female" ? TEXT.partner_female : TEXT.partner_male} <Name run={run} person={partner} />
        </p>
      )}
      <TreeView run={run} glass={glass} />
      <PinButton key={`${run}/${sim}`} run={run} person={person} />
      <KinForm key={`${run}/${sim}`} run={run} sim={sim} pedigree={pedigree} ages={ages} />
      <PairForm
        key={`${run}/${sim}`}
        run={run}
        pedigree={pedigree}
        person={person}
        offered={view.state === "ready" ? view.value.partner_candidates : []}
        ages={ages}
        paired={() => setChanged((n) => n + 1)}
      />
      <PersonChronicle key={changed} run={run} sim={sim} names={namesOf(pedigree)} />
    </section>
  );
}
