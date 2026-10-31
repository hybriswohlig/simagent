import { heightLines, parentsMeanSentence, partRows, TRAIT_HEADINGS, traitSentence, type Sex } from "./card.js";
import { copiesLines, DISEASES, diseaseName, skippedSentence, statusWord } from "./carddiseases.js";
import type { GeneticsCard, TraitCard } from "./generated/export.js";
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

/** A trait of the card: its heading, for the height the lines in cm around the sentence. */
function Trait({ row, card, sex, nowCm }: { row: TraitCard; card: GeneticsCard; sex: Sex; nowCm: number }) {
  const heading = TRAIT_HEADINGS[row.trait];
  const sentence = traitSentence(row, sex);
  if (heading === undefined || sentence === null) {
    return null;
  }
  const lines = row.trait === "core:growth" && card.height !== null ? heightLines(card.height, nowCm) : [];
  const mean = parentsMeanSentence(row);
  return (
    <div>
      <h3>{heading}</h3>
      {lines.slice(0, 1).map((line) => (
        <p key={line}>{line}</p>
      ))}
      <p>{sentence}</p>
      {lines.slice(1).map((line) => (
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

/** The genetics card of a person, as far as the core sends it (ADR-0021 section 5); `names`
 * gives the grandparents of a skipped copy their names. */
export function CardView({
  card,
  sex,
  nowCm,
  names,
}: {
  card: GeneticsCard;
  sex: Sex;
  nowCm: number;
  names: ReadonlyMap<number, string>;
}) {
  const skipped = card.skipped.map((line) => skippedSentence(line, names)).filter((line) => line !== null);
  return (
    <section>
      <h2>{TEXT.card}</h2>
      {card.traits.map((row) => (
        <Trait key={row.trait} row={row} card={card} sex={sex} nowCm={nowCm} />
      ))}
      {skipped.map((line) => (
        <p key={line}>{line}</p>
      ))}
      <Diseases card={card} sex={sex} />
      <p>
        <a href={format({ page: "assumptions" })}>{TEXT.assumptions}</a>
      </p>
    </section>
  );
}
