"""Small stdlib queue peer, run over an owned SSH channel or imported locally.

Only fixed operations under ~/excel-queue; request contents never become commands.
An input is removed only after an atomically published answer exists.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import sys

MAX_BYTES = 16 * 1024 * 1024


def operation(root, message):
    root = Path(root).expanduser()
    for name in ("in", "out"):
        (root / name).mkdir(parents=True, exist_ok=True)
    action = message["op"]
    if action == "take":
        # At most one request per peer in flight. Keep the source until delivery;
        # if either peer dies it is rediscovered and answered from durable state.
        for path in sorted((root / "in").glob("*.json")):
            if not re.fullmatch(r"[0-9a-f]{32}\.json", path.name) or path.is_symlink():
                continue
            with path.open("rb") as stream:
                raw = stream.read(MAX_BYTES + 1)
            return {"id": path.stem, "raw": base64.b64encode(raw).decode("ascii"),
                    "oversized": path.stat().st_size > MAX_BYTES}
        return None
    if action == "put":
        request_id = message["id"]
        if not re.fullmatch("[0-9a-f]{32}", request_id):
            raise ValueError("invalid response id")
        destination = root / "out" / (request_id + ".json")
        source = root / "in" / destination.name
        if source.is_symlink():
            raise ValueError("request is a symlink")
        if source.exists():
            with source.open("rb") as stream:
                digest = hashlib.sha256(stream.read(MAX_BYTES + 1)).hexdigest()
            if digest != message["answer"]["request_sha256"]:
                return False  # Changed while running: rediscover and refuse the reused id.
        temporary = destination.with_suffix(".tmp")
        with temporary.open("w", encoding="utf-8") as stream:
            json.dump(message["answer"], stream, allow_nan=False, ensure_ascii=True)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, destination)
        source.unlink(missing_ok=True)
        return True
    raise ValueError("unknown queue operation")


if __name__ == "__main__":
    for line in sys.stdin:
        try:
            response = {"ok": True, "value": operation(Path.home() / "excel-queue", json.loads(line))}
        except Exception as error:
            response = {"ok": False, "error": str(error)}
        print(json.dumps(response, ensure_ascii=True), flush=True)
