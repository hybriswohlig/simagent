import { heightLines, parentsMeanSentence, partRows, TRAIT_HEADINGS, traitSentence, type Sex } from "./card.js";
import type { GeneticsCard, TraitCard } from "./generated/export.js";
import { bar } from "./render/bar.js";
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

/** The genetics card of a person, as far as the core sends it (ADR-0021 section 5). */
export function CardView({ card, sex, nowCm }: { card: GeneticsCard; sex: Sex; nowCm: number }) {
  return (
    <section>
      <h2>{TEXT.card}</h2>
      {card.traits.map((row) => (
        <Trait key={row.trait} row={row} card={card} sex={sex} nowCm={nowCm} />
      ))}
    </section>
  );
}
