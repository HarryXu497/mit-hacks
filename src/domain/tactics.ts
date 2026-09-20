import { z } from "zod";

export const TACTIC_TAXONOMY_VERSION = "tactics-v2" as const;

export const tacticKeys = ["balanced", "highpress", "gegenpress", "lowblock", "parkthebus", "counterattack", "possession", "wingplay", "narrowmidblock", "alloutattack"] as const;
export const tacticKeySchema = z.enum(tacticKeys);
export type TacticKey = z.infer<typeof tacticKeySchema>;

export const TACTIC_TAXONOMY = [
  {
    "key": "balanced",
    "displayName": "Balanced",
    "downstreamValue": "balanced",
    "description": "Balanced attacking and defensive commitment with neutral pressing and width; choose when the coaching supports this shape, not merely when uncertain."
  },
  {
    "key": "highpress",
    "displayName": "High Press",
    "downstreamValue": "highpress",
    "description": "Coordinated pressure high up the field."
  },
  {
    "key": "gegenpress",
    "displayName": "Gegenpress",
    "downstreamValue": "gegenpress",
    "description": "Immediate aggressive pressure after losing possession."
  },
  {
    "key": "lowblock",
    "displayName": "Low Block",
    "downstreamValue": "lowblock",
    "description": "Compact defensive organization near the own goal."
  },
  {
    "key": "parkthebus",
    "displayName": "Park the Bus",
    "downstreamValue": "parkthebus",
    "description": "Very deep defensive shape with minimal attacking commitment."
  },
  {
    "key": "counterattack",
    "displayName": "Counter-Attack",
    "downstreamValue": "counterattack",
    "description": "Defend deeper and break forward quickly after recovery."
  },
  {
    "key": "possession",
    "displayName": "Possession",
    "downstreamValue": "possession",
    "description": "Spread support and maintain passing options to keep the ball."
  },
  {
    "key": "wingplay",
    "displayName": "Wing Play",
    "downstreamValue": "wingplay",
    "description": "Use wide support to stretch opponents."
  },
  {
    "key": "narrowmidblock",
    "displayName": "Narrow Mid-Block",
    "downstreamValue": "narrowmidblock",
    "description": "Compact central shape with moderate pressure."
  },
  {
    "key": "alloutattack",
    "displayName": "All-Out Attack",
    "downstreamValue": "alloutattack",
    "description": "Commit most support players forward with a high line."
  }
] as const;

export const playerOverrideSchema = z.object({
  playerId: z.number().int().min(1).max(5),
  tactic: tacticKeySchema,
  evidence: z.object({ eventIds: z.array(z.string()), transcriptSegmentIds: z.array(z.string()) }),
});

export const evidenceStrengthSchema = z.enum(["strong", "moderate", "weak"]);
export const modelSelectionReasonSchema = z.literal("best_match");
export const selectionReasonSchema = z.enum([
  "best_match",
  "uncertain_fallback",
  "system_fallback",
]);

export const modelClassificationSchema = z.object({
  primaryTactic: tacticKeySchema,
  secondaryTraits: z.array(tacticKeySchema),
  alternativeTactics: z.array(tacticKeySchema),
  selectionReason: modelSelectionReasonSchema,
  evidenceStrength: evidenceStrengthSchema,
  explanation: z.string(),
});

export const classificationSchema = modelClassificationSchema.extend({
  selectionReason: selectionReasonSchema,
});

export const semanticInterpretationSchema = z.object({
  classification: modelClassificationSchema,
  summary: z.object({
    name: z.string(),
    objective: z.string(),
  }),
  playerOverrides: z.array(playerOverrideSchema),
  phases: z.array(
    z.object({
      id: z.string(),
      primaryTactic: tacticKeySchema,
      instruction: z.string(),
      objective: z.string(),
      actors: z.array(z.number().int().min(1).max(10)),
      evidence: z.object({
        eventIds: z.array(z.string()),
        transcriptSegmentIds: z.array(z.string()),
      }),
    }),
  ),
});

export type SemanticInterpretation = z.infer<typeof semanticInterpretationSchema>;

export function downstreamValueFor(tactic: TacticKey): TacticKey {
  return tactic;
}
