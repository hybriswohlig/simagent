import { useEffect, useState } from "preact/hooks";

import { getJson } from "./api.js";
import { curves, type Curve, type Curves } from "./curves.js";
import type { CensusAnswer } from "./generated/export.js";
import { current, failure, type Loaded } from "./loaded.js";
import { BOTTOM, chart, LEFT, lineColor, RIGHT, TOP, WIDTH } from "./render/chart.js";
import { TEXT } from "./text.js";

/** The curves of a run from its census on the server; only an answer for this run (and this
 * `at`, which asks again when the run moved on) counts. */
function useCurves(run: string, at: string): Loaded<Curves> {
  const key = `${run}@${at}`;
  const [loaded, setLoaded] = useState<Loaded<Curves>>({ state: "loading", key });
  useEffect(() => {
    let live = true;
    setLoaded({ state: "loading", key });
    getJson(`/api/runs/${run}/census`, { format: "simagent.view.census", version: 1 })
      .then((json) => live && setLoaded({ state: "ready", key, value: curves(json as unknown as CensusAnswer) }))
      .catch((error: unknown) => live && setLoaded({ state: "failed", key, text: failure(error) }));
    return () => {
      live = false;
    };
  }, [key]);
  return current(loaded, key);
}

function Picture({ years, curve }: { years: number[]; curve: Curve }) {
  const drawn = chart(
    years,
    curve.lines.map((line) => line.values),
    curve.fromZero,
    curve.decimals,
  );
  return (
    <figure style={{ margin: 0, width: `${WIDTH}px` }}>
      <figcaption>{curve.title}</figcaption>
      {curve.note === null ? null : <p>{curve.note}</p>}
      <svg
        viewBox={`0 0 ${drawn.width} ${drawn.height}`}
        width={drawn.width}
        height={drawn.height}
        role="img"
        aria-label={curve.title}
      >
        {drawn.yTicks.map((tick) => (
          <g key={`y${tick.value}`}>
            <line x1={LEFT} y1={tick.at} x2={RIGHT} y2={tick.at} stroke="#e4e4e4" />
            <text x={LEFT - 6} y={tick.at} text-anchor="end" dominant-baseline="central" font-size="11">
              {curve.tick(tick.value)}
            </text>
          </g>
        ))}
        {drawn.xTicks.map((tick) => (
          <text key={`x${tick.value}`} x={tick.at} y={BOTTOM + 16} text-anchor="middle" font-size="11">
            {String(tick.value)}
          </text>
        ))}
        <line x1={LEFT} y1={TOP} x2={LEFT} y2={BOTTOM} stroke="#888888" />
        <line x1={LEFT} y1={BOTTOM} x2={RIGHT} y2={BOTTOM} stroke="#888888" />
        {drawn.segments.map((runs, line) =>
          runs.map((points, i) => (
            <polyline
              key={`${line}.${i}`}
              points={points.map((point) => `${point.x},${point.y}`).join(" ")}
              fill="none"
              stroke={lineColor(line)}
              stroke-width={2}
            />
          )),
        )}
        {drawn.dots.map((points, line) =>
          points.map((point, i) => (
            <circle key={`${line}:${i}`} cx={point.x} cy={point.y} r={3} fill={lineColor(line)} />
          )),
        )}
      </svg>
      <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexWrap: "wrap", gap: "4px 12px" }}>
        {curve.lines.map((line, i) => (
          <li key={line.name}>
            <span style={{ color: lineColor(i) }}>{"■ "}</span>
            {line.last === null ? line.name : `${line.name} ${line.last}`}
          </li>
        ))}
      </ul>
    </figure>
  );
}

/** The curves of the run (ADR-0021, addendum 2026-10-21): two pictures side by side; the last
 * curves that came stay while the next load, so playing does not flicker. */
export function CurvesView({ run, at }: { run: string; at: string }) {
  const loaded = useCurves(run, at);
  const [shown, setShown] = useState<Curves | null>(null);
  useEffect(() => {
    if (loaded.state === "ready") {
      setShown(loaded.value);
    }
  }, [loaded]);
  return (
    <>
      <h2>{TEXT.curves}</h2>
      {loaded.state === "failed" ? <p>{TEXT[loaded.text]}</p> : null}
      {shown === null || shown.years.length === 0 ? null : (
        <div style={{ display: "flex", flexWrap: "wrap", gap: "16px 24px" }}>
          {shown.curves.map((curve) => (
            <Picture key={curve.title} years={shown.years} curve={curve} />
          ))}
        </div>
      )}
    </>
  );
}
