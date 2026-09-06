"""Statistical helpers for sweep validation. Numpy only; no scipy."""

from __future__ import annotations

import numpy as np


def percentile(samples: np.ndarray, q: float) -> float:
    if samples.size == 0:
        raise ValueError("samples must be non-empty")
    if not 0.0 <= q <= 100.0:
        raise ValueError("q must be in [0, 100]")
    return float(np.percentile(samples, q))


def bootstrap_mean_ci(
    samples: np.ndarray,
    confidence: float = 0.95,
    n_boot: int = 2000,
    seed: int = 0,
) -> tuple[float, float, float]:
    """Return (mean, low, high) using percentile bootstrap."""
    if samples.size == 0:
        raise ValueError("samples must be non-empty")
    if not 0.0 < confidence < 1.0:
        raise ValueError("confidence must be in (0, 1)")
    rng = np.random.default_rng(seed)
    n = samples.size
    draws = rng.choice(samples, size=(n_boot, n), replace=True)
    means = draws.mean(axis=1)
    alpha = (1.0 - confidence) / 2.0
    low = float(np.quantile(means, alpha))
    high = float(np.quantile(means, 1.0 - alpha))
    return float(samples.mean()), low, high


def paired_delta(a: np.ndarray, b: np.ndarray) -> float:
    """Mean of paired differences `a - b`. Lengths must match."""
    if a.shape != b.shape:
        raise ValueError("paired samples must have the same shape")
    if a.size == 0:
        raise ValueError("samples must be non-empty")
    return float((a - b).mean())
