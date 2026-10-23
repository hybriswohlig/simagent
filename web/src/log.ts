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

/** The entries an input accepted with the stamp `stamp` wrote: those of the second after it,
 * in which it acts (ADR-0004; the addendum to ADR-0023 before S2). */
export function enteredBy(run: string, stamp: number): Promise<EventItem[]> {
  const second = stamp + 1;
  return readLog(run, `from=${second}&to=${second}`);
}

/**
 * The log of a run page window by window: each call reads the entries from the second the
 * last read that came through ended at (at first `start`) up to `until`, its own end, and
 * then ends there. Reads go one after another; a failed one moves nothing, so its window is
 * read again with the next. An entry at a shared end comes in both; the chronicle keeps it
 * once.
 */
export function windows(run: string, start: number): (until: number) => Promise<EventItem[]> {
  let from = start;
  let queue: Promise<unknown> = Promise.resolve();
  return (until: number) => {
    const read = queue.then(async () => {
      // Read already, by the call before that ended here.
      if (until === from) {
        return [];
      }
      const items = await readLog(run, `from=${from}&to=${until}`);
      from = until;
      return items;
    });
    queue = read.catch(() => undefined);
    return read;
  };
}
