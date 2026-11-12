import { useEffect, useRef, useState } from "preact/hooks";

import { getJson, getJsonOrRefusal, postJson } from "./api.js";
import { answered, pause, play, playText, press, refused, standing, tick, type Clock, type Next, type Step } from "./clock.js";
import { commandChoices, type CommandChoice } from "./command.js";
import { dateText } from "./dates.js";
import { dayGroups, dayLine, doing, reserveWord, type DayLine } from "./day.js";
import type { AdvanceAnswer, EventItem, EventsAnswer, HouseAnswer, HouseBelief, HouseBond, HouseChild, HouseLayout, HouseMemory, HousePerson, HousesAnswer, Pedigree, PeopleAnswer, Person, WhyAnswer } from "./generated/export.js";
import { current, failure, type Loaded } from "./loaded.js";
import { fullName, surnames } from "./names.js";
import { usePedigree } from "./person.js";
import { usePeople } from "./peopleview.js";
import { Portrait } from "./portrait.js";
import { families } from "./families.js";
import { SCROLLED } from "./lines.js";
import { failureCode, inputFailureText, refusalText } from "./refusals.js";
import { labels, plan, spots, timeOfDay } from "./render/plan.js";
import { format } from "./router.js";
import { readable } from "./contract.js";
import { beliefSentence, bondValues, companyWords, feelingWords, memorySentence, named } from "./social.js";
import { KIND_WORDS, ROOM_WORDS, TEXT } from "./text.js";
import { weightWords, whyText } from "./why.js";

const HOUSE = { format: "simagent.view.house", version: 11 } as const;
const ADVANCED = { format: "simagent.view.advanced", version: 1 } as const;
const INPUT = { format: "simagent.view.input", version: 1 } as const;
const EVENTS = { format: "simagent.view.events", version: 2 } as const;
const HOUSES = { format: "simagent.view.houses", version: 2 } as const;
/** How many pages of a person's day the page reads at most (a day has some hundred entries). */
const DAY_PAGES = 10;
/** How far back a person's day list reaches from the clock, in seconds: the last 24 hours, so
 * that after "Bis morgen früh" the evening before is still there (the playtest of 2026-11-04). */
const DAY_SECONDS = 86_400;
/** The most members a household with a house has (ADR-0025 section 3). */
export const LARGEST = 8;
const WHY = { format: "simagent.view.why", version: 11 } as const;
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

/** The household's `core:felt` lines of the last 24 hours, one read per person with needs, so a
 * day line can name whom an arrival infected (ADR-0032 section 7). A failed read leaves the
 * lines out; each person's own day still loads. */
