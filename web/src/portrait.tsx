import type { BodyParams } from "./generated/body.js";
import { bust, type Shape } from "./render/portrait.js";

/** One shape as SVG; `web/` draws, `render/` computed the numbers. */
function Drawn({ shape }: { shape: Shape }) {
  switch (shape.kind) {
    case "ellipse":
      return (
        <ellipse
          cx={shape.cx}
          cy={shape.cy}
          rx={shape.rx}
          ry={shape.ry}
          fill={shape.fill}
          opacity={shape.opacity}
          data-part={shape.part}
        />
      );
    case "path":
      return (
        <path
          d={shape.d}
          fill={shape.fill}
          stroke={shape.stroke}
          stroke-width={shape.width}
          opacity={shape.opacity}
          data-part={shape.part}
        />
      );
    case "line":
      return (
        <line
          x1={shape.x1}
          y1={shape.y1}
          x2={shape.x2}
          y2={shape.y2}
          stroke={shape.stroke}
          stroke-width={shape.width}
          stroke-linecap="round"
          opacity={shape.opacity}
          data-part={shape.part}
        />
      );
  }
}

/** The portrait v0 of one person: a bust from the body values. */
export function Portrait({ params, label }: { params: BodyParams; label: string }) {
  return (
    <svg viewBox="0 0 100 120" width="150" height="180" role="img" aria-label={label}>
      {bust(params).map((shape, i) => (
        <Drawn key={i} shape={shape} />
      ))}
    </svg>
  );
}
