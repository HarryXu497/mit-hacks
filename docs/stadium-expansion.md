# Stadium expansion work

Objective: substantially larger five-a-side jungle stadium, preserving the geometric art style, with convincing animated water, an elevated field in a layered landscape, coherent boundaries, and spectators performing waves.

## Acceptance evidence required

- Ten actual player entities, five on each team, with distinct formation positions that survive round and goal resets.
- Expanded pitch and arena with matching decorative and collision boundaries; player and ball dynamics preserved.
- Elevated stadium supported by visible rock strata, with surrounding lower landscape, river, vegetation and bridges.
- Waterfalls with animated water surfaces, foam and landing pools, integrated into the landscape.
- Populated spectator terraces with traveling crowd waves, visible in captures at multiple times.
- Broadcast camera frames the new stadium and keeps action readable.
- Runtime captures, appropriate formation/reset checks, and compilation before completion.

## In progress

Implemented: a 48 x 32 pitch, ten roster entities with distinct formation slots used by resets, crowd-wave components, animated water ribbons/highlights/foam, a lower river landscape, faceted foundation, and bridges. A native capture at `stadium-expansion-preview.png` confirms ten visible players and the larger setting.

The thick foundation slab has been replaced with faceted strata. Native captures `stadium-wave-a.png` (frame 90) and `stadium-wave-b.png` (frame 210) show changed crowd poses and waterfall highlights. Spectators raise their arms during the traveling wave. Five library tests pass, including actual calls to all three reset systems verifying ten restored formation positions and zero velocities. The native executable builds and both captures exit successfully.

Player dynamics remain unchanged. The existing training wrapper has not been expanded into a ten-agent training API; keyboard input controls one player per team while the remaining players expose their existing PlayerInput components for agent integration.
