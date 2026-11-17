import { useEffect, useRef, useState } from "preact/hooks";

import type { BodyParams } from "./generated/body.js";
import { Portrait } from "./portrait.js";
import { FIGURE_HEIGHT, FIGURE_WIDTH, mountFigure } from "./render/figure.js";
import type { Sex } from "./render/portrait.js";

/**
 * The person's figure. While it loads, and if the files or WebGL are missing, the
 * portrait stays. A failure is not written on the card.
 */
export function Figure({ params, sex, sim, label }: { params: BodyParams; sex: Sex; sim: string; label: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [failed, setFailed] = useState(false);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas === null) return;
    let live = true;
    let dispose = () => {};
    setReady(false);
    mountFigure(canvas, params, sex, sim)
      .then((stop) => {
        if (!live) {
          stop();
          return;
        }
        dispose = stop;
        setReady(true);
      })
      .catch(() => {
        if (live) setFailed(true);
      });
    return () => {
      live = false;
      dispose();
    };
  }, [params, sex, sim]);
  if (failed) return <Portrait params={params} sex={sex} label={label} />;
  const box = { width: `${FIGURE_WIDTH}px`, height: `${FIGURE_HEIGHT}px` };
  return (
    <div style={{ position: "relative", ...box }}>
      {ready ? null : (
        <div style={{ position: "absolute", inset: "0" }}>
          <Portrait params={params} sex={sex} label={label} />
        </div>
      )}
      <canvas
        ref={canvasRef}
        role="img"
        aria-hidden={!ready}
        aria-label={label}
        style={{ ...box, display: "block", touchAction: "none", cursor: "grab" }}
      />
    </div>
  );
}
