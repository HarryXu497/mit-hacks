# Training strategy

Do not train the 3D foundation model from scratch for the initial product. Keep geometry generation
frozen and train the narrow components that provide project-specific value.

## Phase 1: no training

- Canonical monkey body and morph targets.
- Deterministic palette and toon shader.
- Frozen image-to-3D model.
- Hard mesh validation and socket fitting.

This establishes the baseline that every trained component must beat.

## Phase 2: style ranker

Render each generated candidate from eight fixed cameras. Encode the views with a frozen visual
encoder, mean-pool the embeddings, and train a small MLP to predict pairwise human preference.

Suggested record format is in `training/dataset.example.jsonl`. Store only team-owned or properly
licensed art and rendered outputs.

Pairwise objective:

```text
loss = -log(sigmoid(score(preferred) - score(rejected)))
```

At inference, combine the learned score with hard measurements such as triangle budget, disconnected
components, bounding-box compliance, and socket intersection.

The repository includes a runnable pairwise baseline that uses these six normalized features:

```text
semantic_similarity, silhouette_score, palette_score,
socket_fit_score, triangle_budget_score, disconnected_penalty
```

Train it with:

```powershell
python scripts\train_style_ranker.py `
  --dataset training\dataset.example.jsonl `
  --output output\training\style-ranker.json
```

The checked-in examples are plumbing smoke data, not evidence of visual quality. Replace them with
team-owned renders and human pairwise choices. The next upgrade is to append frozen CLIP/DINO or
render-encoder embeddings while keeping the same pairwise objective.

## Phase 3: 2D style adapter

Train a LoRA on original concept sheets so a rough sketch becomes a clean, isolated accessory in the
project's visual language. A useful starter set is 40–100 original props, rendered from four views
and paired with line-art variants and captions.

## Phase 4: 3D fine-tuning

Only consider adapting the texture or image-conditioning portion of the 3D model after collecting a
few hundred high-quality, consistently rendered 3D assets. Full shape-model training is a research
project, not a hackathon dependency.
