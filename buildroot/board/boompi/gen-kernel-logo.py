#!/usr/bin/env python3
"""Generate the kernel boot-logo PPM from branding/logo.png.

The Boompi logo drawn during kernel boot (see linux-splash.fragment and
patches/linux/0005) is the stock CLUT224 logo mechanism fed a custom
image. The kernel's pnmtologo tool has hard requirements this script
enforces so a bad branding change fails the build loudly instead of
producing a broken (or silently missing) splash:

  - ASCII PPM (P3) - pnmtologo cannot read binary (P6) PNM
  - at most 224 unique colors (CLUT224)
  - small enough to fit every panel the unified image drives
    (HyperPixel 4.0: 480x800 portrait, drawn on the rotated 800x480
    console; nates' HDMI panel: 1024x600)

Invoked by the BOOMPI_LINUX_INSTALL_LOGO hook in external.mk, which
writes straight over drivers/video/logo/logo_linux_clut224.ppm in the
extracted kernel tree. branding/logo.png is the single source of truth.

Usage: gen-kernel-logo.py <branding/logo.png> <output.ppm>
"""

import sys

try:
    from PIL import Image
except ImportError:
    sys.exit(
        "gen-kernel-logo.py: python3 Pillow is required on the build host "
        "(CI installs python3-pil; locally: pip install pillow)"
    )

# Must fit the smallest console the image drives, with margin. The
# HyperPixel's rotated console is 800x480; fbcon refuses logos taller
# than the screen and pnmtologo refuses >224 colors.
MAX_WIDTH = 480
MAX_HEIGHT = 320
MAX_COLORS = 224
THUMBNAIL = (320, 214)


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(f"usage: {sys.argv[0]} <logo.png> <output.ppm>")
    src_path, out_path = sys.argv[1], sys.argv[2]

    try:
        src = Image.open(src_path).convert("RGBA")
    except OSError as err:
        sys.exit(f"gen-kernel-logo.py: cannot read {src_path}: {err}")

    # Composite over black (the panel background during boot), shrink,
    # and quantize into the CLUT224 budget.
    bg = Image.new("RGBA", src.size, "black")
    bg.alpha_composite(src)
    img = bg.convert("RGB")
    img.thumbnail(THUMBNAIL, Image.Resampling.LANCZOS)
    img = img.quantize(colors=MAX_COLORS, method=Image.Quantize.MEDIANCUT)
    img = img.convert("RGB")

    w, h = img.size
    if w > MAX_WIDTH or h > MAX_HEIGHT:
        sys.exit(
            f"gen-kernel-logo.py: {w}x{h} exceeds {MAX_WIDTH}x{MAX_HEIGHT} "
            "(would not fit the smallest panel)"
        )
    colors = img.getcolors(maxcolors=1 << 24)
    if colors is None or len(colors) > MAX_COLORS:
        n = "?" if colors is None else len(colors)
        sys.exit(
            f"gen-kernel-logo.py: {n} unique colors exceeds the CLUT224 "
            f"limit of {MAX_COLORS} (quantization failed?)"
        )

    # ASCII P3, one pixel per line: the exact shape pnmtologo parses.
    with open(out_path, "w", encoding="ascii") as out:
        out.write(f"P3\n{w} {h}\n255\n")
        out.write("\n".join("%d %d %d" % px for px in img.getdata()))
        out.write("\n")

    print(f"gen-kernel-logo.py: {out_path}: {w}x{h}, {len(colors)} colors")


if __name__ == "__main__":
    main()
