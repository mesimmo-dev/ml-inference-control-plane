"""Environment metadata for experiment artifacts."""

from __future__ import annotations

import platform
import subprocess
import sys
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd

PACKAGE_VERSION = "0.1.0"


def repo_root(start: Path | None = None) -> Path | None:
    cur = (start or Path.cwd()).resolve()
    for candidate in [cur, *cur.parents]:
        if (candidate / "Cargo.toml").is_file() and (candidate / "python").is_dir():
            return candidate
    return None


def git_sha(root: Path | None = None) -> str | None:
    cwd = root or repo_root() or Path.cwd()
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "HEAD"],
            cwd=cwd,
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=5,
        )
        return out.strip() or None
    except (OSError, subprocess.SubprocessError):
        return None


def environment_info(root: Path | None = None) -> dict[str, Any]:
    """Software/environment block. Local machine facts, not a cluster."""
    return {
        "python": sys.version.split()[0],
        "platform": platform.platform(),
        "machine": platform.machine(),
        "numpy": np.__version__,
        "pandas": pd.__version__,
        "micp_eval": PACKAGE_VERSION,
        "git_sha": git_sha(root),
    }
