"""Generate the Microsoft Store (MSIX) tile and icon images from the app logo.

Requires Pillow (`pip install pillow`). Run from the repo root after
scripts/gen_icons.py:

    python scripts/gen_msix_assets.py

Reads assets/logo-256.png (big tiles) and the .ico in assets/ (small icons,
which use the simpler small artwork) and writes msix/Assets/.
"""
import glob
import os

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "msix", "Assets")


def ico_frame(ico, size):
    """The .ico frame closest to `size` (at least as big), scaled to `size`."""
    sizes = sorted(ico.info["sizes"], key=lambda s: s[0])
    best = next((s for s in sizes if s[0] >= size), sizes[-1])
    ico.size = best
    return ico.convert("RGBA").resize((size, size), Image.LANCZOS)


def padded(logo, size, fraction):
    """`logo` scaled to `fraction` of a transparent `size` square, centred."""
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    inner = round(size * fraction)
    canvas.alpha_composite(logo.resize((inner, inner), Image.LANCZOS), ((size - inner) // 2,) * 2)
    return canvas


def main():
    os.makedirs(OUT, exist_ok=True)
    logo = Image.open(os.path.join(ROOT, "assets", "logo-256.png")).convert("RGBA")
    ico = Image.open(glob.glob(os.path.join(ROOT, "assets", "*.ico"))[0])

    def save(img, name):
        img.save(os.path.join(OUT, name))

    # Start menu / taskbar icon, plus the unplated sizes Windows prefers.
    save(ico_frame(ico, 44), "Square44x44Logo.png")
    for size in (16, 24, 32, 48, 256):
        icon = ico_frame(ico, size) if size < 256 else logo
        save(icon, f"Square44x44Logo.targetsize-{size}.png")
        save(icon, f"Square44x44Logo.targetsize-{size}_altform-unplated.png")
    save(padded(logo, 150, 0.66), "Square150x150Logo.png")
    save(padded(logo, 50, 1.0), "StoreLogo.png")
    wide = Image.new("RGBA", (310, 150), (0, 0, 0, 0))
    wide.alpha_composite(logo.resize((100, 100), Image.LANCZOS), (105, 25))
    save(wide, "Wide310x150Logo.png")
    print(f"Wrote {OUT}")


if __name__ == "__main__":
    main()
