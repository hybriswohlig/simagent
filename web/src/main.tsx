import { render } from "preact";
import { useEffect, useState } from "preact/hooks";

import { AssumptionsPage } from "./assumptionspage.js";
import { cheatOn, setCheat } from "./cheat.js";
import { DiffPage } from "./diffpage.js";
import { PeoplePage } from "./peopleview.js";
import { PersonPage } from "./person.js";
import { browserStorage } from "./pin.js";
import { format, parse, type Route } from "./router.js";
import { RunsPage } from "./runs.js";
import { Sheet } from "./sheet.js";
import { RunPage } from "./town.js";
import { TEXT } from "./text.js";

/** One page per route; the pages fill in with the server (ADR-0021). */
function Page({ route, cheat }: { route: Route; cheat: boolean }) {
  switch (route.page) {
    case "runs":
      return <RunsPage />;
    case "portraits":
      return <Sheet />;
    case "assumptions":
      return <AssumptionsPage />;
    case "run":
      return <RunPage run={route.run} />;
    case "people":
      return <PeoplePage run={route.run} />;
    case "person":
    case "tree":
      return <PersonPage run={route.run} sim={route.sim} cheat={cheat} />;
    case "diff":
      return <DiffPage a={route.a} b={route.b} />;
    case "not_found":
      return (
        <p>
          {TEXT.not_found} <a href={format({ page: "runs" })}>{TEXT.back}</a>
        </p>
      );
  }
}

function App() {
  const [route, setRoute] = useState(() => parse(location.hash));
  const [cheat, setCheatShown] = useState(() => cheatOn(browserStorage()));
  useEffect(() => {
    const follow = () => setRoute(parse(location.hash));
    addEventListener("hashchange", follow);
    return () => removeEventListener("hashchange", follow);
  }, []);
  const toggle = (on: boolean) => {
    setCheat(browserStorage(), on);
    setCheatShown(on);
  };
  return (
    <>
      <header>
        <label>
          <input type="checkbox" checked={cheat} onChange={(event) => toggle(event.currentTarget.checked)} /> {TEXT.cheat}
        </label>
      </header>
      <Page route={route} cheat={cheat} />
    </>
  );
}

const root = document.getElementById("app");
if (root !== null) {
  document.title = TEXT.title;
  render(<App />, root);
}
