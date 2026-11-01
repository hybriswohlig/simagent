import { useEffect, useRef, useState } from "preact/hooks";

import { getJson } from "./api.js";
import { children, origin, picture, type Member, type Picture } from "./family.js";
import type { Pedigree, Person, PersonAnswer, Shown } from "./generated/export.js";
import { current, failure, type Loaded } from "./loaded.js";
import { fullName } from "./names.js";
import { Portrait } from "./portrait.js";
import { format } from "./router.js";
import { TEXT } from "./text.js";

type Ages = ReadonlyMap<number, readonly Shown[]>;

/** The bodies at the reached ages of `sims`, each from its person's answer; only an answer
 * for these sims counts. */
function useAges(run: string, sims: readonly number[], at: string): Loaded<Ages> {
  const key = `${run}@${at}:${sims.join(",")}`;
  const [loaded, setLoaded] = useState<Loaded<Ages>>({ state: "loading", key });
  useEffect(() => {
    let live = true;
    setLoaded({ state: "loading", key });
    const asked = [...sims];
    Promise.all(
      asked.map((sim) => getJson(`/api/runs/${run}/persons/${sim}`, { format: "simagent.view.person", version: 2 })),
    )
      .then((answers) => {
        const ages: Ages = new Map(answers.map((json, i) => [asked[i] ?? 0, (json as unknown as PersonAnswer).at_ages]));
        if (live) {
          setLoaded({ state: "ready", key, value: ages });
        }
      })
      .catch((error: unknown) => live && setLoaded({ state: "failed", key, text: failure(error) }));
    return () => {
      live = false;
    };
  }, [key]);
  return current(loaded, key);
}

/** A picture as a row of portraits, the page's person by name, everyone else by what they are
 * to it with a link to their page. */
function Row({ run, title, shown }: { run: string; title: string; shown: Picture }) {
  return (
    <>
      <h3>{title.replace("{age}", String(shown.age))}</h3>
      <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
        {shown.sitters.map((sitter) => (
          <figure key={sitter.person.sim}>
            <Portrait params={sitter.body} label={fullName(sitter.person)} />
            <figcaption>
              {sitter.role === null ? (
                <strong>{fullName(sitter.person)}</strong>
              ) : (
                <>
                  {`${sitter.role} `}
                  <a href={format({ page: "person", run, sim: String(sitter.person.sim) })}>{fullName(sitter.person)}</a>
                </>
              )}
            </figcaption>
          </figure>
        ))}
      </div>
    </>
  );
}

/** The person among their family, each picture at one age, so that what the genes made alike
 * shows: parents and full siblings, and the children. The last ages that came stay while newer
 * ones load after a change, so the pictures do not go away in between. */
export function FamilyView({
  run,
  pedigree,
  person,
  at,
  ages,
}: {
  run: string;
  pedigree: Pedigree;
  person: Person;
  at: string;
  ages: readonly Shown[];
}) {
  const born = origin(pedigree, person);
  const kids = children(pedigree, person);
  const loaded = useAges(
    run,
    [...born, ...kids].map((member) => member.person.sim),
    at,
  );
  const last = useRef<{ person: string; value: Ages } | null>(null);
  if (loaded.state === "ready") {
    last.current = { person: `${run}/${person.sim}`, value: loaded.value };
  }
  if (loaded.state === "failed") {
    return <p>{TEXT[loaded.text]}</p>;
  }
  const kept = last.current?.person === `${run}/${person.sim}` ? last.current.value : null;
  if (kept === null) {
    return null;
  }
  const sitters = (members: readonly Member[]) =>
    members.map((member) => ({ person: member.person, role: member.role, at: kept.get(member.person.sim) ?? [] }));
  const center = { person, role: null, at: ages };
  const up = picture(center, sitters(born));
  const down = picture(center, sitters(kids));
  if (up === null && down === null) {
    return null;
  }
  return (
    <details>
      <summary>{TEXT.family}</summary>
      {up === null ? null : <Row run={run} title={TEXT.family_origin} shown={up} />}
      {down === null ? null : <Row run={run} title={TEXT.family_children} shown={down} />}
    </details>
  );
}
