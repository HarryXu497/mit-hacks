# GPU worker contract

The API service makes one multipart request to the configured `MONKEYFORGE_GPU_ENDPOINT`.

## Request

`POST /v1/image-to-3d`

Fields:

- `reference`: PNG or JPEG concept/sketch.
- `prompt`: isolated accessory description.
- `seed`: deterministic integer seed.
- `target_triangles`: desired final budget. The compiler still enforces it.
- `part_schema`: JSON array of semantic parts; may be empty.

Headers:

- `Authorization: Bearer <MONKEYFORGE_GPU_TOKEN>` when a token is configured.
- `X-MonkeyForge-Contract: 1`

## Response

Return the generated GLB directly:

```http
HTTP/1.1 200 OK
Content-Type: model/gltf-binary
X-Generator: pixal3d
X-Generator-Version: <commit-or-model-id>

<binary GLB>
```

The maximum accepted response size defaults to 100 MiB. Errors should use an ordinary non-2xx HTTP
status and a short JSON body. The orchestrator records the error without repeatedly retrying an
invalid request.

## Recommended worker implementation

1. Remove the image background.
2. Run Pixal3D at 512 or 1024 resolution.
3. Export GLB with material maps.
4. Return the raw GLB; leave low-poly conversion and sockets to the compiler.
5. Cache by the SHA-256 of the image bytes, prompt, seed, and model version.

