# MonkeyForge GPU worker

Implements `docs/gpu-worker-contract.md`. Deployed and verified on the ASUS Ascent GX10
(NVIDIA GB10, aarch64, DGX OS / Ubuntu 24.04, CUDA 13.0, Python 3.12).

The generator is pluggable on purpose. Install the contract first with a stub that needs no GPU,
prove the network path, and only then fight TRELLIS.2. If you install both at once, a failure is
ambiguous — CUDA, multipart, auth, or export — and you will burn hours isolating it.

## Stage 1 — prove the wire (no GPU) — VERIFIED

`setup_gx10.sh` installs from `offline/wheels/` when present, so this works with the box joined to
nothing but the laptop. Build the bundle first from a machine with internet; see `offline/README.md`.

```bash
scp -r worker/ <user>@<gx10-host>:~/monkeyforge-worker/
ssh <user>@<gx10-host>
cd ~/monkeyforge-worker/worker
bash setup_gx10.sh          # prints arch + python tag, installs, self-tests before opening a port
```

Start it:

```bash
cd ~/monkeyforge-worker/worker
MONKEYFORGE_WORKER_GENERATOR=stub \
MONKEYFORGE_WORKER_TOKEN=<token> \
MONKEYFORGE_WORKER_CACHE=$HOME/monkeyforge-worker/cache \
~/monkeyforge-worker/.venv/bin/uvicorn server:app --host 0.0.0.0 --port 8600
```

Point the orchestrator at it in `.env`:

```dotenv
MONKEYFORGE_PROVIDER=http
MONKEYFORGE_GPU_ENDPOINT=http://<gx10-ip>:8600/v1/image-to-3d
MONKEYFORGE_GPU_TOKEN=<the same token>
MONKEYFORGE_COMPILER=blender
```

Verified end to end on 2026-09-19: orchestrator on Windows -> HTTP contract -> GX10 worker ->
binary GLB -> Blender compile -> socket attachment -> measurement -> features -> validation, with
`valid: true`, `measured: true`, all seven sockets present and `socket_fit_score: 1.0`.

## Stage 2 — TRELLIS.2 on aarch64

Everything below was needed in practice. The order matters.

**1. CUDA toolkit.** The box ships the driver only; there is no `nvcc`.

```bash
sudo apt-get install -y cuda-toolkit-13-0     # ~3-4 GB, from the sbsa repo
export PATH=/usr/local/cuda/bin:$PATH
export CUDA_HOME=/usr/local/cuda
```

**2. torch must be newer than upstream pins.** `setup.sh` asks for torch 2.6.0+cu124, but cu124
predates Blackwell and has **no `sm_120`/`sm_121` kernels**, so TRELLIS.2's own pinned stack cannot
run on a GB10. Use cu130:

```bash
pip install torch --index-url https://download.pytorch.org/whl/cu130
```

GB10 reports compute capability **12.1**. torch 2.14's arch list stops at `sm_120`, so `sm_121` runs
via PTX JIT — which costs nothing measurable once warm: 85.8 TFLOP/s fp16, 240 GB/s copy bandwidth
(88% of the 273 GB/s spec). Benchmark *after* warmup; a cold first matmul reads ~12x slower because
JIT compilation lands inside the timing loop.

**3. Build the CUDA extensions.** Three hard requirements, each of which fails confusingly:

```bash
sudo apt-get install -y python3.12-dev        # else: fatal error: Python.h: No such file
sed -i 's/c++17/c++20/g' setup.py             # else: #error C++20 or later required to use PyTorch
git clone --recurse-submodules ...            # else: missing api_gpu.cu / Eigen/Dense
```

torch 2.14 requires C++20 while these extensions hardcode `-std=c++17`. And **both** CuMesh and
TRELLIS.2 carry git submodules — a `--depth 1` clone without `--recurse-submodules` fails later, at
compile time, with a missing-source error that looks like a code bug.

```bash
export TORCH_CUDA_ARCH_LIST="12.0;12.1"
pip install --no-build-isolation git+https://github.com/JeffreyXiang/FlexGEMM.git
pip install --no-build-isolation ~/CuMesh            # cloned with submodules
pip install --no-build-isolation ~/TRELLIS.2/o-voxel
pip install git+https://github.com/NVlabs/nvdiffrast.git
pip install "git+https://github.com/EasternJournalist/utils3d.git@9a4eb15e4021b67b12c460c7057d642626897ec8"
```

All of `flex_gemm`, `cumesh` and `o_voxel` compile for `sm_121`. `nvcc` accepts
`-gencode arch=compute_121,code=sm_121`, so **the GPU architecture is not a blocker** — the blockers
are all toolchain version mismatches.

