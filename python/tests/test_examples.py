from __future__ import annotations

import json
from pathlib import Path

import pandas as pd


def test_curated_examples_parse() -> None:
    root = Path(__file__).resolve().parents[1] / "examples" / "artifacts"
    data = json.loads((root / "sample_evaluate.json").read_text(encoding="utf-8"))
    assert data["kind"] == "evaluate"
    assert data["result"]["origin"] == "modeled"
    assert data["result"]["recommended_key"]
    df = pd.read_csv(root / "sample_sensitivity.csv")
    assert "axis" in df.columns
    assert len(df) >= 3
