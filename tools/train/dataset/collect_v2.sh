#!/usr/bin/env bash
# Sprint 17-v2 collection pipeline — every stage resumable; re-run freely.
set -uo pipefail
cd "$(dirname "$0")/../../.."   # repo root (script lives at tools/train/dataset/)

# Openverse credentials (registered app); file is outside the repo, chmod 600
[ -f "$HOME/.config/photogremlin/openverse.env" ] && . "$HOME/.config/photogremlin/openverse.env"

PY="${PYTHON:-tools/train/.venv/bin/python}"
if [ ! -x "$PY" ]; then PY="python3"; fi

echo "=== [1/4] Tier A: Open Images machine labels (bulk, ~35-40GB)"
"$PY" tools/train/dataset/download_oi_machine.py "$@" || exit 1

echo "=== [2/4] Tier B: Openverse demographic crawl"
"$PY" tools/train/dataset/openverse_crawl.py "$@" || exit 1

echo "=== [3/4] Tier B+: Wikimedia Commons deep crawl (time-bounded, resumable)"
"$PY" tools/train/dataset/commons_crawl.py --minutes "${COMMONS_MINUTES:-75}" || exit 1

echo "=== [4/4] Build corpus v2 (dedup + manifests)"
"$PY" tools/train/dataset/build_corpus_v2.py || exit 1

echo "=== [4/4] done"
echo "next:"
echo "  # stage 1 - noisy pre-train on all tiers:"
echo "  tools/train/.venv/bin/python tools/train/train.py --corpus ml-corpus/corpus_v2/train.csv --epochs 5 --mixup 0.2"
echo "  # stage 2 - fine-tune on clean human labels:"
echo "  tools/train/.venv/bin/python tools/train/train.py --corpus ml-corpus/corpus_v2/train_clean.csv --init-from <stage1>/last.pt --epochs 10"
