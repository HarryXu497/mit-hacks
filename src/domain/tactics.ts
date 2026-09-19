import { z } from "zod";

export const TACTIC_TAXONOMY_VERSION = "tactics-v1" as const;

export const tacticKeys = ["balanced", "high_press", "low_block", "wide"] as const;
export const tacticKeySchema = z.enum(tacticKeys);
export type TacticKey = z.infer<typeof tacticKeySchema>;

export const TACTIC_TAXONOMY = [
  {
    key: "balanced",
    displayName: "Balanced",
    downstreamValue: "Balanced",
    description: "No other supported tactic clearly dominates the session.",
  },
  {
    key: "high_press",
    displayName: "High Press",
    downstreamValue: "HighPress",
    description: "Coordinated pressure in advanced areas to recover possession.",
  },
  {
    key: "low_block",
    displayName: "Low Block",
    downstreamValue: "LowBlock",
    description: "Compact defensive organization close to the defending goal.",
  },
  {
    key: "wide",
    displayName: "Wide",
    downstreamValue: "Wide",
    description: "Use of field width to stretch the opposition and create space.",
  },
] as const satisfies ReadonlyArray<{
  key: TacticKey;
  displayName: string;
  downstreamValue: string;
  description: string;
}>;

export const evidenceStrengthSchema = z.enum(["strong", "moderate", "weak"]);
export const modelSelectionReasonSchema = z.enum(["best_match", "uncertain_fallback"]);
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

export function downstreamValueFor(
  tactic: TacticKey,
): "Balanced" | "HighPress" | "LowBlock" | "Wide" {
  return TACTIC_TAXONOMY.find((entry) => entry.key === tactic)!.downstreamValue;
}
