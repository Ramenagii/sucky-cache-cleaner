"""App icons for Sucky — spotted pleco in an ember apron, on warm paper."""
import os
from PIL import Image, ImageDraw

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "src-tauri", "icons")


def sc(v, s):
    return int(round(v * s))


def draw_icon(size):
    s = size / 512.0
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    d.ellipse([sc(8, s), sc(8, s), sc(504, s), sc(504, s)], fill=(237, 237, 231, 255))
    # fins
    d.polygon(
        [(sc(78, s), sc(300, s)), (sc(168, s), sc(250, s)), (sc(176, s), sc(360, s))],
        fill=(166, 124, 82, 255),
    )
    d.polygon(
        [(sc(434, s), sc(300, s)), (sc(344, s), sc(250, s)), (sc(336, s), sc(360, s))],
        fill=(166, 124, 82, 255),
    )
    # dorsal
    d.polygon(
        [(sc(220, s), sc(118, s)), (sc(256, s), sc(48, s)), (sc(292, s), sc(118, s))],
        fill=(140, 98, 64, 255),
    )
    # body + belly
    d.ellipse([sc(108, s), sc(108, s), sc(404, s), sc(420, s)], fill=(196, 149, 106, 255))
    d.ellipse([sc(168, s), sc(220, s), sc(344, s), sc(340, s)], fill=(240, 210, 168, 255))
    # spots
    for cx, cy, r in ((150, 210, 16), (256, 150, 13), (360, 214, 15), (132, 280, 11), (378, 286, 11)):
        d.ellipse([sc(cx - r, s), sc(cy - r, s), sc(cx + r, s), sc(cy + r, s)], fill=(92, 58, 36, 255))
    # apron, pocket, flame
    d.rounded_rectangle([sc(168, s), sc(268, s), sc(344, s), sc(400, s)], radius=sc(28, s), fill=(194, 65, 12, 255))
    d.rounded_rectangle([sc(220, s), sc(318, s), sc(292, s), sc(366, s)], radius=sc(12, s), fill=(154, 52, 18, 255))
    d.polygon(
        [
            (sc(256, s), sc(286, s)),
            (sc(242, s), sc(314, s)),
            (sc(256, s), sc(304, s)),
            (sc(272, s), sc(318, s)),
            (sc(262, s), sc(292, s)),
        ],
        fill=(255, 194, 51, 255),
    )
    # eyes
    d.ellipse([sc(156, s), sc(168, s), sc(236, s), sc(258, s)], fill=(255, 255, 255, 255))
    d.ellipse([sc(276, s), sc(168, s), sc(356, s), sc(258, s)], fill=(255, 255, 255, 255))
    d.ellipse([sc(188, s), sc(196, s), sc(226, s), sc(242, s)], fill=(20, 20, 20, 255))
    d.ellipse([sc(308, s), sc(196, s), sc(346, s), sc(242, s)], fill=(20, 20, 20, 255))
    d.ellipse([sc(208, s), sc(204, s), sc(218, s), sc(216, s)], fill=(255, 255, 255, 255))
    d.ellipse([sc(328, s), sc(204, s), sc(338, s), sc(216, s)], fill=(255, 255, 255, 255))
    # smile
    d.arc(
        [sc(214, s), sc(228, s), sc(298, s), sc(292, s)],
        start=20,
        end=160,
        fill=(20, 20, 20, 255),
        width=max(2, sc(10, s)),
    )
    return img


def main():
    os.makedirs(ROOT, exist_ok=True)
    for size, name in ((32, "32x32.png"), (128, "128x128.png"), (256, "128x128@2x.png")):
        img = draw_icon(512 if size == 256 else max(size, 128))
        if img.size[0] != size:
            img = img.resize((size, size), Image.LANCZOS)
        img.save(os.path.join(ROOT, name))
    base = draw_icon(512)
    base.save(
        os.path.join(ROOT, "icon.ico"),
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print("icons written to", ROOT)


if __name__ == "__main__":
    main()
