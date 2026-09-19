import { MoreVertical, Pencil, RotateCcw, Square } from "lucide-react";
import type { SessionStatus } from "../domain/types";

interface TopBarProps {
  title: string;
  status: SessionStatus;
  elapsedLabel: string;
  onTitleChange: (title: string) => void;
  onPrimaryAction: () => void;
  onReset: () => void;
}

export function TopBar({
  title,
  status,
  elapsedLabel,
  onTitleChange,
  onPrimaryAction,
  onReset,
}: TopBarProps) {
  const isRecording = status === "recording";
  const primaryLabel = isRecording
    ? "Stop recording"
    : status === "ready"
      ? "Start recording"
      : "Resume";

  return (
    <header className="topbar">
      <div className="brand-lockup">
        <h1>Tactic Lab</h1>
        <span className="topbar-divider" />
        <label className="session-title">
          <span className="sr-only">Session title</span>
          <input value={title} onChange={(event) => onTitleChange(event.target.value)} />
          <Pencil aria-hidden="true" size={15} />
        </label>
      </div>
      <div className="topbar-actions">
        <button
          className={`record-button ${isRecording ? "is-recording" : ""}`}
          onClick={onPrimaryAction}
        >
          <span className="record-symbol" aria-hidden="true">
            {isRecording ? <Square size={10} fill="currentColor" /> : null}
          </span>
          {primaryLabel}
        </button>
        <span className="elapsed-time" aria-label={`Elapsed time ${elapsedLabel}`}>
          {elapsedLabel}
        </span>
        <span className="topbar-divider compact" />
        <button className="quiet-button" onClick={onReset}>
          <RotateCcw size={16} aria-hidden="true" />
          Reset
        </button>
        <span className="topbar-divider compact" />
        <button className="icon-button" aria-label="Session menu">
          <MoreVertical size={18} />
        </button>
      </div>
    </header>
  );
}
