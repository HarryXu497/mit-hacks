# Canopy Clash — visual vision

## One-sentence thesis

**A fast, readable two-tribe soccer match staged inside a monkey-built jungle arena, where the pitch is a clean broadcast surface and the surrounding canopy feels alive.**

The game should read instantly from a distant camera: grass, ball, monkeys, goals, score, and team colors first; spectacle second.

## What the source image is telling us

The supplied reference begins with a sparse gray reinforcement-learning box and imagines it as a lush, colorful sports game. The important transformation is not “add random jungle objects.” It is:

- turn the rectangular test area into a deliberate stadium;
- make the field substantially larger and easier to read;
- move visual richness to the perimeter and background;
- use monkeys and tribal props to give the simulation a memorable identity;
- keep the gameplay surface geometric and uncluttered.

The composition has three layers:

```text
background: cliffs, canopy, waterfalls, huts, bridges, haze
midground: goals, banners, spectators, scoreboard shrine
foreground: clean pitch, monkeys, ball, markings, readable shadows
```

## Creative north star

The target feeling is **premium playful jungle broadcast**, not a generic tropical map and not a photorealistic rainforest.

The arena should feel like a place the tribes built for an important annual match. Every major prop should answer at least one of these questions:

- Which tribe does it belong to?
- Does it help the viewer understand the match?
- Does it make the jungle feel inhabited?
- Does it frame the action more clearly?

If it does none of these, it probably does not belong in the first pass.

## Signature element

The memorable set piece should be a **living scoreboard shrine** behind the far touchline:

- a dark wooden scoreboard held inside a carved stone or timber frame;
- vines and broad leaves wrapping the edges;
- a narrow waterfall or bright river gorge behind it;
- orange and blue tribal emblems flanking the score;
- subtle firefly or pollen particles around the structure.

This gives the game a recognizable silhouette and turns the existing scoreboard into part of the world instead of a detached UI panel.

## Five design principles

### 1. Readability before decoration

The ball, player bodies, goals, and field markings must remain legible at the gameplay camera distance. The center circle and goal mouths should never be buried under foliage, shadows, or particles.

### 2. The jungle frames the field

Most complexity belongs outside the playable rectangle. The field should feel open, bright, and intentional. Jungle density should increase toward the outer frame and distant background.

### 3. Tribe identity comes from shape and material

Orange and blue are useful team colors, but identity should also come from banner shapes, carved symbols, goal construction, accessories, and environmental storytelling.

### 4. Stylized, not generic

Use rounded low-poly forms, broad color blocks, oversized faces, and soft shadows. Avoid default sci-fi neon, realistic foliage scans, and interchangeable asset-store jungle clutter.

### 5. One strong idea per surface

Let the scoreboard shrine be the hero background element. Let the monkeys own the field. Let the waterfalls and canopy establish depth. Do not make every object compete for attention.

## What we are deliberately not doing yet

- no rules or scoring redesign;
- no change to the RL action or observation contract;
- no increase in agent count as part of the art pass;
- no decorative collision geometry;
- no copied Bloons TD6 meshes, textures, logos, or exact character designs;
- no fully custom toon-rendering pipeline before the low-poly composition works;
- no field clutter that makes agent behavior difficult to inspect.

## Success test

At a glance, a viewer should be able to answer:

1. Where is the ball?
2. Which player is orange and which is blue?
3. Where are the goals?
4. What kind of world is this?
5. What is the current score?

If the jungle makes any of those answers harder, the jungle needs to move outward, become quieter, or lose contrast.
