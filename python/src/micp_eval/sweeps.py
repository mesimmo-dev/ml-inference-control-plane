"""Grids over objective weights and bounded scenario-parameter sweeps."""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from dataclasses import dataclass
from itertools import product
from typing import Any

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


@dataclass(frozen=True)
class SweepAxis:
    """One control-plane dimension and the values to visit."""

    path: str
    values: tuple[Any, ...]

    def __post_init__(self) -> None:
        if not self.path:
            raise ValueError("axis path must be non-empty")
        if not self.values:
            raise ValueError("axis values must be non-empty")


@dataclass(frozen=True)
class SweepSpec:
    """Deterministic bounded grid over a named scenario.

    Cartesian product is reduced, never expanded, when it would exceed
    `max_points`. Endpoints of each axis are kept.
    """

    name: str
    scenario_id: str
    axes: tuple[SweepAxis, ...]
    max_points: int = 64
    seed: int = 0

    def __post_init__(self) -> None:
        if self.max_points < 1:
            raise ValueError("max_points must be >= 1")
        if not self.axes:
            raise ValueError("sweep requires at least one axis")


def _prod(sizes: Sequence[int]) -> int:
    n = 1
    for s in sizes:
        n *= s
    return n


def _downsample_keep_ends(values: Sequence[Any]) -> list[Any]:
    if len(values) <= 2:
        return list(values)
    kept = [values[0]]
    interior = list(values[1:-1])
    # Drop every other interior point; always keep the last.
    kept.extend(interior[::2] if len(interior) > 1 else interior)
    kept.append(values[-1])
    # Dedup while preserving order in case of odd collapse.
    out: list[Any] = []
    for v in kept:
        if not out or out[-1] != v:
            out.append(v)
    return out


def bound_axes(axes: Sequence[SweepAxis], max_points: int) -> list[SweepAxis]:
    """Shrink the longest axes until the cartesian product fits `max_points`."""
    current = [SweepAxis(a.path, tuple(a.values)) for a in axes]
    guard = 0
    while _prod([len(a.values) for a in current]) > max_points and guard < 64:
        guard += 1
        longest = max(range(len(current)), key=lambda i: len(current[i].values))
        if len(current[longest].values) <= 2:
            break
        current[longest] = SweepAxis(current[longest].path, tuple(_downsample_keep_ends(current[longest].values)))
    return current


def iter_sweep_points(spec: SweepSpec) -> list[dict[str, Any]]:
    """Return the (possibly reduced) cartesian product as patch dicts.

    If the reduced product still exceeds `max_points` (all axes already
    length 2), take a deterministic prefix that includes the first and
    last corners.
    """
    axes = bound_axes(spec.axes, spec.max_points)
    bags = [list(a.values) for a in axes]
    paths = [a.path for a in axes]
    raw = list(product(*bags))
    if len(raw) > spec.max_points:
        chosen = [raw[0]]
        if spec.max_points > 2:
            step = max(1, (len(raw) - 1) // (spec.max_points - 1))
            for i in range(step, len(raw) - 1, step):
                chosen.append(raw[i])
                if len(chosen) >= spec.max_points - 1:
                    break
        if raw[-1] not in chosen:
            chosen.append(raw[-1])
        raw = chosen[: spec.max_points]
    return [{path: value for path, value in zip(paths, combo, strict=True)} for combo in raw]


def parse_axis(token: str) -> SweepAxis:
    """Parse `path=v1,v2,v3` used by the CLI."""
    if "=" not in token:
        raise ValueError(f"axis must look like path=v1,v2: {token!r}")
    path, raw = token.split("=", 1)
    values: list[Any] = []
    for item in raw.split(","):
        item = item.strip()
        if item == "":
            continue
        try:
            if item.isdigit() or (item.startswith("-") and item[1:].isdigit()):
                values.append(int(item))
            else:
                values.append(float(item))
        except ValueError:
            values.append(item)
    return SweepAxis(path.strip(), tuple(values))
