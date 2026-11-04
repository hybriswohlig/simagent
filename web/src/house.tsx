import { useEffect, useRef, useState } from "preact/hooks";

import { getJson, getJsonOrRefusal, postJson } from "./api.js";
import { answered, pause, play, playText, press, refused, standing, tick, type Clock, type Next, type Step } from "./clock.js";
import { commandChoices, offerWords, type CommandChoice } from "./command.js";
import { dateText } from "./dates.js";
import type { AdvanceAnswer, EventItem, EventsAnswer, HouseAnswer, HouseChild, HouseLayout, HousePerson, HousesAnswer, Pedigree, PeopleAnswer, WhyAnswer } from "./generated/export.js";
import { current, failure, type Loaded } from "./loaded.js";
import { fullName } from "./names.js";
import { usePedigree } from "./person.js";
import { usePeople } from "./peopleview.js";
import { Portrait } from "./portrait.js";
import { failureCode, inputFailureText, refusalText } from "./refusals.js";
import { plan, spots, timeOfDay, wholeMinutes } from "./render/plan.js";
import { format } from "./router.js";
import { readable } from "./contract.js";
import { CODES, KIND_WORDS, OFFER_WORDS, ROOM_WORDS, TEXT } from "./text.js";

const HOUSE = { format: "simagent.view.house", version: 4 } as const;
const ADVANCED = { format: "simagent.view.advanced", version: 1 } as const;
const INPUT = { format: "simagent.view.input", version: 1 } as const;
const EVENTS = { format: "simagent.view.events", version: 2 } as const;
const HOUSES = { format: "simagent.view.houses", version: 1 } as const;
/** How many pages of a person's day the page reads at most (a day has some hundred entries). */
const DAY_PAGES = 10;
/** The most members a household with a house has (ADR-0025 section 3). */
const LARGEST = 8;
const WHY = { format: "simagent.view.why", version: 2 } as const;
const NEED_KEYS: { readonly [need: string]: keyof typeof TEXT } = {
  hunger: "need_hunger",
  energy: "need_energy",
  bladder: "need_bladder",
  hygiene: "need_hygiene",
  fun: "need_fun",
};
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

/** The words of a room, its name, "into" it and "in" it. */
function roomWords(layout: HouseLayout, index: number): readonly [string, string, string] {
  const id = layout.rooms[index]?.id ?? "";
  return ROOM_WORDS[id] ?? [id, id, id];
}

/** Where a child under 6 is as one line, all from the server (ADR-0024 section 4): at the
 * kindergarten until when, asleep in its room, or in its room. */
function youngLine(layout: HouseLayout, child: HouseChild): string {
  const away = child.place.away;
  if (away !== undefined) {
    return TEXT.house_young_kita.replace("{time}", timeOfDay(away.until.date.second_of_day));
  }
  const room = roomWords(layout, child.place.room ?? 0)[2];
  return child.place.asleep === true ? TEXT.house_young_asleep.replace("{room}", room) : room;
}

/** What a person does as one line: away where and until when, on the way where to (or out
 * of the house), lying on the floor of its room, at an object what and until when, or
 * nothing. */
