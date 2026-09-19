import { Eraser, MousePointer2, Pencil, Redo2, Undo2 } from "lucide-react";
import type { Tool } from "../domain/types";

interface BoardToolbarProps {
  tool: Tool;
  disabled: boolean;
  canUndo: boolean;
  onToolChange: (tool: Tool) => void;
  onUndo: () => void;
}

const tools: Array<{ id: Tool; label: string; icon: typeof MousePointer2 }> = [
  { id: "select", label: "Select", icon: MousePointer2 },
  { id: "arrow", label: "Arrow", icon: Redo2 },
  { id: "draw", label: "Draw", icon: Pencil },
  { id: "erase", label: "Erase", icon: Eraser },
];

export function BoardToolbar({
  tool,
  disabled,
  canUndo,
  onToolChange,
  onUndo,
}: BoardToolbarProps) {
  return (
    <div className="board-toolbar" role="toolbar" aria-label="Board tools">
      {tools.map(({ id, label, icon: Icon }) => (
        <button
          key={id}
          className={tool === id ? "is-selected" : ""}
          disabled={disabled}
          onClick={() => onToolChange(id)}
          aria-pressed={tool === id}
        >
          <Icon size={18} strokeWidth={1.9} />
          <span>{label}</span>
        </button>
      ))}
      <span className="toolbar-divider" />
      <button disabled={disabled || !canUndo} onClick={onUndo}>
        <Undo2 size={18} strokeWidth={1.9} />
        <span>Undo</span>
      </button>
    </div>
  );
}
