# Summit reference fidelity audit

## Target

Match the supplied elevated jungle-stadium composition: playable summit and side stands on one substantial hill, deep forested ravines, mountains, waterfalls, rope bridges, warm timber, temple carvings, banners, and a bright readable pitch. Geometric modelling and cel lighting remain intentional. Physics and training interfaces must remain unchanged.

## Current evidence and iteration

- `summit-review-01.png`: actual engine render. Summit and side stands implemented, but the close camera cropped the cliff and distant terrain resembled columns. Not accepted as completion.
- `summit-review-02.png`: widened native view proves the summit, temple facade, side stands, bridges, banners, and lower waterfalls are present. Reveals excessive bare cliff faces, undersized scoreboard, repetitive vegetation, and overly dominant background peaks. Not accepted as completion.
- Subsequent source changes raise the view angle, taper background mountains, flare the summit base, shorten the decorative buttresses, and increase forest/ledge planting. These require a fresh render before visual claims.
- `summit-review-03.png` verified the planting/camera changes but exposed embedded facade ornaments and cascades after widening the cliff. The upper cliff is being tapered inward again while retaining the broader lower foot. This is a visual regression caught by rendering, not by structural tests.
- `summit-review-04.png`: native render confirms the facade and near cascades are visible again. Build and six library tests pass. The scene still lacks the reference's broad horizon, dense integrated cliff vegetation, prominent scoreboard and refined materials; the central background shrine pillar is especially oversized in the frame. These remain active work, not completed fidelity gates.

## Remaining visual gates

Latest pass: `summit-review-05.png` verifies lower distant peaks and a larger scoreboard. It also revealed vines overlapping the resized display and a shrine foundation needing support. Follow-up edits move the vines to the new frame height, make score segments unlit, and add shrine foundation geometry. A regression test now verifies inactive score segments retain StandardMaterial handles so normal scoring updates continue to work (seven tests passed before these final presentation adjustments).

`summit-review-06.png` verifies the corrected frame foliage and shrine foundation. The build and all seven tests pass after these changes. Remaining prominent differences: no visible sky/ocean horizon, sparse vegetation on the outcrop tops, over-regular stands and cliff silhouettes, and a much simpler character/material finish than the supplied image. Reference fidelity is still not achieved.

- Compare pitch occupancy and camera angle directly to the reference; preserve visibility of the near cliff and a substantial distant landscape.
- Improve the scoreboard size, brightness, frame and ornamentation.
- Make mountain and forest layering read as a vast landscape, not a wall of repeated props.
- Refine palm silhouettes, ledges, water sources and landing pools; check for embedded or floating geometry.
- Improve colour, lighting and surface detail toward the reference's warm, lush appearance.
- Verify all ten players, goals and line markings remain legible.
- Run tests and a native render after the last change. Green tests verify structural invariants, not reference-level beauty.

The goal remains active. No claim of near-1:1 reference fidelity has been established.
