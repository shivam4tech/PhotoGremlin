#!/usr/bin/env python3
"""Deep-dive evaluation for a trained two-head checkpoint.

Reports, on the val split:
  - fine top-1/top-5, coarse top-1 (as trained)
  - coarse x coarse confusion: biggest cross-group leaks
  - accuracy under a MERGED taxonomy (sibling groups collapsed)
  - per-fine-class report (support + accuracy)

Usage:
  tools/train/.venv/bin/python tools/train/eval_ckpt.py --checkpoint tools/train/runs/<ts>/best.pt
"""
from __future__ import annotations

import argparse
import csv
import json
import pathlib
import sys
from collections import defaultdict

import torch

from train import SceneDataset, TwoHeadNet

# Sibling coarse groups collapsed: misclassifying inside a column is a
# near-harmless "same kind of place" mistake for filter UX.
MERGED_GROUPS = {
    "nature": "nature", "nature_water": "nature",
    "urban": "urban",
    "indoor_home": "home_stay", "residential": "home_stay", "hotel": "home_stay",
    "indoor": "public_indoor", "indoor_cultural": "public_indoor",
    "indoor_retail": "public_indoor", "workplace": "public_indoor",
    "education": "public_indoor", "healthcare": "public_indoor",
    "religious": "faith_history", "historic": "faith_history",
    "sports": "sports_leisure", "sports_stadium": "sports_leisure",
    "food_dining": "food_night",
    "public_transport": "transport", "transport_vehicle": "transport",
    "industrial": "industry",
    "other": "other",
}

@torch.no_grad()
def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--checkpoint", required=True)
    ap.add_argument("--repo", default=".")
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()

    ck = torch.load(args.checkpoint, map_location="cpu")
    fine_classes = ck["fine_classes"]
    coarse_classes = ck["coarse_classes"]
    model = TwoHeadNet(len(fine_classes), len(coarse_classes))
    model.load_state_dict(ck["model"])
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    model = model.to(device)
    model.eval()

    samples = repo / "ml-corpus/openimages/samples"
    thumbs = repo / "ml-corpus/openimages/images/thumb"
    ds = SceneDataset(samples / "val.csv", thumbs, fine_classes, coarse_classes, train=False)
    loader = torch.utils.data.DataLoader(ds, batch_size=256, num_workers=4)

    fine_ix = {c: i for i, c in enumerate(fine_classes)}
    n = f1 = f5 = c1 = m1 = 0
    coarse_conf = torch.zeros(len(coarse_classes), len(coarse_classes), dtype=torch.long)
    per_class_ok = defaultdict(int)
    per_class_n = defaultdict(int)

    for images, fine, coarse in loader:
        images = images.to(device)
        lf, lc = model(images)
        pf = lf.argmax(1).cpu()
        pc = lc.argmax(1).cpu()
        k5 = min(5, lf.shape[1])
        f1 += (pf == fine).sum().item()
        f5 += (lf.topk(k5, dim=1).indices.cpu() == fine.unsqueeze(1)).any(1).sum().item()
        c1 += (pc == coarse).sum().item()
        for a, b in zip(pc.tolist(), coarse.tolist()):
            coarse_conf[a, b] += 1
        for pred, truth in zip(pf.tolist(), fine.tolist()):
            per_class_n[truth] += 1
            per_class_ok[truth] += int(pred == truth)
        n += images.size(0)

    # merged-coarse accuracy: predicted merged == truth merged
    ci = {c: i for i, c in enumerate(coarse_classes)}
    ok_m = 0
    for a, row in enumerate(coarse_conf):
        for b, cnt in enumerate(row):
            ma = MERGED_GROUPS.get(coarse_classes[a], "other")
            mb = MERGED_GROUPS.get(coarse_classes[b], "other")
            if ma == mb:
                ok_m += cnt.item()
    print(f"val n={n}")
    print(f"fine top-1 : {f1/n:.4f}   fine top-5: {f5/n:.4f}")
    print(f"coarse top-1 (21 groups): {c1/n:.4f}")
    print(f"coarse top-1 ({len(set(MERGED_GROUPS.values()))} merged groups): {ok_m/n:.4f}")

    print("\nbiggest coarse leaks (predicted -> truth):")
    cc = coarse_conf.clone()
    cc.fill_diagonal_(0)
    flat = [(cc[a, b].item(), coarse_classes[a], coarse_classes[b])
            for a in range(len(ci)) for b in range(len(ci))]
    for cnt, pa, tb in sorted(flat, reverse=True)[:10]:
        if cnt:
            same = "SAME-MERGED" if MERGED_GROUPS.get(pa) == MERGED_GROUPS.get(tb) else ""
            print(f"  {pa:18} -> {tb:18} {cnt:4}  {same}")

    print("\nworst fine classes (>=5 val imgs):")
    rows = []
    for i, cname in enumerate(fine_classes):
        if per_class_n[i] >= 5:
            rows.append((per_class_ok[i] / per_class_n[i], per_class_n[i], cname))
    for acc, sup, cname in sorted(rows)[:15]:
        print(f"  {cname:24} {acc:.2f}  (n={sup})")

if __name__ == "__main__":
    main()