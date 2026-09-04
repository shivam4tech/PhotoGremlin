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
from PIL import Image, ImageFilter
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

class MultiLabelDataset(Dataset):
    """Rows may repeat an image_id (one row per verified label). Targets are
    multi-hot vectors; images whose every label was pruned are dropped."""

    def __init__(self, csv_path: pathlib.Path, thumbs: pathlib.Path,
                 fine_ix: dict[str, int], coarse_ix: dict[str, int], train: bool):
        self.thumbs = thumbs
        per_image: dict[str, tuple[set[int], set[int]]] = {}
        with open(csv_path, newline="") as f:
            next(f)
            for image_id, _mid, fine, coarse, _conf in csv.reader(f):
                fi, ci = fine_ix.get(fine), coarse_ix.get(coarse)
                if fi is None or ci is None:
                    continue
                tgt = per_image.setdefault(image_id, (set(), set()))
                tgt[0].add(fi)
                tgt[1].add(ci)
        self.ids = []
        self.fine_targets = torch.zeros(len(per_image), len(fine_ix))
        self.coarse_targets = torch.zeros(len(per_image), len(coarse_ix))
        kept = 0
        for image_id, (fs, cs) in sorted(per_image.items()):
            p = thumbs / f"{image_id}.jpg"
            if not p.exists():
                continue
            i = kept
            kept += 1
            self.ids.append(p)
            self.fine_targets[i, list(fs)] = 1.0
            self.coarse_targets[i, list(cs)] = 1.0
        self.fine_targets = self.fine_targets[:kept]
        self.coarse_targets = self.coarse_targets[:kept]
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
        return len(self.ids)

    def __getitem__(self, i: int):
        img = Image.open(self.ids[i]).convert("RGB")
        return self.tf(img), self.fine_targets[i], self.coarse_targets[i]

class CorpusDataset(Dataset):
    """corpus_v2/train.csv: path,fine,confidence,source (rows may repeat a
    path with different labels -> grouped into multi-hot targets)."""

    def __init__(self, csv_path: pathlib.Path, base: pathlib.Path,
                 fine_ix: dict[str, int], coarse_ix: dict[str, int], train: bool):
        per_image: dict[str, tuple[set[int], set[int]]] = {}
        with open(csv_path, newline="") as f:
            next(f)
            for path, fine, _conf, _source in csv.reader(f):
                fi, ci = fine_ix.get(fine), coarse_ix.get(coarse_of_fine(fine))
                if fi is None or ci is None:
                    continue
                tgt = per_image.setdefault(path, (set(), set()))
                tgt[0].add(fi)
                tgt[1].add(ci)
        self.paths = []
        self.fine_targets = torch.zeros(len(per_image), len(fine_ix))
        self.coarse_targets = torch.zeros(len(per_image), len(coarse_ix))
        kept = 0
        for rel, (fs, cs) in sorted(per_image.items()):
            p = base / rel
            if not p.exists():
                continue
            i = kept
            kept += 1
            self.paths.append(p)
            self.fine_targets[i, list(fs)] = 1.0
            self.coarse_targets[i, list(cs)] = 1.0
        self.fine_targets = self.fine_targets[:kept]
        self.coarse_targets = self.coarse_targets[:kept]
        self.tf = build_transform(train)

    def __len__(self) -> int:
        return len(self.paths)

    def __getitem__(self, i: int):
        img = Image.open(self.paths[i]).convert("RGB")
        return self.tf(img), self.fine_targets[i], self.coarse_targets[i]

def coarse_of_fine(fine: str) -> str:
    if not _COARSE_OF_FINE:
        raise RuntimeError("coarse mapping not initialized (set _COARSE_OF_FINE)")
    return _COARSE_OF_FINE.get(fine, "other")

_COARSE_OF_FINE: dict[str, str] = {}

def build_transform(train: bool):
    norm = transforms.Normalize(IMAGENET_MEAN, IMAGENET_STD)
    if not train:
        return transforms.Compose([
            transforms.Resize(256), transforms.CenterCrop(224),
            transforms.ToTensor(), norm,
        ])
    layers: list = [
        transforms.RandomResizedCrop(224, scale=(0.6, 1.0)),
        transforms.RandomHorizontalFlip(),
        transforms.RandAugment(num_ops=2, magnitude=7),
        RandomRobustness(p=0.25),
        transforms.ToTensor(), norm,
    ]
    return transforms.Compose(layers)

