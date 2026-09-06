import numpy as np
import pytest

from micp_eval.stats import bootstrap_mean_ci, paired_delta, percentile


def test_percentile_median():
    x = np.arange(1, 101, dtype=float)
    assert percentile(x, 50) == pytest.approx(50.5)


def test_bootstrap_ci_covers_true_mean():
    rng = np.random.default_rng(0)
    x = rng.normal(loc=10.0, scale=1.0, size=400)
    mean, low, high = bootstrap_mean_ci(x, confidence=0.95, n_boot=1000, seed=1)
    assert low < mean < high
    assert low < 10.0 < high


def test_paired_delta():
    a = np.array([1.0, 2.0, 3.0])
    b = np.array([0.0, 2.0, 2.0])
    assert paired_delta(a, b) == pytest.approx(2.0 / 3.0)


def test_rejects_empty():
    with pytest.raises(ValueError):
        percentile(np.array([]), 50)
