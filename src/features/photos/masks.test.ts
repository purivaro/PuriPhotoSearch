import { describe, expect, it } from 'vitest';
import type { EditMask } from '@/lib/ipc/types';
import { NEUTRAL } from './adjust';
import { FULL, IDENTITY } from './editGeometry';
import { applyMasks, skinWeight } from './masks';

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

// ค่าเดียวกับ masks::MASK_SAMPLES / MASK_EXPECTED ใน Rust
const SAMPLES: [number, number][] = [
  [5, 5],
  [20, 14],
  [24, 16],
  [30, 20],
  [44, 28],
];
const EXPECTED: [boolean, number[][]][] = [
  [true, [[70, 192, 60], [124, 60, 144], [94, 78, 142], [160, 75, 160], [166, 60, 234]]],
  [false, [[70, 192, 60], [151, 75, 175], [116, 97, 174], [190, 91, 190], [166, 60, 234]]],
];

const faceMask = (skin: boolean): EditMask => ({
  id: 'm1',
  area: { kind: 'faces', boxes: [[0.3, 0.2, 0.35, 0.5]], skin },
  adjust: { ...NEUTRAL, exposure: 0.6, texture: -40 },
  amount: 100,
  invert: false,
});

const canvas = { width: 48, height: 32, view: FULL, sourceW: 48, sourceH: 32, settings: IDENTITY };

describe('ปรับเฉพาะส่วน (DECISIONS ข้อ 28)', () => {
  it('ได้ค่าเดียวกับ Rust ทุกค่า', () => {
    for (const [skin, expected] of EXPECTED) {
      const px = testImage(48, 32);
      applyMasks(px, canvas, [faceMask(skin)], 48);
      SAMPLES.forEach(([x, y], k) => {
        const i = (y * 48 + x) * 4;
        expect([px[i], px[i + 1], px[i + 2]], `skin ${skin} @${x},${y}`).toEqual(expected[k]);
      });
    }
  });

  it('สีผิวเป็นผิว สีจีวรและสีเทาไม่ใช่', () => {
    expect(skinWeight(210, 160, 130)).toBeGreaterThan(0.95);
    expect(skinWeight(240, 150, 30)).toBeLessThan(0.01);
    expect(skinWeight(128, 128, 128)).toBeLessThan(0.01);
    expect(skinWeight(220, 220, 225)).toBeLessThan(0.01);
  });

  it('ฉากหลังสีทองข้างใบหน้าไม่ถูกนับเป็นผิว (เทียบกับสีผิวของคนนั้น)', () => {
    const [w, h] = [80, 80];
    const px = new Uint8ClampedArray(w * h * 4);
    for (let y = 0; y < h; y += 1) {
      for (let x = 0; x < w; x += 1) {
        const i = (y * w + x) * 4;
        const rgb = Math.hypot(x - 40, y - 40) < 11 ? [200, 150, 120] : [220, 170, 100];
        px.set([...rgb, 255], i);
      }
    }
    const mask: EditMask = { ...faceMask(true), area: { kind: 'faces', boxes: [[0.36, 0.34, 0.28, 0.3]], skin: true }, adjust: { ...NEUTRAL, exposure: 1 } };
    applyMasks(px, { width: w, height: h, view: FULL, sourceW: w, sourceH: h, settings: IDENTITY }, [mask], 80);
    expect(px[(40 * w + 40) * 4 + 1]).toBeGreaterThan(175);
    expect(Array.from(px.slice((56 * w + 40) * 4, (56 * w + 40) * 4 + 3))).toEqual([220, 170, 100]);
  });

  it('แสดงบริเวณเป็นสีแดงทับ แม้ยังไม่ได้ปรับค่า · ความแรง 0 ไม่มีผล', () => {
    const plain = { ...faceMask(false), adjust: NEUTRAL };
    const px = testImage(48, 32);
    applyMasks(px, canvas, [plain], 48, 'm1');
    const center = (16 * 48 + 22) * 4;
    expect(px[center]).toBeGreaterThan(testImage(48, 32)[center]!);
    const untouched = testImage(48, 32);
    applyMasks(untouched, canvas, [{ ...faceMask(false), amount: 0 }], 48);
    expect(untouched).toEqual(testImage(48, 32));
  });

  it('Vignette ในค่าของบริเวณไม่มีผล (เป็นของกรอบทั้งภาพ) — เหมือน masks.rs', () => {
    const run = (vignette: boolean) => {
      const px = testImage(48, 32);
      const mask = faceMask(false);
      const adjust = vignette ? { ...mask.adjust, vignette: { amount: -100, midpoint: 50, feather: 50, roundness: 0 } } : mask.adjust;
      applyMasks(px, canvas, [{ ...mask, adjust }], 48);
      return px;
    };
    expect(run(true)).toEqual(run(false));
  });
});
