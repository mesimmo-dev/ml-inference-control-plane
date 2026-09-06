"""Off-path evaluation tooling for the ML inference control plane."""

from .benchmarks import Candidate, generate_fleet
from .stats import bootstrap_mean_ci, percentile, paired_delta
from .sweeps import WeightGrid, iter_weight_grid
from .workloads import ArrivalSpec, generate_arrivals

__all__ = [
    "ArrivalSpec",
    "Candidate",
    "WeightGrid",
    "bootstrap_mean_ci",
    "generate_arrivals",
    "generate_fleet",
    "iter_weight_grid",
    "paired_delta",
    "percentile",
]

__version__ = "0.1.0"
