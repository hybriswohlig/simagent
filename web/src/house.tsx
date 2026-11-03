import { useEffect, useRef, useState } from "preact/hooks";

import { getJsonOrRefusal, postJson } from "./api.js";
import { answered, pause, play, playText, press, refused, standing, tick, type Clock, type Next, type Step } from "./clock.js";
import { dateText } from "./dates.js";
import type { AdvanceAnswer, HouseAnswer, HouseLayout, HousePerson, Pedigree, PeopleAnswer } from "./generated/export.js";
import { current, failure, type Loaded } from "./loaded.js";
import { fullName } from "./names.js";
import { usePedigree } from "./person.js";
import { usePeople } from "./peopleview.js";
import { Portrait } from "./portrait.js";
import { failureCode, inputFailureText, refusalText } from "./refusals.js";
import { plan, spots, timeOfDay } from "./render/plan.js";
import { format } from "./router.js";
import { KIND_WORDS, OFFER_WORDS, ROOM_WORDS, TEXT } from "./text.js";

const HOUSE = { format: "simagent.view.house", version: 1 } as const;
const ADVANCED = { format: "simagent.view.advanced", version: 1 } as const;
const INPUT = { format: "simagent.view.input", version: 1 } as const;
/** The pace of playing: how long each step stands before the next is asked for. */
const PACE_MS = 800;

const STEPS: readonly (readonly [Step, keyof typeof TEXT])[] = [
  ["minutes10", "step_minutes10"],
  ["hour", "step_hour"],
  ["morning", "step_morning"],
];

const NEEDS: readonly (keyof typeof TEXT)[] = ["need_hunger", "need_energy", "need_bladder", "need_hygiene", "need_fun"];

/** A household's house from the server (ADR-0021, addendum of 2026-11-03); asked again for
 * each `at`. A household without a house is `null`. */
function useHouse(run: string, household: string, at: string): Loaded<HouseAnswer | null> {
  const key = `${run}/${household}@${at}`;
  const [loaded, setLoaded] = useState<Loaded<HouseAnswer | null>>({ state: "loading", key });
  useEffect(() => {
    let live = true;
    setLoaded({ state: "loading", key });
    getJsonOrRefusal(`/api/runs/${run}/house/${household}`, HOUSE)
      .then((json) => live && setLoaded({ state: "ready", key, value: json as unknown as HouseAnswer }))
      .catch((error: unknown) => {
        if (!live) {
          return;
        }
        if (failureCode(error) === "not_in_house") {
          setLoaded({ state: "ready", key, value: null });
        } else {
          setLoaded({ state: "failed", key, text: failure(error) });
        }
      });
    return () => {
      live = false;
    };
  }, [key]);
  return current(loaded, key);
}

/** The words of a room, its name and "into" it. */
function roomWords(layout: HouseLayout, index: number): readonly [string, string] {
  const id = layout.rooms[index]?.id ?? "";
  return ROOM_WORDS[id] ?? [id, id];
}

/** What a person does as one line: on the way where to, at an object what and until when,
 * or nothing. */
function doing(layout: HouseLayout, person: HousePerson): string {
  const walking = person.place.walking;
  if (walking !== undefined) {
    return TEXT.house_walks.replace("{where}", roomWords(layout, walking.to)[1]);
  }
  const action = person.action;
  if (action === null) {
    return TEXT.house_idle;
  }
  const object = layout.objects[action.object];
  const offer = object?.offers[action.offer] ?? "";
  const kind = KIND_WORDS[object?.kind ?? ""];
  const does = TEXT.house_does
    .replace("{offer}", OFFER_WORDS[offer] ?? offer)
    .replace("{at}", kind?.[1] ?? object?.kind ?? "");
  const until = action.until === null ? "" : ` ${TEXT.house_until.replace("{time}", timeOfDay(action.until.date.second_of_day))}`;
  return `${does}${until}`;
}

