import numpy as np

from micp_eval.benchmarks import generate_fleet
from micp_eval.sweeps import WeightGrid, iter_weight_grid, weight_matrix


def test_weight_grid_sums_to_one():
    rows = list(iter_weight_grid(WeightGrid(step=0.5)))
    assert rows
    for w in rows:
        assert abs(sum(w.values()) - 1.0) < 1e-9
        assert set(w) == {"latency", "quality", "cost", "throughput", "reliability"}


def test_weight_matrix_shape():
    m = weight_matrix(WeightGrid(step=0.5))
    assert m.ndim == 2
    assert m.shape[1] == 5
    assert np.allclose(m.sum(axis=1), 1.0)


def test_fleet_size_and_ids():
    fleet = generate_fleet(4, seed=0)
    assert len(fleet) == 4
    assert [c.model_id for c in fleet] == ["m00", "m01", "m02", "m03"]
    assert all(0.0 < c.quality <= 1.0 for c in fleet)
