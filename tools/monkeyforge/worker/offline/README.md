# Offline wheel bundle

`wheels/` holds linux-aarch64 wheels so `setup_gx10.sh` can install the stage 1 worker on a GX10
with no internet — which is the normal case when the laptop is joined to the box's own hotspot
rather than to a router.

The bundle is git-ignored. Rebuild it from a machine that has internet:

```powershell
python -m pip download --dest worker\offline\wheels `
  --platform manylinux2014_aarch64 --python-version 312 --implementation cp `
  --only-binary=:all: "fastapi>=0.116,<1" "uvicorn>=0.35,<1" "python-multipart>=0.0.20,<1"

# pydantic-core is the only compiled dependency, so cover the other interpreters too
foreach ($v in @("310","311","313")) {
  python -m pip download --dest worker\offline\wheels `
    --platform manylinux2014_aarch64 --python-version $v --implementation cp `
    --only-binary=:all: --no-deps "pydantic-core==2.46.5"
}
```

Notes:

- Plain `uvicorn`, not `uvicorn[standard]`. The `standard` extra pulls `uvloop`, `httptools` and
  `websockets`, which are compiled and would each need an aarch64 wheel. The worker does not need
  them, and avoiding them keeps this bundle to a handful of pure-Python files plus one binary.
- `pydantic_core` is that one binary, and it is interpreter-specific. `setup_gx10.sh` checks the
  box's `cpXY` tag against the bundle and fails with a clear message rather than a confusing
  resolver error if it is missing.
- `--only-binary=:all:` is deliberate: without it pip may fetch an sdist that then tries to compile
  on the box, which is exactly what this bundle exists to avoid.
