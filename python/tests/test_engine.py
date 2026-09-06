from __future__ import annotations

import json
from pathlib import Path

import pytest

from micp_eval.client import EngineError
from micp_eval.engine import FixtureEngine

from fakes import FakeEngine, toy_scenario


def test_fake_unknown_scenario() -> None:
    with pytest.raises(EngineError):
        FakeEngine().get_scenario("nope")


def test_fixture_engine_replays(tmp_path: Path) -> None:
    scenario = toy_scenario()
    plan = FakeEngine().evaluate(scenario).plan
    assert plan is not None
    (tmp_path / "scenarios").mkdir()
    (tmp_path / "plans").mkdir()
    (tmp_path / "scenarios" / "interactive_assistant.json").write_text(json.dumps(scenario))
    (tmp_path / "plans" / "interactive_assistant.json").write_text(json.dumps(plan))
    eng = FixtureEngine(tmp_path)
    cards = eng.list_scenarios()
    assert cards[0]["id"] == "interactive_assistant"
    got = eng.get_scenario("interactive_assistant")
    out = eng.evaluate(got)
    assert out.feasible
    patched = dict(got)
    patched["name"] = "changed"
    with pytest.raises(EngineError):
        eng.evaluate(patched)
