# Jungle Soccer visual direction

This folder captures the visual direction for turning the current Cube Soccer RL environment into a polished jungle sports game without changing the simulation yet.

Working title: **Canopy Clash**.

Current implementation: see [Running the geometric jungle scene](RUNNING.md). The accepted direction is geometric from the start: block-headed monkeys, faceted rocks and foliage, simple wood structures, and broad material colors. Earlier mentions of rounded character detail describe possible future polish, not the first build.

[Accepted geometric concept](geometric-concept.png) is the generated design reference. It is not a screenshot of the implemented game.

Read the documents in this order:

1. [Visual vision](jungle-soccer-vision.md) — the creative north star and non-negotiable principles.
2. [Art direction](jungle-soccer-art-direction.md) — the concrete visual system for characters, pitch, jungle, lighting, and UI.
3. [Implementation plan](jungle-soccer-implementation-plan.md) — how to apply the art direction to the existing Bevy project while preserving RL behavior.

## Scope

This is an art and presentation pass. It does not redesign the rules, action space, observations, physics, scoring, or agent training environment.

The current simulation is treated as the stable foundation. Visual entities may be added, replaced, or re-parented, but decorative entities should not become gameplay colliders unless explicitly approved later.

## Reference policy

The supplied jungle-soccer image is the primary composition reference. The Bloons TD6 model archive is a secondary style reference for readable silhouettes, exaggerated proportions, and bold character accents. We should create original meshes, materials, props, and tribal motifs rather than ship extracted game assets or copy distinctive character designs.
