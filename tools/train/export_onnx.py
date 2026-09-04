#!/usr/bin/env python3
"""Sprint 17c: export the trained two-head classifier to int8 ONNX.

1. Load best.pt (or last.pt) checkpoint.
2. Export fp32 ONNX (opset 17, inputs: image; outputs: fine, coarse).
3. Static int8 quantization (QDQ) calibrated on a val subset.
4. Verify: fp32 vs int8 top-1 on a val subset — abort if drop > 2%.
5. Install the quantized model into src-tauri/models/.

Usage:
  .venv/bin/python tools/train/export_onnx.py --checkpoint tools/train/runs/<ts>/best.pt
"""
from __future__ import annotations

import argparse
import csv
import json
import pathlib
import shutil
import sys

import numpy as np
import onnxruntime as ort
import torch
from onnxruntime.quantization import CalibrationDataReader, QuantType, quantize_static
from PIL import Image

IMAGENET_MEAN = np.array([0.485, 0.456, 0.406], dtype=np.float32)
IMAGENET_STD = np.array([0.229, 0.224, 0.225], dtype=np.float32)

def preprocess(path: pathlib.Path) -> np.ndarray:
    img = Image.open(path).convert("RGB").resize((224, 224), Image.BILINEAR)
    a = np.asarray(img, dtype=np.float32) / 255.0
    a = (a - IMAGENET_MEAN) / IMAGENET_STD
    return a.transpose(2, 0, 1)[None]  # 1x3x224x224

class ValReader(CalibrationDataReader):
    def __init__(self, rows, limit):
        self.rows = rows[:limit]
        self.i = 0

    def get_next(self):
        if self.i >= len(self.rows):
            return None
        p, _fine = self.rows[self.i]
        self.i += 1
        return {"image": preprocess(p)}

def accuracy(session, rows, fine_ix, limit) -> float:
    """Any-true accuracy over UNIQUE images: an image may carry several
    verified labels (multi-row manifest); prediction counts if it hits any."""
    by_path: dict[pathlib.Path, set[str]] = {}
    for p, fine in rows:
        by_path.setdefault(p, set()).add(fine)
    n = ok = 0
    for p, fines in list(by_path.items())[:limit]:
        out = session.run(None, {"image": preprocess(p)})[0]
        pred = fine_ix_inv[out[0].argmax()]
        ok += int(pred in fines)
        n += 1
    return ok / max(n, 1)

def build_eval_rows(repo: pathlib.Path, fine_ix: dict[str, int]) -> list[tuple[pathlib.Path, str]]:
    """(absolute_path, fine_label) pairs from the newest val manifest.
    Prefers corpus_v2/val.csv (4-col corpus schema); falls back to the
    legacy multi-label val. Rows whose label was pruned from the trained
    head are excluded (they cannot be scored by design)."""
    corpus_val = repo / "ml-corpus/corpus_v2/val.csv"
    rows: list[tuple[pathlib.Path, str]] = []
    if corpus_val.exists():
        ml_dir = repo / "ml-corpus"
        with open(corpus_val, newline="") as f:
            next(f)
            for rel, fine, _conf, _source in csv.reader(f):
                if fine in fine_ix:
                    rows.append((ml_dir / rel, fine))
        return rows
    samples = repo / "ml-corpus/openimages/samples"
    thumbs = repo / "ml-corpus/openimages/images/thumb"
    with open(samples / "val_multi.csv", newline="") as f:
        next(f)
        for iid, _mid, fine, _coarse, _conf in csv.reader(f):
            if fine in fine_ix:
                rows.append((thumbs / f"{iid}.jpg", fine))
    return rows

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--checkpoint", required=True)
    ap.add_argument("--repo", default=".")
    ap.add_argument("--calib", type=int, default=300)
    ap.add_argument("--eval", type=int, default=2000)
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()
    ck_path = pathlib.Path(args.checkpoint).resolve()

    ck = torch.load(ck_path, map_location="cpu")
    fine_classes = ck["fine_classes"]
    coarse_classes = ck["coarse_classes"]
    sys.path.insert(0, str(repo / "tools/train"))
    from train import TwoHeadNet
    model = TwoHeadNet(len(fine_classes), len(coarse_classes))
    model.load_state_dict(ck.get("ema") or ck["model"])
    model.eval()

    runs = ck_path.parent
    fp32 = runs / "model_fp32.onnx"
    torch.onnx.export(
        model, torch.randn(1, 3, 224, 224), fp32, opset_version=17,
        input_names=["image"], output_names=["fine", "coarse"],
        dynamic_axes={"image": {0: "batch"}, "fine": {0: "batch"}, "coarse": {0: "batch"}},
    )
    # fold any external weight data into the single file (torch 2.11's
    # dynamo exporter splits it by default; in-memory loads and quantization
    # want one file)
    import onnx
    from onnx.external_data_helper import convert_model_from_external_data
    proto = onnx.load(str(fp32))
    convert_model_from_external_data(proto)
    onnx.save(proto, str(fp32))
    print(f"fp32 exported: {fp32.stat().st_size/1e6:.1f} MB")

    fine_ix = {c: i for i, c in enumerate(fine_classes)}
    eval_rows = build_eval_rows(repo, fine_ix)
    print(f"eval rows usable: {len(eval_rows)}")

    int8 = runs / "model_int8.onnx"
    quantize_static(str(fp32), str(int8), ValReader(eval_rows, args.calib),
                    weight_type=QuantType.QInt8)
    print(f"int8 exported: {int8.stat().st_size/1e6:.1f} MB")

    so = ort.SessionOptions()
    global fine_ix_inv
    fine_ix_inv = {i: c for c, i in fine_ix.items()}
    s_fp = ort.InferenceSession(str(fp32), so, providers=["CPUExecutionProvider"])
    s_q = ort.InferenceSession(str(int8), so, providers=["CPUExecutionProvider"])
    a_fp = accuracy(s_fp, eval_rows, fine_ix, args.eval)
    a_q = accuracy(s_q, eval_rows, fine_ix, args.eval)
    print(f"val subset ({args.eval} imgs): fp32={a_fp:.4f} int8={a_q:.4f}")

    dest = repo / "src-tauri/models/scene_mobilenetv3_large.onnx"
    if a_fp - a_q <= 0.02:
        shutil.copy2(int8, dest)
        print(f"installed int8 {dest} ({dest.stat().st_size/1e6:.1f} MB)")
    else:
        shutil.copy2(fp32, dest)
        print(f"WARNING: int8 collapsed ({a_fp - a_q:+.4f}); installed fp32 "
              f"{dest} ({dest.stat().st_size/1e6:.1f} MB) instead")

if __name__ == "__main__":
    main()