"""Dotted-path get/set over nested scenario JSON.

Paths look like `workload.traffic.mean_rps`, `fleet[id=fast-8b].capacity.degraded_factor`,
or `fleet.*.capacity.degraded_factor` (apply to every list element).
"""

from __future__ import annotations

from copy import deepcopy
from typing import Any


def split_path(path: str) -> list[str]:
    if not path or path.startswith(".") or path.endswith("."):
        raise ValueError(f"invalid path: {path!r}")
    parts: list[str] = []
    buf: list[str] = []
    i = 0
    while i < len(path):
        ch = path[i]
        if ch == "[":
            if buf:
                parts.append("".join(buf))
                buf = []
            close = path.find("]", i)
            if close < 0:
                raise ValueError(f"unclosed bracket in path: {path!r}")
            parts.append(path[i : close + 1])
            i = close + 1
            if i < len(path) and path[i] == ".":
                i += 1
            continue
        if ch == ".":
            if not buf:
                raise ValueError(f"empty path segment in {path!r}")
            parts.append("".join(buf))
            buf = []
            i += 1
            continue
        buf.append(ch)
        i += 1
    if buf:
        parts.append("".join(buf))
    if not parts:
        raise ValueError(f"invalid path: {path!r}")
    return parts


def _item_id(item: Any) -> str | None:
    if not isinstance(item, dict):
        return None
    ident = item.get("id")
    if isinstance(ident, dict):
        ident = ident.get("id") or ident.get("0")
    return str(ident) if ident is not None else None


def _index_token(part: str) -> str:
    if part.startswith("[") and part.endswith("]"):
        return part[1:-1]
    return part


def get_path(obj: Any, path: str) -> Any:
    cur = obj
    for part in split_path(path):
        token = _index_token(part)
        if token == "*":
            if not isinstance(cur, list):
                raise KeyError(f"wildcard applied to non-list at {part}")
            raise ValueError("cannot get a wildcard path; set is supported")
        if token.startswith("id="):
            key = token[3:]
            if not isinstance(cur, list):
                raise KeyError(f"id selector applied to non-list at {part}")
            found = next((item for item in cur if _item_id(item) == key), None)
            if found is None:
                raise KeyError(f"no list item with id={key}")
            cur = found
            continue
        if token.isdigit():
            if not isinstance(cur, list):
                raise KeyError(f"numeric index applied to non-list at {part}")
            cur = cur[int(token)]
            continue
        if not isinstance(cur, dict) or part not in cur:
            raise KeyError(part)
        cur = cur[part]
    return cur


def set_path(obj: Any, path: str, value: Any) -> Any:
    """Return a deep copy of `obj` with `path` set to `value`."""
    clone = deepcopy(obj)
    _set(clone, split_path(path), value)
    return clone


def _set(cur: Any, parts: list[str], value: Any) -> None:
    part = parts[0]
    last = len(parts) == 1
    token = _index_token(part)

    if token == "*":
        if last:
            raise ValueError("wildcard cannot be a leaf")
        if not isinstance(cur, list):
            raise KeyError("wildcard applied to non-list")
        for item in cur:
            _set(item, parts[1:], value)
        return

    if token.startswith("id="):
        key = token[3:]
        if not isinstance(cur, list):
            raise KeyError("id selector applied to non-list")
        found = next((item for item in cur if _item_id(item) == key), None)
        if found is None:
            raise KeyError(f"no list item with id={key}")
        if last:
            raise ValueError("id selector cannot be a leaf")
        _set(found, parts[1:], value)
        return

    if token.isdigit():
        idx = int(token)
        if not isinstance(cur, list):
            raise KeyError("numeric index applied to non-list")
        if last:
            cur[idx] = value
        else:
            _set(cur[idx], parts[1:], value)
        return

    if not isinstance(cur, dict):
        raise KeyError(f"cannot set {part} on non-object")
    if last:
        cur[part] = value
    else:
        if part not in cur:
            raise KeyError(part)
        _set(cur[part], parts[1:], value)


def apply_patches(obj: Any, patches: dict[str, Any]) -> Any:
    out = obj
    for path, value in patches.items():
        out = set_path(out, path, value)
    return out
