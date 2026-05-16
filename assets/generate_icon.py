#!/usr/bin/env python3
"""Generate the ram-monitor app icon: an old-school DIP RAM chip on a PCB-tinted background."""

from __future__ import annotations

import os
import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageFont

SIZE = 1024
HERE = Path(__file__).resolve().parent

# --- palette ---------------------------------------------------------------
PCB_DARK   = (10, 38, 26)        # dark PCB green background
PCB_MID    = (18, 70, 48)
PCB_TRACE  = (200, 168, 80, 70)  # faint copper trace
CHIP_BODY  = (24, 24, 26)        # ceramic black
CHIP_HI    = (60, 60, 65)        # top highlight on body
CHIP_LO    = (8, 8, 10)          # bottom shadow on body
LEG_BASE   = (140, 140, 150)     # pin metal
LEG_HI     = (210, 210, 220)
LEG_LO     = (70, 70, 80)
SILK_WHITE = (235, 230, 215)     # silkscreen / labels
PIN1_DOT   = (235, 230, 215)
NOTCH_SHAD = (4, 4, 6)


def rounded_rect_mask(size, radius):
    mask = Image.new("L", size, 0)
    d = ImageDraw.Draw(mask)
    d.rounded_rectangle((0, 0, size[0] - 1, size[1] - 1), radius=radius, fill=255)
    return mask


def find_font(size, prefer_bold=True):
    candidates = []
    if prefer_bold:
        candidates += [
            "/System/Library/Fonts/Supplemental/Courier New Bold.ttf",
            "/Library/Fonts/Courier New Bold.ttf",
        ]
    candidates += [
        "/System/Library/Fonts/Supplemental/Courier New.ttf",
        "/System/Library/Fonts/Menlo.ttc",
        "/System/Library/Fonts/Monaco.ttf",
    ]
    for p in candidates:
        if os.path.exists(p):
            try:
                return ImageFont.truetype(p, size)
            except OSError:
                continue
    return ImageFont.load_default()


