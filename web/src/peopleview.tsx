import { useEffect, useState } from "preact/hooks";
import type { ComponentChildren } from "preact";

import { getJson } from "./api.js";
import type { Looks, PeopleAnswer, Pedigree, Person, Traits } from "./generated/export.js";
import { current, failure, type Loaded } from "./loaded.js";
import { fullName } from "./names.js";
import { found, NO_FILTER, toggled, type Filter, type Found, type Word } from "./people.js";
import { usePedigree } from "./person.js";
import { Portrait } from "./portrait.js";
import { format } from "./router.js";
import { LOOK_WORDS, TEXT, TRAIT_WORDS } from "./text.js";

/** The people of a run from the server; only an answer for this run (and this `at`) counts. */
export function usePeople(run: string, at = ""): Loaded<PeopleAnswer> {
  const key = `${run}@${at}`;
  const [loaded, setLoaded] = useState<Loaded<PeopleAnswer>>({ state: "loading", key });
  useEffect(() => {
    let live = true;
    setLoaded({ state: "loading", key });
    getJson(`/api/runs/${run}/people`, { format: "simagent.view.people", version: 1 })
      .then((json) => live && setLoaded({ state: "ready", key, value: json as unknown as PeopleAnswer }))
      .catch((error: unknown) => live && setLoaded({ state: "failed", key, text: failure(error) }));
    return () => {
      live = false;
    };
  }, [key]);
  return current(loaded, key);
}

/** The words of the looks in the order the filter offers them. */
const WORDS: readonly Word[] = ["hair", "eyes", "skin", "height", "cognition", "curls", "freckles"];

const LABELS: { readonly [W in Word]: string } = {
  hair: TEXT.look_hair,
  eyes: TEXT.look_eyes,
  skin: TEXT.look_skin,
  curls: TEXT.look_curls,
  freckles: TEXT.look_freckles,
  height: TEXT.look_height,
  cognition: TEXT.look_cognition,
};

/** One group of words as boxes to tick; none ticked keeps every word. */
function Choice({ word, filter, set }: { word: Word; filter: Filter; set: (filter: Filter) => void }) {
  const chosen = filter[word] as ReadonlySet<string>;
  const words = LOOK_WORDS[word] as { readonly [code: string]: string };
  return (
    <fieldset>
      <legend>{LABELS[word]}</legend>
      {Object.entries(words).map(([code, text]) => (
        <label key={code} style={{ marginRight: "8px" }}>
          <input
            type="checkbox"
            checked={chosen.has(code)}
            onChange={() => set({ ...filter, [word]: toggled(chosen, code) })}
          />{" "}
          {text}
        </label>
      ))}
    </fieldset>
  );
}

/** The traits a filter chooses among, with their labels. */
type Step = "smart" | "strong" | "beauty";
const STEPS: readonly [Step, string][] = [
  ["smart", TEXT.trait_smart],
  ["strong", TEXT.trait_strong],
  ["beauty", TEXT.trait_beauty],
];
const ORDER = [3, 2, 1, 0, -1, -2, -3] as const;

/** One trait's steps as boxes to tick; none ticked keeps every step. */
function StepChoice({ step, label, filter, set }: { step: Step; label: string; filter: Filter; set: (filter: Filter) => void }) {
  const chosen = filter[step];
  return (
    <fieldset>
      <legend>{label}</legend>
      {ORDER.map((n) => (
        <label key={n} style={{ marginRight: "8px" }}>
          <input type="checkbox" checked={chosen.has(n)} onChange={() => set({ ...filter, [step]: toggled(chosen, n) })} />{" "}
          {TRAIT_WORDS[step][n]}
        </label>
      ))}
    </fieldset>
  );
}

/** The words of a person's traits that are not none, strongest kind first. */
export function traitsLine(traits: Traits): string {
  const steps: [Step, number][] = [
    ["smart", traits.cognition],
    ["strong", traits.strength],
    ["beauty", traits.beauty],
  ];
  return steps
    .filter(([, n]) => n !== 0)
    .map(([step, n]) => TRAIT_WORDS[step][n as (typeof ORDER)[number]])
    .join(" · ");
}

const age = (text: string): number | null => (text === "" || Number.isNaN(Number(text)) ? null : Number(text));

