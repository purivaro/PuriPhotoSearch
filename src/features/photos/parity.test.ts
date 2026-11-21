import { readFileSync, writeFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import type { EditSettings } from '@/lib/ipc/types';
import type { Profile } from './adjust';
import { renderPixels } from './render';

/**
 * ชุดสุ่มจาก `crates/core/tests/preview_parity.rs`: Rust เขียนภาพเข้า ค่าการแต่ง และผลของภาพที่บันทึก แล้วเรียกไฟล์นี้
 * (ผ่าน PURI_PARITY_CASES) ให้โค้ดตัวอย่างสดของหน้าแต่งภาพคิดชุดเดียวกันและเทียบทีละพิกเซล — รัน `pnpm test` เฉย ๆ จะข้าม
 */
interface ParityCase {
  id: number;
  w: number;
  h: number;
  sourceW: number;
  sourceH: number;
  profile: Profile;
  features: string[];
  settings: EditSettings;
  input: string;
  output: string;
}

const file = process.env.PURI_PARITY_CASES;

describe.skipIf(!file)('ตัวอย่างสด ↔ ภาพที่บันทึก (ชุดสุ่มจาก Rust)', () => {
  it('ทุกชุดต่างไม่เกินค่าที่ยอมได้ทุกพิกเซล', () => {
    const { tolerance, cases } = JSON.parse(readFileSync(file!, 'utf8')) as { tolerance: number; cases: ParityCase[] };
    const failures: object[] = [];
    for (const c of cases) {
      const rgb = Buffer.from(c.input, 'hex');
      const want = Buffer.from(c.output, 'hex');
      const px = new Uint8ClampedArray(c.w * c.h * 4);
      for (let i = 0; i < c.w * c.h; i++) {
        px.set([rgb[i * 3]!, rgb[i * 3 + 1]!, rgb[i * 3 + 2]!, 255], i * 4);
      }
      // แท็บแสงและสี/เฉพาะส่วนของหน้าแต่งภาพ: canvas คือส่วนที่ตัดของกรอบ (view = กรอบตัด) ทั้งส่วน
      const view = c.settings.crop ?? { x: 0, y: 0, w: 1, h: 1 };
      renderPixels(px, c.w, c.h, c.settings, c.profile, { view, sourceW: c.sourceW, sourceH: c.sourceH, whole: true });
      let max = 0;
      let over = 0;
      let first: object | null = null;
      for (let i = 0; i < c.w * c.h; i++) {
        for (let k = 0; k < 3; k++) {
          const d = Math.abs(px[i * 4 + k]! - want[i * 3 + k]!);
          max = Math.max(max, d);
          if (d > tolerance) {
            over += 1;
            first ??= { x: i % c.w, y: Math.floor(i / c.w), input: [...rgb.subarray(i * 3, i * 3 + 3)], rust: [...want.subarray(i * 3, i * 3 + 3)], ts: [px[i * 4], px[i * 4 + 1], px[i * 4 + 2]] };
          }
        }
        if (px[i * 4 + 3] !== 255) over += 1;
      }
      if (over > 0) failures.push({ id: c.id, size: `${c.w}×${c.h}`, features: c.features.join(' '), max, over, first });
    }
    if (failures.length > 0 && process.env.PURI_PARITY_REPORT) {
      writeFileSync(process.env.PURI_PARITY_REPORT, failures.map((f) => JSON.stringify(f)).join('\n'));
    }
    expect(failures.length, `${failures.length}/${cases.length} ชุดไม่ตรง (ดูรายงาน)`).toBe(0);
  });
});
