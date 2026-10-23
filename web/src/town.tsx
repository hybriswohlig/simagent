import { useEffect, useRef, useState } from "preact/hooks";

import { postJson } from "./api.js";
import { added } from "./chronicle.js";
import { answered, pause, play, press, refused, standing, tick, type Clock, type Next, type Step } from "./clock.js";
import { CurvesView } from "./curvesview.js";
import { ForkForm, type Writes } from "./forkform.js";
import type { AdvanceAnswer, CalendarDate, EventItem, InputAnswer, Pedigree, Person } from "./generated/export.js";
import { fullName, namesOf } from "./names.js";
import { dateText } from "./dates.js";
import { Chronicle } from "./lines.js";
import { enteredBy, windows } from "./log.js";
import { TreeView, usePedigree } from "./person.js";
import { browserStorage, pinnedKey } from "./pin.js";
import { hourglass } from "./tree.js";
import { format } from "./router.js";
import { useRuns } from "./runs.js";
import { failureCode, inputFailureText, refusalText } from "./refusals.js";
import { HUNGERS, diseasesOn, shockInput, switchInput, switchLabel } from "./shocks.js";
import { TEXT } from "./text.js";

const ADVANCED = { format: "simagent.view.advanced", version: 1 } as const;
const INPUT = { format: "simagent.view.input", version: 1 } as const;

/** The pace of playing: how long each step stands before the next is asked for. */
const PACE_MS = 800;


const STEPS: readonly (readonly [Step, keyof typeof TEXT])[] = [
  ["days28", "step_days28"],
  ["year", "step_year"],
  ["decade", "step_decade"],
];