`utils3d` must come from that exact commit; the PyPI releases have conflicting dependencies.

**4. Attention backend.** TRELLIS.2 has **two independent** switches in different modules:

| module | global | env var | accepts `sdpa`? |
|---|---|---|---|
| `modules/attention/config.py` | `BACKEND` | `ATTN_BACKEND` | yes |
| `modules/sparse/config.py` | `ATTN` | `SPARSE_ATTN_BACKEND`, falls back to `ATTN_BACKEND` | **no** |

Sparse attention only dispatches `xformers` / `flash_attn` / `flash_attn_3`, then raises
`ValueError`. Sparse attention *is* the model for structured latents, so one of those three is
mandatory — and flash-attn has no aarch64+Blackwell wheel while xformers ships only an sdist (whose
build pulls a second copy of torch and stalls).

This deployment therefore adds a native sdpa path:
`trellis2/modules/sparse/attention/sdpa_varlen.py`, patched into five sites (the whitelist,
`full_attn.py`, and three in `windowed_attn.py` — including `calc_window_partition`, which otherwise
leaves `attn_func_args` undefined). Verified against a naive masked reference at ~1e-6 max error
across uniform, ragged, cross-attention, single-sequence and many-window cases.

Note for anyone considering xformers instead: upstream's windowed **cross-attention** branch does
`q = q.unsqueeze(0)` on the SparseTensor rather than `q_feats`, then calls `q.replace(out)` — that
path looks broken upstream regardless of whether xformers builds.

**5. DINOv3 access — BLOCKED, needs a human.**

`Trellis2ImageTo3DPipeline.from_pretrained` loads
[`facebook/dinov3-vitl16-pretrain-lvd1689m`](https://huggingface.co/facebook/dinov3-vitl16-pretrain-lvd1689m)
as its image conditioning model. That repo is **gated**: it returns
`GatedRepoError: 401` until a logged-in user accepts Meta's licence.

To unblock, on any machine:

1. Sign in at huggingface.co.
2. Open the model page above and click **Agree and access repository**.
3. Create a read token at huggingface.co/settings/tokens.
4. On the box:

```bash
export HF_TOKEN=hf_xxxxxxxxxxxx
# or: hf auth login
```

Do not substitute DINOv2 — TRELLIS.2's flow models were trained on DINOv3 features, so the encoders
are not interchangeable and the output would be noise. The ONNX community mirror is not usable
either: `DINOv3ViTModel.from_pretrained` needs PyTorch weights via transformers.

**6. Weights.** TRELLIS.2-4B is 16.24 GB over 22 files. Download them out-of-band; the in-process
download stalls.

```bash
HF_HUB_DISABLE_XET=1 hf download microsoft/TRELLIS.2-4B --max-workers 4
```

HuggingFace's Xet chunk reassembly deadlocks on unauthenticated parallel requests — observed as five
ESTABLISHED connections, `futex_do_wait`, and 33 KB moved in 45 seconds with zero of the five 2.58 GB
shards completing in 13 minutes. `HF_HUB_DISABLE_XET=1` falls back to plain HTTP range requests and
sustains 3-6 MB/s (~25 minutes). Doing it with the CLI rather than inside the pipeline also makes it
resumable and independently diagnosable.

Two of the five DiT shards are 1024-resolution variants (5.16 GB). `docs/compute.md` argues 512 is
the right demo target, so pinning resolution would cut the download by roughly a third.

Then restart with `MONKEYFORGE_WORKER_GENERATOR=trellis`.

## Notes

- **Warm, never cold.** Weights load in the startup hook. 16 GB loaded inside a request would ruin
  any latency budget. `/health` reports `ready`; wait for it before sending traffic.
- **Responses are cached** by SHA-256 of image, prompt, seed, triangle target and generator version,
  per contract step 5. A rehearsed demo replays instantly. Delete the cache dir to force rebuilds.
- **The GX10 will not give real-time generation.** 273 GB/s against an H100's 3.35 TB/s, and this
  workload is bandwidth-bound rather than capacity-bound, so 128 GB of unified memory does not help.
  Expect minutes per accessory. Its real value is as a free, always-warm box for grinding out the
  candidate library the style ranker needs — see `docs/compute.md`.
- **`pipeline.json` runs 12 sampler steps** with guidance interval `[0.6, 1.0]`. Step count is the
  first lever to trim if generation is slower than the demo allows.
- **Keep the live demo on the procedural path.** Generate the library ahead of time; let the reveal
  read from it.