def draw_background(img):
    w, h = img.size
    base = Image.new("RGB", (w, h), PCB_DARK)
    # vertical gradient to PCB_MID at top
    overlay = Image.new("L", (w, h), 0)
    for y in range(h):
        overlay.putpixel((0, y), int(80 * (1 - y / h)))
    overlay = overlay.resize((w, h))
    grad = Image.new("RGB", (w, h), PCB_MID)
    base.paste(grad, mask=overlay)

    d = ImageDraw.Draw(base, "RGBA")
    # subtle PCB traces
    step = w // 12
    for i in range(-2, 14):
        x = i * step + (w // 24)
        d.line([(x, 0), (x, h)], fill=PCB_TRACE, width=4)
    for j in range(-2, 14):
        y = j * step + (h // 24)
        d.line([(0, y), (w, y)], fill=PCB_TRACE, width=4)

    # rounded mask for app icon shape (macOS-ish squircle radius)
    mask = rounded_rect_mask((w, h), radius=int(w * 0.22))
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    out.paste(base, (0, 0), mask=mask)
    return out


def draw_chip(canvas):
    w, h = canvas.size
    d = ImageDraw.Draw(canvas, "RGBA")

    # chip body bounds (a wide DIP-ish rectangle, slightly portrait-rectangular)
    chip_w = int(w * 0.62)
    chip_h = int(h * 0.46)
    cx, cy = w // 2, h // 2
    x0, y0 = cx - chip_w // 2, cy - chip_h // 2
    x1, y1 = cx + chip_w // 2, cy + chip_h // 2

    # legs (pins) on left and right
    pin_rows = 8
    pin_h = int(chip_h / (pin_rows * 1.6))
    pin_gap = (chip_h - pin_h * pin_rows) / (pin_rows + 1)
    pin_w = int(w * 0.07)

    def draw_pin(side_x_outer, side_x_inner, y_top):
        # darker base, light highlight on top, dark line on bottom
        d.rectangle([side_x_outer, y_top, side_x_inner, y_top + pin_h], fill=LEG_BASE)
        d.rectangle([side_x_outer, y_top, side_x_inner, y_top + max(2, pin_h // 4)], fill=LEG_HI)
        d.rectangle(
            [side_x_outer, y_top + pin_h - max(2, pin_h // 5), side_x_inner, y_top + pin_h],
            fill=LEG_LO,
        )

    for i in range(pin_rows):
        py = int(y0 + pin_gap * (i + 1) + pin_h * i)
        # left pins extend outward from x0
        draw_pin(x0 - pin_w, x0 + int(pin_w * 0.15), py)
        # right pins extend outward to x1 + pin_w
        draw_pin(x1 - int(pin_w * 0.15), x1 + pin_w, py)

    # chip body with subtle gradient (top brighter, bottom darker)
    body = Image.new("RGB", (chip_w, chip_h), CHIP_BODY)
    bd = ImageDraw.Draw(body)
    for y in range(chip_h):
        t = y / chip_h
        r = int(CHIP_HI[0] * (1 - t) + CHIP_LO[0] * t)
        g = int(CHIP_HI[1] * (1 - t) + CHIP_LO[1] * t)
        b = int(CHIP_HI[2] * (1 - t) + CHIP_LO[2] * t)
        bd.line([(0, y), (chip_w, y)], fill=(r, g, b))
    # darken middle with the base colour
    middle = Image.new("RGB", (chip_w, chip_h), CHIP_BODY)
    mask = Image.new("L", (chip_w, chip_h), 0)
    md = ImageDraw.Draw(mask)
    md.rounded_rectangle((4, 4, chip_w - 5, chip_h - 5), radius=24, fill=200)
    body.paste(middle, (0, 0), mask=mask)

    # rounded corner mask for the chip itself
    chip_mask = rounded_rect_mask((chip_w, chip_h), radius=28)
    canvas.paste(body, (x0, y0), mask=chip_mask)

    # top notch (semicircle) cut into the chip
    notch_r = int(chip_w * 0.08)
    notch_cx = cx
    notch_cy = y0  # sits on top edge
    # draw a filled background-coloured circle to "cut" the notch
    # we instead overlay the PCB colour
    pcb_pixel = canvas.getpixel((cx, 6)) if cy > 10 else PCB_DARK
    d.ellipse(
        [notch_cx - notch_r, notch_cy - notch_r, notch_cx + notch_r, notch_cy + notch_r],
        fill=pcb_pixel,
    )
    # inner shadow on notch
    d.arc(
        [notch_cx - notch_r, notch_cy - notch_r, notch_cx + notch_r, notch_cy + notch_r],
        start=0,
        end=180,
        fill=NOTCH_SHAD,
        width=6,
    )

    # pin-1 dot indicator (top-left corner of chip, just inside)
    dot_r = int(chip_w * 0.025)
    dot_cx = x0 + int(chip_w * 0.10)
    dot_cy = y0 + int(chip_h * 0.18)
    d.ellipse(
        [dot_cx - dot_r, dot_cy - dot_r, dot_cx + dot_r, dot_cy + dot_r],
        outline=PIN1_DOT,
        width=4,
    )

    # silkscreen labels
    font_part = find_font(int(chip_h * 0.22), prefer_bold=True)
    font_sub = find_font(int(chip_h * 0.11), prefer_bold=True)

    part_text = "MK4164"
    sub_text = "RAM-MON"

    # measure
    bbox_part = d.textbbox((0, 0), part_text, font=font_part)
    bbox_sub = d.textbbox((0, 0), sub_text, font=font_sub)
    part_w = bbox_part[2] - bbox_part[0]
    part_h = bbox_part[3] - bbox_part[1]
    sub_w = bbox_sub[2] - bbox_sub[0]
    sub_h = bbox_sub[3] - bbox_sub[1]

    part_x = cx - part_w // 2
    part_y = cy - part_h // 2 - int(chip_h * 0.08)
    sub_x = cx - sub_w // 2
    sub_y = part_y + part_h + int(chip_h * 0.06)

    # small subtle drop shadow for engraving feel
    d.text((part_x + 2, part_y + 2), part_text, font=font_part, fill=(0, 0, 0, 200))
    d.text((part_x, part_y), part_text, font=font_part, fill=SILK_WHITE)
    d.text((sub_x + 1, sub_y + 1), sub_text, font=font_sub, fill=(0, 0, 0, 200))
    d.text((sub_x, sub_y), sub_text, font=font_sub, fill=SILK_WHITE)


def main():
    img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    bg = draw_background(img)
    img.alpha_composite(bg)
    draw_chip(img)

    # outer glow / inner shadow for icon edge
    mask = rounded_rect_mask((SIZE, SIZE), radius=int(SIZE * 0.22))
    final = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    final.paste(img, (0, 0), mask=mask)

    out_dir = HERE
    out_path = out_dir / "icon-1024.png"
    final.save(out_path, "PNG")
    print(f"wrote {out_path}")


if __name__ == "__main__":
    main()
