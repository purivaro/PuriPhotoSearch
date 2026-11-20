import { describe, expect, it } from 'vitest';
import { DEFAULT_RESIZE_CHOICE, parseResizeChoice, toResize, type ResizeChoice } from './resizeChoice';

const choice = (patch: Partial<ResizeChoice>): ResizeChoice => ({ ...DEFAULT_RESIZE_CHOICE, ...patch });

describe('ขนาดของสำเนาที่ส่งออก', () => {
  it('แปลงตัวเลือกสำเร็จรูปเป็นคำสั่งย่อ', () => {
    expect(toResize(choice({ preset: 'original' }))).toEqual({ resize: { mode: 'original' } });
    expect(toResize(choice({ preset: '1600' }))).toEqual({ resize: { mode: 'long_edge', px: 1600 } });
  });

  it('ด้านยาวที่กำหนดเองต้องเป็นจำนวนเต็มในช่วง', () => {
    expect(toResize(choice({ preset: 'custom', customPx: ' 1280 ' }))).toEqual({ resize: { mode: 'long_edge', px: 1280 } });
    for (const bad of ['', '200', '12.5', 'abc', '20000']) {
      expect(toResize(choice({ preset: 'custom', customPx: bad }))).toHaveProperty('error');
    }
  });

  it('ขนาดไฟล์คิดแบบฐาน 1024 และรับทศนิยมแบบจุลภาค', () => {
    expect(toResize(choice({ preset: 'max', maxValue: '1.5', maxUnit: 'MB' }))).toEqual({ resize: { mode: 'max_bytes', bytes: 1_572_864 } });
    expect(toResize(choice({ preset: 'max', maxValue: '0,5', maxUnit: 'MB' }))).toEqual({ resize: { mode: 'max_bytes', bytes: 524_288 } });
    expect(toResize(choice({ preset: 'max', maxValue: '500', maxUnit: 'KB' }))).toEqual({ resize: { mode: 'max_bytes', bytes: 512_000 } });
    for (const bad of ['', '10', '0']) {
      expect(toResize(choice({ preset: 'max', maxValue: bad, maxUnit: 'KB' }))).toHaveProperty('error');
    }
    expect(toResize(choice({ preset: 'max', maxValue: '200', maxUnit: 'MB' }))).toHaveProperty('error');
  });

  it('ค่าที่เก็บไว้เสียหายใช้ค่าเริ่มต้น', () => {
    expect(parseResizeChoice(null)).toEqual(DEFAULT_RESIZE_CHOICE);
    expect(parseResizeChoice('{oops')).toEqual(DEFAULT_RESIZE_CHOICE);
    expect(parseResizeChoice('{"preset":"nope","maxUnit":"KB"}')).toEqual({ ...DEFAULT_RESIZE_CHOICE, maxUnit: 'KB' });
    expect(parseResizeChoice('{"preset":"1080"}').preset).toBe('1080');
  });
});
