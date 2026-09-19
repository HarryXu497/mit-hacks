import { Check, Clipboard, Download, Edit3, Info, Mic2 } from "lucide-react";
import { useState } from "react";
import type { TacticalOutput } from "../domain/interpret";
import type { SessionStatus, TranscriptSegment } from "../domain/types";
import { formatTime } from "../domain/time";

interface TranscriptPanelProps {
  transcripts: TranscriptSegment[];
  activeTranscriptId?: string;
  status: SessionStatus;
  speechSupported: boolean;
  listening: boolean;
  result: TacticalOutput | null;
  isInterpreting: boolean;
  interpretationNotice: string | null;
  onManualAdd: (text: string) => void;
  onEdit: (segmentId: string, text: string) => void;
  onGenerate: () => void;
  onBackToTranscript: () => void;
}

export function TranscriptPanel({
  transcripts,
  activeTranscriptId,
  status,
  speechSupported,
  listening,
  result,
  isInterpreting,
  interpretationNotice,
  onManualAdd,
  onEdit,
  onGenerate,
  onBackToTranscript,
}: TranscriptPanelProps) {
  const [manualText, setManualText] = useState("");
  const [copied, setCopied] = useState(false);

  if (result) {
    const json = JSON.stringify(result, null, 2);
    const copy = async () => {
      await navigator.clipboard.writeText(json);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    };
    const download = () => {
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = "tactic-session.json";
      anchor.click();
      URL.revokeObjectURL(url);
    };

    return (
      <aside className="side-panel result-panel" aria-label="Tactical JSON result">
        <div className="panel-header result-header">
          <div>
            <span className="panel-kicker">Interpretation complete</span>
            <h2>Tactical JSON</h2>
          </div>
          <button className="text-button" onClick={onBackToTranscript}>Transcript</button>
        </div>
        <div className="result-summary">
          <div className="success-mark"><Check size={16} strokeWidth={2.5} /></div>
          <div>
            <strong>{result.summary.name}</strong>
            <p>{result.steps.length} tactical {result.steps.length === 1 ? "step" : "steps"} with evidence links</p>
          </div>
        </div>
        {interpretationNotice ? <p className="interpretation-notice">{interpretationNotice}</p> : null}
        <pre className="json-preview" tabIndex={0}>{json}</pre>
        <div className="result-actions">
          <button className="quiet-button" onClick={copy}>
            {copied ? <Check size={16} /> : <Clipboard size={16} />}
            {copied ? "Copied" : "Copy"}
          </button>
          <button className="primary-button" onClick={download}>
            <Download size={16} />
            Download JSON
          </button>
        </div>
      </aside>
    );
  }

  const recording = status === "recording";
  const canGenerate = status === "review" || status === "interpreted";

  return (
    <aside className="side-panel" aria-label="Live transcript">
      <div className="panel-header">
        <h2>Live transcript</h2>
        <div className={`listening-state ${recording ? "active" : ""}`}>
          <span className="listening-dot" />
          <Mic2 size={15} aria-hidden="true" />
          {recording
            ? listening
              ? "Listening…"
              : speechSupported
                ? "Mic paused"
                : "Manual mode"
            : "Not recording"}
        </div>
      </div>

      <div className="transcript-list" aria-live="polite">
        {transcripts.length === 0 ? (
          <div className="transcript-empty">
            <Mic2 size={22} strokeWidth={1.6} />
            <p>Your timestamped transcript will appear here.</p>
          </div>
        ) : (
          transcripts.map((segment) => (
            <div
              className={`transcript-row ${activeTranscriptId === segment.id ? "is-active" : ""}`}
              key={segment.id}
            >
              <time>{formatTime(segment.startMs)}</time>
              <input
                aria-label={`Transcript at ${formatTime(segment.startMs)}`}
                defaultValue={segment.text}
                key={`${segment.id}-${segment.text}`}
                onBlur={(event) => {
                  const text = event.target.value.trim();
                  if (text && text !== segment.text) onEdit(segment.id, text);
                }}
              />
              <Edit3 size={14} aria-hidden="true" />
            </div>
          ))
        )}
      </div>

      {recording && (!speechSupported || !listening) ? (
        <form
          className="manual-transcript"
          onSubmit={(event) => {
            event.preventDefault();
            const value = manualText.trim();
            if (!value) return;
            onManualAdd(value);
            setManualText("");
          }}
        >
          <label htmlFor="manual-transcript">Add or correct a transcript line</label>
          <div>
            <input
              id="manual-transcript"
              value={manualText}
              onChange={(event) => setManualText(event.target.value)}
              placeholder="Type what the coach said…"
            />
            <button type="submit">Add</button>
          </div>
        </form>
      ) : null}

      <div className="interpret-zone">
        <div className="interpret-note">
          <Info size={17} aria-hidden="true" />
          <span>
            {canGenerate
              ? "Generate a structured, evidence-linked JSON representation."
              : "Stop recording to generate a structured JSON representation of this session."}
          </span>
        </div>
        <button
          className="generate-button"
          disabled={!canGenerate || isInterpreting}
          onClick={onGenerate}
        >
          {isInterpreting ? "Interpreting…" : "Generate JSON"}
        </button>
      </div>
    </aside>
  );
}
