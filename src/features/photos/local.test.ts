import { describe, expect, it } from 'vitest';
import { NEUTRAL } from './adjust';
import { applyLocal, hasLocal } from './local';

/** ภาพเดียวกับ local::test_image ใน Rust (RGBA เหมือน canvas) */
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

// ค่าเดียวกับ local::LOCAL_SAMPLES / LOCAL_EXPECTED ใน Rust: [Texture, Clarity, Dehaze] → พิกเซลที่ตำแหน่งเหล่านี้
const SAMPLES: [number, number][] = [
  [0, 0],
  [5, 5],
  [20, 10],
  [40, 25],
  [47, 31],
];
const EXPECTED: [[number, number, number], number[][]][] = [
  [[-60, 0, 0], [[35, 65, 45], [68, 190, 58], [80, 192, 120], [184, 77, 199], [110, 63, 234]]],
  [[0, 50, 0], [[27, 57, 37], [75, 197, 65], [81, 193, 121], [185, 78, 200], [110, 63, 234]]],
  [[0, 0, 40], [[17, 50, 27], [58, 192, 47], [67, 192, 110], [183, 67, 198], [101, 52, 235]]],
  [[0, 0, -30], [[57, 80, 67], [91, 192, 84], [100, 192, 135], [189, 96, 203], [125, 83, 232]]],
  [[40, -30, 0], [[29, 59, 39], [68, 190, 58], [79, 191, 119], [186, 79, 201], [110, 63, 234]]],
];

describe('Texture / Clarity / Dehaze (DECISIONS ข้อ 27)', () => {
  it('ได้ค่าเดียวกับ Rust (ต่างได้ไม่เกิน 1 ระดับจากการปัดทศนิยม)', () => {
    for (const [[texture, clarity, dehaze], expected] of EXPECTED) {
      const px = testImage(48, 32);
      applyLocal(px, 48, 32, 4, 48, { ...NEUTRAL, texture, clarity, dehaze });
      SAMPLES.forEach(([x, y], k) => {
        const i = (y * 48 + x) * 4;
        for (let c = 0; c < 3; c += 1) expect(Math.abs(px[i + c]! - expected[k]![c]!), `${texture}/${clarity}/${dehaze} @${x},${y}`).toBeLessThanOrEqual(1);
        expect(px[i + 3]).toBe(255);
      });
    }
  });

  it('ค่าที่บันทึกก่อนมีขั้นนี้ (ไม่มีช่อง texture/clarity/dehaze) = ไม่ปรับ ไม่ทำภาพดำ', () => {
    const px = testImage(48, 32);
    const old = { ...NEUTRAL } as Partial<typeof NEUTRAL>;
    delete old.texture;
    delete old.clarity;
    delete old.dehaze;
    applyLocal(px, 48, 32, 4, 48, { ...(old as typeof NEUTRAL), texture: -60 });
    expect(px[(5 * 48 + 5) * 4]).toBe(68);
  });

  it('ไม่ปรับ = ภาพเดิม', () => {
    const px = testImage(48, 32);
    expect(hasLocal(NEUTRAL)).toBe(false);
    applyLocal(px, 48, 32, 4, 48, NEUTRAL);
    expect(px).toEqual(testImage(48, 32));
  });
});
