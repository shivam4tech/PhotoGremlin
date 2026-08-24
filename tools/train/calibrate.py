#!/usr/bin/env python3
"""Calibrate the trained two-head model for honest UI confidences.

1. Temperature-scales fine logits on the clean multi-label val split.
2. Picks the display threshold tau so that shown tags meet --precision
   (default 0.90) at maximal coverage.
3. Writes calibration.json next to the checkpoint:
   {"temperature": T, "tau": tau, "fine_classes": [...], "precision": p, "coverage": c}

Usage:
  tools/train/.venv/bin/python tools/train/calibrate.py --checkpoint tools/train/runs/<ts>/best.pt
"""
from __future__ import annotations

import argparse
import json
import pathlib

import numpy as np
import torch
from torch.utils.data import DataLoader

import train as train_mod
from train import CorpusDataset, MultiLabelDataset, TwoHeadNet

@torch.no_grad()
def collect_logits(model, loader, device):
    L, Y = [], []
    for images, ft, _ct in loader:
        lf, _lc = model(images.to(device))
        L.append(lf.float().cpu())
        Y.append(ft)
    return torch.cat(L), torch.cat(Y)

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--checkpoint", required=True)
    ap.add_argument("--repo", default=".")
    ap.add_argument("--precision", type=float, default=0.90)
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()

    ck = torch.load(args.checkpoint, map_location="cpu")
    fine_classes = ck["fine_classes"]
    coarse_classes = ck["coarse_classes"]
    multilabel = bool(ck.get("multilabel", False))
    if not multilabel:
        raise SystemExit("calibration expects a multi-label checkpoint (round 3+)")

    model = TwoHeadNet(len(fine_classes), len(coarse_classes))
    model.load_state_dict(ck.get("ema") or ck["model"])
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    model = model.to(device).eval()

    samples = repo / "ml-corpus/openimages/samples"
    thumbs = repo / "ml-corpus/openimages/images/thumb"
    mapping = json.loads((repo / "tools/train/class-map.json").read_text())
    train_mod._COARSE_OF_FINE = {k: v["coarse"] for k, v in mapping.items()}
    corpus_val = repo / "ml-corpus/corpus_v2/val.csv"
    if corpus_val.exists():
        ds = CorpusDataset(corpus_val, repo / "ml-corpus",
                           {c: i for i, c in enumerate(fine_classes)},
                           {c: i for i, c in enumerate(coarse_classes)}, train=False)
    else:
        ds = MultiLabelDataset(samples / "val_multi.csv", thumbs,
                               {c: i for i, c in enumerate(fine_classes)},
                               {c: i for i, c in enumerate(coarse_classes)}, train=False)
    loader = DataLoader(ds, batch_size=256, num_workers=4)
    logits, targets = collect_logits(model, loader, device)
    truth = targets > 0.5
    print(f"val: {logits.shape[0]} images")

    # 1. temperature minimizing NLL over positive labels (grid search)
    best_T, best_nll = 1.0, float("inf")
    pos = truth.float()
    n_pos = pos.sum().clamp(min=1)
    for T in np.arange(0.5, 3.01, 0.05):
        p = torch.sigmoid(logits / T)
        nll = -(pos * torch.log(p.clamp(1e-6)) + (1 - pos) * torch.log((1 - p).clamp(1e-6))).sum() / n_pos
        if nll < best_nll:
            best_T, best_nll = float(T), float(nll)
    print(f"temperature: {best_T:.2f} (nll {best_nll:.4f})")

    # 2. tau for >= precision at MAX coverage: precision is non-increasing
    # along the confidence-sorted prefix, so take the LAST prefix index that
    # still meets the bar.
    probs = torch.sigmoid(logits / best_T)
    flat_p = probs.flatten().numpy()
    flat_y = truth.flatten().numpy()
    order = np.argsort(-flat_p)
    ys = flat_y[order]
    tp = np.cumsum(ys)
    total_shown = np.arange(1, len(ys) + 1)
    prec = tp / total_shown
    ok_idx = np.nonzero(prec >= args.precision)[0]
    if len(ok_idx) == 0:
        # even the single most-confident tag misses the bar: clamp to it so
        # the UI shows essentially nothing rather than wrong things
        idx = 0
    else:
        idx = int(ok_idx[-1])
    tau = float(flat_p[order[idx]])
    print(f"tau={tau:.3f} -> shown-tag precision={prec[idx]:.3f} "
          f"coverage={total_shown[idx]/len(ys):.3f} of all label slots")

    out = pathlib.Path(args.checkpoint).parent / "calibration.json"
    out.write_text(json.dumps({
        "temperature": best_T, "tau": tau,
        "fine_classes": fine_classes,
        "precision_at_tau": round(float(prec[idx]), 4),
        "coverage_at_tau": round(float(total_shown[idx] / len(ys)), 4),
    }, indent=2))
    print(f"wrote {out}")

if __name__ == "__main__":
    main()