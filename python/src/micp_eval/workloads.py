"""Synthetic arrival processes for experiment scripts.

Mirrors the contract of `micp-sim` (exponential inter-arrivals, class
mix, seed) using numpy's Generator so large sweeps stay in Python.
Timestamps are not required to be bit-identical with the Rust crate.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

CLASS_INTERACTIVE = "interactive"
CLASS_BATCH = "batch"
CLASS_OFFLINE = "offline"


@dataclass(frozen=True)
class ArrivalSpec:
    arrival_rate_rps: float
    duration_s: float
    seed: int
    class_mix: dict[str, float] = field(default_factory=lambda: {CLASS_INTERACTIVE: 0.8, CLASS_BATCH: 0.2})


@dataclass(frozen=True)
class Arrival:
    t_s: float
    traffic_class: str


def generate_arrivals(spec: ArrivalSpec) -> list[Arrival]:
    if spec.arrival_rate_rps <= 0 or spec.duration_s <= 0:
        raise ValueError("arrival_rate_rps and duration_s must be positive")
    labels = list(spec.class_mix.keys())
    weights = np.array([spec.class_mix[k] for k in labels], dtype=float)
    if weights.sum() <= 0:
        raise ValueError("class_mix must have positive weight")
    weights = weights / weights.sum()

    rng = np.random.default_rng(spec.seed)
    t = 0.0
    out: list[Arrival] = []
    while True:
        t += rng.exponential(1.0 / spec.arrival_rate_rps)
        if t >= spec.duration_s:
            break
        cls = labels[int(rng.choice(len(labels), p=weights))]
        out.append(Arrival(t_s=float(t), traffic_class=cls))
    return out