/** The filter: the sex (unless a pairing fixes it), the ages, single only, the words. */
export function FilterForm({ filter, set, sexFixed }: { filter: Filter; set: (filter: Filter) => void; sexFixed: boolean }) {
  return (
    <div>
      {sexFixed ? null : (
        <fieldset>
          <legend>{TEXT.filter_sex}</legend>
          {([null, "female", "male"] as const).map((sex) => (
            <label key={String(sex)} style={{ marginRight: "8px" }}>
              <input type="radio" checked={filter.sex === sex} onChange={() => set({ ...filter, sex })} />{" "}
              {sex === null ? TEXT.filter_any : sex === "female" ? TEXT.female : TEXT.male}
            </label>
          ))}
        </fieldset>
      )}
      <fieldset>
        <legend>{TEXT.filter_age}</legend>
        <label>
          {`${TEXT.filter_age_from} `}
          <input type="number" min="0" style={{ width: "4em" }} value={filter.ageFrom ?? ""}
            onInput={(e) => set({ ...filter, ageFrom: age(e.currentTarget.value) })} />
        </label>{" "}
        <label>
          {`${TEXT.filter_age_to} `}
          <input type="number" min="0" style={{ width: "4em" }} value={filter.ageTo ?? ""}
            onInput={(e) => set({ ...filter, ageTo: age(e.currentTarget.value) })} />
        </label>{" "}
        {sexFixed ? null : (
          <>
            <label>
              <input type="checkbox" checked={filter.singleOnly} onChange={() => set({ ...filter, singleOnly: !filter.singleOnly })} />{" "}
              {TEXT.filter_single}
            </label>{" "}
            <label>
              <input type="checkbox" checked={filter.pregnantOnly} onChange={() => set({ ...filter, pregnantOnly: !filter.pregnantOnly })} />{" "}
              {TEXT.filter_pregnant}
            </label>
          </>
        )}
      </fieldset>
      {STEPS.map(([step, label]) => (
        <StepChoice key={step} step={step} label={label} filter={filter} set={set} />
      ))}
      {WORDS.map((word) => (
        <Choice key={word} word={word} filter={filter} set={set} />
      ))}
      <p>
        <button type="button" onClick={() => set({ ...NO_FILTER, sex: sexFixed ? filter.sex : null })}>
          {TEXT.filter_clear}
        </button>
      </p>
    </div>
  );
}

/** The words of a person's looks in one line. */
function looksLine(looks: Looks): string {
  return WORDS.map((word) => `${LABELS[word]} ${(LOOK_WORDS[word] as { readonly [code: string]: string })[looks[word]]}`).join(" · ");
}

/** How many a list shows at first, and more with each press. */
const PAGE = 24;

/** The people found as small portraits with name, age and looks; `action` adds a button. */
export function PeopleList({ run, people, action }: { run: string; people: readonly Found[]; action?: (person: Person) => ComponentChildren }) {
  const [shown, setShown] = useState(PAGE);
  if (people.length === 0) {
    return <p>{TEXT.people_none}</p>;
  }
  return (
    <>
      <div style={{ display: "flex", flexWrap: "wrap", gap: "12px" }}>
        {people.slice(0, shown).map(({ person, entry }) => (
          <figure key={person.sim} style={{ width: "160px", margin: "0" }}>
            <Portrait params={entry.shown.body} sex={person.sex} label={fullName(person)} width={90} />
            <figcaption>
              <a href={format({ page: "person", run, sim: String(person.sim) })}>{fullName(person)}</a>
              {`, ${entry.shown.at.age_years} ${TEXT.years}${entry.pregnant ? `, ${TEXT.pregnant}` : ""}`}
              {traitsLine(entry.traits) === "" ? null : (
                <>
                  <br />
                  <strong>{traitsLine(entry.traits)}</strong>
                </>
              )}
              <br />
              <small>{looksLine(entry.looks)}</small>
              {action === undefined ? null : <div>{action(person)}</div>}
            </figcaption>
          </figure>
        ))}
      </div>
      {people.length > shown ? (
        <p>
          <button type="button" onClick={() => setShown(shown + PAGE)}>
            {TEXT.people_more}
          </button>
        </p>
      ) : null}
    </>
  );
}

/** The people of a run, filtered (the filter of the person list). */
export function PeoplePage({ run }: { run: string }) {
  const pedigree = usePedigree(run);
  const people = usePeople(run);
  const [filter, setFilter] = useState<Filter>(NO_FILTER);
  const back = (
    <p>
      <a href={format({ page: "run", run })}>{TEXT.back}</a>
    </p>
  );
  if (pedigree.state === "failed" || people.state === "failed") {
    return (
      <section>
        {back}
        <p role="alert">{TEXT.people_failed}</p>
      </section>
    );
  }
  if (pedigree.state !== "ready" || people.state !== "ready") {
    return <section>{back}</section>;
  }
  const kept = found(pedigree.value as Pedigree, people.value.people, filter);
  return (
    <section>
      {back}
      <h2>{TEXT.people}</h2>
      <FilterForm filter={filter} set={setFilter} sexFixed={false} />
      <p role="status">
        {TEXT.people_found.replace("{n}", String(kept.length)).replace("{all}", String(people.value.people.length))}
      </p>
      <PeopleList run={run} people={kept} />
    </section>
  );
}
