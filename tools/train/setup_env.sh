#!/usr/bin/env bash
# One-time training environment setup (Sprint 17b).
# Creates tools/train/.venv on python3.11 with CUDA torch wheels.
#
# The RTX 5060 is Blackwell (sm_120): needs torch >= 2.7 built for cu128.
# System python3 is 3.14 (no torch wheels) — we use ~/.local/bin/python3.11.
set -euo pipefail
cd "$(dirname "$0")"

PY="${PYTHON:-$HOME/.local/bin/python3.11}"
if ! command -v "$PY" >/dev/null 2>&1; then
  PY="$(command -v python3.11 || command -v python3)"
fi
echo "using python: $PY ($("$PY" --version))"

if [ ! -x .venv/bin/python ]; then
  "$PY" -m venv .venv
fi
./.venv/bin/pip install --quiet --upgrade pip wheel

echo "installing torch/torchvision (cu128)..."
./.venv/bin/pip install --quiet torch torchvision --index-url https://download.pytorch.org/whl/cu128
./.venv/bin/pip install --quiet onnx onnxruntime pillow numpy

echo "--- GPU smoke check ---"
./.venv/bin/python - <<'EOF'
import torch, torchvision
print("torch", torch.__version__, "| torchvision", torchvision.__version__)
print("cuda available:", torch.cuda.is_available())
if torch.cuda.is_available():
    p = torch.cuda.get_device_properties(0)
    print(f"device: {p.name}  capability: sm_{p.major}{p.minor}  vram: {p.total_memory/1e9:.1f} GB")
    x = torch.randn(8, 3, 224, 224, device="cuda").to(torch.bfloat16)
    print("bf16 tensor ok:", tuple(x.shape))
else:
    print("WARNING: no CUDA device visible — training will be CPU-slow")
EOF
echo "done. train with: ./.venv/bin/python tools/train/train.py"