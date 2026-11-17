import { useEffect, useRef, useState } from "preact/hooks";

import type { BodyParams } from "./generated/body.js";
import { Portrait } from "./portrait.js";
import { FAMILY_HEIGHT_PX, FAMILY_LIMIT, FAMILY_SLOT_PX, familyCanvasWidth, familyNamePercent, visualKey } from "./render/family-layout.js";
import { FIGURE_HEIGHT, FIGURE_WIDTH, mountFamily, mountFigure, type FigureMember } from "./render/figure.js";
import type { Sex } from "./render/portrait.js";
import { format } from "./router.js";

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

/** One living member of the row: the server's body, and the name that links to the person. */
export type FamilyMember = {
  readonly sim: number;
  readonly firstName: string;
  readonly sex: Sex;
  readonly body: BodyParams;
};

/** Sim ids and the body values to one decimal: a clock step that changes no figure visibly
 * keeps the same key. */
function figureKey(members: readonly FamilyMember[]): string {
  const ids = members.map((member) => member.sim).join(",");
  return `${ids}|${members.map((member) => visualKey(member.body)).join("|")}`;
}

/**
 * The household in one row. While it loads, and if the files or WebGL are missing,
 * it shows nothing: the page already lists the members. At most eight. A lost context
 * hides the row until it is drawn again.
 */
export function FamilyFigures({ run, members }: { run: string; members: readonly FamilyMember[] }) {
  const shown = members.length > FAMILY_LIMIT ? members.slice(0, FAMILY_LIMIT) : members;
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [ready, setReady] = useState(false);
  // The effect follows the sim ids and the body values to one decimal, so the minute and
  // hour steps of the clock do not build the figures again.
  const key = figureKey(shown);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas === null || shown.length === 0) return;
    const cancel = { cancelled: false };
    let dispose = () => {};
    let lost = false;
    setReady(false);
    const draw: FigureMember[] = shown.map((member) => ({ body: member.body, sex: member.sex, simId: String(member.sim) }));
    mountFamily(canvas, draw, cancel, (isLost) => {
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
        if (!cancel.cancelled) setReady(false);
      });
    return () => {
      cancel.cancelled = true;
      dispose();
    };
  }, [key]);
  if (shown.length === 0) return null;
  const width = familyCanvasWidth(shown.length);
  // Clipped, not removed, while loading: the canvas keeps a size so the one WebGL
  // buffer can be drawn, and a failure leaves no gap and no message.
  return (
    <div style={{ position: "relative", width: `${width}px`, height: ready ? undefined : 0, overflow: ready ? "visible" : "hidden" }}>
      <canvas
        ref={canvasRef}
        aria-hidden="true"
        style={{ width: "100%", height: `${FAMILY_HEIGHT_PX}px`, display: "block", touchAction: "none", cursor: "grab" }}
      />
      <div hidden={!ready} style={{ position: "relative", height: "1.4em" }}>
        {shown.map((member, index) => (
          <a
            key={member.sim}
            href={format({ page: "person", run, sim: String(member.sim) })}
            style={{
              position: "absolute",
              left: `${familyNamePercent(index, shown.length)}%`,
              width: `${FAMILY_SLOT_PX}px`,
              transform: "translateX(-50%)",
              textAlign: "center",
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {member.firstName}
          </a>
        ))}
      </div>
    </div>
  );
}