function doing(layout: HouseLayout, person: HousePerson): string {
  if (person.collapsed) {
    return TEXT.house_lies.replace("{room}", roomWords(layout, person.place.room ?? 0)[2]);
  }
  const away = person.place.away;
  if (away !== undefined) {
    const text = away.place === "school" ? TEXT.house_away_school : TEXT.house_away_work;
    return text.replace("{time}", timeOfDay(away.until.date.second_of_day));
  }
  const walking = person.place.walking;
  if (walking !== undefined) {
    if (person.action === null) {
      return TEXT.house_leaves;
    }
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

/** The line of a house entry of the day, or null for one the day leaves out (an arrival). */
function dayLine(layout: HouseLayout, item: EventItem): { text: string; choice: boolean } | null {
  const [code, body] = Object.entries(item.entry.payload)[0] ?? ["", {}];
  const fields = body as { [key: string]: unknown };
  const number = (key: string) => (typeof fields[key] === "number" ? (fields[key] as number) : 0);
  const school = fields["place"] === "school";
  switch (code) {
    case "core:action_started": {
      // A commanded start says so (ADR-0024 section 5); "Warum?" tells the points beside it.
      const said = fields["commanded"] === true ? TEXT.house_commanded : TEXT.house_choice;
      return { text: said.replace("{what}", offerWords(layout, number("object"), number("offer"))), choice: true };
    }
    case "core:action_ended": {
      const codes: Record<string, string> = CODES;
      const reason = codes[`end_reason ${String(fields["reason"])}`] ?? String(fields["reason"]);
      return { text: TEXT.house_end.replace("{why}", reason), choice: false };
    }
    case "core:idle": {
      const waits = fields["waits"];
      if (typeof waits === "number") {
        const kind = KIND_WORDS[layout.objects[waits]?.kind ?? ""];
        return { text: TEXT.house_waits.replace("{object}", kind?.[0] ?? String(waits)), choice: true };
      }
      return { text: TEXT.house_idle, choice: true };
    }
    case "core:need_critical": {
      const need = NEED_KEYS[String(fields["need"])];
      return {
        text: TEXT.house_critical.replace("{need}", need === undefined ? String(fields["need"]) : TEXT[need]).replace("{level}", String(number("level"))),
        choice: false,
      };
    }
    // The day's duty (ADR-0024 section 4): where, when; leaving, late; back; missed.
    case "core:day_planned":
      return { text: school ? TEXT.house_planned_school : TEXT.house_planned_work, choice: false };
    case "core:left_home": {
      const minutes = wholeMinutes(number("late_secs"));
      const late = minutes > 0 ? ` ${TEXT.house_late.replace("{minutes}", String(minutes))}` : "";
      return { text: `${school ? TEXT.house_left_school : TEXT.house_left_work}${late}`, choice: false };
    }
    case "core:came_home":
      return { text: school ? TEXT.house_came_school : TEXT.house_came_work, choice: false };
    case "core:missed":
      return { text: school ? TEXT.house_missed_school : TEXT.house_missed_work, choice: false };
    // A need at 0 and what it brings (ADR-0026, addendum of 2026-11-04 on a need at 0).
    case "core:need_empty": {
      const need = NEED_KEYS[String(fields["need"])];
      return { text: TEXT.house_empty.replace("{need}", need === undefined ? String(fields["need"]) : TEXT[need]), choice: false };
    }
    case "core:collapsed":
      return { text: TEXT.house_collapsed, choice: false };
    case "core:accident":
      return { text: TEXT.house_accident, choice: false };
    default:
      return null;
  }
}

/** "Warum?" for a choice: what it chose with its points and what came behind, from the
 * server (ADR-0026 section 6); in the night that the bed comes first, and by day the beds
 * closed until the night (ADR-0026, addendum of 2026-11-04). */
function Why({ run, layout, entry }: { run: string; layout: HouseLayout; entry: number }) {
  const [why, setWhy] = useState<WhyAnswer | null | "failed">(null);
  const ask = () => {
    getJson(`/api/runs/${run}/why/${entry}`, WHY)
      .then((json) => setWhy(json as unknown as WhyAnswer))
      .catch(() => setWhy("failed"));
  };
  if (why === null) {
    return (
      <button type="button" onClick={ask}>
        {TEXT.house_why}
      </button>
    );
  }
  if (why === "failed") {
    return <span>{TEXT.failed_network}</span>;
  }
  const points = (n: number) => n.toLocaleString("de-DE");
  const chosen = why.chosen;
  const opens = why.night === null ? "" : timeOfDay(why.night.from.date.second_of_day);
  const behind = why.offers
    .filter((offer) => chosen === null || offer.object !== chosen.object || offer.offer !== chosen.offer)
    .slice(0, chosen === null ? 4 : 3)
    .map((offer) => {
      const what = offerWords(layout, offer.object, offer.offer);
      if (offer.points === null) {
        return TEXT.house_why_closed.replace("{what}", what).replace("{time}", opens);
      }
      return `${TEXT.house_why_offer.replace("{what}", what).replace("{points}", points(offer.points))}${offer.busy ? ` ${TEXT.house_why_busy}` : ""}`;
    })
    .join(", ");
  // Idle while waiting for a taken object, as its entry says (ADR-0026 section 5).
  const waited = why.waits === null ? undefined : why.offers.find((offer) => offer.object === why.waits);
  const ends = why.night?.until ?? null;
  const night = why.night !== null && why.night.now && !why.commanded
    ? `${ends === null ? TEXT.house_why_night_last : TEXT.house_why_night.replace("{time}", timeOfDay(ends.date.second_of_day))} `
    : "";
  return (
    <div style={{ marginLeft: "1em", color: "#444" }}>
      {why.commanded ? `${TEXT.house_why_commanded} ` : ""}
      {night}
      {chosen === null && waited !== undefined
        ? TEXT.house_why_waits
            .replace("{what}", offerWords(layout, waited.object, waited.offer))
            .replace("{points}", waited.points === null ? "" : points(waited.points))
        : chosen === null
        ? TEXT.house_why_none.replace("{enough}", points(why.enough))
        : TEXT.house_why_chosen
            .replace("{what}", offerWords(layout, chosen.object, chosen.offer))
            .replace("{points}", points(chosen.points))}{" "}
      {behind === "" ? "" : TEXT.house_why_behind.replace("{list}", behind)}
    </div>
  );
}

/** A person's house entries of the day so far, from the run's log, each choice with its
 * "Warum?". */
function Day({ run, house, sim }: { run: string; house: HouseAnswer; sim: number }) {
  const [items, setItems] = useState<readonly EventItem[] | "failed">([]);
  const dayStart = house.at.t - house.at.date.second_of_day;
  useEffect(() => {
    let live = true;
    // The day up to the second the house shows, page by page (500 entries each).
    const read = async () => {
      const all: EventItem[] = [];
      let after: number | null = null;
      for (let page = 0; page < DAY_PAGES; page += 1) {
        const from = after === null ? "" : `&after=${after}`;
        const json = await getJson(`/api/runs/${run}/events?subject=${sim}&from=${dayStart}&to=${house.at.t}${from}`, EVENTS);
        const answer = json as unknown as EventsAnswer;
        all.push(...answer.events);
        if (answer.next === null) {
          break;
        }
        after = answer.next;
      }
      return all;
    };
    read()
      .then((all) => live && setItems(all))
      .catch(() => live && setItems("failed"));
    return () => {
      live = false;
    };
  }, [run, sim, house.at.t]);
  if (items === "failed") {
    return <p>{TEXT.failed_network}</p>;
  }
  const lines = items
    .map((item) => [item, dayLine(house.layout, item)] as const)
    .filter((pair): pair is readonly [EventItem, { text: string; choice: boolean }] => pair[1] !== null);
  if (lines.length === 0) {
    return null;
  }
  return (
    <details>
      <summary>{TEXT.house_today}</summary>
      <ol style={{ paddingLeft: "1.2em" }}>
        {lines.map(([item, line]) => (
          <li key={item.entry.id}>
            {timeOfDay(item.date.second_of_day)} {line.text}{" "}
            {line.choice && <Why run={run} layout={house.layout} entry={item.entry.id} />}
          </li>
        ))}
      </ol>
    </details>
  );
}

/** The floor plan from above, each floor beside the other, with the people where they are. */
function Plan({ house, names }: { house: HouseAnswer; names: ReadonlyMap<number, string> }) {
  const { layout } = house;
  const drawn = plan(layout);
  // Who is away at school, work or the kindergarten is not in the house.
  const inside = house.persons.filter((person) => person.place.away === undefined);
  const young = house.children.filter((child) => child.place.away === undefined);
  const placed = spots(layout, [...inside, ...young], house.at.t);
  const dots = [
    ...inside.map((person) => ({ sim: person.sim, fill: person.action === null ? "#8a6d3b" : "#2f6f73", line: doing(layout, person) })),
    ...young.map((child) => ({ sim: child.sim, fill: "#9a5a8c", line: youngLine(layout, child) })),
  ];
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
      {dots.map((dot, index) => {
        const spot = placed[index] ?? { x: 0, y: 0 };
        const name = names.get(dot.sim) ?? String(dot.sim);
        return (
          <g key={`person-${dot.sim}`}>
            <circle cx={spot.x} cy={spot.y} r={0.3} fill={dot.fill} stroke="#fff" stroke-width={0.05} />
            <text x={spot.x} y={spot.y + 0.11} text-anchor="middle" font-size={0.32} fill="#fff">
              {name.slice(0, 1)}
            </text>
            <title>{`${name}: ${dot.line}`}</title>
          </g>
        );
      })}
    </svg>
  );
}

/** Whether a person can be given a command from the page: at home, neither away nor on its
 * way out (the core refuses the rest with its reason, ADR-0024 section 5). */
function commandable(person: HousePerson): boolean {
  return person.place.away === undefined && (person.place.walking === undefined || person.action !== null);
}

/** The key of a choice in its list. */
function choiceKey(choice: CommandChoice): string {
  return `${choice.object}:${choice.offer}`;
}

/** A command for a person with needs at home (ADR-0024 section 5): an offer of an object of
 * the house and "Befehlen", an input over the server. Once it is taken the house is read
 * again (`given`); a refusal stands beside it with the core's reason. */
function Command({
  run,
  layout,
  sim,
  name,
  given,
}: {
  run: string;
  layout: HouseLayout;
  sim: number;
  name: string;
  given: () => void;
}) {
  const choices = commandChoices(layout);
  const [chosen, setChosen] = useState("");
  const [sending, setSending] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const choice = choices.find((each) => choiceKey(each) === chosen) ?? choices[0];
  if (choice === undefined) {
    return null;
  }
  const give = () => {
    setSending(true);
    setRefused(null);
    postJson(`/api/runs/${run}/input`, { command: { sim, object: choice.object, offer: choice.offer } }, INPUT)
      .then(() => {
        if (mounted.current) {
          setSending(false);
          given();
        }
      })
      .catch((error: unknown) => {
        if (mounted.current) {
          setSending(false);
          setRefused(inputFailureText(error));
        }
      });
  };
  return (
    <div role="group" aria-label={TEXT.house_command_for.replace("{name}", name)}>
      <select aria-label={TEXT.house_command_what} value={choiceKey(choice)} onChange={(event) => setChosen(event.currentTarget.value)}>
        {choices.map((each) => (
          <option key={choiceKey(each)} value={choiceKey(each)}>
            {each.label}
          </option>
        ))}
      </select>{" "}
      <button type="button" disabled={sending} onClick={give}>
        {TEXT.house_command}
      </button>
      {refused !== null && <span role="status"> {refused}</span>}
    </div>
  );
}

/** A resident's portrait, name and line; for a person with needs its five needs as bars, its
 * day and, at home, a command; for a child under 6 (`person` null) nothing more. */
function Resident({
  run,
  house,
  sim,
  line,
  person,
  pedigree,
  people,
  given,
}: {
  run: string;
  house: HouseAnswer;
  sim: number;
  line: string;
  person: HousePerson | null;
  pedigree: Pedigree;
  people: PeopleAnswer | null;
  given: () => void;
}) {
  const known = pedigree.persons.find((p) => p.sim === sim);
  const name = known === undefined ? String(sim) : fullName(known);
  const shown = people !== null && readable(people.contract_version)
    ? people.people.find((p) => p.sim === sim)?.shown
    : undefined;
  return (
    <li style={{ display: "flex", gap: "1em", alignItems: "flex-start", listStyle: "none", marginBottom: "1em" }}>
      {shown !== undefined && known !== undefined && (
        <Portrait params={shown.body} sex={known.sex} label={name} width={64} />
      )}
      <div>
        <p>
          <a href={format({ page: "person", run, sim: String(sim) })}>{name}</a>
          {": "}
          {line}
          {person !== null && person.smells && <Smell />}
        </p>
        {person !== null && (
          <>
            <Needs person={person} />
            <Day run={run} house={house} sim={sim} />
            {commandable(person) && <Command run={run} layout={house.layout} sim={sim} name={name} given={given} />}
          </>
        )}
      </div>
    </li>
  );
}

/** The small cloud and the word beside one who smells, hygiene at most 50 by the server
 * (ADR-0026, addendum of 2026-11-04 on a need at 0). */
function Smell() {
  return (
    <span>
      {" "}
      <svg viewBox="0 0 24 16" width="1.5em" height="1em" aria-hidden="true" style={{ verticalAlign: "middle" }}>
        <circle cx={7} cy={10} r={5} fill="#a3ad6b" />
        <circle cx={13} cy={7} r={6} fill="#a3ad6b" />
        <circle cx={18} cy={10} r={5} fill="#a3ad6b" />
      </svg>{" "}
      {TEXT.house_smells}
    </span>
  );
}

/** A person's five needs as bars. */
function Needs({ person }: { person: HousePerson }) {
  return (
    <dl style={{ margin: 0 }}>
      {NEEDS.map((need, index) => {
        const value = person.needs[index] ?? 0;
        return (
          <div key={need} style={{ display: "flex", gap: "0.5em" }}>
            <dt style={{ width: "5em" }} id={`need-${person.sim}-${index}`}>{TEXT[need]}</dt>
            <dd style={{ margin: 0 }}>
              <meter
                min={0}
                max={1000}
                low={150}
                high={400}
                optimum={1000}
                value={value}
                aria-labelledby={`need-${person.sim}-${index}`}
              />{" "}
              {value}
            </dd>
          </div>
        );
      })}
    </dl>
  );
}

/** The house of a household (ADR-0021, addendum of 2026-11-03): its plan with the people
 * where they are, the clock in steps of minutes and hours, each person with what it does
 * and its needs, each child under 6 with where it is. */
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
  // The last house stays while the next loads after a step, so the clock and its pause
  // button, open days and answers stay (review).
  const last = useRef<HouseAnswer | null>(null);
  if (loaded.state === "ready" && loaded.value !== null) {
    last.current = loaded.value;
  }
  const house = loaded.state === "ready" ? loaded.value : last.current;
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
  if (loaded.state === "loading" && house === null) {
    return <p>{TEXT.loading}</p>;
  }
  if (loaded.state === "failed" && house === null) {
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
  // A command changed the house: read it again at the same clock.
  const reload = () => {
    if (live.current) {
      setAt((count) => count + 1);
    }
  };
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
      {loaded.state === "failed" && <p role="alert">{TEXT[loaded.text]}</p>}
      {pedigree.state === "failed" && <p role="alert">{TEXT[pedigree.text]}</p>}
      {!house.morning && <p>{TEXT.house_before_morning}</p>}
      <Plan house={house} names={names} />
      {pedigree.state === "ready" && house.persons.length + house.children.length > 0 && (
        <>
          <h2>{TEXT.house_people}</h2>
          <ul style={{ padding: 0 }}>
            {house.persons.map((person) => (
              <Resident
                key={person.sim}
                run={run}
                house={house}
                sim={person.sim}
                line={doing(house.layout, person)}
                person={person}
                pedigree={pedigree.value}
                people={people.state === "ready" ? people.value : null}
                given={reload}
              />
            ))}
            {house.children.map((child) => (
              <Resident
                key={child.sim}
                run={run}
                house={house}
                sim={child.sim}
                line={youngLine(house.layout, child)}
                person={null}
                pedigree={pedigree.value}
                people={people.state === "ready" ? people.value : null}
                given={reload}
              />
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

/** On a person's page, the way to its household's house: there if it has one, else "Ins
 * Haus", which moves the household in (an input, ADR-0024) and opens the house. */
export function HouseLink({ run, household, members }: { run: string; household: number; members: number }) {
  const [state, setState] = useState<"asking" | "in" | "out" | "moving" | "failed">("asking");
  const [refused, setRefused] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    let live = true;
    setState("asking");
    setRefused(null);
    // The list of houses, not the house itself: a household without one is no error.
    getJson(`/api/runs/${run}/houses`, HOUSES)
      .then((json) => {
        const listed = (json as unknown as HousesAnswer).households.some((entry) => entry.household === household);
        if (live) {
          setState(listed ? "in" : "out");
        }
      })
      .catch(() => live && setState("failed"));
    return () => {
      live = false;
    };
  }, [run, household]);
  const to = format({ page: "house", run, household: String(household) });
  if (state === "asking") {
    return null;
  }
  if (state === "failed") {
    return <p>{TEXT.failed_network}</p>;
  }
  if (state === "in") {
    return (
      <p>
        <a href={to}>{TEXT.house_to_house}</a>
      </p>
    );
  }
  if (members > LARGEST) {
    return null;
  }
  const moveIn = () => {
    setState("moving");
    setRefused(null);
    postJson(`/api/runs/${run}/input`, { move_in: { household } }, INPUT)
      .then(() => {
        // Only while this page is still open (review): an answer after the person left it
        // does not pull them back.
        if (mounted.current) {
          location.hash = to;
        }
      })
      .catch((error: unknown) => {
        if (mounted.current) {
          setState("out");
          setRefused(inputFailureText(error));
        }
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