class RandomRobustness:
    """Edge-case hardening: occasionally darken, blur or JPEG-crush the crop."""

    def __init__(self, p: float = 0.25):
        self.p = p

    def __call__(self, img: Image.Image) -> Image.Image:
        r = random.random()
        if r > self.p:
            return img
        kind = random.choice(["dark", "blur", "jpeg", "lowres"])
        if kind == "dark":
            return transforms.functional.adjust_brightness(img, random.uniform(0.35, 0.75))
        if kind == "blur":
            return img.filter(ImageFilter.GaussianBlur(random.uniform(1.0, 3.0)))
        if kind == "jpeg":
            import io
            buf = io.BytesIO()
            img.save(buf, "JPEG", quality=random.randint(8, 30))
            buf.seek(0)
            return Image.open(buf).convert("RGB")
        w, h = img.size
        small = img.resize((max(24, w // 4), max(24, h // 4)), Image.BILINEAR)
        return small.resize((w, h), Image.BILINEAR)

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
def evaluate(model, loader, device, multilabel: bool) -> dict[str, float]:
    model.eval()
    n = f1 = f5 = c1 = 0
    for batch in loader:
        images, fine, coarse = batch
        images = images.to(device, non_blocking=True)
        with torch.autocast("cuda", dtype=torch.bfloat16, enabled=device.type == "cuda"):
            lf, lc = model(images)
        lf, lc = lf.float().cpu(), lc.float().cpu()
        k5 = min(5, lf.shape[1])
        if multilabel:
            # "correct" = the predicted class is one of the image's labels
            ftrue = fine > 0.5
            ctrue = coarse > 0.5
            top1 = lf.topk(1, dim=1).indices
            top5 = lf.topk(k5, dim=1).indices
            f1 += ftrue.gather(1, top1).sum().item()
            f5 += (ftrue.gather(1, top5).sum(dim=1) > 0).sum().item()
            c1 += ctrue.gather(1, lc.argmax(dim=1, keepdim=True)).sum().item()
        else:
            fine = fine.to(device, non_blocking=True)
            coarse = coarse.to(device, non_blocking=True)
            lf, lc = lf.to(device), lc.to(device)
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
    ap.add_argument("--init-from", help="checkpoint to initialize weights from "
                    "(two-stage: pre-trained backbone, fresh optimizer)")
    ap.add_argument("--corpus", help="corpus_v2 train.csv -> CorpusDataset "
                    "(multi-label BCE, robustness augs)")
    ap.add_argument("--val-csv", help="val csv for --corpus mode "
                    "(default ml-corpus/corpus_v2/val.csv)")
    ap.add_argument("--mixup", type=float, default=0.0,
                    help="Beta(alpha,alpha) for mixup in multi-label mode (e.g. 0.2)")
    ap.add_argument("--ema-decay", type=float, default=0.999,
                    help="0 disables EMA; otherwise eval+best use EMA weights")
    ap.add_argument("--multilabel", action="store_true",
                    help="train on ALL verified labels per image (BCE) using "
                         "samples/*_multi.csv instead of single-label CE")
    ap.add_argument("--limit", type=int, default=0, help="smoke-test row cap")
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()

    mapping = json.loads((repo / "tools/train/class-map.json").read_text())
    fine_classes = sorted(mapping)                                   # places labels
    coarse_classes = sorted({v["coarse"] for v in mapping.values()})

    # Prune unlearnable micro-tails before sizing the heads: classes with a
    # handful of downloaded images only steal probability mass from neighbors.
    # Count ON-DISK images (the CSV lists pre-license-filter samples).
    ds_dir = repo / "ml-corpus/dataset/train"
    disk_freq: dict[str, int] = defaultdict(int)
    if ds_dir.exists():
        for p in ds_dir.rglob("*.jpg"):
            disk_freq[p.parent.name] += 1
    def disk_count(label: str) -> int:
        return disk_freq.get(label.replace(" ", "_"), 0)
    dropped = [c for c in fine_classes if disk_count(c) < args.min_class_train]
    if dropped and not args.resume:
        fine_classes = [c for c in fine_classes if c not in dropped]
        print(f"pruned {len(dropped)} tail classes (<{args.min_class_train} imgs): {sorted(dropped)}")
    elif dropped:
        print(f"resume: keeping all {len(fine_classes)} classes (head size must match checkpoint)")

    print(f"classes: fine={len(fine_classes)} coarse={len(coarse_classes)}")

    samples = repo / "ml-corpus/openimages/samples"
    thumbs = repo / "ml-corpus/openimages/images/thumb"
    global _COARSE_OF_FINE
    _COARSE_OF_FINE = {k: v["coarse"] for k, v in mapping.items()}
    fine_ix = {c: i for i, c in enumerate(fine_classes)}
    coarse_ix = {c: i for i, c in enumerate(coarse_classes)}
    multilabel_mode = args.multilabel or bool(args.corpus)

    if args.corpus:
        base = repo / "ml-corpus"
        val_csv = repo / (args.val_csv or "ml-corpus/corpus_v2/val.csv")
        tr_ds = CorpusDataset(repo / args.corpus, base, fine_ix, coarse_ix, train=True)
        va_ds = CorpusDataset(val_csv, base, fine_ix, coarse_ix, train=False)
    elif multilabel_mode:
        tr_ds = MultiLabelDataset(samples / "train_multi.csv", thumbs,
                                  fine_ix, coarse_ix, train=True)
        va_ds = MultiLabelDataset(samples / "val_multi.csv", thumbs,
                                  fine_ix, coarse_ix, train=False)
    else:
        tr_ds = SceneDataset(samples / "train.csv", thumbs, fine_classes, coarse_classes, train=True)
        va_ds = SceneDataset(samples / "val.csv", thumbs, fine_classes, coarse_classes, train=False)
    if args.limit:
        def _clip(ds, n):
            if hasattr(ds, "rows"):
                ds.rows = ds.rows[:n]
            else:
                ds.paths = ds.paths[:n]
                ds.fine_targets = ds.fine_targets[:n]
                ds.coarse_targets = ds.coarse_targets[:n]
        _clip(tr_ds, args.limit)
        _clip(va_ds, max(200, args.limit // 10))
    print(f"dataset: train={len(tr_ds)} val={len(va_ds)}")

    common = dict(num_workers=args.workers, pin_memory=True, persistent_workers=args.workers > 0)
    # Long-tail: sample by 1/count**power. Power 1.0 = per-class uniform
    # (oversamples groups with many tiny classes and skews coarse priors);
    # 0.5 softens the head-tail trade while keeping coarse priors sane.
    if multilabel_mode:
        counts = torch.bincount(tr_ds.fine_targets.argmax(1), minlength=len(fine_classes)).clamp(min=1)
        weights = torch.tensor(
            [1.0 / (counts[tr_ds.fine_targets[i].argmax()].item() ** args.sample_power)
             for i in range(len(tr_ds))])
    else:
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
    if args.init_from:
        ck0 = torch.load(args.init_from, map_location="cpu")
        model.load_state_dict(ck0["model"])
        print(f"weights initialized from {args.init_from} (its epoch: {ck0['epoch']})")

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
    bce = nn.BCEWithLogitsLoss()

    ema = None
    if args.ema_decay > 0:
        ema = {k: v.detach().clone().float().cpu()
               for k, v in model.state_dict().items()}

    def ema_update():
        with torch.no_grad():
            d = args.ema_decay
            sd = model.state_dict()
            for k, v in ema.items():
                if v.dtype.is_floating_point:
                    v.mul_(d).add_(sd[k].detach().float().cpu(), alpha=1 - d)
                else:
                    v.copy_(sd[k].cpu())

    def swap_ema_in():
        backup = {k: v.detach().clone() for k, v in model.state_dict().items()}
        model.load_state_dict({k: v.to(device) for k, v in ema.items()})
        return backup

    for epoch in range(start_epoch, args.epochs):
        model.train()
        t0, seen, loss_sum = time.time(), 0, 0.0
        for bi, (images, fine, coarse) in enumerate(tr):
            images = images.to(device, non_blocking=True)
            fine = fine.to(device, non_blocking=True)
            coarse = coarse.to(device, non_blocking=True)
            opt.zero_grad(set_to_none=True)
            use_mix = multilabel_mode and args.mixup > 0 and random.random() < 0.5
            if use_mix:
                lam = float(torch.distributions.Beta(args.mixup, args.mixup).sample())
                perm = torch.randperm(images.size(0), device=images.device)
                images = lam * images + (1 - lam) * images[perm]
                fine = lam * fine + (1 - lam) * fine[perm]
                coarse = lam * coarse + (1 - lam) * coarse[perm]
            with torch.autocast("cuda", dtype=torch.bfloat16, enabled=device.type == "cuda"):
                lf, lc = model(images)
                if multilabel_mode:
                    loss = bce(lf, fine) + bce(lc, coarse)
                else:
                    loss = ce(lf, fine) + ce(lc, coarse)
            loss.backward()
            opt.step()
            if ema is not None:
                ema_update()
            loss_sum += loss.item() * images.size(0)
            seen += images.size(0)
            if bi % 100 == 0:
                print(f"  e{epoch} {bi}/{len(tr)} loss={loss.item():.3f}", flush=True)
        sched.step()
        backup = swap_ema_in() if ema is not None else None
        m = evaluate(model, va, device, multilabel_mode)
        if backup is not None:
            model.load_state_dict(backup)
        lr = sched.get_last_lr()[0]
        line = f"{epoch},{loss_sum/max(seen,1):.4f},{m['fine_top1']:.4f},{m['fine_top5']:.4f},{m['coarse_top1']:.4f},{lr:.2e},{time.time()-t0:.0f}"
        print(line, flush=True)
        log.write(line + "\n")
        log.flush()
        state = {
            "model": model.state_dict(), "opt": opt.state_dict(),
            "sched": sched.state_dict(), "epoch": epoch, "best": best,
            "fine_classes": fine_classes, "coarse_classes": coarse_classes,
            "multilabel": multilabel_mode,
        }
        if ema is not None:
            state["ema"] = {k: v.clone() for k, v in ema.items()}
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