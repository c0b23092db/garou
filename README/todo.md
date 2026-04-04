# TODO

- smolによる非同期の画像処理
- imageによるリサイズ
- RGBAキャッシュ
- u32チャンク比較
- filebarを消しているとき、statusbarが画像切り替え時に点滅するバグ

## 画像の表示ができない・遅い
transport_mode = file
1920x1080: 1\~2ms
3840x2160: 1\~3ms
3024x4032: 2\~3ms
3500x1724: 2.5\~3.5ms
2894x4093: 10\~15ms
7680x5120: 表示不可
ターミナルのセル解像度を超えた情報は物理的に表示できない。
`表示上限 = セル数 × 1セルのピクセルサイズ × DPIスケール`

### 対策：画像を圧縮して送信
imageクレートのresizeを行う。

### 対策：Numpy的に早くする
- std::simd（nightly）
- bytemuck + 手動 SIMD
- チャンク比較
```
// 4バイト（RGBA1ピクセル）をu32として比較 → ループ回数が1/4
let prev_u32 = bytemuck::cast_slice::<u8, u32>(prev_pixels);
let next_u32 = bytemuck::cast_slice::<u8, u32>(next_pixels);

for (p, n) in prev_u32.iter().zip(next_u32.iter()) {
    // RGBマスク（Aを無視）
    if (p ^ n) & 0x00FFFFFF != 0 {
        // 変化あり
    }
}
```

### クレートを採用する
- rayon による並列差分判定
- memmap2 によるメモリマップ I/O
- zune-image / image の代替デコーダ

## 一瞬硬直する
同期的に処理しているため、処理するときに硬直が見られる

### 対策：yaziのように、画像を非同期で処理する
- ロード中という表記があれば嬉しいかも
- Windowsのフォトみたいに荒い画像から綺麗な画像にする
