import { segments, template } from "./chronicle.js";
import { dateText } from "./dates.js";
import type { EventItem } from "./generated/export.js";
import { format } from "./router.js";
import { TEXT } from "./text.js";

/** A line of the chronicle: its date and the template of its code with the persons it is
 * about, each a link to their page. */
function ChronicleLine({ run, item, names }: { run: string; item: EventItem; names: ReadonlyMap<number, string> }) {
  const sims = item.entry.subjects.map((subject) => subject.sim);
  return (
    <li>
      {`${dateText(item.date)}: `}
      {segments(template(item), sims, ` ${TEXT.and} `, ", ").map((segment, i) =>
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

/** A long list in a box of its own height that scrolls, so the page stays short (finding F4
 * of the playtest); focusable, so that a keyboard scrolls it too. */
export const SCROLLED = { maxHeight: "24em", overflowY: "auto" } as const;

/** A chronicle: its lines, newest first, and why a part of the log could not be read, with a
 * way to read it again; without a line, `empty` if given. */
export function Chronicle({
  run,
  items,
  failed,
  retry,
  names,
  empty,
}: {
  run: string;
  items: readonly EventItem[];
  failed: string | null;
  retry: () => void;
  names: ReadonlyMap<number, string>;
  empty?: string;
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
      {items.length === 0 && failed === null && empty !== undefined ? <p>{empty}</p> : null}
      <ol style={SCROLLED} tabIndex={0} aria-label={TEXT.chronicle}>
        {items.map((item) => (
          <ChronicleLine key={item.entry.id} run={run} item={item} names={names} />
        ))}
      </ol>
    </>
  );
}
