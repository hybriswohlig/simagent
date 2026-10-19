import { BODY_PARAM_KEYS, BODY_PARAMS, type BodyParamKey, type BodyParams } from "./generated/body.js";
import { Portrait } from "./portrait.js";
import { TEXT } from "./text.js";

/** The contract's defaults with some values set by hand: fixtures, never computed. */
function variant(set: Partial<Record<BodyParamKey, number>>): BodyParams {
  const entries = BODY_PARAM_KEYS.map((key) => [key, set[key] ?? BODY_PARAMS[key].default] as const);
  return Object.fromEntries(entries) as BodyParams;
}

const VARIANTS: readonly (readonly [keyof typeof TEXT, BodyParams])[] = [
  ["portrait_default", variant({})],
  ["portrait_child", variant({ body_form_age: 6, face_form_age: 6, height_cm: 115, breast_development: 0 })],
  [
    "portrait_elder",
    variant({ body_form_age: 72, face_form_age: 72, hair_grey_fraction: 0.9, face_wrinkles: 0.8, skin_laxity: 0.6 }),
  ],
  [
    "portrait_red",
    variant({ hair_eumelanin: 0.05, hair_pheomelanin: 0.9, skin_eumelanin: 0.05, freckle_density: 0.7, iris_melanin: 0.15 }),
  ],
  ["portrait_dark", variant({ skin_eumelanin: 0.9, skin_pheomelanin: 0.3, hair_eumelanin: 0.95, iris_melanin: 0.95 })],
  ["portrait_beard", variant({ beard_density: 0.9, beard_coverage: 0.8, breast_development: 0, shoulder_width_z: 1 })],
  ["portrait_bald", variant({ balding_stage: 7, breast_development: 0 })],
  ["portrait_curls", variant({ hair_curl: 1, hair_density: 0.9 })],
];

/** A sheet of portraits from hand-set values, to look at the bust (ADR-0021 G10 later). */
export function Sheet() {
  return (
    <section>
      <h1>{TEXT.portraits}</h1>
      <div style={{ display: "flex", flexWrap: "wrap", gap: "12px" }}>
        {VARIANTS.map(([label, params]) => (
          <figure key={label}>
            <Portrait params={params} label={TEXT[label]} />
            <figcaption>{TEXT[label]}</figcaption>
          </figure>
        ))}
      </div>
    </section>
  );
}
