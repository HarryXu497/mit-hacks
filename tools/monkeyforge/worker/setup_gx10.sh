#!/usr/bin/env bash
# Stage 1 setup for the ASUS Ascent GX10 (NVIDIA GB10, arm64, DGX OS).
#
# Installs ONLY the contract worker with the stub generator -- no CUDA build, no weights.
# The point is to prove the network path end to end before spending hours on aarch64 wheels.
# TRELLIS.2 is stage 2; see README.md.
#
# Installs from the bundled offline/wheels directory when it is present, so this works with the
# box attached to nothing but the laptop. Falls back to PyPI when the box has internet.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT="${MONKEYFORGE_WORKER_PORT:-8600}"
VENV="${MONKEYFORGE_WORKER_VENV:-$HOME/monkeyforge-worker/.venv}"
WHEELS="$HERE/offline/wheels"

echo "==> platform"
uname -a
ARCH="$(uname -m)"
echo "arch: $ARCH"
if [ "$ARCH" != "aarch64" ]; then
  echo "WARNING: expected aarch64; the offline wheels are arm64-only."
fi

echo
echo "==> GPU"
if command -v nvidia-smi >/dev/null 2>&1; then
  nvidia-smi || echo "WARNING: nvidia-smi present but failed"
else
  echo "note: nvidia-smi not found. Fine for the stub; a blocker for TRELLIS."
fi

echo
echo "==> python"
python3 --version
PY_TAG="$(python3 -c 'import sys; print(f"cp{sys.version_info.major}{sys.version_info.minor}")')"
echo "tag: $PY_TAG"

python3 -m venv "$VENV"
# shellcheck disable=SC1091
source "$VENV/bin/activate"

echo
if [ -d "$WHEELS" ] && [ -n "$(ls -A "$WHEELS" 2>/dev/null)" ]; then
  echo "==> installing from offline bundle ($WHEELS)"
  if ! ls "$WHEELS"/pydantic_core-*"$PY_TAG"-*.whl >/dev/null 2>&1; then
    echo "ERROR: no pydantic_core wheel for $PY_TAG in the bundle."
    echo "       Bundle has:"
    ls "$WHEELS"/pydantic_core-*.whl | sed 's/^/         /'
    echo "       Re-download on the laptop with --python-version ${PY_TAG#cp}"
    exit 1
  fi
  python -m pip install --quiet --no-index --find-links "$WHEELS" \
    fastapi uvicorn python-multipart
else
  echo "==> no offline bundle; installing from PyPI (box needs internet)"
  python -m pip install --upgrade pip --quiet
  python -m pip install --quiet "fastapi>=0.116,<1" "uvicorn>=0.35,<1" "python-multipart>=0.0.20,<1"
fi

echo
echo "==> verifying imports"
python - <<'PYTHON'
import fastapi, uvicorn, multipart, pydantic
print(f"  fastapi          {fastapi.__version__}")
print(f"  uvicorn          {uvicorn.__version__}")
print(f"  pydantic         {pydantic.__version__}")
print("  python-multipart ok")
PYTHON

echo
echo "==> worker self-test (stub generator, no network)"
cd "$HERE"
python - <<'PYTHON'
import os
os.environ.setdefault("MONKEYFORGE_WORKER_GENERATOR", "stub")
import asyncio
from generators import load_generator

generator = load_generator("stub")
asyncio.run(generator.warm())
glb = asyncio.run(
    generator.generate(image=b"x", prompt="test", seed=1, target_triangles=2500, part_schema=[])
)
assert glb.startswith(b"glTF"), "stub did not produce a GLB"
print(f"  stub generator OK ({len(glb)} bytes, magic={glb[:4]!r})")
PYTHON

echo
echo "==> firewall"
if command -v ufw >/dev/null 2>&1 && ufw status 2>/dev/null | grep -q "Status: active"; then
  sudo ufw allow "$PORT"/tcp || echo "WARNING: could not open port $PORT"
else
  echo "ufw inactive or absent; nothing to open"
fi

echo
echo "==> reachable addresses"
hostname -I 2>/dev/null || ip -4 addr show | grep -oP '(?<=inet\s)\d+(\.\d+){3}'

cat <<EOF

Setup complete. Start the worker with:

    source $VENV/bin/activate
    cd $HERE
    MONKEYFORGE_WORKER_GENERATOR=stub \\
    MONKEYFORGE_WORKER_TOKEN=<pick-a-token> \\
    uvicorn server:app --host 0.0.0.0 --port $PORT

Then from the laptop:  curl http://<gx10-ip>:$PORT/health
EOF
