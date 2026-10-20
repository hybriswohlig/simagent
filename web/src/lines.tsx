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
function ChronicleLine({ run, item }: { run: string; item: EventItem }) {
  const template = CODES[`payload ${payloadCode(item) ?? ""}`] ?? "";
  const sims = item.entry.subjects.map((subject) => subject.sim);
  return (
    <li>
      {`${dateText(item.date)}: `}
      {segments(template, sims, ` ${TEXT.and} `, ", ").map((segment, i) =>
        typeof segment === "number" ? (
          <a key={i} href={format({ page: "person", run, sim: String(segment) })}>
            {`${TEXT.person} ${segment}`}
          </a>
        ) : (
          (segment ?? TEXT.unknown)
        ),
      )}
    </li>
  );
}

/** A chronicle: its lines, newest first, and a note if a part of the log could not be read. */
export function Chronicle({ run, items, failed }: { run: string; items: readonly EventItem[]; failed: boolean }) {
  return (
    <>
      <h2>{TEXT.chronicle}</h2>
      {failed ? <p role="alert">{TEXT.chronicle_failed}</p> : null}
      <ol>
        {items.map((item) => (
          <ChronicleLine key={item.entry.id} run={run} item={item} />
        ))}
      </ol>
    </>
  );
}
