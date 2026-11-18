import { useEffect, useRef, useState } from "preact/hooks";

import type { BodyParams } from "./generated/body.js";
import { Portrait } from "./portrait.js";
import { FIGURE_HEIGHT, FIGURE_WIDTH, mountFigure } from "./render/figure.js";
import type { Sex } from "./render/portrait.js";

/**
 * The person's figure. While it loads, and if the files or WebGL are missing, the
 * portrait stays. A failure is not written on the card. The canvas stays mounted, so a
 * later body can try again, and a lost WebGL context shows the portrait until it returns.
 */
export function Figure({ params, sex, sim, label }: { params: BodyParams; sex: Sex; sim: string; label: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [failed, setFailed] = useState(false);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas === null) return;
    const cancel = { cancelled: false };
    let dispose = () => {};
    let lost = false;
    setFailed(false);
    setReady(false);
    mountFigure(canvas, params, sex, sim, cancel, (isLost) => {
      lost = isLost;
      if (!cancel.cancelled) setReady(!isLost);
    })
      .then((stop) => {
        if (cancel.cancelled) {
          stop();
          return;
        }
        dispose = stop;
        if (!lost) setReady(true);
      })
      .catch(() => {
        if (!cancel.cancelled) setFailed(true);
      });
    return () => {
      cancel.cancelled = true;
      dispose();
    };
  }, [params, sex, sim]);
  const box = { width: `${FIGURE_WIDTH}px`, height: `${FIGURE_HEIGHT}px` };
  const showPortrait = failed || !ready;
  return (
    <div style={{ position: "relative", ...box }}>
      {showPortrait ? (
        <div style={{ position: "absolute", inset: "0" }}>
          <Portrait params={params} sex={sex} label={label} />
        </div>
      ) : null}
      <canvas
        ref={canvasRef}
        role="img"
        aria-hidden={showPortrait}
        aria-label={label}
        style={{ ...box, display: "block", touchAction: "none", cursor: "grab", visibility: showPortrait ? "hidden" : "visible" }}
      />
    </div>
  );
}
