import { useEffect, useMemo, useRef, useState } from "react";
import type {
  Annotation,
  BoardState,
  EntityRef,
  Point,
  Tool,
} from "../domain/types";
import { clampPoint, createId } from "../domain/session";
import { BoardToolbar } from "./BoardToolbar";

const VIEWBOX = { width: 800, height: 900, left: 34, top: 32, fieldWidth: 732, fieldHeight: 836 };

interface TacticsBoardProps {
  board: BoardState;
  tool: Tool;
  interactive: boolean;
  canUndo: boolean;
  elapsedMs: number;
  onToolChange: (tool: Tool) => void;
  onMove: (
    entity: EntityRef,
    from: Point,
    to: Point,
    path: Point[],
    startedAtMs: number,
  ) => void;
  onAnnotationAdd: (annotation: Annotation, startedAtMs: number) => void;
  onAnnotationRemove: (annotationId: string) => void;
  onUndo: () => void;
}

type DragState =
  | {
      kind: "entity";
      entity: EntityRef;
      from: Point;
      current: Point;
      path: Point[];
      startedAtMs: number;
    }
  | {
      kind: "annotation";
      annotationKind: "arrow" | "freehand";
      points: Point[];
      startedAtMs: number;
    };

export function TacticsBoard({
  board,
  tool,
  interactive,
  canUndo,
  elapsedMs,
  onToolChange,
  onMove,
  onAnnotationAdd,
  onAnnotationRemove,
  onUndo,
}: TacticsBoardProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [drag, setDrag] = useState<DragState | null>(null);

  useEffect(() => {
    if (!interactive) setDrag(null);
  }, [interactive]);

  const previewBoard = useMemo(() => {
    if (!drag || drag.kind !== "entity") return board;
    if (drag.entity.kind === "ball") return { ...board, ball: drag.current };
    const playerId = drag.entity.id;
    return {
      ...board,
      players: board.players.map((player) =>
        player.id === playerId ? { ...player, position: drag.current } : player,
      ),
    };
  }, [board, drag]);

  const pointFromEvent = (event: React.PointerEvent): Point => {
    const rect = svgRef.current!.getBoundingClientRect();
    const xSvg = ((event.clientX - rect.left) / rect.width) * VIEWBOX.width;
    const ySvg = ((event.clientY - rect.top) / rect.height) * VIEWBOX.height;
    return clampPoint({
      x: (xSvg - VIEWBOX.left) / VIEWBOX.fieldWidth,
      y: (ySvg - VIEWBOX.top) / VIEWBOX.fieldHeight,
    });
  };

  const beginEntityDrag = (event: React.PointerEvent, entity: EntityRef, from: Point) => {
    event.stopPropagation();
    if (!interactive || tool !== "select") return;
    svgRef.current?.setPointerCapture(event.pointerId);
    setDrag({ kind: "entity", entity, from, current: from, path: [from], startedAtMs: elapsedMs });
  };

  const beginBoardGesture = (event: React.PointerEvent<SVGSVGElement>) => {
    if (!interactive || (tool !== "arrow" && tool !== "draw")) return;
    const point = pointFromEvent(event);
    event.currentTarget.setPointerCapture(event.pointerId);
    setDrag({
      kind: "annotation",
      annotationKind: tool === "arrow" ? "arrow" : "freehand",
      points: [point],
      startedAtMs: elapsedMs,
    });
  };

  const moveGesture = (event: React.PointerEvent<SVGSVGElement>) => {
    if (!drag) return;
    const point = pointFromEvent(event);
    setDrag((current) => {
      if (!current) return current;
      const previous = current.kind === "entity" ? current.path.at(-1)! : current.points.at(-1)!;
      if (Math.hypot(point.x - previous.x, point.y - previous.y) < 0.008) return current;
      if (current.kind === "entity") {
        return { ...current, current: point, path: [...current.path, point] };
      }
      return { ...current, points: [...current.points, point] };
    });
  };

  const endGesture = (event: React.PointerEvent<SVGSVGElement>) => {
    if (!drag) return;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (drag.kind === "entity") {
      if (distance(drag.from, drag.current) > 0.005) {
        onMove(drag.entity, drag.from, drag.current, drag.path, drag.startedAtMs);
      }
    } else if (drag.points.length > 1) {
      onAnnotationAdd(
        { id: createId("annotation"), kind: drag.annotationKind, points: drag.points },
        drag.startedAtMs,
      );
    }
    setDrag(null);
  };

  const draftAnnotation =
    drag?.kind === "annotation"
      ? { id: "draft", kind: drag.annotationKind, points: drag.points }
      : null;

  return (
    <section className="board-region" aria-label="Tactics board">
      <div className={`board-frame ${interactive ? "is-interactive" : "is-frozen"}`}>
        <div className="frame-top" />
        <div className="field-surface">
          <svg
            ref={svgRef}
            className={`field-svg tool-${tool}`}
            viewBox={`0 0 ${VIEWBOX.width} ${VIEWBOX.height}`}
            role="img"
            aria-label="Portrait soccer pitch with ten movable players and a ball"
            onPointerDown={beginBoardGesture}
            onPointerMove={moveGesture}
            onPointerUp={endGesture}
            onPointerCancel={endGesture}
          >
            <defs>
              <filter id="token-shadow" x="-40%" y="-40%" width="180%" height="180%">
                <feDropShadow dx="0" dy="4" stdDeviation="3" floodColor="#031008" floodOpacity=".55" />
              </filter>
              <filter id="chalk-shadow" x="-10%" y="-10%" width="120%" height="120%">
                <feGaussianBlur in="SourceAlpha" stdDeviation="1.2" result="blur" />
                <feOffset in="blur" dy="1" result="offset" />
                <feMerge><feMergeNode in="offset" /><feMergeNode in="SourceGraphic" /></feMerge>
              </filter>
              <marker id="arrowhead" markerWidth="8" markerHeight="8" refX="6" refY="3" orient="auto">
                <path d="M0,0 L0,6 L7,3 z" fill="none" stroke="white" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
              </marker>
            </defs>
            <PitchMarkings />

            {[...previewBoard.annotations, ...(draftAnnotation ? [draftAnnotation] : [])].map(
              (annotation) => (
                <AnnotationPath
                  key={annotation.id}
                  annotation={annotation}
                  erasable={interactive && tool === "erase" && annotation.id !== "draft"}
                  onErase={() => onAnnotationRemove(annotation.id)}
                />
              ),
            )}

            {previewBoard.players.map((player) => {
              const point = toSvg(player.position);
              return (
                <g
                  key={player.id}
                  className={`player-token ${player.team}`}
                  transform={`translate(${point.x} ${point.y})`}
                  onPointerDown={(event) =>
                    beginEntityDrag(event, { kind: "player", id: player.id }, player.position)
                  }
                  role="button"
                  aria-label={`${player.team} player ${player.id}`}
                >
                  <circle r="22" />
                  <circle className="token-ring" r="18.5" />
                  <text textAnchor="middle" dominantBaseline="central">{player.id}</text>
                </g>
              );
            })}

            <BallToken
              point={previewBoard.ball}
              onPointerDown={(event) =>
                beginEntityDrag(event, { kind: "ball" }, previewBoard.ball)
              }
            />
          </svg>
        </div>
        <BoardToolbar
          tool={tool}
          disabled={!interactive}
          canUndo={canUndo}
          onToolChange={onToolChange}
          onUndo={onUndo}
        />
      </div>
    </section>
  );
}

