# Base model preparation

## Current implementation: original rigged bases

`scripts/blender_build_original_base.py` builds a team-owned low-poly monkey from primitives. It
exports the same seven sockets and a canonical skeleton for four role silhouettes:

```text
balanced -> runner -> defender -> goalkeeper
```

Each generated GLB is approximately 1K triangles for the body and has rigid per-part weights so
the attachment contract can be tested before detailed deformation weights are authored. The API
selects the profile from `CharacterSpec.archetype`.

```powershell
$blender = "C:\Program Files\Blender Foundation\Blender 5.2\blender.exe"
& $blender --background --python scripts\blender_build_original_base.py -- `
  --profile balanced `
  --blend-output assets\work\monkeyforge_balanced.blend `
  --glb-output assets\base\monkeyforge_balanced.glb
```

This is the public-distribution path. The downloaded BTD6 assets are kept only as private
proportion and silhouette references.

The local Dart Monkey prototype is a 614-vertex, 706-triangle OBJ in a usable T-pose. It has UVs and
materials but no skeleton, skin weights, animations, sockets, or license file. Ninja Monkey is also
unrigged and should be treated as a costume reference after the base skeleton works.

## Automated preparation

After installing Blender 4.x, run from the repository root:

```powershell
& "C:\Program Files\Blender Foundation\Blender 4.5\blender.exe" `
  --background `
  --python scripts\blender_prepare_reference.py `
  -- `
  --input "assets\reference\btd6_prototype\Dart Monkey\dartmonkey.obj" `
  --blend-output "assets\work\monkey_base_clean.blend" `
  --glb-output "assets\base\monkey_base.glb"
```

The script removes the held dart, renames the body pieces, scales the character to 1.4 metres,
places its feet at ground level, creates approximate attachment sockets, saves an editable Blender
file, and exports a static GLB.

## Manual work required after the script

1. Open `assets/work/monkey_base_clean.blend`.
2. Confirm the model faces Blender `-Y`; rotate and apply transforms if necessary.
3. Reposition every `SOCKET_*` empty against the actual surface.
4. Add the canonical skeleton described below.
5. Parent the body and eyelid meshes using automatic weights.
6. Correct shoulder, elbow, hip, knee, hand, foot, face, and tail weights.
7. Parent each socket to the appropriate bone while preserving world transforms.
8. Test the deformation poses before exporting over `assets/base/monkey_base.glb`.

Recommended minimum bones:

```text
root -> pelvis -> spine -> chest -> neck -> head
chest -> upper_arm.L -> forearm.L -> hand.L
chest -> upper_arm.R -> forearm.R -> hand.R
pelvis -> thigh.L -> shin.L -> foot.L
pelvis -> thigh.R -> shin.R -> foot.R
pelvis -> tail.01 -> tail.02 -> tail.03
```

Because the source body is only a few hundred vertices, automatic weighting will need manual
correction around deforming joints. Preserve the large head and chunky silhouette; add geometry only
where a joint cannot bend cleanly.

## Release constraint

The prototype sources and generated base GLB are ignored by Git. Replace them with an original or
properly licensed derivative before public distribution.
