"""Statistical helpers for sweep and simulation summaries.

Bootstrap intervals here are **simulation-derived uncertainty estimates**.
They are not empirical production confidence intervals and they are not
a significance test.
"""

from __future__ import annotations

from typing import Any

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


def summarize(
    samples: np.ndarray,
    *,
    confidence: float = 0.95,
    n_boot: int = 1000,
    seed: int = 0,
) -> dict[str, Any]:
    """Descriptive stats plus a bootstrap CI on the mean.

    The CI is labeled as simulation-derived. No p-values.
    """
    x = np.asarray(samples, dtype=float)
    if x.size == 0:
        raise ValueError("samples must be non-empty")
    mean, low, high = bootstrap_mean_ci(x, confidence=confidence, n_boot=n_boot, seed=seed)
    return {
        "n": int(x.size),
        "mean": mean,
        "median": float(np.median(x)),
        "std": float(x.std(ddof=1)) if x.size > 1 else 0.0,
        "min": float(x.min()),
        "max": float(x.max()),
        "q50": percentile(x, 50),
        "q95": percentile(x, 95),
        "q99": percentile(x, 99),
        "mean_ci": {
            "confidence": confidence,
            "low": low,
            "high": high,
            "method": "percentile_bootstrap",
            "label": "simulation-derived uncertainty; not an empirical production interval",
        },
    }
