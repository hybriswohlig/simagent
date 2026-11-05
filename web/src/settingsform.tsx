import type { FoundingValue, RunSetting } from "./generated/export.js";
import { changedValues, type Fields, hintOf, labelOf, markOf } from "./settings.js";
import { TEXT } from "./text.js";

/**
 * The folded section of own values where a town is founded (ADR-0021, addendum of
 * 2026-11-05): a field for each value the server names, at the base game's value at first,
 * with a line of what it does. One component, so that it can move to a settings page later.
 */
export function SettingsFields({
  founding,
  fields,
  change,
}: {
  founding: readonly FoundingValue[];
  fields: Fields;
  change: (key: string, text: string) => void;
}) {
  if (founding.length === 0) {
    return null;
  }
  return (
    <details>
      <summary>{TEXT.settings_own}</summary>
      <p>{TEXT.settings_note}</p>
      {founding.map((value) => (
        <p key={value.key}>
          <label>
            {labelOf(value.key)}{" "}
            <input
              type="number"
              step="any"
              aria-label={labelOf(value.key)}
              value={fields[value.key] ?? ""}
              onInput={(event) => change(value.key, event.currentTarget.value)}
            />
          </label>
          <br />
          <small>{hintOf(value)}</small>
        </p>
      ))}
      {changedValues(founding, fields) === null ? <p role="status">{TEXT.settings_not_number}</p> : null}
    </details>
  );
}

/** The mark of a town with own values, "eigene Werte" and the values; nothing for a town of
 * the base game. */
export function OwnMark({ settings }: { settings: readonly RunSetting[] }) {
  const values = markOf(settings);
  if (values === null) {
    return null;
  }
  return (
    <>
      <mark>{TEXT.settings_mark}</mark>
      {`: ${values}`}
    </>
  );
}
