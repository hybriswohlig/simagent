import { heightLines, parentsMeanSentence, partRows, TRAIT_HEADINGS, traitSentence, type Sex } from "./card.js";
import { copiesLines, DISEASES, diseaseName, skippedSentence, statusWord } from "./carddiseases.js";
import type { GeneticsCard, TraitCard } from "./generated/export.js";
import { originSentence } from "./origin.js";
import { bar } from "./render/bar.js";
import { format } from "./router.js";
import { TEXT } from "./text.js";

/** The parts of `z` as rows with a bar from the middle (Grok G5, M2). */
function Parts({ row }: { row: TraitCard }) {
  const parts = partRows(row.parts);
  if (parts === null) {
    return null;
  }
  return (
    <>
      <table>
        <tbody>
          {parts.rows.map((part) => {
            const { left, width } = bar(part.value);
            return (
              <tr key={part.label}>
                <td>{part.label}</td>
                <td style={{ width: "120px" }}>
                  <div style={{ position: "relative", height: "8px", background: "#eeeeee" }}>
                    <div style={{ position: "absolute", left: `${left}%`, width: `${width}%`, height: "8px", background: "#666666" }} />
                  </div>
                </td>
                <td style={{ textAlign: "right" }}>{part.text}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {parts.note === null ? null : <p>{parts.note}</p>}
    </>
  );
}

/** Whether a line in cm is the parents' target height, which the card keeps under "Genauer"
 * (Grok's round 45, H6). */
const isTarget = (line: string) => line.startsWith(TEXT.card_parents_target);

/** A trait of the card under "Genauer": its heading, the sentence of its percentile, for the
 * height the parents' target. */
function Trait({ row, card, sex, nowCm }: { row: TraitCard; card: GeneticsCard; sex: Sex; nowCm: number }) {
  const heading = TRAIT_HEADINGS[row.trait];
  const sentence = traitSentence(row, sex);
  if (heading === undefined || sentence === null) {
    return null;
  }
  const lines = row.trait === "core:growth" && card.height !== null ? heightLines(card.height, nowCm).filter(isTarget) : [];
  const mean = parentsMeanSentence(row);
  return (
    <div>
      <h3>{heading}</h3>
      <p>{sentence}</p>
      {lines.map((line) => (
        <p key={line}>{line}</p>
      ))}
      {mean === null ? null : <p>{mean}</p>}
      <Parts row={row} />
    </div>
  );
}

/** The hereditary diseases (Grok G1, G4, L7, M1), folded (the genetics is about looks, not
 * about defects): the switch when off, then per disease its name, status word, one-liner and
 * copies. */
function Diseases({ card, sex }: { card: GeneticsCard; sex: Sex }) {
  return (
    <details>
      <summary>{TEXT.card_diseases}</summary>
      {card.diseases_off ? <p>{TEXT.card_diseases_off}</p> : null}
      {card.diseases.map((disease) => {
        const status = statusWord(disease.status, sex);
        const copies = copiesLines(disease.copies);
        return (
          <div key={disease.disease}>
            <p>
              <strong>{diseaseName(disease.disease)}</strong>
              {status === null ? null : `: ${status}`}
            </p>
            {DISEASES[disease.disease] === undefined ? null : <p>{DISEASES[disease.disease]?.line}</p>}
            {(copies ?? []).map((line) => (
              <p key={line}>{line}</p>
            ))}
          </div>
        );
      })}
    </details>
  );
}

/** The locus a copy line of the origin speaks of. */
const COPY_LOCI: { readonly [kind: string]: string } = { red_copies: "core:mc1r", blue_copies: "core:herc2" };

/** The genetics card of a person, as far as the core sends it (ADR-0021 section 5): open the
 * sentences of where looks and traits come from (Grok's round 45), each copy line with the
 * lines "übersprungen" of its locus beneath, and the heights in cm; folded the percentiles
 * and their parts ("Genauer") and the hereditary diseases. `names` gives the grandparents of
 * a skipped copy their names. */
export function CardView({
  card,
  origin,
  sex,
  nowCm,
  names,
}: {
  card: GeneticsCard;
  origin: readonly { readonly [key: string]: unknown }[];
  sex: Sex;
  nowCm: number;
  names: ReadonlyMap<number, string>;
}) {
  const skipped = card.skipped.flatMap((line) => {
    const text = skippedSentence(line, names);
    const locus = (line as { readonly locus?: unknown }).locus;
    return text === null ? [] : [{ text, locus: typeof locus === "string" ? locus : "" }];
  });
  const placed = new Set<string>();
  const open = origin.flatMap((line, i) => {
    const sentence = originSentence(line);
    const own = sentence === null ? [] : [<p key={`o${i}`}>{sentence}</p>];
    const locus = COPY_LOCI[String(line["kind"])];
    const beneath = skipped
      .filter((each) => locus !== undefined && each.locus === locus)
      .map((each) => {
        placed.add(each.text);
        return <p key={each.text}>{each.text}</p>;
      });
    return [...own, ...beneath];
  });
  const heights = card.height === null ? [] : heightLines(card.height, nowCm).filter((line) => !isTarget(line));
  return (
    <section>
      <h2>{TEXT.card}</h2>
      {open}
      {heights.map((line) => (
        <p key={line}>{line}</p>
      ))}
      <details>
        <summary>{TEXT.card_more}</summary>
        {card.traits.map((row) => (
          <Trait key={row.trait} row={row} card={card} sex={sex} nowCm={nowCm} />
        ))}
        {skipped
          .filter((each) => !placed.has(each.text))
          .map((each) => (
            <p key={each.text}>{each.text}</p>
          ))}
      </details>
      <Diseases card={card} sex={sex} />
      <p>
        <a href={format({ page: "assumptions" })}>{TEXT.assumptions}</a>
      </p>
    </section>
  );
}
