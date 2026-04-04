import numpy as np
from PIL import Image
import random
from pathlib import Path

# 出力ディレクトリを作成
output_dir = Path("generated_images")
output_dir.mkdir(exist_ok=True)

# 生成する回数
count = 1

# 画像サイズの定義
sizes = [(10000, 10000)]

# 指定したpxごとにランダムに色が変わる画像を作成
block_size = 100
print("=== {}x{}pxごとにランダム色の画像を作成中 ===".format(block_size, block_size))

for count in range(count):
    for width, height in sizes:
        # 画像配列を作成
        img_array = np.zeros((height, width, 3), dtype=np.uint8)

        # 指定したpxのブロックごとに色を設定
        for y in range(0, height, block_size):
            for x in range(0, width, block_size):
                # ランダムなRGB色を生成
                color = (
                    random.randint(0, 255),
                    random.randint(0, 255),
                    random.randint(0, 255)
                )

                # ブロックの終端座標を計算（画像境界を超えないように）
                y_end = min(y + block_size, height)
                x_end = min(x + block_size, width)

                # ブロック領域に色を設定
                img_array[y:y_end, x:x_end] = color

        # PIL Imageに変換して保存
        img = Image.fromarray(img_array)
        filename = output_dir / f"{width}x{height}_{count}.png"
        img.save(filename)
        print(f"作成: {filename} ({width}x{height}px)")

print()
print("=== 完了 ===")
print(f"すべての画像を '{output_dir}' ディレクトリに保存しました")
