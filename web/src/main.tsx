import { render } from "preact";
import { useEffect, useState } from "preact/hooks";

import { PersonPage } from "./person.js";
import { format, parse, type Route } from "./router.js";
import { Sheet } from "./sheet.js";
import { TEXT } from "./text.js";

/** One page per route; the pages fill in with the server (ADR-0021). */
function Page({ route }: { route: Route }) {
  switch (route.page) {
    case "runs":
      return <h1>{TEXT.runs}</h1>;
    case "portraits":
      return <Sheet />;
    case "run":
      return <h1>{`${TEXT.run} ${route.run}`}</h1>;
    case "person":
    case "tree":
      return <PersonPage run={route.run} sim={route.sim} />;
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
  useEffect(() => {
    const follow = () => setRoute(parse(location.hash));
    addEventListener("hashchange", follow);
    return () => removeEventListener("hashchange", follow);
  }, []);
  return <Page route={route} />;
}

const root = document.getElementById("app");
if (root !== null) {
  document.title = TEXT.title;
  render(<App />, root);
}
