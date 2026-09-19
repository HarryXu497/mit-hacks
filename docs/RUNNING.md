# Running the geometric jungle scene

This workspace contains the original Aijo24/Cube-soccer-3D source at commit `e08a93f09a23e1327997d1378e6f971da85aaec7`, with a procedural jungle presentation layer. The original MIT license is retained in `LICENSE`.

## Start

Install Rust and the native linker for your platform, then run:

```sh
cargo run --bin cube-soccer
```

On the development Windows machine, Rust was installed without modifying PATH. Use:

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" run --bin cube-soccer
```

The initial native build downloads and compiles Bevy and Rapier and can take several minutes. Later builds reuse the compilation cache.

Orange moves with WASD and jumps with Space. Blue moves with the arrow keys and jumps with Enter. Close the window to exit.

The game permanently uses the smooth ball-following perspective broadcast camera. It may crop distant portions of the ground; camera tracking does not affect physics or controls.

The playable layout is now a 48 x 32 field with 8-unit goal openings and 5-unit goal height. Player/ball sizes, gravity, acceleration, jump force and damping retain their existing values. Level dimensions and five-player formation slots must be shared with the training integration.

The stepped spectator terraces are decorative and have no colliders.

## Capture the actual rendered scene

```powershell
$env:CANOPY_CAPTURE = "$PWD\docs\jungle-preview.png"
& "$env:USERPROFILE\.cargo\bin\cargo.exe" run --bin cube-soccer
Remove-Item Env:CANOPY_CAPTURE
```

Capture mode saves the camera image after rendering has warmed up and then exits. This is a real engine render, not an image-generation mockup.

## Where the artwork lives

`src/jungle.rs` builds the procedural field, markings, monkey children, goals, foliage, bridges, huts, water, and scoreboard housing. It reuses meshes and materials. No downloaded models or textures are required.

The scene uses geometric characters and faceted environment forms. There are five physical players per team, with distinct formation positions restored after every reset. Keyboard controls move formation slot zero on each team; the other eight players await agent inputs. The upstream RL interface remains a two-agent placeholder, so five-agent training integration still belongs to the gameplay owner.

The river gorge, water ribbons, foam, swaying vegetation and spectator waves are animated presentation geometry without colliders. Water is a stylized visual effect rather than a fluid simulation. Set `CANOPY_CAPTURE_FRAME` to a frame number (default 90) to inspect different animation phases in capture mode.

`src/systems/camera.rs` controls the match camera, and `src/rendering/lighting.rs` controls daylight and fill. The existing scoreboard digits still display real score and round time.

The upstream RL environment remains a placeholder. This visual implementation does not make the Python training environment functional or add multi-agent gameplay. Existing gameplay code and its known limitations remain the baseline.
