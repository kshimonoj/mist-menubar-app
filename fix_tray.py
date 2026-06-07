# fix_tray.py
from PIL import Image
import os

src = "src-tauri/icons/tray_source.png"
if not os.path.exists(src):
    src = "src-tauri/icons/tray.png"
    print(f"tray_source.png not found, using {src}")

img = Image.open(src).convert("RGBA")
pixels = img.load()
w, h = img.size

for y in range(h):
    for x in range(w):
        r, g, b, a = pixels[x, y]
        brightness = (r + g + b) / 3
        if brightness < 40:
            pixels[x, y] = (0, 0, 0, 0)   # 黒背景 → 透明
        else:
            pixels[x, y] = (r, g, b, 255) # 白シルエット → 保持

img.save("src-tauri/icons/tray.png")
print(f"saved tray.png ({w}x{h})")