function useFelt(run: string, house: HouseAnswer | null): readonly EventItem[] {
  const [felts, setFelts] = useState<readonly EventItem[]>([]);
  const at = house?.at.t ?? null;
  const sims = house === null ? "" : house.persons.map((person) => person.sim).join(",");
  useEffect(() => {
    if (at === null || sims === "") {
      setFelts([]);
      return;
    }
    let live = true;
    const from = at - DAY_SECONDS;
    const to = at + 1;
    const read = async () => {
      const all: EventItem[] = [];
      for (const sim of sims.split(",")) {
        let after: number | null = null;
        for (let page = 0; page < DAY_PAGES; page += 1) {
          const next = after === null ? "" : `&after=${after}`;
          const json = await getJson(`/api/runs/${run}/events?subject=${sim}&from=${from}&to=${to}${next}`, EVENTS);
          const answer = json as unknown as EventsAnswer;
          for (const item of answer.events) {
            if (item.entry.payload["core:felt"] !== undefined) {
              all.push(item);
            }
          }
          if (answer.next === null) {
            break;
          }
          after = answer.next;
        }
      }
      return all;
    };
    read()
      .then((all) => live && setFelts(all))
      .catch(() => live && setFelts([]));
    return () => {
      live = false;
    };
  }, [run, at, sims]);
  return felts;
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

/** "Warum?" for a choice: what it chose with its worth and what came behind, from the server
 * (ADR-0026 section 6 and its addenda of 2026-11-04, `whyText`); for an idle choice that is
 * still the person's state also the comparison at the clock, asked again with each house answer
 * (`house`, after a step or a command). */
function Why({ run, house, entry, names }: { run: string; house: HouseAnswer; entry: number; names: ReadonlyMap<number, string> }) {
  const [why, setWhy] = useState<WhyAnswer | null | "failed">(null);
  const ask = () => {
    getJson(`/api/runs/${run}/why/${entry}`, WHY)
      .then((json) => setWhy(json as unknown as WhyAnswer))
      .catch(() => setWhy("failed"));
  };
  // The comparison at the clock goes with the house: asked again with each of its answers.
  useEffect(() => {
    if (why !== null && why !== "failed" && why.now !== null) {
      ask();
    }
  }, [house]);
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
  const { told, now } = whyText(why, house.layout);
  const social = companyWords(why.company, house.layout, named(names), why.temper);
  return (
    <div style={{ marginLeft: "1em", color: "#444" }}>
      {told}
      {social === "" ? null : ` ${social}`}
      {now === null ? null : (
        <>
          <br />
          {now}
        </>
      )}
    </div>
  );
}

/** A person's house entries of the last 24 hours up to the second after the clock, in which an
 * input given at the clock acts (ADR-0004 section 4), from the run's log, by day, each choice
 * with its "Warum?"; read again with each house answer, so a command shows at once. */
function Day({
  run,
  house,
  sim,
  names,
  cheat,
  felts,
}: {
  run: string;
  house: HouseAnswer;
  sim: number;
  names: ReadonlyMap<number, string>;
  cheat: boolean;
  felts: readonly EventItem[];
}) {
  const [items, setItems] = useState<readonly EventItem[] | "failed">([]);
  const from = house.at.t - DAY_SECONDS;
  const to = house.at.t + 1;
  useEffect(() => {
    let live = true;
    // The window, page by page (500 entries each).
    const read = async () => {
      const all: EventItem[] = [];
      let after: number | null = null;
      for (let page = 0; page < DAY_PAGES; page += 1) {
        const next = after === null ? "" : `&after=${after}`;
        const json = await getJson(`/api/runs/${run}/events?subject=${sim}&from=${from}&to=${to}${next}`, EVENTS);
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
  }, [run, sim, house]);
  if (items === "failed") {
    return <p>{TEXT.failed_network}</p>;
  }
  const view = { sim, name: named(names), cheat };
  const lines = items
    .map((item) => ({ item, date: item.date, line: dayLine(house.layout, item, view, felts) }))
    .filter((pair): pair is { item: EventItem; date: EventItem["date"]; line: DayLine } => pair.line !== null);
  if (lines.length === 0) {
    return null;
  }
  return (
    <details>
      <summary>{TEXT.house_day}</summary>
      {dayGroups(lines).map((group) => (
        <div key={`${group.date.year}-${group.date.day_of_year}`}>
          <p style={{ margin: "0.4em 0 0" }}>
            <strong>{group.title}</strong>
          </p>
          <ol style={{ paddingLeft: "1.2em", margin: 0 }}>
            {group.items.map(({ item, line }) => (
              <li key={item.entry.id}>
                {timeOfDay(item.date.second_of_day)} {line.text}{" "}
                {line.choice && <Why run={run} house={house} entry={item.entry.id} names={names} />}
              </li>
            ))}
          </ol>
        </div>
      ))}
    </details>
  );
}

/** The floor plan from above, each floor beside the other, with the people where they are; the
 * names of objects and rooms where `labels` puts them, none over another or past its wall. */
function Plan({ house, names }: { house: HouseAnswer; names: ReadonlyMap<number, string> }) {
  const { layout } = house;
  const drawn = plan(layout);
  // Who is away at school, work or the kindergarten is not in the house.
  const inside = house.persons.filter((person) => person.place.away === undefined);
  const young = house.children.filter((child) => child.place.away === undefined);
  const named = labels(
    layout,
    drawn,
    layout.objects.map((object) => KIND_WORDS[object.kind]?.[0] ?? object.kind),
    layout.rooms.map((room) => (ROOM_WORDS[room.id] ?? [room.id])[0]),
  );
  // Who stands in front of a thing stands past its name.
  const placed = spots(
    layout,
    [...inside, ...young],
    house.at.t,
    named.objects.map((label) => label.box),
    [...named.objects, ...named.rooms].map((label) => label.box),
  );
  const dots = [
    ...inside.map((person) => ({ sim: person.sim, fill: person.action === null ? "#8a6d3b" : "#2f6f73", line: doing(layout, person) })),
    ...young.map((child) => ({ sim: child.sim, fill: "#9a5a8c", line: youngLine(layout, child) })),
  ];
  const pad = 0.6;
  const view = `${-pad} ${-pad - 0.8} ${drawn.width + 2 * pad} ${drawn.height + 2 * pad + 0.8}`;
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
          <rect key={`object-${index}`} x={left + object.x} y={object.y} width={object.w} height={object.h} fill="#d9d4c7" stroke="#8a8270" stroke-width={0.03} />
        );
      })}
      {named.objects.map((label, index) => (
        <text key={`object-name-${index}`} data-label="object" x={label.x} y={label.y} font-size={label.size} fill="#6b6455">
          {label.text}
        </text>
      ))}
      {named.rooms.map((label, index) => (
        <text
          key={`room-name-${index}`}
          data-label="room"
          x={label.x}
          y={label.y}
          font-size={label.size}
          fill="#555"
          stroke="#f7f5ef"
          stroke-width={0.12}
          paint-order="stroke"
        >
          {label.text}
        </text>
      ))}
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
  return person.place.away === undefined && person.place.walking?.out !== true;
}