/** The clock of a run and its living, moved by the server one step at a time. */
/** Opening the comparison of this run with another of the list (Grok W5). */
function CompareForm({ run, others }: { run: string; others: readonly string[] }) {
  const [other, setOther] = useState("");
  if (others.length === 0) {
    return null;
  }
  return (
    <>
      <h2>{TEXT.compare}</h2>
      <p>
        <label>
          {TEXT.compare_with}{" "}
          <select value={other} onChange={(event) => setOther(event.currentTarget.value)}>
            <option value="" />
            {others.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        </label>{" "}
        <button
          type="button"
          disabled={other === ""}
          onClick={() => {
            location.hash = format({ page: "diff", a: run, b: other });
          }}
        >
          {TEXT.compare_open}
        </button>
      </p>
    </>
  );
}

function Town({
  run,
  closed,
  date,
  others,
}: {
  run: string;
  closed: number;
  date: CalendarDate;
  others: readonly string[];
}) {
  const [clock, setClock] = useState<Clock>(() => standing(closed, date));
  const [chosen, setChosen] = useState<Step>("days28");
  const [listedAt, setListedAt] = useState(closed);
  const [living, setLiving] = useState<readonly Person[]>([]);
  const [names, setNames] = useState<ReadonlyMap<number, string>>(new Map());
  const [kept, setKept] = useState<Pedigree | null>(null);
  const [chronicle, setChronicle] = useState<readonly EventItem[]>([]);
  // A window of the log, and the entries of an input, that could not be read.
  const [windowFailed, setWindowFailed] = useState(false);
  const [enteredFailed, setEnteredFailed] = useState(false);
  const [retries, setRetries] = useState(0);
  const [diseases, setDiseases] = useState<boolean | null>(null);
  const [sending, setSending] = useState(false);
  const [inputRefused, setInputRefused] = useState<string | null>(null);
  const [forking, setForking] = useState(false);
  const read = useRef(windows(run, closed));
  const latest = useRef(clock);
  const live = useRef(true);
  // An input on its way (the addendum to ADR-0023 before S2): no step starts and no other
  // input goes until it is answered, so it acts at the stop the page shows.
  const writing = useRef(false);
  // The stamp of an accepted input whose entries could not be read yet; read again with the
  // chronicle (a window past the stop reads them too).
  const unread = useRef<number | null>(null);
  // The last read of the switch asked for: only its answer counts.
  const switchReads = useRef(0);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  const apply = ([next, step]: Next) => {
    if (!live.current) {
      return;
    }
    latest.current = next;
    setClock(next);
    if (step === null) {
      return;
    }
    postJson(`/api/runs/${run}/advance`, { step }, ADVANCED)
      .then((json) => {
        const answer = json as unknown as AdvanceAnswer;
        apply(answered(latest.current, answer.closed_through, answer.date));
      })
      .catch((error: unknown) => {
        apply(refused(latest.current, failureCode(error)));
      });
  };
  useEffect(() => {
    if (!clock.busy) {
      setListedAt(clock.closed);
    }
    if (clock.busy || clock.playing === null) {
      return;
    }
    const timer = setTimeout(() => apply(tick(latest.current)), PACE_MS);
    return () => clearTimeout(timer);
  }, [clock]);
  // The entries of the input stamped `stamp` into the chronicle; kept to read again if they
  // do not come.
  // (Every input of one stop acts in the same second, so a read of it reads them all.)
  const enteredAt = (stamp: number) =>
    enteredBy(run, stamp).then(
      (items) => {
        if (unread.current === stamp) {
          unread.current = null;
        }
        if (live.current) {
          setChronicle((kept) => added(kept, items));
          setEnteredFailed(unread.current !== null);
        }
      },
      () => {
        unread.current = stamp;
        if (live.current) {
          setEnteredFailed(true);
        }
      },
    );
  // Where the switch stands, by the last read asked for; unknown, and so without its button,
  // while it cannot be read.
  const readSwitch = () => {
    const asked = ++switchReads.current;
    const last = () => live.current && asked === switchReads.current;
    return diseasesOn(run).then(
      (on) => last() && setDiseases(on),
      () => last() && setDiseases(null),
    );
  };
  // After each step (and when asked again after a failure), the log from the last stop that
  // was read on; the entries of an input that did not come are read on their own while no
  // window reaches past its stop.
  useEffect(() => {
    const until = clock.closed;
    const stamp = unread.current;
    if (stamp !== null && until <= stamp) {
      void enteredAt(stamp);
    }
    read
      .current(until)
      .then((items) => {
        if (!live.current) {
          return;
        }
        setChronicle((kept) => added(kept, items));
        setWindowFailed(false);
        if (unread.current !== null && until > unread.current) {
          unread.current = null;
          setEnteredFailed(false);
        }
      })
      .catch(() => live.current && setWindowFailed(true));
  }, [clock.closed, retries]);
  // Where the switch stands, read when the page opens, after each step and when asked again;
  // a refused input is no longer news then.
  useEffect(() => {
    setInputRefused(null);
    void readSwitch();
  }, [listedAt, retries]);
  // An input at the standing clock (ADR-0023 section 6 and its addendum before S2): its
  // entries in the chronicle at once and the switch read again, each on its own.
  const send = (input: unknown) => {
    if (writing.current || latest.current.busy || latest.current.playing !== null) {
      return;
    }
    writing.current = true;
    setSending(true);
    setInputRefused(null);
    postJson(`/api/runs/${run}/input`, input, INPUT)
      .then(
        (json) => {
          const answer = json as unknown as InputAnswer;
          return Promise.all([enteredAt(answer.t), readSwitch()]);
        },
        (error: unknown) => live.current && setInputRefused(inputFailureText(error)),
      )
      .finally(() => {
        writing.current = false;
        if (live.current) {
          setSending(false);
        }
      });
  };
  // The living of the last pedigree that came, kept while the next one loads.
  const pedigree = usePedigree(run, String(listedAt));
  useEffect(() => {
    if (pedigree.state === "ready") {
      setLiving(pedigree.value.persons.filter((person) => person.died === null && person.emigrated === null));
      setNames(namesOf(pedigree.value));
      setKept(pedigree.value);
    }
  }, [pedigree]);
  // The interventions wait for a standing clock: no step on its way, not playing.
  const standingStill = clock.busy || clock.playing !== null || sending || forking;
  // A fork takes the run's writes as an input does (the addendum G2 of ADR-0021 section 2):
  // it starts only at a standing clock, and no step, play or input goes while it is computed.
  const writes: Writes = {
    idle: !standingStill,
    begin: () => {
      if (writing.current || latest.current.busy || latest.current.playing !== null) {
        return false;
      }
      writing.current = true;
      setForking(true);
      return true;
    },
    end: () => {
      writing.current = false;
      if (live.current) {
        setForking(false);
      }
    },
  };
  return (
    <section>
      <p>
        <a href={format({ page: "runs" })}>{TEXT.back}</a>
      </p>
      <h1>{`${TEXT.run} ${run}`}</h1>
      <p>{dateText(clock.date)}</p>
      <p role="group" aria-label={TEXT.steps}>
        {STEPS.map(([step, text]) => (
          <button
            type="button"
            key={step}
            disabled={clock.busy || sending || forking}
            onClick={() => {
              if (writing.current) {
                return;
              }
              setChosen(step);
              apply(press(latest.current, step));
            }}
          >
            {TEXT[text]}
          </button>
        ))}
        {clock.playing === null ? (
          <button type="button" disabled={sending || forking} onClick={() => writing.current || apply(play(latest.current, chosen))}>
            {TEXT.play}
          </button>
        ) : (
          <button type="button" onClick={() => apply(pause(latest.current))}>
            {TEXT.pause}
          </button>
        )}
      </p>
      <p role="group" aria-label={TEXT.interventions}>
        {HUNGERS.map(([shock, label]) => (
          <button type="button" key={shock} disabled={standingStill} onClick={() => send(shockInput(shock))}>
            {label}
          </button>
        ))}
        {diseases === null ? null : (
          <button type="button" disabled={standingStill} onClick={() => send(switchInput(diseases))}>
            {switchLabel(diseases)}
          </button>
        )}
      </p>
      {clock.refused === null ? null : <p role="alert">{refusalText(clock.refused)}</p>}
      {inputRefused === null ? null : <p role="alert">{inputRefused}</p>}
      <Chronicle
        run={run}
        items={chronicle}
        failed={windowFailed || enteredFailed ? TEXT.chronicle_failed : null}
        retry={() => setRetries((n) => n + 1)}
        names={names}
      />
      <CurvesView run={run} at={String(listedAt)} />
      <PinnedFamily run={run} pedigree={kept} />
      <h2>{TEXT.living}</h2>
      {pedigree.state === "failed" ? <p>{TEXT[pedigree.text]}</p> : null}
      <ul>
        {living.map((person) => (
          <li key={person.sim}>
            <a href={format({ page: "person", run, sim: String(person.sim) })}>
              {fullName(person)}
            </a>
            {` · ${person.sex === "female" ? TEXT.female : TEXT.male}`}
          </li>
        ))}
      </ul>
      <ForkForm run={run} year={clock.date.year} writes={writes} />
      <CompareForm run={run} others={others} />
    </section>
  );
}

/** The family pinned to this run page (ADR-0021 section 6, Grok A3, A4): the hourglass of its
 * person in the last pedigree that came, or why it is not there. */
function PinnedFamily({ run, pedigree }: { run: string; pedigree: Pedigree | null }) {
  const key = pinnedKey(browserStorage(), run);
  if (key === null || pedigree === null) {
    return null;
  }
  const person = pedigree.persons.find((each) => each.lineage_key === key);
  const glass = person === undefined ? null : hourglass(pedigree, person.sim);
  if (glass === null) {
    return <p>{TEXT.pinned_missing}</p>;
  }
  return (
    <>
      <h2>{TEXT.pinned_family.replace("{name}", fullName(glass.center))}</h2>
      <TreeView run={run} glass={glass} />
    </>
  );
}

/** A run's page: its clock and its living, from where the run list says it stands. */
export function RunPage({ run }: { run: string }) {
  const runs = useRuns(run);
  // A run page that cannot be shown still leads back to the list.
  const back = (text: string) => (
    <p>
      {text} <a href={format({ page: "runs" })}>{TEXT.back}</a>
    </p>
  );
  if (runs.state !== "ready") {
    return back(runs.state === "loading" ? TEXT.loading : TEXT[runs.text]);
  }
  const entry = runs.value.runs.find((candidate) => candidate.run === run);
  if (entry === undefined) {
    return back(TEXT.not_found);
  }
  if (entry.manifest === undefined || entry.date === undefined) {
    return back(entry.error === "run_failed" ? TEXT.refused_run_failed : TEXT.run_no_manifest);
  }
  const others = runs.value.runs
    .filter((candidate) => candidate.run !== run && candidate.manifest !== undefined)
    .map((candidate) => candidate.run);
  return <Town key={run} run={run} closed={entry.manifest.closed_through} date={entry.date} others={others} />;
}
