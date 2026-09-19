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

## Capture the actual rendered scene

```powershell
$env:CANOPY_CAPTURE = "$PWD\docs\jungle-preview.png"
& "$env:USERPROFILE\.cargo\bin\cargo.exe" run --bin cube-soccer
Remove-Item Env:CANOPY_CAPTURE
```

Capture mode saves the camera image after rendering has warmed up and then exits. This is a real engine render, not an image-generation mockup.

## Where the artwork lives

`src/jungle.rs` builds the procedural field, markings, monkey children, goals, foliage, bridges, huts, water, and scoreboard housing. It reuses meshes and materials. No downloaded models or textures are required.

The scene deliberately uses geometric characters and faceted environment forms. The earlier rounded-character concept has been superseded. There are two simulated players in this version; additional trained team members are a separate integration with the gameplay owner.

`src/systems/camera.rs` controls the match camera, and `src/rendering/lighting.rs` controls daylight and fill. The existing scoreboard digits still display real score and round time.

The upstream RL environment remains a placeholder. This visual implementation does not make the Python training environment functional or add multi-agent gameplay. Existing gameplay code and its known limitations remain the baseline.