/** The key of a choice in its list. */
function choiceKey(choice: CommandChoice): string {
  return `${choice.object}:${choice.offer}`;
}

/** A command for a person with needs at home (ADR-0024 section 5): an offer of an object of
 * the house among those the server allows the person now (version 5: by its age, sleep only in
 * its night) and "Befehlen", an input over the server; one whose object has no free place for
 * the person stands there marked and cannot be chosen, the one it does now marked too (version
 * 8). Once it is taken the house is read again (`given`) and the choice stays while it is free;
 * a refusal stands beside it with the core's reason. */
function Command({
  run,
  layout,
  person,
  name,
  given,
}: {
  run: string;
  layout: HouseLayout;
  person: HousePerson;
  name: string;
  given: () => void;
}) {
  const sim = person.sim;
  const choices = commandChoices(layout, person.commands, person.action);
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
  const free = choices.filter((each) => !each.taken);
  const choice = free.find((each) => choiceKey(each) === chosen) ?? free[0];
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
          <option key={choiceKey(each)} value={choiceKey(each)} disabled={each.taken}>
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

/** A resident's portrait, name and line; for a person with needs its feelings, its five needs
 * as bars, its bonds, memories and beliefs, its day and, at home, a command; for a child under
 * 6 (`person` null) where it is, and its bonds. */
function Resident({
  run,
  house,
  sim,
  line,
  person,
  pedigree,
  people,
  given,
  names,
  cheat,
  bonds,
  memories,
  beliefs,
  felts,
}: {
  run: string;
  house: HouseAnswer;
  sim: number;
  line: string;
  person: HousePerson | null;
  pedigree: Pedigree;
  people: PeopleAnswer | null;
  given: () => void;
  names: ReadonlyMap<number, string>;
  cheat: boolean;
  bonds: readonly HouseBond[];
  memories: readonly HouseMemory[];
  beliefs: readonly HouseBelief[];
  felts: readonly EventItem[];
}) {
  const known = pedigree.persons.find((p) => p.sim === sim);
  const name = known === undefined ? String(sim) : fullName(known);
  const shown = people !== null && readable(people.contract_version)
    ? people.people.find((p) => p.sim === sim)?.shown
    : undefined;
  // The worst band of the reserve beside what the person does (ADR-0026, addendum of 2026-11-06
  // on hunger at 0).
  const status = person === null ? null : reserveWord(person.reserve);
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
          {status !== null && `, ${status}`}
          {person !== null && person.smells && <Smell />}
        </p>
        {person !== null && (
          <>
            <p style={{ margin: "0.2em 0 0" }}>{feelingWords(person)}</p>
            <Needs person={person} name={known?.first_name ?? name} comfort={house.comfort} />
            <Social run={run} names={names} bonds={bonds} memories={memories} beliefs={beliefs} />
            <Day run={run} house={house} sim={sim} names={names} cheat={cheat} felts={felts} />
            {commandable(person) && <Command run={run} layout={house.layout} person={person} name={name} given={given} />}
          </>
        )}
        {person === null && <Social run={run} names={names} bonds={bonds} memories={memories} beliefs={beliefs} />}
      </div>
    </li>
  );
}

/** The social part of a person card (ADR-0030 section 2, ADR-0031 section 8): one row per other
 * person with the six values, the strongest memories, and each belief. The numbers and the
 * basis word are the server's. The card does not say whether a belief is true. */
