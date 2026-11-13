from PIL import Image, ImageDraw
import math

def draw_icon(size):
    s = size / 512.0
    img = Image.new("RGBA", (size, size), (238, 244, 236, 255))
    d = ImageDraw.Draw(img)
    # soft radial backdrop circle
    d.ellipse([int(24*s), int(24*s), int(488*s), int(488*s)], fill=(250, 252, 248, 255), outline=(220, 231, 216, 255), width=max(2, int(6*s)))
    # tail
    d.polygon([(int(60*s), int(256*s)), (int(150*s), int(190*s)), (int(150*s), int(322*s))], fill=(143, 203, 159, 255))
    # body
    d.ellipse([int(118*s), int(148*s), int(420*s), int(364*s)], fill=(169, 212, 178, 255))
    d.ellipse([int(150*s), int(262*s), int(360*s), int(370*s)], fill=(221, 238, 223, 255))
    # eye
    d.ellipse([int(300*s), int(196*s), int(356*s), int(252*s)], fill=(255, 255, 255, 255))
    d.ellipse([int(318*s), int(212*s), int(352*s), int(244*s)], fill=(31, 58, 45, 255))
    d.ellipse([int(328*s), int(216*s), int(336*s), int(226*s)], fill=(255, 255, 255, 255))
    # smile
    d.arc([int(272*s), int(240*s), int(356*s), int(310*s)], start=20, end=160, fill=(58, 127, 84, 255), width=max(2, int(10*s)))
    # spots
    for cx, cy, r in [(215, 200, 13), (255, 175, 11), (185, 245, 11)]:
        d.ellipse([int((cx-r)*s), int((cy-r)*s), int((cx+r)*s), int((cy+r)*s)], fill=(109, 183, 132, 255))
    return img

for size, name in [(32, "icons/32x32.png"), (128, "icons/128x128.png"), (256, "icons/128x128@2x.png")]:
    img = draw_icon(512 if size == 256 else size)
    if img.size[0] != size:
        img = img.resize((size, size), Image.LANCZOS)
    img.save("src-tauri/" + name)

# .ico with multiple sizes
base = draw_icon(512)
base.save("src-tauri/icons/icon.ico", sizes=[(16,16),(32,32),(48,48),(64,64),(128,128),(256,256)])
print("icons written")
