"""Generate macOS menu bar template icons for Thunder WiFi Switcher.

Designed on a 36x36 grid (= 18pt @2x). Template images are pure black;
macOS uses only the alpha channel and tints them for light/dark menu bars.
Inactive arcs are drawn at low alpha, like the system Wi-Fi icon.

Writes the SVGs and 1x/2x/3x PNGs next to this script, replacing the
committed ones. Needs rsvg-convert (macOS: brew install librsvg).

Usage: python3 assets/icons/menubar/gen.py
"""
import os, subprocess

OUT = os.path.dirname(os.path.abspath(__file__))

CX, CY = 18, 30
RADII = [9, 15.5, 22]
STROKE = 4
FAINT = 0.28
BOLT = "M17.58 24.5 L21.4 24.5 L18.78 28.53 L22 28.53 L15.43 35.5 L17.1 30.18 L14 30.18 Z"


def arc(r):
    d = r * 0.70710678
    return f"M{CX-d:.2f} {CY-d:.2f} A{r} {r} 0 0 1 {CX+d:.2f} {CY-d:.2f}"


def svg(level, disconnected=False):
    arcs = []
    for i, r in enumerate(RADII):
        op = 1 if (not disconnected and i < level) else FAINT
        arcs.append(f'<path d="{arc(r)}" stroke-opacity="{op}"/>')
    bolt_op = FAINT if disconnected else 1
    body = (
        f'<g fill="none" stroke="#000" stroke-width="{STROKE}" stroke-linecap="round">{"".join(arcs)}</g>'
        f'<path d="{BOLT}" fill="#000" fill-opacity="{bolt_op}" stroke="#000" stroke-opacity="{bolt_op}" stroke-width="0.8" stroke-linejoin="round"/>'
    )
    if disconnected:
        # knock a gap out around the slash so it reads at 18px
        body = (
            '<defs><mask id="m" maskUnits="userSpaceOnUse" x="0" y="0" width="36" height="36">'
            '<rect width="36" height="36" fill="#fff"/>'
            '<line x1="6" y1="5" x2="31" y2="33" stroke="#000" stroke-width="8" stroke-linecap="round"/>'
            f'</mask></defs><g mask="url(#m)">{body}</g>'
            '<line x1="6" y1="5" x2="31" y2="33" stroke="#000" stroke-width="3.6" stroke-linecap="round"/>'
        )
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36" width="36" height="36">{body}</svg>\n'


states = {f"ThunderWiFi{n}": dict(level=n) for n in range(4)}
states["ThunderWiFiOff"] = dict(level=0, disconnected=True)

for name, kw in states.items():
    p = os.path.join(OUT, f"{name}Template.svg")
    open(p, "w").write(svg(**kw))
    for scale, suffix in ((1, ""), (2, "@2x"), (3, "@3x")):
        subprocess.run(["rsvg-convert", "-w", str(18 * scale), "-h", str(18 * scale), p,
                        "-o", os.path.join(OUT, f"{name}Template{suffix}.png")], check=True)
    print(name)
