#!/usr/bin/env python3
"""Sprint 17b: train the two-head scene classifier (fine + coarse).

MobileNetV3-Large (ImageNet-pretrained backbone) with two classification
heads on the shared 960-d pooling feature:
  - fine head:   ~130 Places365-name scene classes
  - coarse head: 21 broad groups (filter chips)

Data: ml-corpus/openimages/samples/{train,val}.csv (image_id -> thumbnail,
single-label). Labels come from tools/train/class-map.json.

Usage:
  .venv/bin/python tools/train/train.py                     # full run
  .venv/bin/python tools/train/train.py --limit 2000        # smoke test
  .venv/bin/python tools/train/train.py --resume runs/<ts>/last.pt
"""
from __future__ import annotations

import argparse
import csv
import json
import pathlib
import random
import time
from collections import defaultdict

import torch
import torch.nn as nn
from PIL import Image
from torch.utils.data import DataLoader, Dataset
from torchvision import models, transforms

IMAGENET_MEAN = [0.485, 0.456, 0.406]
IMAGENET_STD = [0.229, 0.224, 0.225]

class SceneDataset(Dataset):
    def __init__(self, csv_path: pathlib.Path, thumbs: pathlib.Path,
                 fine_classes: list[str], coarse_classes: list[str],
                 train: bool):
        self.thumbs = thumbs
        self.fine_ix = {c: i for i, c in enumerate(fine_classes)}
        self.coarse_ix = {c: i for i, c in enumerate(coarse_classes)}
        self.rows: list[tuple[pathlib.Path, int, int]] = []
        with open(csv_path, newline="") as f:
            next(f)
            for image_id, _mid, fine, coarse, _conf in csv.reader(f):
                if fine in self.fine_ix and coarse in self.coarse_ix:
                    p = thumbs / f"{image_id}.jpg"
                    if p.exists():
                        self.rows.append((p, self.fine_ix[fine], self.coarse_ix[coarse]))
        norm = transforms.Normalize(IMAGENET_MEAN, IMAGENET_STD)
        if train:
            self.tf = transforms.Compose([
                transforms.RandomResizedCrop(224, scale=(0.7, 1.0)),
                transforms.RandomHorizontalFlip(),
                transforms.ColorJitter(0.2, 0.2, 0.2),
                transforms.ToTensor(), norm,
            ])
        else:
            self.tf = transforms.Compose([
                transforms.Resize(256), transforms.CenterCrop(224),
                transforms.ToTensor(), norm,
            ])

    def __len__(self) -> int:
        return len(self.rows)

    def __getitem__(self, i: int):
        path, fine, coarse = self.rows[i]
        img = Image.open(path).convert("RGB")
        return self.tf(img), fine, coarse

class TwoHeadNet(nn.Module):
    def __init__(self, n_fine: int, n_coarse: int):
        super().__init__()
        base = models.mobilenet_v3_large(weights=models.MobileNet_V3_Large_Weights.DEFAULT)
        self.features = base.features
        self.pool = base.avgpool
        self.embed = nn.Flatten()
        self.head_fine = nn.Linear(960, n_fine)
        self.head_coarse = nn.Linear(960, n_coarse)

    def forward(self, x):
        x = self.features(x)
        x = self.pool(x)
        x = self.embed(x)
        return self.head_fine(x), self.head_coarse(x)

