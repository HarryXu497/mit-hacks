# Forest stadium: natural geometry pass

## Intent

A match-day gathering in a rainforest clearing, rather than a pitch displayed on a rock pedestal. Geometric describes the modelling language, not the placement: silhouettes remain faceted, but terrain, planting, and water follow continuous, irregular forms.

## Implemented

- Replaced the stacked island and perimeter rock ring with a continuous, vertex-coloured height field. The playing plane stays flat; hills and a carved river valley live outside it.
- Expanded the surrounding world to a 300-unit terrain with layered trees, buttress roots, palms, low foliage and foreground ferns. Deterministic irregular placement avoids repeated rows of scenery.
- Replaced sparse seating with nine terrace rows, four audience sections, dense static team-colour crowd silhouettes, access stairs and structural posts. Fur and clothing remain readable at broadcast distance without per-spectator animation.
- Added team entrance structures, fruit stalls, paths, banners and a timber river crossing to suggest how spectators arrive and spend match day.
- Added an embedded forward material shader with stepped illumination. Quantizing light rather than RGB preserves material colour variation and existing cast shadows.
- Added a winding river with moving surface normals, Fresnel sky tint, glints and bank-edge foam, plus a connected waterfall with moving streaks and landing foam.
- Batched far forest and mountain vegetation into static material groups with fewer, broader canopy masses; close venue geometry remains individually modeled for readability.

## Boundaries and limitations

No physics configuration, field/goal collider dimensions, forces, controls, scoring or training interfaces were changed in this pass. The existing permanent broadcast camera edits were preserved. Larger scale comes from the venue and surrounding landscape, not new simulation dimensions.

Water is a stylized shader effect, not a fluid solver. Its Fresnel colour is an approximation of sky reflection, not a reflection of nearby scene geometry. Shore foam is based on ribbon coordinates, not scene-depth sampling. Spectators are static procedural low-detail silhouettes, not individually simulated agents. Foliage is static; waterfall and surface-water motion are retained as the highest-value animation, with reduced strip/ripple counts for runtime speed. The environmental story is conveyed through props and layout, not a narrative system.

## Verification

`cargo test --lib` covers the flat playable surface, submerged river bed, crowd population, unchanged presentation-layer colliders, ten-player formations and all reset paths. Native screenshots exercise shader compilation and the permanent broadcast view; see `forest-cel-review.png` for the rendered result.

Code: `src/jungle/landscape.rs`, `src/rendering/stylized.rs`, and `assets/shaders/jungle.wgsl`. The shader is embedded in the executable so running outside the repository does not lose the material assets.
