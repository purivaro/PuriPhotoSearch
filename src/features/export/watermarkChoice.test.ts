import { describe, expect, it } from 'vitest';
import type { WatermarkInfo } from '@/lib/ipc/types';
import { DEFAULT_WATERMARK_CHOICE, parseWatermarkChoice, toWatermarkSpec, watermarkRect } from './watermarkChoice';

describe('ลายน้ำ', () => {
  it('ตำแหน่งตรงกับสูตรของ Rust (media::watermark_rect)', () => {
    // ค่าเดียวกับเทสต์ watermark_lands_where_the_preview_shows_it_… ฝั่ง Rust
    expect(watermarkRect(1000, 800, 200, 100, 0.25, 'bottom_right')).toEqual({ x: 776, y: 676, width: 200, height: 100 });
    expect(watermarkRect(1000, 800, 200, 100, 0.25, 'top_left')).toEqual({ x: 24, y: 24, width: 200, height: 100 });
    expect(watermarkRect(1000, 800, 200, 100, 0.25, 'center')).toEqual({ x: 400, y: 350, width: 200, height: 100 });
    expect(watermarkRect(1000, 800, 4000, 1000, 1, 'bottom_right')).toEqual({ x: 176, y: 576, width: 800, height: 200 });
  });

  it('เปิดไว้แต่ยังไม่ได้เลือกโลโก้: บอกให้เลือก ไม่ส่งออกแบบไม่มีลายน้ำเงียบ ๆ', () => {
    const logo: WatermarkInfo = { id: 'a', name: 'โลโก้', width: 400, height: 200, transparent: true };
    const on = { ...DEFAULT_WATERMARK_CHOICE, enabled: true };
    expect(toWatermarkSpec(DEFAULT_WATERMARK_CHOICE, [])).toEqual({ spec: null });
    expect(toWatermarkSpec(on, [])).toHaveProperty('error');
    expect(toWatermarkSpec({ ...on, id: 'ลบไปแล้ว' }, [logo])).toHaveProperty('error');
    expect(toWatermarkSpec({ ...on, id: 'a' }, [logo])).toEqual({ spec: { id: 'a', anchor: 'bottom_right', scale: 25, opacity: 80 } });
  });

  it('ค่าที่เก็บไว้เสียหายใช้ค่าเริ่มต้น และหนีบให้อยู่ในช่วง', () => {
    expect(parseWatermarkChoice(null)).toEqual(DEFAULT_WATERMARK_CHOICE);
    expect(parseWatermarkChoice('{oops')).toEqual(DEFAULT_WATERMARK_CHOICE);
    expect(parseWatermarkChoice('{"enabled":true,"id":"a","anchor":"nowhere","scale":500,"opacity":1}')).toEqual({
      enabled: true,
      id: 'a',
      anchor: 'bottom_right',
      scale: 100,
      opacity: 5,
    });
  });
});
