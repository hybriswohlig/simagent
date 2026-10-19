import { render } from "preact";
import { useEffect, useState } from "preact/hooks";

import { format, parse, type Route } from "./router.js";
import { TEXT } from "./text.js";

/** One page per route; the pages fill in with the server (ADR-0021). */
function Page({ route }: { route: Route }) {
  switch (route.page) {
    case "runs":
      return <h1>{TEXT.runs}</h1>;
    case "run":
      return <h1>{`${TEXT.run} ${route.run}`}</h1>;
    case "person":
      return <h1>{`${TEXT.person} ${route.sim}`}</h1>;
    case "tree":
      return <h1>{`${TEXT.tree} ${route.sim}`}</h1>;
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