@torch.no_grad()
def evaluate(model, loader, device) -> dict[str, float]:
    model.eval()
    n = f1 = f5 = c1 = 0
    for images, fine, coarse in loader:
        images = images.to(device, non_blocking=True)
        fine = fine.to(device, non_blocking=True)
        coarse = coarse.to(device, non_blocking=True)
        with torch.autocast("cuda", dtype=torch.bfloat16, enabled=device.type == "cuda"):
            lf, lc = model(images)
        k5 = min(5, lf.shape[1])
        f1 += (lf.topk(1, dim=1).indices.squeeze(1) == fine).sum().item()
        f5 += (lf.topk(k5, dim=1).indices == fine.unsqueeze(1)).any(1).sum().item()
        c1 += (lc.argmax(1) == coarse).sum().item()
        n += images.size(0)
    return {"fine_top1": f1 / n, "fine_top5": f5 / n, "coarse_top1": c1 / n}

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--batch-size", type=int, default=128)
    ap.add_argument("--epochs", type=int, default=8)
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--lr-backbone", type=float, default=3e-4)
    ap.add_argument("--lr-heads", type=float, default=3e-3)
    ap.add_argument("--weight-decay", type=float, default=1e-4)
    ap.add_argument("--min-class-train", type=int, default=25,
                    help="drop fine classes with fewer train images (unlearnable tails)")
    ap.add_argument("--sample-power", type=float, default=0.5,
                    help="sampling weight exponent: 1=per-class uniform (skews coarse priors), 0=natural")
    ap.add_argument("--resume", help="path to last.pt to continue")
    ap.add_argument("--limit", type=int, default=0, help="smoke-test row cap")
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()

    mapping = json.loads((repo / "tools/train/class-map.json").read_text())
    fine_classes = sorted(mapping)                                   # places labels
    coarse_classes = sorted({v["coarse"] for v in mapping.values()})

    # Prune unlearnable micro-tails before sizing the heads: classes with a
    # handful of train images only steal probability mass from neighbors.
    freq: dict[str, int] = defaultdict(int)
    with open(repo / "ml-corpus/openimages/samples/train.csv", newline="") as f:
        next(f)
        for row in csv.reader(f):
            if len(row) >= 3:
                freq[row[2]] += 1
    dropped = [c for c in fine_classes if freq.get(c, 0) < args.min_class_train]
    if dropped and not args.resume:
        fine_classes = [c for c in fine_classes if c not in dropped]
        print(f"pruned {len(dropped)} tail classes (<{args.min_class_train} imgs): {sorted(dropped)}")
    elif dropped:
        print(f"resume: keeping all {len(fine_classes)} classes (head size must match checkpoint)")

    print(f"classes: fine={len(fine_classes)} coarse={len(coarse_classes)}")

    samples = repo / "ml-corpus/openimages/samples"
    thumbs = repo / "ml-corpus/openimages/images/thumb"
    tr_ds = SceneDataset(samples / "train.csv", thumbs, fine_classes, coarse_classes, train=True)
    va_ds = SceneDataset(samples / "val.csv", thumbs, fine_classes, coarse_classes, train=False)
    if args.limit:
        tr_ds.rows = tr_ds.rows[: args.limit]
        va_ds.rows = va_ds.rows[: max(200, args.limit // 10)]
    print(f"dataset: train={len(tr_ds)} val={len(va_ds)}")

    common = dict(num_workers=args.workers, pin_memory=True, persistent_workers=args.workers > 0)
    # Long-tail: sample by 1/count**power. Power 1.0 = per-class uniform
    # (oversamples groups with many tiny classes and skews coarse priors);
    # 0.5 softens the head-tail trade while keeping coarse priors sane.
    counts: dict[int, int] = {}
    for _, fine, _ in tr_ds.rows:
        counts[fine] = counts.get(fine, 0) + 1
    weights = [1.0 / (counts[fine] ** args.sample_power) for _, fine, _ in tr_ds.rows]
    sampler = torch.utils.data.WeightedRandomSampler(
        weights, num_samples=len(tr_ds), replacement=True)
    tr = DataLoader(tr_ds, batch_size=args.batch_size, sampler=sampler, **common)
    va = DataLoader(va_ds, batch_size=args.batch_size, shuffle=False, **common)

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    model = TwoHeadNet(len(fine_classes), len(coarse_classes)).to(device)

    backbone_params = list(model.features.parameters()) + list(model.pool.parameters())
    head_params = list(model.head_fine.parameters()) + list(model.head_coarse.parameters())
    opt = torch.optim.AdamW([
        {"params": backbone_params, "lr": args.lr_backbone},
        {"params": head_params, "lr": args.lr_heads},
    ], weight_decay=args.weight_decay)
    sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, T_max=args.epochs)

    start_epoch, best = 0, 0.0
    out = repo / "tools/train/runs" / time.strftime("%Y%m%d-%H%M%S")
    if args.resume:
        ck = torch.load(args.resume, map_location="cpu")
        model.load_state_dict(ck["model"])
        opt.load_state_dict(ck["opt"])
        sched.load_state_dict(ck["sched"])
        start_epoch, best = ck["epoch"] + 1, ck.get("best", 0.0)
        out = pathlib.Path(args.resume).parent
        print(f"resumed from {args.resume} at epoch {start_epoch}")
    out.mkdir(parents=True, exist_ok=True)
    log = open(out / "log.csv", "a")
    if start_epoch == 0:
        log.write("epoch,train_loss,fine_top1,fine_top5,coarse_top1,lr,secs\n")

    ce = nn.CrossEntropyLoss(label_smoothing=0.1)
    for epoch in range(start_epoch, args.epochs):
        model.train()
        t0, seen, loss_sum = time.time(), 0, 0.0
        for bi, (images, fine, coarse) in enumerate(tr):
            images = images.to(device, non_blocking=True)
            fine = fine.to(device, non_blocking=True)
            coarse = coarse.to(device, non_blocking=True)
            opt.zero_grad(set_to_none=True)
            with torch.autocast("cuda", dtype=torch.bfloat16, enabled=device.type == "cuda"):
                lf, lc = model(images)
                loss = ce(lf, fine) + ce(lc, coarse)
            loss.backward()
            opt.step()
            loss_sum += loss.item() * images.size(0)
            seen += images.size(0)
            if bi % 100 == 0:
                print(f"  e{epoch} {bi}/{len(tr)} loss={loss.item():.3f}", flush=True)
        sched.step()
        m = evaluate(model, va, device)
        lr = sched.get_last_lr()[0]
        line = f"{epoch},{loss_sum/max(seen,1):.4f},{m['fine_top1']:.4f},{m['fine_top5']:.4f},{m['coarse_top1']:.4f},{lr:.2e},{time.time()-t0:.0f}"
        print(line, flush=True)
        log.write(line + "\n")
        log.flush()
        state = {
            "model": model.state_dict(), "opt": opt.state_dict(),
            "sched": sched.state_dict(), "epoch": epoch, "best": best,
            "fine_classes": fine_classes, "coarse_classes": coarse_classes,
        }
        torch.save(state, out / "last.pt")
        if m["fine_top1"] > best:
            best = m["fine_top1"]
            state["best"] = best
            torch.save(state, out / "best.pt")
            print(f"  new best fine_top1={best:.4f}", flush=True)
    log.close()
    print(f"done. best fine_top1={best:.4f}  checkpoints in {out}")

if __name__ == "__main__":
    main()