function Social({
  run,
  names,
  bonds,
  memories,
  beliefs,
}: {
  run: string;
  names: ReadonlyMap<number, string>;
  bonds: readonly HouseBond[];
  memories: readonly HouseMemory[];
  beliefs: readonly HouseBelief[];
}) {
  if (bonds.length === 0 && memories.length === 0 && beliefs.length === 0) {
    return null;
  }
  const name = named(names);
  return (
    <div>
      {bonds.length === 0 ? null : (
        <>
          <p style={{ margin: "0.4em 0 0" }}>{TEXT.social_relations}</p>
          <dl style={{ margin: 0 }}>
            {bonds.map((bond) => (
              <div key={bond.other} style={{ display: "flex", gap: "0.5em" }}>
                <dt>
                  <a href={format({ page: "person", run, sim: String(bond.other) })}>{name(bond.other)}</a>
                  {": "}
                </dt>
                <dd style={{ margin: 0 }}>{bondValues(bond)}</dd>
              </div>
            ))}
          </dl>
        </>
      )}
      {memories.length === 0 ? null : (
        <>
          <p style={{ margin: "0.4em 0 0" }}>{TEXT.social_memories}</p>
          <ul style={{ margin: 0, paddingLeft: "1.2em" }}>
            {memories.map((memory) => (
              <li key={`${memory.kind}-${memory.at.t}-${memory.with.join("-")}`}>{memorySentence(memory, name)}</li>
            ))}
          </ul>
        </>
      )}
      {beliefs.length === 0 ? null : (
        <ul style={{ margin: "0.4em 0 0", paddingLeft: "1.2em" }}>
          {beliefs.map((belief, index) => (
            <li key={`${belief.since.t}-${index}`}>{beliefSentence(belief, name)}</li>
          ))}
        </ul>
      )}
    </div>
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

/** A person's five needs as bars; away, that they stand still meanwhile (away every rate is 0,
 * ADR-0024 section 4; the playtest of 2026-11-04). A need below full that weighs nothing in a
 * choice now, by the server's weights, says from where it counts, its comfort ("zählt ab 700";
 * ADR-0026, addendum of 2026-11-04 on comfort and the time-filler). */
function Needs({ person, name, comfort }: { person: HousePerson; name: string; comfort: readonly number[] }) {
  return (
    <>
    {person.place.away !== undefined && <p style={{ margin: 0 }}>{TEXT.house_needs_still.replace("{name}", name)}</p>}
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
              {weightWords(person.weights[index], comfort[index])}
            </dd>
          </div>
        );
      })}
    </dl>
    </>
  );
}

/** The house of a household (ADR-0021, addendum of 2026-11-03), named by the surnames of its
 * members: its plan with the people where they are, the clock in steps of minutes and hours,
 * each person with what it does and its needs, each child under 6 with where it is; before the
 * first morning the members moving in. */
