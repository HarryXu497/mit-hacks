# Canopy Clash — art direction

## Style reference, interpreted

The Bloons TD6 model references are useful for their design grammar rather than their specific content. The recurring lessons are:

- oversized eyes and faces remain readable from far away;
- bodies are chunky and economical rather than anatomically detailed;
- each character has a strong silhouette;
- a small number of saturated accent colors carries the identity;
- accessories communicate role and personality quickly;
- the rendering is clean, smooth, and intentionally simplified.

Use those principles to make original soccer monkeys. Do not import extracted archive assets into the project.

Reference archive: <https://models.spriters-resource.com/pc_computer/bloonstd6/>.

## Core palette

These are starting tokens, not a restriction on every material.

| Token | Hex | Use |
|---|---:|---|
| Canopy deep | `#1E4E3C` | distant leaves, shadows, deep wood |
| Leaf green | `#75B84B` | foliage, grass variation, banners |
| Sun gold | `#F6C84A` | highlights, flowers, trim, score accents |
| Tribe orange | `#E67E32` | orange team, warm flags, clay, fire |
| River blue | `#47B7D7` | blue team, water, cool flags, haze |
| Jungle ink | `#203239` | scoreboard panel, outlines, deepest contrast |

Use warm ivory rather than pure white for field lines and nets where possible: `#FFF3D5`.

## Character direction

### Shared monkey silhouette

- compact torso;
- oversized rounded head;
- large high-contrast eyes;
- short limbs with readable hands and feet;
- curled tail visible in silhouette;
- slightly forward athletic stance;
- no tiny facial details that disappear at match distance.

The model should read as “monkey” before the viewer notices the tribe costume.

### Orange tribe — Sunfruit clan

Visual language: warm wood, banana yellow, clay orange, sun disks, leaf-gold trim.

Possible readable accents:

- orange sash or wrist wraps;
- banana-shaped crest or belt token;
- warm headband;
- small sun emblem on the chest;
- amber goal net or rope trim.

### Blue tribe — Riverstone clan

Visual language: river blue, cyan, cool stone, shell or moon shapes, waterfall highlights.

Possible readable accents:

- blue sash or wrist wraps;
- rounded river-stone shoulder piece;
- cyan headband;
- crescent or wave emblem on the chest;
- blue goal net or rope trim.

The tribes should share the same base monkey model so the match remains visually fair and the content pipeline stays small. Variation should come from color, accessories, and emblem shape.

### Animation-ready visual requirements

The first visual model should support at least:

- idle breathing or weight shift;
- run cycle with the tail acting as a readable counterweight;
- jump pose;
- contact or impact reaction;
- goal/reset celebration later.

These can initially be simple transform animations or procedural motion. The art pass does not require a full animation system immediately.

## Pitch and markings

The field is the quietest major surface.

- replace the current dark gray platform with saturated but not fluorescent grass;
- use two or three broad mowing tones rather than noisy texture detail;
- keep white or warm-ivory markings thick enough to survive the elevated camera;
- preserve a clear center circle, halfway line, penalty areas, and goal mouths;
- use small flowers or leaf marks only near the perimeter;
- avoid placing decorative rocks, roots, or tall grass inside the active play area.

The pitch is intentionally larger than the original test box: the current layout uses a 36 x 24 field and larger goals while preserving player/ball dynamics. Camera framing, field material, and surrounding set dressing reinforce that scale without changing physics parameters.

## Goals

Goals should feel handmade by the tribes:

- timber or bamboo uprights;
- rope lashings at joints;
- bright, readable netting;
- carved emblems at the corners;
- tribe-colored cloth banners;
- small fire bowls or glowing flowers near, but not inside, the goal mouth.

The goal silhouette must stay clean against the jungle background. Use darker foliage behind the net and lighter trim on the posts for contrast.

## Jungle composition

### Near frame

Use large leaves, broad vines, stones, and partial foreground silhouettes sparingly. They can frame the camera and add depth, but should not cover the players or goals.

### Stadium ring

Use the arena perimeter for:

- rope bridges;
- viewing platforms;
- huts and crates;
- banners;
- small monkey spectators or carved effigies;
- torches, flowers, and tribe markers.

These objects should be decorative and non-collidable in the first pass.

### Distant background

Layer large, low-detail forms behind the field:

- cliff walls;
- a river gorge;
- one or two waterfalls;
- stacked canopy planes;
- distant bridges and huts;
- atmospheric haze.

The background should use lower contrast and slightly cooler values than the playable area so depth comes from color separation as well as geometry.

## Camera and composition

The current camera is an elevated perspective view. Keep the same broad 3/4 match view as the starting point, then test a more orthographic-looking projection if it improves RL readability.

Composition priorities:

1. full pitch fits comfortably inside the frame;
2. goals are visible without camera movement;
3. scoreboard shrine anchors the far edge;
4. waterfall or gorge creates a strong center-back horizon;
5. foreground foliage provides depth without becoming a vignette that blocks play.

Do not use an extreme top-down view. The jungle architecture, goal depth, shadows, and monkey silhouettes need some vertical face to read.

## Lighting and materials

Aim for soft stylized PBR rather than photorealism:

- warm directional sunlight from one side;
- strong ambient fill so the pitch remains readable;
- soft contact shadows under players and props;
- slightly saturated materials;
- broad specular highlights on wet stone, water, and painted wood;
- darker, cooler background values;
- restrained emissive accents for scoreboard segments, fireflies, and tribal markers.

The first implementation can achieve most of the look with low-poly meshes, material tuning, lighting, and careful value separation. A custom cel shader is optional and should come only after the composition is working.

## Scoreboard and broadcast layer

The scoreboard is part of the jungle architecture:

- orange score on the left;
- timer in the center;
- blue score on the right;
- dark wood or jungle-ink panel;
- vine framing;
- team emblems rather than generic color blocks;
- small animated glow, dust, or firefly accents.

The existing seven-segment display is valuable because it is already readable. Reskin the housing and surroundings before replacing the digits.

## Motion and atmosphere

Choose a few loops with high payoff:

- slow leaf sway;
- waterfall motion;
- tiny floating pollen or fireflies;
- flags reacting gently to wind;
- subtle scoreboard pulse;
- soft grass or flower movement near the boundary.

These should establish a living world without creating screen-wide noise. Avoid continuous camera shake, excessive bloom, or particles crossing the pitch.

## Readability checklist

| Element | Required treatment |
|---|---|
| Ball | highest local contrast against grass; minimal obstruction |
| Players | clear silhouette, face, tail, and team accent |
| Goals | bright posts and dark contrasting background |
| Field lines | warm ivory, consistent thickness, no texture interference |
| Scoreboard | dark panel, bright digits, framed by jungle architecture |
| Background | lower contrast and slightly cooler than gameplay elements |

## Originality guardrail

The project can be inspired by the broad appeal of stylized monkey games while remaining visually its own. The distinctive identity should come from the two soccer tribes, the living scoreboard shrine, the river-gorge stadium, and the handmade goal architecture—not from reproducing recognizable Bloons characters or props.
