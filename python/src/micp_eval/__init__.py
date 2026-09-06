"""Off-path evaluation tooling for the ML inference control plane.

Experiment orchestration, sweeps, sensitivity, and statistical summaries
live here. Modeled and simulated numbers come from the Rust engine via
`micp-api`. Python reference formulas are labeled as such and are not
substitutes for the engine.
"""

from .benchmarks import Candidate, generate_fleet, local_bench
from .engine import SCENARIO_IDS, FixtureEngine, HttpEngine, PlanOutcome
from .experiments import run_all_scenarios, run_scenario, run_simulations, run_sweep
from .sensitivity import run_sensitivity
from .stats import bootstrap_mean_ci, paired_delta, percentile, summarize
from .sweeps import SweepAxis, SweepSpec, WeightGrid, iter_sweep_points, iter_weight_grid
from .workloads import ArrivalSpec, generate_arrivals

__all__ = [
    "ArrivalSpec",
    "Candidate",
    "FixtureEngine",
    "HttpEngine",
    "PlanOutcome",
    "SCENARIO_IDS",
    "SweepAxis",
    "SweepSpec",
    "WeightGrid",
    "bootstrap_mean_ci",
    "generate_arrivals",
    "generate_fleet",
    "iter_sweep_points",
    "iter_weight_grid",
    "local_bench",
    "paired_delta",
    "percentile",
    "run_all_scenarios",
    "run_scenario",
    "run_sensitivity",
    "run_simulations",
    "run_sweep",
    "summarize",
]

__version__ = "0.1.0"
