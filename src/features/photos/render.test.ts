import { describe, expect, it } from 'vitest';
import { NEUTRAL } from './adjust';
import { IDENTITY } from './editGeometry';
import { applyLocal } from './local';
import { renderPixels } from './render';

/** ภาพเดียวกับ local::test_image ใน Rust (RGBA) */
function testImage(w: number, h: number): Uint8ClampedArray {
  const out = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y += 1) {
    for (let x = 0; x < w; x += 1) {
      const i = (y * w + x) * 4;
      out[i] = ((x * 37 + y * 11) % 200) + 30;
      out[i + 1] = ((Math.floor(x / 6) + Math.floor(y / 4)) % 2) * 120 + 60 + ((x * y) % 7) * 3;
      out[i + 2] = 40 + x * 4 + (y % 5) * 6;
      out[i + 3] = 255;
    }
  }
  return out;
}

describe('ขั้นพิกเซลของตัวอย่างสด (renderPixels)', () => {
  it('ภาพที่ตัด: รัศมีของ Texture/Clarity/Dehaze คิดจากทั้งกรอบ — เท่ากับ edit::render_with ของ Rust', () => {
    // แท็บแสงและสีแสดงครึ่งซ้ายของกรอบ (view = กรอบตัด) บน canvas 48×32 → ทั้งกรอบกว้าง 96
    const crop = { x: 0, y: 0, w: 0.5, h: 1 };
    const adjust = { ...NEUTRAL, texture: 80, clarity: 60, dehaze: 30 };
    const shown = testImage(48, 32);
    renderPixels(shown, 48, 32, { ...IDENTITY, crop, adjust }, 'display', { view: crop, sourceW: 96, sourceH: 32, whole: true });
    const want = testImage(48, 32);
    applyLocal(want, 48, 32, 4, 96, adjust);
    expect(shown).toEqual(want);
  });
});
