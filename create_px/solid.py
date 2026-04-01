import numpy as np
from PIL import Image
import random
from pathlib import Path

# 出力ディレクトリを作成
output_dir = Path("generated_images")
output_dir.mkdir(exist_ok=True)

# 画像サイズの定義
sizes = [(500, 500), (1000, 1000)]

# 一色の画像を作成
print("=== 一色の画像を作成中 ===")
solid_colors = {
    'red': (255, 0, 0),
    'green': (0, 255, 0),
    'blue': (0, 0, 255),
    'cyan': (0, 255, 255),
    'magenta': (255, 0, 255),
    'yellow': (255, 255, 0),
    'black': (0, 0, 0),
    'white': (255, 255, 255),
}

for color_name, color_rgb in solid_colors.items():
    for width, height in sizes:
        img = Image.new('RGB', (width, height), color_rgb)
        filename = output_dir / f"solid_{color_name}_{width}x{height}.png"
        img.save(filename)
        print(f"作成: {filename} ({width}x{height}px, {color_name})")

print()

print("=== 完了 ===")
print(f"すべての画像を '{output_dir}' ディレクトリに保存しました")
