#!/usr/bin/env python3
"""Build tests/fixtures/plugins/runory.django-deployer into a .runory-plugin ZIP."""

from __future__ import annotations

import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "tests" / "fixtures" / "plugins" / "runory.django-deployer"
OUT = SRC.parent / "runory.django-deployer.runory-plugin"


def main() -> None:
    if OUT.exists():
        OUT.unlink()
    with zipfile.ZipFile(OUT, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in SRC.rglob("*"):
            if path.is_file():
                archive.write(path, path.relative_to(SRC).as_posix())
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
