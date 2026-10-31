/** The pages of the viewer, a closed list (ADR-0021 section 3). */
export type Route =
  | { page: "runs" }
  | { page: "portraits" }
  | { page: "assumptions" }
  | { page: "run"; run: string }
  | { page: "people"; run: string }
  | { page: "person"; run: string; sim: string }
  | { page: "tree"; run: string; sim: string }
  | { page: "diff"; a: string; b: string }
  | { page: "not_found"; hash: string };

/** A run is a directory name: letters, digits, `.`, `_` and `-`, not starting with `.`. */
const RUN = /^[A-Za-z0-9_-][A-Za-z0-9._-]*$/;
/** Whether `name` may name a run: the rule of the server for its run directories. */
export function runName(name: string): boolean {
  return RUN.test(name);
}

/** A sim is its id in decimal, without a leading zero. */
const SIM = /^[1-9][0-9]*$/;

/** The route of a location hash; anything else is `not_found`. */
export function parse(hash: string): Route {
  const path = hash.startsWith("#") ? hash.slice(1) : hash;
  if (path === "" || path === "/") {
    return { page: "runs" };
  }
  if (path === "/portraits") {
    return { page: "portraits" };
  }
  if (path === "/assumptions") {
    return { page: "assumptions" };
  }
  const parts = path.split("/");
  const [empty, runs, run, persons, sim, tree] = parts;
  const known = empty === "" && runs === "runs" && run !== undefined && RUN.test(run);
  if (known && parts.length === 3) {
    return { page: "run", run };
  }
  if (known && persons === "people" && parts.length === 4) {
    return { page: "people", run };
  }
  const person = known && persons === "persons" && sim !== undefined && SIM.test(sim);
  if (person && parts.length === 5) {
    return { page: "person", run, sim };
  }
  if (person && parts.length === 6 && tree === "tree") {
    return { page: "tree", run, sim };
  }
  if (known && persons === "diff" && sim !== undefined && RUN.test(sim) && parts.length === 5) {
    return { page: "diff", a: run, b: sim };
  }
  return { page: "not_found", hash };
}

/** The hash of a route; for every route that `parse` returns, `parse(format(route))` is it. */
export function format(route: Route): string {
  switch (route.page) {
    case "runs":
      return "#/";
    case "portraits":
      return "#/portraits";
    case "assumptions":
      return "#/assumptions";
    case "run":
      return `#/runs/${route.run}`;
    case "people":
      return `#/runs/${route.run}/people`;
    case "person":
      return `#/runs/${route.run}/persons/${route.sim}`;
    case "tree":
      return `#/runs/${route.run}/persons/${route.sim}/tree`;
    case "diff":
      return `#/runs/${route.a}/diff/${route.b}`;
    case "not_found":
      return route.hash;
  }
}
