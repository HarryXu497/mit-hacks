import type { Session } from "./types";

const STORAGE_KEY = "tactic-lab/session/v1";

export function saveSession(session: Session): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(session));
  } catch {
    // Persistence is best-effort; the live session must continue if storage is unavailable.
  }
}

export function loadSession(): Session | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<Session>;
    if (
      parsed.schemaVersion !== 1 ||
      typeof parsed.id !== "string" ||
      typeof parsed.title !== "string" ||
      !Array.isArray(parsed.events)
    ) {
      return null;
    }
    return parsed as Session;
  } catch {
    return null;
  }
}

export function clearSavedSession(): void {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // No action required.
  }
}
