from __future__ import annotations

import os
import socket
import subprocess
import time
from collections.abc import Iterator
from pathlib import Path

import pytest

from micp_eval.engine import HttpEngine
from micp_eval.metadata import repo_root


def fixtures_dir() -> Path:
    return Path(__file__).parent / "fixtures"


@pytest.fixture
def fxdir() -> Path:
    return fixtures_dir()


def _api_bin() -> Path | None:
    env = os.environ.get("MICP_API_BIN")
    if env:
        p = Path(env)
        return p if p.is_file() else None
    root = repo_root(Path(__file__))
    if root is None:
        return None
    p = root / "target" / "release" / "micp-api"
    return p if p.is_file() else None


def _free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


@pytest.fixture(scope="session")
def live_engine() -> Iterator[HttpEngine]:
    binary = _api_bin()
    if binary is None:
        pytest.skip("micp-api release binary not present")
    port = _free_port()
    env = os.environ.copy()
    env["MICP_BIND"] = f"127.0.0.1:{port}"
    proc = subprocess.Popen(
        [str(binary)],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        cwd=str(repo_root(Path(__file__)) or Path.cwd()),
    )
    url = f"http://127.0.0.1:{port}"
    engine = HttpEngine(url, timeout_s=5.0)
    deadline = time.time() + 8.0
    last_err: Exception | None = None
    while time.time() < deadline:
        try:
            status, body = engine.client.get("/health")
            if status == 200 and isinstance(body, dict):
                break
        except Exception as exc:  # noqa: BLE001
            last_err = exc
        time.sleep(0.05)
    else:
        proc.kill()
        pytest.skip(f"micp-api did not become ready: {last_err}")
    try:
        yield engine
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
