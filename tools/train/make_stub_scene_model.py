#!/usr/bin/env python3
"""Generate the placeholder scene model + labels for Sprint 18 development.

Produces src-tauri/models/:
  scene_mobilenetv3_large.onnx — tiny randomly-weighted two-head network
      with the REAL input/output contract ("image" [1,3,224,224] ->
      "fine"/"coarse"). Deterministic weights; classifications are garbage
      by design. The trained artifact from export_onnx.py replaces this
      file 1:1 when it is ready.
  scene_labels.json — canonical label lists + merged-group mapping read by
      the Rust handler at runtime.

Run:  tools/train/.venv/bin/python tools/train/make_stub_scene_model.py
"""
from __future__ import annotations

import json
import pathlib
import sys

import torch

REPO = pathlib.Path(__file__).resolve().parents[2]

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

class StubNet(torch.nn.Module):
    def __init__(self, n_fine: int, n_coarse: int):
        super().__init__()
        self.pool = torch.nn.AdaptiveAvgPool2d(1)
        torch.manual_seed(17)
        self.fc_fine = torch.nn.Linear(3, n_fine)
        self.fc_coarse = torch.nn.Linear(3, n_coarse)

    def forward(self, x):
        v = self.pool(x.mean(dim=(2, 3), keepdim=True)).flatten(1)
        return self.fc_fine(v), self.fc_coarse(v)

def main() -> None:
    mapping = json.loads((REPO / "tools/train/class-map.json").read_text())
    fine = sorted(mapping)
    coarse = sorted({v["coarse"] for v in mapping.values()})
    models_dir = REPO / "src-tauri/models"
    models_dir.mkdir(parents=True, exist_ok=True)

    net = StubNet(len(fine), len(coarse)).eval()
    out_path = models_dir / "scene_mobilenetv3_large.onnx"
    torch.onnx.export(
        net, torch.randn(1, 3, 224, 224), out_path, opset_version=17,
        input_names=["image"], output_names=["fine", "coarse"],
        dynamic_axes={"image": {0: "batch"}},
    )
    # The dynamo exporter may emit external weight data (.onnx.data); the
    # app loads models from memory, so fold everything into one file.
    import onnx
    from onnx.external_data_helper import convert_model_from_external_data
    model = onnx.load(str(out_path))
    convert_model_from_external_data(model)
    onnx.save(model, str(out_path))
    print(f"wrote {out_path} ({out_path.stat().st_size/1024:.0f} KB stub)")

    labels = {
        "fine_classes": fine,
        "coarse_classes": coarse,
        "merged_groups": MERGED_GROUPS,
        "note": "stub placeholder — replaced by export_onnx.py output",
    }
    labels_path = models_dir / "scene_labels.json"
    labels_path.write_text(json.dumps(labels, indent=2, sort_keys=True) + "\n")
    print(f"wrote {labels_path} ({len(fine)} fine / {len(coarse)} coarse)")

if __name__ == "__main__":
    sys.exit(main())