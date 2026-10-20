import { getJson } from "./api.js";
import type { EventItem, EventsAnswer } from "./generated/export.js";

const EVENTS = { format: "simagent.view.events", version: 2 } as const;

/** The entries of a run's log that pass `filter` (the query of `…/events`), every page of
 * them: each next page after the id the last one names, until it names none. */
export async function readLog(run: string, filter: string): Promise<EventItem[]> {
  const items: EventItem[] = [];
  let after: number | null = null;
  do {
    const query: string = after === null ? filter : `${filter}&after=${after}`;
    const page = (await getJson(`/api/runs/${run}/events?${query}`, EVENTS)) as unknown as EventsAnswer;
    items.push(...page.events);
    after = page.next;
  } while (after !== null);
  return items;
}

/**
 * The log of a run page window by window: each call reads from the second the last read that
 * came through ended at (at first `start`) and then ends at `until`. Reads go one after
 * another; a failed one moves nothing, so its window is read again with the next.
 */
export function windows(run: string, start: number): (until: number) => Promise<EventItem[]> {
  let from = start;
  let queue: Promise<unknown> = Promise.resolve();
  return (until: number) => {
    const read = queue.then(async () => {
      const items = await readLog(run, `from=${from}`);
      from = until;
      return items;
    });
    queue = read.catch(() => undefined);
    return read;
  };
}