export function HousePage({ run, household, cheat }: { run: string; household: string; cheat: boolean }) {
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
  // So too the last pedigree and people: the residents, their open days, answers and chosen
  // commands stay while the next load after a step or a command (the playtest of 2026-11-04).
  const lastPedigree = useRef<Pedigree | null>(null);
  if (pedigree.state === "ready") {
    lastPedigree.current = pedigree.value;
  }
  const lastPeople = useRef<PeopleAnswer | null>(null);
  if (people.state === "ready") {
    lastPeople.current = people.value;
  }
  const persons = lastPedigree.current;
  const looks = lastPeople.current;
  const felts = useFelt(run, house);
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
  const names = new Map<number, string>(persons === null ? [] : persons.persons.map((p) => [p.sim, fullName(p)]));
  // The living members of the household, as the person page counts them.
  const members: Person[] =
    persons === null
      ? []
      : persons.persons.filter((p) => p.household === house.household && p.died === null && p.emigrated === null);
  const family = surnames(members);
  // A command changed the house: read it again at the same clock.
  const reload = () => {
    if (live.current) {
      setAt((count) => count + 1);
    }
  };
  return (
    <section>
      <h1>{family === "" ? TEXT.house_of_household.replace("{n}", household) : TEXT.house_family.replace("{names}", family)}</h1>
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
      {!house.morning && members.length > 0 && (
        <>
          <h2>{TEXT.house_people}</h2>
          <ul>
            {members.map((member) => (
              <li key={member.sim}>
                <a href={format({ page: "person", run, sim: String(member.sim) })}>{fullName(member)}</a>
                {": "}
                {TEXT.house_moving}
              </li>
            ))}
          </ul>
        </>
      )}
      {persons !== null && house.persons.length + house.children.length > 0 && (
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
                pedigree={persons}
                people={looks}
                given={reload}
                names={names}
                cheat={cheat}
                bonds={person.bonds}
                memories={person.memories}
                beliefs={person.beliefs}
                felts={felts}
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
                pedigree={persons}
                people={looks}
                given={reload}
                names={names}
                cheat={cheat}
                bonds={child.bonds}
                memories={[]}
                beliefs={[]}
                felts={felts}
              />
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

/** The households of a run with a house and the age a child has no needs under, from the
 * server. */
export type Houses = { readonly households: ReadonlySet<number>; readonly youngUntil: number | null };

/** The houses of a run, from the server, asked again for each `again`: `null` while asked,
 * "failed" without an answer. The list of houses, not each house: a household without one is no
 * error. */
export function useHouses(run: string, again = ""): Houses | null | "failed" {
  const [houses, setHouses] = useState<Houses | null | "failed">(null);
  useEffect(() => {
    let live = true;
    setHouses(null);
    getJson(`/api/runs/${run}/houses`, HOUSES)
      .then((json) => {
        const answer = json as unknown as HousesAnswer;
        if (live) {
          setHouses({ households: new Set(answer.households.map((entry) => entry.household)), youngUntil: answer.young_until });
        }
      })
      .catch(() => live && setHouses("failed"));
    return () => {
      live = false;
    };
  }, [run, again]);
  return houses;
}

/** "Ins Haus" for a household without a house: it moves the household in (an input, ADR-0024)
 * and opens its house; a refusal stands beside it with the core's reason. */
export function MoveIn({ run, household }: { run: string; household: number }) {
  const [moving, setMoving] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const to = format({ page: "house", run, household: String(household) });
  const moveIn = () => {
    setMoving(true);
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
          setMoving(false);
          setRefused(inputFailureText(error));
        }
      });
  };
  return (
    <>
      <button type="button" disabled={moving} onClick={moveIn}>
        {moving ? TEXT.house_moving_in : TEXT.house_move_in}
      </button>
      {refused !== null && <span role="status"> {refused}</span>}
    </>
  );
}

/** On a person's page, the way to its household's house: there if it has one, else "Ins
 * Haus" for a household a house can hold. */
export function HouseLink({ run, household, members }: { run: string; household: number; members: number }) {
  const houses = useHouses(run, String(household));
  if (houses === null) {
    return null;
  }
  if (houses === "failed") {
    return <p>{TEXT.failed_network}</p>;
  }
  if (houses.households.has(household)) {
    return (
      <p>
        <a href={format({ page: "house", run, household: String(household) })}>{TEXT.house_to_house}</a>
      </p>
    );
  }
  if (members > LARGEST) {
    return null;
  }
  return (
    <p>
      <MoveIn key={household} run={run} household={household} />
    </p>
  );
}

/** The families of a run for finding one to move into a house (the playtest of 2026-11-04):
 * each household a house can hold with its members and their ages now, from the pedigree and
 * the people answer, by default only those with children, on request only those with a child
 * under the age the server names; each with the way to its house, or "Ins Haus". */
export function Families({ run, pedigree, people }: { run: string; pedigree: Pedigree; people: PeopleAnswer }) {
  const [children, setChildren] = useState(true);
  const [small, setSmall] = useState(false);
  const houses = useHouses(run);
  const young = houses === null || houses === "failed" ? null : houses.youngUntil;
  const kept = families(pedigree, people.people, children, LARGEST, small ? young : null);
  return (
    <>
      <h2>{TEXT.families}</h2>
      <p>
        <label>
          <input type="checkbox" checked={children} onChange={() => setChildren(!children)} /> {TEXT.families_children}
        </label>
        {young === null ? null : (
          <>
            {" "}
            <label>
              <input type="checkbox" checked={small} onChange={() => setSmall(!small)} />{" "}
              {TEXT.families_young.replace("{n}", String(young))}
            </label>
          </>
        )}
      </p>
      <p role="status">{TEXT.families_found.replace("{n}", String(kept.length))}</p>
      {kept.length === 0 ? <p>{TEXT.families_none}</p> : null}
      <ul style={SCROLLED} tabIndex={0} aria-label={TEXT.families}>
        {kept.map((family) => (
          <li key={family.household}>
            <strong>{TEXT.house_family.replace("{names}", surnames(family.members.map((member) => member.person)))}</strong>
            {": "}
            {family.members.map((member, index) => (
              <span key={member.person.sim}>
                {index > 0 ? ", " : ""}
                <a href={format({ page: "person", run, sim: String(member.person.sim) })}>{fullName(member.person)}</a>
                {` ${member.age} ${TEXT.years}`}
              </span>
            ))}{" "}
            {houses === null || houses === "failed" ? null : houses.households.has(family.household) ? (
              <a href={format({ page: "house", run, household: String(family.household) })}>{TEXT.house_to_house}</a>
            ) : (
              <MoveIn run={run} household={family.household} />
            )}
          </li>
        ))}
      </ul>
    </>
  );
}
