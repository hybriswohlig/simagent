import { payloadCode, segments } from "./chronicle.js";
import type { CalendarDate, EventItem } from "./generated/export.js";
import { format } from "./router.js";
import { CODES, DAYS, MONTHS, TEXT } from "./text.js";

/** The date as the core gives it, its units named by the texts. */
export function dateText(date: CalendarDate): string {
  return `${TEXT.year} ${date.year}, ${MONTHS[date.month] ?? ""}, ${DAYS[date.day_of_month] ?? ""}`;
}

/** A line of the chronicle: its date and the template of its code with the persons it is
 * about, each a link to their page. */
function ChronicleLine({ run, item, names }: { run: string; item: EventItem; names: ReadonlyMap<number, string> }) {
  const template = CODES[`payload ${payloadCode(item) ?? ""}`] ?? "";
  const sims = item.entry.subjects.map((subject) => subject.sim);
  return (
    <li>
      {`${dateText(item.date)}: `}
      {segments(template, sims, ` ${TEXT.and} `, ", ").map((segment, i) =>
        typeof segment === "number" ? (
          <a key={i} href={format({ page: "person", run, sim: String(segment) })}>
            {names.get(segment) ?? `${TEXT.person} ${segment}`}
          </a>
        ) : (
          (segment ?? TEXT.unknown)
        ),
      )}
    </li>
  );
}

/** A chronicle: its lines, newest first, and why a part of the log could not be read, with a
 * way to read it again. */
export function Chronicle({
  run,
  items,
  failed,
  retry,
  names,
}: {
  run: string;
  items: readonly EventItem[];
  failed: string | null;
  retry: () => void;
  names: ReadonlyMap<number, string>;
}) {
  return (
    <>
      <h2>{TEXT.chronicle}</h2>
      {failed === null ? null : (
        <p role="alert">
          {failed}{" "}
          <button type="button" onClick={retry}>
            {TEXT.retry}
          </button>
        </p>
      )}
      <ol>
        {items.map((item) => (
          <ChronicleLine key={item.entry.id} run={run} item={item} names={names} />
        ))}
      </ol>
    </>
  );
}
