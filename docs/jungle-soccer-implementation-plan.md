# Canopy Clash — visual implementation plan

## Technical north star

Build a presentation layer around the existing simulation. The visual pass should be removable or replaceable without changing the RL contract.

The following should remain stable unless the gameplay owner explicitly requests a change:

- Rapier bodies, colliders, collision groups, and physics constants;
- player and ball transforms used by observations;
- goal sensors and scoring events;
- action and observation sizes;
- reset and scoring behavior;
- the number of simulated players in the current environment.

## Current-code mapping

| Existing area | Visual work | Gameplay boundary |
|---|---|---|
| `src/entities/cube_player.rs` | replace the visible cube with a monkey visual hierarchy; retain `CubePlayer`, `PlayerInput`, collider, rigid body, and velocity | do not change player physics or observation transforms |
| `src/entities/ball.rs` | improve ball material and add optional visual trail or markings | retain ball collider, mass, and spawn position |
| `src/entities/field.rs` | grass material, mowing stripes, field markings, boundary treatment | retain field colliders and dimensions initially |
| `src/entities/goal.rs` | timber/bamboo posts, tribe nets, emblems, rope details | retain goal colliders and `GoalSensor`s |
| `src/entities/arena.rs` | jungle perimeter, cliffs, scoreboard shrine, banners | decorative additions should have no colliders |
| `src/systems/camera.rs` | tune elevated 3/4 framing and pitch visibility | no gameplay state dependency |
| `src/rendering/lighting.rs` | warm key light, cool fill, stronger value separation | rendering only |
| `src/systems/display.rs` | preserve seven-segment logic; reskin its housing and surrounding props | retain score/timer data flow |
| new `entities/jungle.rs` or `environment` module | spawn reusable foliage, rocks, bridges, huts, waterfall layers, and spectators | default to visual-only entities |

## Asset strategy

Start with procedural Bevy meshes and materials for composition:

- low-poly leaves and broad foliage cards;
- beveled or rounded monkey primitives;
- cylinders and cuboids for bamboo, wood, rope, and stone;
- simple planes or stepped meshes for distant waterfalls and cliffs;
- material variants for the two tribes.

Add imported original assets only after the camera composition is working. Use a small number of reusable meshes with transform and material variation instead of many unique meshes.

Suggested future layout:

```text
assets/
  characters/
    monkey_base.glb
    monkey_orange.glb
    monkey_blue.glb
  environment/
    foliage/
    rocks/
    structures/
    waterfall/
  materials/
  ui/
```

The repository currently has no requirement to use external assets for the first pass. Procedural geometry is enough to validate the direction.

## Staged build plan

### Phase 0 — baseline and visual test scene

- capture the current camera view;
- confirm the current field, goals, players, ball, and scoreboard are still visible;
- create a simple visual-only test arrangement around the existing game;
- establish a repeatable screenshot or short playtest for comparison.

### Phase 1 — pitch and palette

- replace gray field materials with grass tones;
- add mowing stripes and warm-ivory markings;
- remove or visually soften the fluorescent test borders;
- establish the palette tokens from the art-direction document;
- tune lighting so the field remains readable.

### Phase 2 — monkey player visuals

- keep the physics cube hidden or visually subordinated;
- attach a simple shared monkey model as a child of each player;
- add orange and blue accessory variants;
- ensure face, tail, and team accent are readable at the gameplay camera distance;
- verify that child visuals do not interfere with physics queries or reset behavior.

### Phase 3 — goals and scoreboard shrine

- reskin goal posts as timber or bamboo;
- add rope, nets, emblems, and team banners;
- build the scoreboard housing into a shrine structure;
- preserve existing digit entities and update systems;
- frame the structure with vines and controlled foliage.

### Phase 4 — jungle perimeter

- add non-collidable stadium-ring props;
- build layered cliffs, canopy, river, and waterfall silhouettes behind the field;
- add sparse spectators, huts, crates, and bridges;
- use lower contrast in the distance;
- test that no prop hides a goal or player.

### Phase 5 — atmosphere and motion

- animate flags, leaves, waterfall layers, scoreboard glow, and subtle particles;
- add contact shadows and restrained emissive accents;
- add visual goal/reset effects only if they remain readable;
- tune the scene at multiple window sizes and camera distances.

### Phase 6 — polish and performance

- reuse meshes and materials wherever possible;
- remove unnecessary decorative colliders;
- reduce overdraw from transparent foliage and nets;
- measure frame pacing with the target number of visual entities;
- verify that headless or training configurations do not depend on rendering-only resources.

## Important implementation constraints

### Keep gameplay and visuals separate

Decorative objects should not be included in movement raycasts, collision groups, goal detection, or observation queries. If a visual object needs a collider later, that should be a deliberate gameplay decision.

### Preserve the two-player assumption for now

The current project is built around two simulated players. Make monkey visuals reusable and scalable, but do not add spectators or extra players as simulated agents just to match the concept image.

### Avoid hiding the physics body too early

During development, an optional debug toggle should be able to show the original cube collider or a simple wireframe proxy. This makes it possible to diagnose RL and physics behavior after the monkey visuals are attached.

### Do not couple rendering to training

The art pass should not force the RL environment to open a window, allocate visual-only assets, or depend on camera systems. Rendering should remain optional.

## Acceptance checklist

### Visual

- the scene reads as a jungle stadium within one second;
- the two tribes are distinguishable without relying only on hue;
- the ball and players are visible from the match camera;
- goal mouths and field markings remain clear;
- the scoreboard is integrated into the environment;
- the background has depth without overpowering the pitch.

### Technical

- player and ball physics are unchanged;
- score detection is unchanged;
- resets still place the visual children correctly;
- no decorative object unexpectedly collides with players or ball;
- headless/training paths do not require visual assets;
- rendering remains performant with the selected amount of foliage and particles.

### Originality

- no extracted Bloons TD6 assets are shipped;
- monkey characters use original proportions, accessories, and markings;
- the scoreboard shrine and tribe architecture are specific to this project;
- the final scene feels like a jungle soccer world, not a reskin of another game.
