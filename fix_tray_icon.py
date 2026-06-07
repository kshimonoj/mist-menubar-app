from PIL import Image
import os

src = "src-tauri/icons/tray_source.png"
src = src if os.path.exists(src) else "src-tauri/icons/tray.png"

img = Image.open(src).convert("RGBA")
pixels = img.load()
w, h = img.size

for y in range(h):
    for x in range(w):
        r, g, b, a = pixels[x, y]
        brightness = (r + g + b) / 3
        if brightness < 40:
            pixels[x, y] = (0, 0, 0, 0)        # 暗い背景 → 透明
        else:
            pixels[x, y] = (r, g, b, 255)      # 白いシルエットを保持

img.save("src-tauri/icons/tray.png")
print(f"tray icon saved: {w}x{h}px from {src}")
