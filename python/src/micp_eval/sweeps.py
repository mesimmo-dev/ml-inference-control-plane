"""Grids over objective weights for experiment sweeps."""

from __future__ import annotations

from collections.abc import Iterator
from dataclasses import dataclass

import numpy as np


OBJECTIVES = ("latency", "quality", "cost", "throughput", "reliability")


@dataclass(frozen=True)
class WeightGrid:
    """Simplex grid: every point has non-negative weights summing to 1."""

    step: float = 0.25

    def __post_init__(self) -> None:
        if self.step <= 0 or self.step > 1:
            raise ValueError("step must be in (0, 1]")


def iter_weight_grid(grid: WeightGrid) -> Iterator[dict[str, float]]:
    """Enumerate the 5-simplex at `grid.step` resolution.

    The last weight is implied so every yielded dict already sums to 1.
    """
    n = int(round(1.0 / grid.step))
    if abs(n * grid.step - 1.0) > 1e-9:
        raise ValueError("step must evenly divide 1.0")
    levels = [i * grid.step for i in range(n + 1)]
    for w0 in levels:
        for w1 in levels:
            if w0 + w1 > 1.0 + 1e-12:
                continue
            for w2 in levels:
                if w0 + w1 + w2 > 1.0 + 1e-12:
                    continue
                for w3 in levels:
                    rem = 1.0 - (w0 + w1 + w2 + w3)
                    if rem < -1e-12:
                        continue
                    if rem > 1.0 + 1e-12:
                        continue
                    # Keep points that land on the grid.
                    snapped = round(rem / grid.step) * grid.step
                    if abs(snapped - rem) > 1e-9:
                        continue
                    yield {
                        "latency": float(w0),
                        "quality": float(w1),
                        "cost": float(w2),
                        "throughput": float(w3),
                        "reliability": float(max(snapped, 0.0)),
                    }


def weight_matrix(grid: WeightGrid) -> np.ndarray:
    rows = [list(w.values()) for w in iter_weight_grid(grid)]
    return np.asarray(rows, dtype=float)