function PitchMarkings() {
  return (
    <g className="pitch-markings" fill="none" stroke="currentColor" strokeWidth="3.2">
      <rect x="34" y="32" width="732" height="836" />
      <path d="M34 450H766" />
      <circle cx="400" cy="450" r="72" />
      <circle cx="400" cy="450" r="3.5" fill="currentColor" stroke="none" />
      <rect x="194" y="32" width="412" height="142" />
      <rect x="292" y="32" width="216" height="72" />
      <path d="M338 174A68 68 0 0 0 462 174" />
      <circle cx="400" cy="126" r="3.5" fill="currentColor" stroke="none" />
      <rect x="194" y="726" width="412" height="142" />
      <rect x="292" y="796" width="216" height="72" />
      <path d="M338 726A68 68 0 0 1 462 726" />
      <circle cx="400" cy="774" r="3.5" fill="currentColor" stroke="none" />
      <path d="M34 55A23 23 0 0 0 57 32M743 32A23 23 0 0 0 766 55M34 845A23 23 0 0 1 57 868M743 868A23 23 0 0 1 766 845" />
      <path d="M355 32V15H445V32M355 868V885H445V868" strokeWidth="2.4" />
    </g>
  );
}

function AnnotationPath({
  annotation,
  erasable,
  onErase,
}: {
  annotation: Annotation;
  erasable: boolean;
  onErase: () => void;
}) {
  const path = pathFromPoints(annotation.points);
  return (
    <path
      className={`chalk-line ${erasable ? "is-erasable" : ""}`}
      d={path}
      fill="none"
      stroke="white"
      strokeWidth="5"
      strokeLinecap="round"
      strokeLinejoin="round"
      markerEnd={annotation.kind === "arrow" ? "url(#arrowhead)" : undefined}
      filter="url(#chalk-shadow)"
      onPointerDown={(event) => {
        if (!erasable) return;
        event.stopPropagation();
        onErase();
      }}
    />
  );
}

function BallToken({
  point,
  onPointerDown,
}: {
  point: Point;
  onPointerDown: (event: React.PointerEvent<SVGGElement>) => void;
}) {
  const position = toSvg(point);
  return (
    <g
      className="ball-token"
      transform={`translate(${position.x} ${position.y})`}
      onPointerDown={onPointerDown}
      role="button"
      aria-label="Ball"
    >
      <circle r="16" fill="#f7f7f2" />
      <path d="M0-7 6-2 4 6H-4L-6-2ZM-6-2-13-6M6-2 13-6M4 6 8 13M-4 6-8 13" fill="#111" stroke="#111" strokeWidth="2" strokeLinejoin="round" />
    </g>
  );
}

function toSvg(point: Point) {
  return {
    x: VIEWBOX.left + point.x * VIEWBOX.fieldWidth,
    y: VIEWBOX.top + point.y * VIEWBOX.fieldHeight,
  };
}

function pathFromPoints(points: Point[]) {
  if (!points.length) return "";
  const [first, ...rest] = points.map(toSvg);
  if (rest.length === 0) return `M${first.x} ${first.y}`;
  return `M${first.x} ${first.y} ${rest.map((point) => `L${point.x} ${point.y}`).join(" ")}`;
}

function distance(a: Point, b: Point) {
  return Math.hypot(a.x - b.x, a.y - b.y);
}
