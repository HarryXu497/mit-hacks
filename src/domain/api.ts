import { tacticalOutputSchema, type TacticalOutput } from "./interpret";
import type { Session } from "./types";

export async function requestInterpretation(
  session: Session,
  signal?: AbortSignal,
): Promise<TacticalOutput> {
  const response = await fetch("/api/interpret", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ schemaVersion: "1.0", session }),
    signal,
  });

  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as
      | { code?: string; message?: string }
      | null;
    throw new InterpretationRequestError(
      body?.code ?? "INTERPRETATION_FAILED",
      body?.message ?? "The tactical interpretation request failed.",
    );
  }

  return tacticalOutputSchema.parse(await response.json());
}

export class InterpretationRequestError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
  }
}
