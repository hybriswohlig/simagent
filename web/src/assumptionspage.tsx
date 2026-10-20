import { ASSUMPTIONS } from "./assumptions.js";
import { format } from "./router.js";
import { TEXT } from "./text.js";

/** The page of the model's assumptions, linked under every genetics card (Grok O2). */
export function AssumptionsPage() {
  return (
    <section>
      <h1>{TEXT.assumptions}</h1>
      {ASSUMPTIONS.map((section) => (
        <div key={section.heading}>
          <h2>{section.heading}</h2>
          {section.paragraphs.map((paragraph) => (
            <p key={paragraph}>{paragraph}</p>
          ))}
        </div>
      ))}
      <p>
        <a href={format({ page: "runs" })}>{TEXT.back}</a>
      </p>
    </section>
  );
}