/** The floor plan from above, each floor beside the other, with the people where they are. */
function Plan({ house, names }: { house: HouseAnswer; names: ReadonlyMap<number, string> }) {
  const { layout } = house;
  const drawn = plan(layout);
  const placed = spots(layout, house.persons, house.at.t);
  const pad = 0.6;
  // Room on the right for the names of objects at the wall.
  const view = `${-pad} ${-pad - 0.8} ${drawn.width + 2 * pad + 1.2} ${drawn.height + 2 * pad + 0.8}`;
  return (
    <svg viewBox={view} role="img" aria-label={TEXT.house_people} style={{ width: "100%", maxWidth: "60em" }}>
      {drawn.offsets.map((left, floor) => (
        <text key={`floor-${floor}`} x={left} y={-0.4} font-size={0.45} fill="#333">
          {floor === 0 ? TEXT.house_floor_0 : TEXT.house_floor_1}
        </text>
      ))}
      {layout.rooms.map((room) => {
        const left = drawn.offsets[room.floor] ?? 0;
        return (
          <rect key={room.id} x={left + room.x} y={room.y} width={room.w} height={room.h} fill="#f7f5ef" stroke="#444" stroke-width={0.06} />
        );
      })}
      {layout.objects.map((object, index) => {
        const room = layout.rooms.find((room) => room.id === object.room);
        const left = drawn.offsets[room?.floor ?? 0] ?? 0;
        return (
          <g key={`object-${index}`}>
            <rect x={left + object.x} y={object.y} width={object.w} height={object.h} fill="#d9d4c7" stroke="#8a8270" stroke-width={0.03} />
            <text x={left + object.x + 0.05} y={object.y + object.h + 0.28} font-size={0.24} fill="#6b6455">
              {KIND_WORDS[object.kind]?.[0] ?? object.kind}
            </text>
          </g>
        );
      })}
      {layout.rooms.map((room) => {
        const left = drawn.offsets[room.floor] ?? 0;
        return (
          <text
            key={`name-${room.id}`}
            x={left + room.x + room.w / 2}
            y={room.y + room.h - 0.2}
            text-anchor="middle"
            font-size={0.34}
            fill="#555"
            stroke="#f7f5ef"
            stroke-width={0.12}
            paint-order="stroke"
          >
            {(ROOM_WORDS[room.id] ?? [room.id])[0]}
          </text>
        );
      })}
      {house.persons.map((person, index) => {
        const spot = placed[index] ?? { x: 0, y: 0 };
        const name = names.get(person.sim) ?? String(person.sim);
        return (
          <g key={`person-${person.sim}`}>
            <circle cx={spot.x} cy={spot.y} r={0.3} fill={person.action === null ? "#8a6d3b" : "#2f6f73"} stroke="#fff" stroke-width={0.05} />
            <text x={spot.x} y={spot.y + 0.11} text-anchor="middle" font-size={0.32} fill="#fff">
              {name.slice(0, 1)}
            </text>
            <title>{`${name}: ${doing(layout, person)}`}</title>
          </g>
        );
      })}
    </svg>
  );
}

/** A person's portrait, name, what it does and its five needs as bars. */
function Resident({
  run,
  house,
  person,
  pedigree,
  people,
}: {
  run: string;
  house: HouseAnswer;
  person: HousePerson;
  pedigree: Pedigree;
  people: PeopleAnswer | null;
}) {
  const known = pedigree.persons.find((p) => p.sim === person.sim);
  const name = known === undefined ? String(person.sim) : fullName(known);
  const shown = people?.people.find((p) => p.sim === person.sim)?.shown;
  return (
    <li style={{ display: "flex", gap: "1em", alignItems: "flex-start", listStyle: "none", marginBottom: "1em" }}>
      {shown !== undefined && known !== undefined && (
        <Portrait params={shown.body} sex={known.sex} label={name} width={64} />
      )}
      <div>
        <p>
          <a href={format({ page: "person", run, sim: String(person.sim) })}>{name}</a>
          {": "}
          {doing(house.layout, person)}
        </p>
        <dl style={{ margin: 0 }}>
          {NEEDS.map((need, index) => {
            const value = person.needs[index] ?? 0;
            return (
              <div key={need} style={{ display: "flex", gap: "0.5em" }}>
                <dt style={{ width: "5em" }}>{TEXT[need]}</dt>
                <dd style={{ margin: 0 }}>
                  <meter min={0} max={1000} low={150} high={400} optimum={1000} value={value} />{" "}
                  {value}
                </dd>
              </div>
            );
          })}
        </dl>
      </div>
    </li>
  );
}

/** The house of a household (ADR-0021, addendum of 2026-11-03): its plan with the people
 * where they are, the clock in steps of minutes and hours, each person with what it does
 * and its needs. */
