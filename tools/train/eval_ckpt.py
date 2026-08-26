#!/usr/bin/env python3
"""Deep-dive evaluation for a trained two-head checkpoint.

Supports both single-label and multi-label (--multilabel) checkpoints.
Reports, on the val split:
  - fine top-1/top-5, coarse top-1 ("correct" = prediction is one of the
    image's verified labels in multi-label mode)
  - accuracy under the MERGED taxonomy (sibling groups collapsed)
  - biggest cross-group leaks
  - weakest fine classes by recall

Usage:
  tools/train/.venv/bin/python tools/train/eval_ckpt.py --checkpoint tools/train/runs/<ts>/best.pt
"""
from __future__ import annotations

import argparse
import json
import pathlib
from collections import defaultdict

import torch

import train as train_mod
from train import CorpusDataset, MultiLabelDataset, SceneDataset, TwoHeadNet

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
    ap.add_argument("--gates", action="store_true",
                    help="print PASS/FAIL against the product definition-of-done")
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()

    ck = torch.load(args.checkpoint, map_location="cpu")
    fine_classes = ck["fine_classes"]
    coarse_classes = ck["coarse_classes"]
    multilabel = bool(ck.get("multilabel", False))
    model = TwoHeadNet(len(fine_classes), len(coarse_classes))
    model.load_state_dict(ck["model"])
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    model = model.to(device)
    model.eval()

    mapping = json.loads((repo / "tools/train/class-map.json").read_text())
    train_mod._COARSE_OF_FINE = {k: v["coarse"] for k, v in mapping.items()}
    corpus_val = repo / "ml-corpus/corpus_v2/val.csv"
    if multilabel and corpus_val.exists():
        ds = CorpusDataset(corpus_val, repo / "ml-corpus",
                           {c: i for i, c in enumerate(fine_classes)},
                           {c: i for i, c in enumerate(coarse_classes)}, train=False)
    else:
        samples = repo / "ml-corpus/openimages/samples"
        thumbs = repo / "ml-corpus/openimages/images/thumb"
        name = "val_multi.csv" if multilabel else "val.csv"
        ds = MultiLabelDataset(samples / name, thumbs,
                               {c: i for i, c in enumerate(fine_classes)},
                               {c: i for i, c in enumerate(coarse_classes)}, train=False)
    loader = torch.utils.data.DataLoader(ds, batch_size=256, num_workers=4)

    n = f1 = f5 = c1 = m_ok = 0
    leaks: dict[tuple[str, str], int] = defaultdict(int)
    cls_recall_n = defaultdict(int)
    cls_recall_hit = defaultdict(int)

    for images, ft, ct in loader:
        images = images.to(device)
        lf, lc = model(images)
        lf, lc = lf.float().cpu(), lc.float().cpu()
        ftrue = ft > 0.5
        ctrue = ct > 0.5
        k5 = min(5, lf.shape[1])
        top1_f = lf.argmax(1)
        top5_f = lf.topk(k5, dim=1).indices
        pred_c = lc.argmax(1)

        n += images.size(0)
        f1 += ftrue.gather(1, top1_f.unsqueeze(1)).sum().item()
        f5 += (ftrue.gather(1, top5_f).sum(dim=1) > 0).sum().item()
        c1 += ctrue.gather(1, pred_c.unsqueeze(1)).sum().item()

        for i in range(images.size(0)):
            pm = MERGED_GROUPS.get(coarse_classes[pred_c[i].item()], "other")
            tms = {MERGED_GROUPS.get(coarse_classes[j], "other")
                   for j in range(len(coarse_classes)) if ctrue[i, j]}
            if pm in tms:
                m_ok += 1
            pc_name = coarse_classes[pred_c[i].item()]
            for j in range(len(coarse_classes)):
                if ctrue[i, j]:
                    tc_name = coarse_classes[j]
                    if tc_name != pc_name:
                        leaks[(pc_name, tc_name)] += 1
            for j in range(ft.shape[1]):
                if ftrue[i, j]:
                    cls_recall_n[j] += 1
                    cls_recall_hit[j] += int(top1_f[i].item() == j)

    print(f"mode={'multi' if multilabel else 'single'}-label   val n={n}")
    print(f"fine top-1 : {f1/n:.4f}   fine top-5: {f5/n:.4f}")
    print(f"coarse top-1 (21 groups): {c1/n:.4f}")
    print(f"coarse top-1 ({len(set(MERGED_GROUPS.values()))} merged groups): {m_ok/n:.4f}")

    print("\nbiggest coarse leaks (predicted -> truth-label):")
    for (pa, tb), cnt in sorted(leaks.items(), key=lambda kv: -kv[1])[:10]:
        same = "SAME-MERGED" if MERGED_GROUPS.get(pa) == MERGED_GROUPS.get(tb) else ""
        print(f"  {pa:18} -> {tb:18} {cnt:4}  {same}")

    print("\nweakest fine classes by recall (n>=10):")
    rows = [(cls_recall_hit[i] / cls_recall_n[i], cls_recall_n[i], cname)
            for i, cname in enumerate(fine_classes) if cls_recall_n[i] >= 10]
    for acc, sup, cname in sorted(rows)[:15]:
        print(f"  {cname:24} {acc:.2f}  (n={sup})")

    # ---- region-bias audit (openverse held-out slice) ----------------------
    audit_csv = repo / "ml-corpus/corpus_v2/audit.csv"
    if audit_csv.exists():
        import csv as _csv
        from PIL import Image

        mapping = json.loads((repo / "tools/train/class-map.json").read_text())
        coarse_of_fine = {k: v["coarse"] for k, v in mapping.items()}
        REGIONS = {"indian", "nigerian", "ethiopian", "kenyan", "egyptian",
                   "moroccan", "chinese", "japanese", "korean", "vietnamese",
                   "thai", "indonesian", "filipino", "turkish", "iranian",
                   "arab", "israeli", "russian", "polish", "greek", "spanish",
                   "portuguese", "italian", "mexican", "brazilian", "peruvian",
                   "colombian", "argentinian", "cuban", "nepalese"}
        per_region: dict[str, list[int]] = defaultdict(lambda: [0, 0])
        with open(audit_csv, newline="") as f:
            next(f)
            for path, fine, region_query in _csv.reader(f):
                if fine not in coarse_of_fine:
                    continue
                p = repo / "ml-corpus" / path
                if not p.exists():
                    continue
                first = (region_query or "").split(" ")[0]
                region = first if first in REGIONS else "global"
                x = ds.tf(Image.open(p).convert("RGB")).unsqueeze(0).to(device)
                lf, lc = model(x)
                pred_c = int(lc.argmax(1).item())
                per_region[region][1] += 1
                if coarse_classes[pred_c] == coarse_of_fine[fine]:
                    per_region[region][0] += 1
        print("\nregion audit (coarse strict top-1 on openverse holdout):")
        rates = {}
        for region, (ok, tot) in sorted(per_region.items(), key=lambda kv: -kv[1][1]):
            rate = ok / max(tot, 1)
            rates[region] = rate
            print(f"  {region:12} {rate:.3f}  (n={tot})")
        if len(rates) >= 2:
            vals = list(rates.values())
            print(f"  region delta: {max(vals)-min(vals):.3f}")

    if args.gates:
        print("\n==== GATES ====")
        gates = [
            ("fine top-5 >= 0.95", f5 / n >= 0.95),
        ]
        for name, passed in gates:
            print(f"  [{'PASS' if passed else 'FAIL'}] {name}")
        print(f"  [{'PASS' if m_ok / n >= 0.90 else 'FAIL'}] merged-coarse >= 0.90 "
              f"(got {m_ok / n:.4f}; see line above)")
        calib = pathlib.Path(args.checkpoint).parent / "calibration.json"
        if calib.exists():
            cj = json.loads(calib.read_text())
            passed = cj.get("precision_at_tau", 0) >= 0.90
            print(f"  [{'PASS' if passed else 'FAIL'}] calibrated tag precision >= 0.90 "
                  f"(got {cj.get('precision_at_tau')})")
        else:
            print("  [SKIP ] calibrated tag precision — run calibrate.py first")

if __name__ == "__main__":
    main()