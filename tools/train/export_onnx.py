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
    def __init__(self, rows, thumbs, limit):
        self.rows = rows[:limit]
        self.thumbs = thumbs
        self.i = 0

    def get_next(self):
        if self.i >= len(self.rows):
            return None
        image_id = self.rows[self.i][0]
        self.i += 1
        p = self.thumbs / f"{image_id}.jpg"
        return {"image": preprocess(p)} if p.exists() else self.get_next()

def accuracy(session, rows, thumbs, fine_ix, limit) -> float:
    n = ok = 0
    for row in rows[:limit]:
        p = thumbs / f"{row[0]}.jpg"
        if not p.exists():
            continue
        out = session.run(None, {"image": preprocess(p)})[0]
        ok += int(out[0].argmax() == fine_ix[row[2]])
        n += 1
    return ok / max(n, 1)

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
    model.load_state_dict(ck["model"])
    model.eval()

    runs = ck_path.parent
    fp32 = runs / "model_fp32.onnx"
    torch.onnx.export(
        model, torch.randn(1, 3, 224, 224), fp32, opset_version=17,
        input_names=["image"], output_names=["fine", "coarse"],
        dynamic_axes={"image": {0: "batch"}, "fine": {0: "batch"}, "coarse": {0: "batch"}},
    )
    print(f"fp32 exported: {fp32.stat().st_size/1e6:.1f} MB")

    samples = repo / "ml-corpus/openimages/samples"
    thumbs = repo / "ml-corpus/openimages/images/thumb"
    with open(samples / "val.csv", newline="") as f:
        rows = list(csv.reader(f))[1:]

    int8 = runs / "model_int8.onnx"
    quantize_static(str(fp32), str(int8), ValReader(rows, thumbs, args.calib),
                    weight_type=QuantType.QInt8)
    print(f"int8 exported: {int8.stat().st_size/1e6:.1f} MB")

    so = ort.SessionOptions()
    s_fp = ort.InferenceSession(str(fp32), so, providers=["CPUExecutionProvider"])
    s_q = ort.InferenceSession(str(int8), so, providers=["CPUExecutionProvider"])
    fine_ix = {c: i for i, c in enumerate(fine_classes)}
    a_fp = accuracy(s_fp, rows, thumbs, fine_ix, args.eval)
    a_q = accuracy(s_q, rows, thumbs, fine_ix, args.eval)
    print(f"val subset ({args.eval}): fp32={a_fp:.4f} int8={a_q:.4f} delta={a_fp-a_q:+.4f}")
    if a_fp - a_q > 0.02:
        sys.exit("int8 dropped more than 2% — keeping fp32; investigate calibration")

    dest = repo / "src-tauri/models/scene_mobilenetv3_large.onnx"
    shutil.copy2(int8, dest)
    print(f"installed {dest} ({dest.stat().st_size/1e6:.1f} MB)")

if __name__ == "__main__":
    main()