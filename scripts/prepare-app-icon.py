"""Convert the brand logo JPEG/PNG (black corners) into a transparent master icon."""

from __future__ import annotations

from collections import deque
from pathlib import Path

import numpy as np
from PIL import Image

SRC = Path(
    r"C:\Users\firef\.cursor\projects\d-WorkSpace-Product-Runory\assets"
    r"\c__Users_firef_AppData_Roaming_Cursor_User_workspaceStorage_empty-window_images"
    r"_runory-logo-5cc64ecc-a2a5-4350-9018-ab04af2b4d9f.png"
)
ROOT = Path(__file__).resolve().parents[1]
BRAND = ROOT / "docs" / "assets" / "brand" / "runory-logo.png"
WEB = ROOT / "apps" / "web" / "public" / "runory-logo.png"
TAURI_SRC = ROOT / "src-tauri" / "app-icon.png"
PREVIEW = ROOT / "docs" / "assets" / "brand" / "_preview-transparent.png"

THR = 25
SOFT = 50


def main() -> None:
    arr = np.array(Image.open(SRC).convert("RGBA"))
    h, w = arr.shape[:2]
    dark = (
        arr[:, :, 0].astype(np.int16)
        + arr[:, :, 1].astype(np.int16)
        + arr[:, :, 2].astype(np.int16)
    )

    bg = np.zeros((h, w), dtype=bool)
    q: deque[tuple[int, int]] = deque()
    for seed in ((0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)):
        if dark[seed[1], seed[0]] <= THR:
            bg[seed[1], seed[0]] = True
            q.append(seed)
    while q:
        x, y = q.popleft()
        for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
            if 0 <= nx < w and 0 <= ny < h and not bg[ny, nx] and dark[ny, nx] <= THR:
                bg[ny, nx] = True
                q.append((nx, ny))

    dilated = bg.copy()
    ys, xs = np.where(bg)
    for y, x in zip(ys, xs, strict=False):
        dilated[max(0, y - 2) : min(h, y + 3), max(0, x - 2) : min(w, x + 3)] = True

    out = arr.copy()
    new_a = out[:, :, 3].copy()
    soft_a = np.clip((dark.astype(np.float32) / SOFT) * 255, 0, 255).astype(np.uint8)
    new_a[dilated] = soft_a[dilated]
    new_a[bg] = 0
    out[:, :, 3] = new_a

    result = Image.fromarray(out)
    if result.size != (1024, 1024):
        raise SystemExit(f"expected 1024x1024, got {result.size}")

    result.save(BRAND, format="PNG", optimize=True)
    result.save(WEB, format="PNG", optimize=True)
    result.save(TAURI_SRC, format="PNG", optimize=True)
    if PREVIEW.exists():
        PREVIEW.unlink()

    alpha = out[:, :, 3]
    cys, cxs = np.where(alpha > 10)
    cw = int(cxs.max() - cxs.min() + 1)
    ch = int(cys.max() - cys.min() + 1)
    print(f"saved {BRAND}")
    print(f"saved {WEB}")
    print(f"saved {TAURI_SRC}")
    print(f"size={result.size} mode={result.mode}")
    print(f"content fill={cw / w * 100:.1f}% x {ch / h * 100:.1f}%")
    print(f"transparent={(alpha == 0).mean() * 100:.2f}%")


if __name__ == "__main__":
    main()
