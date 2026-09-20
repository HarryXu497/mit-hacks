# Compute and latency planning

These are planning ranges, not guarantees. Measure the exact worker image before the demo and keep
it warm. Model loading, CUDA extension compilation, background removal, GLB export, network transfer,
and Blender processing are not included in headline model benchmarks.

## One character variant

With the canonical body approach, body morph and palette application are effectively immediate. The
expensive unit is each newly generated accessory.

| Path | Recommended GPU | Warm end-to-end estimate for one accessory | Notes |
|---|---|---:|---|
| Curated/procedural prop | CPU/local GPU | under 5 s | No foundation-model generation |
| TRELLIS.2/Pixal3D at 512 | H100/H200 | 20–60 s | Best live-demo target |
| TRELLIS.2/Pixal3D at 1024 | H100/H200 | 35–90 s | Better detail than most low-poly props need |
| Low-VRAM local path | RTX 4090/5090 | roughly 1.5–6 min | Strongly dependent on quantization/offload |
| Constrained 12 GB path | RTX 4070-class | roughly 6–10 min | Useful for development, weak for a live reveal |

Four sequential candidates approximately quadruple the generation portion. For the demo, generate
two candidates or run candidates concurrently on separate workers.

The official TRELLIS.2 H100 measurements report approximately 3 seconds at 512 cubed, 17 seconds at
1024 cubed, and 60 seconds at 1536 cubed for its shape-plus-material stages. Treat the table above as
the fuller product latency around those core numbers.

## Memory

- Official TRELLIS.2 minimum: one NVIDIA GPU with 24 GB VRAM.
- Comfortable generation worker: 48–80 GB VRAM.
- API/orchestrator: 2–4 CPU cores and 4–8 GB RAM.
- Generation worker system RAM: 32 GB minimum; 64 GB recommended.
- Disk: reserve 50–100 GB for weights, CUDA builds, environments, caches, and generated assets.

An H200 is not necessary for a single asset. H100 and H200 use the same Hopper generation; H200's
main advantages are 141 GB memory and 4.8 TB/s memory bandwidth versus 80 GB and 3.35 TB/s for H100.
That headroom matters more for large training jobs, large batches, or multiple resident models than
for one 4B image-to-3D inference request.

## Rental economics snapshot

As of September 2026, RunPod lists secure-cloud rates around $0.74/hour for RTX 4090, $3.49/hour for
H100 SXM, and $4.59/hour for H200. At those rates, one fully utilized minute costs approximately
$0.012, $0.058, and $0.077 respectively. Cold starts and idle time usually cost more than the actual
generation.

Useful source links:

- TRELLIS.2 requirements and H100 timings: https://github.com/microsoft/TRELLIS.2
- Pixal3D low-VRAM and multi-view usage: https://github.com/TencentARC/Pixal3D
- NVIDIA H100/H200 specifications: https://docs.nvidia.com/enterprise-reference-architectures/hgx-ai-factory-h100-h200-b200/latest/components.html
- Current RunPod pricing: https://www.runpod.io/pricing

