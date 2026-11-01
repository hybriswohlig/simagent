import { useEffect, useState } from "preact/hooks";

import { getJson } from "./api.js";
import { readable } from "./contract.js";
import type { SheetAnswer } from "./generated/export.js";
import { current, failure, type Loaded } from "./loaded.js";
import { Portrait } from "./portrait.js";
import { sheetRows, yearsText } from "./sheetrows.js";
import { TEXT } from "./text.js";

const SHEET = { format: "simagent.view.sheet", version: 1 } as const;
/** A portrait with its height centred beneath it. */
const CELL = { textAlign: "center" } as const;

/** The sheet of portraits (ADR-0021 G10): the core's bodies without environment of the same
 * genes in both sexes, a row per variant and sex, a column per age. */
export function Sheet() {
  const key = "sheet";
  const [loaded, setLoaded] = useState<Loaded<SheetAnswer>>({ state: "loading", key });
  useEffect(() => {
    let live = true;
    getJson("/api/sheet", SHEET)
      .then((json) => live && setLoaded({ state: "ready", key, value: json as unknown as SheetAnswer }))
      .catch((error: unknown) => live && setLoaded({ state: "failed", key, text: failure(error) }));
    return () => {
      live = false;
    };
  }, []);
  const shown = current(loaded, key);
  if (shown.state !== "ready") {
    return <p>{shown.state === "loading" ? TEXT.loading : TEXT[shown.text]}</p>;
  }
  if (!readable(shown.value.contract_version)) {
    return <p>{TEXT.failed_contract}</p>;
  }
  const rows = sheetRows(shown.value);
  const ages = rows[0]?.cells.map((cell) => cell.age) ?? [];
  return (
    <section>
      <h1>{TEXT.portraits}</h1>
      <p>{TEXT.sheet_note}</p>
      <table>
        <thead>
          <tr>
            <th scope="col" />
            {ages.map((age) => (
              <th scope="col" key={age}>
                {yearsText(age)}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.label}>
              <th scope="row">{row.label}</th>
              {row.cells.map((cell) => (
                <td key={cell.age} style={CELL}>
                  <Portrait params={cell.body} sex={row.sex} label={cell.title} />
                  <div>{cell.height}</div>
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