export function HousePage({ run, household }: { run: string; household: string }) {
  const [at, setAt] = useState(0);
  const loaded = useHouse(run, household, String(at));
  const pedigree = usePedigree(run, String(at));
  const people = usePeople(run, String(at));
  const [clock, setClock] = useState<Clock | null>(null);
  const latest = useRef<Clock | null>(null);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  const house = loaded.state === "ready" ? loaded.value : null;
  useEffect(() => {
    if (house !== null && latest.current === null) {
      const start = standing(house.at.t, house.at.date);
      latest.current = start;
      setClock(start);
    }
  }, [house]);
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
        if (latest.current !== null) {
          apply(answered(latest.current, answer.closed_through, answer.date));
        }
        setAt((count) => count + 1);
      })
      .catch((error: unknown) => {
        if (latest.current !== null) {
          apply(refused(latest.current, failureCode(error)));
        }
      });
  };
  useEffect(() => {
    if (clock === null || clock.busy || clock.playing === null) {
      return;
    }
    const timer = setTimeout(() => latest.current !== null && apply(tick(latest.current)), PACE_MS);
    return () => clearTimeout(timer);
  }, [clock]);
  if (loaded.state === "loading") {
    return <p>{TEXT.loading}</p>;
  }
  if (loaded.state === "failed") {
    return <p role="alert">{TEXT[loaded.text]}</p>;
  }
  const back = <a href={format({ page: "run", run })}>{TEXT.to_run.replace("{run}", run)}</a>;
  if (house === null) {
    return (
      <p>
        {TEXT.house_none} {back}
      </p>
    );
  }
  const names = new Map<number, string>(
    pedigree.state === "ready" ? pedigree.value.persons.map((p) => [p.sim, fullName(p)]) : [],
  );
  return (
    <section>
      <h1>{TEXT.house_of_household.replace("{n}", household)}</h1>
      <p>
        {back} · {TEXT.house_now} {dateText(house.at.date)}, {timeOfDay(house.at.date.second_of_day)}
      </p>
      {clock !== null && (
        <div role="group" aria-label={TEXT.steps}>
          {STEPS.map(([step, label]) => (
            <button key={step} type="button" disabled={clock.busy} onClick={() => apply(press(clock, step))}>
              {TEXT[label]}
            </button>
          ))}{" "}
          {clock.playing === null ? (
            <button type="button" onClick={() => apply(play(clock, "minutes10"))}>
              {playText("minutes10")}
            </button>
          ) : (
            <button type="button" onClick={() => apply(pause(clock))}>
              {TEXT.pause}
            </button>
          )}
          {clock.refused !== null && <span role="status"> {refusalText(clock.refused)}</span>}
        </div>
      )}
      {!house.morning && <p>{TEXT.house_before_morning}</p>}
      <Plan house={house} names={names} />
      {pedigree.state === "ready" && house.persons.length > 0 && (
        <>
          <h2>{TEXT.house_people}</h2>
          <ul style={{ padding: 0 }}>
            {house.persons.map((person) => (
              <Resident
                key={person.sim}
                run={run}
                house={house}
                person={person}
                pedigree={pedigree.value}
                people={people.state === "ready" ? people.value : null}
              />
            ))}
          </ul>
          <p>{TEXT.house_young}</p>
        </>
      )}
    </section>
  );
}

/** On a person's page, the way to its household's house: there if it has one, else "Ins
 * Haus", which moves the household in (an input, ADR-0024) and opens the house. */
export function HouseLink({ run, household }: { run: string; household: number }) {
  const [state, setState] = useState<"asking" | "in" | "out" | "moving">("asking");
  const [refused, setRefused] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setState("asking");
    setRefused(null);
    getJsonOrRefusal(`/api/runs/${run}/house/${household}`, HOUSE)
      .then(() => live && setState("in"))
      .catch((error: unknown) => live && failureCode(error) === "not_in_house" && setState("out"));
    return () => {
      live = false;
    };
  }, [run, household]);
  const to = format({ page: "house", run, household: String(household) });
  if (state === "asking") {
    return null;
  }
  if (state === "in") {
    return (
      <p>
        <a href={to}>{TEXT.house_to_house}</a>
      </p>
    );
  }
  const moveIn = () => {
    setState("moving");
    setRefused(null);
    postJson(`/api/runs/${run}/input`, { move_in: { household } }, INPUT)
      .then(() => {
        location.hash = to;
      })
      .catch((error: unknown) => {
        setState("out");
        setRefused(inputFailureText(error));
      });
  };
  return (
    <p>
      <button type="button" disabled={state === "moving"} onClick={moveIn}>
        {state === "moving" ? TEXT.house_moving_in : TEXT.house_move_in}
      </button>
      {refused !== null && <span role="status"> {refused}</span>}
    </p>
  );
}
