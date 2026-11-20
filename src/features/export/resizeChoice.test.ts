import { describe, expect, it } from 'vitest';
import { DEFAULT_RESIZE_CHOICE, DEFAULT_SHRINK_CHOICE, SHRINK_OPTIONS, parseResizeChoice, toResize, type ResizeChoice } from './resizeChoice';

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

describe('ขนาดของไฟล์ต้นฉบับที่ย่อในที่เดิม', () => {
  const shrink = (patch: Partial<ResizeChoice>) => toResize({ ...DEFAULT_SHRINK_CHOICE, ...patch }, SHRINK_OPTIONS);

  it('ไม่มีตัวเลือก "ต้นฉบับ" และ 1080 px', () => {
    expect(shrink({ preset: 'original' })).toHaveProperty('error');
    expect(shrink({ preset: '1080' })).toHaveProperty('error');
    expect(shrink({})).toEqual({ resize: { mode: 'long_edge', px: 2048 } });
  });

  it('ไม่ต่ำกว่า 1280 px และ 300 KB เพื่อให้การจดจำใบหน้าแม่นเท่าเดิม', () => {
    expect(shrink({ preset: 'custom', customPx: '1280' })).toEqual({ resize: { mode: 'long_edge', px: 1280 } });
    expect(shrink({ preset: 'custom', customPx: '1279' })).toEqual({ error: 'ใส่ด้านยาวเป็นจำนวนเต็มระหว่าง 1,280 ถึง 10,000 px' });
    expect(shrink({ preset: 'max', maxValue: '300', maxUnit: 'KB' })).toEqual({ resize: { mode: 'max_bytes', bytes: 307_200 } });
    expect(shrink({ preset: 'max', maxValue: '200', maxUnit: 'KB' })).toEqual({ error: 'ใส่ขนาดไฟล์ระหว่าง 300 KB ถึง 100 MB' });
  });

  it('ค่าที่เก็บไว้ของงานส่งออกไม่ทำให้ตัวเลือกที่ใช้ไม่ได้หลุดเข้ามา', () => {
    expect(parseResizeChoice('{"preset":"original"}', DEFAULT_SHRINK_CHOICE, SHRINK_OPTIONS).preset).toBe('2048');
    expect(parseResizeChoice('{"preset":"max","maxValue":"500","maxUnit":"KB"}', DEFAULT_SHRINK_CHOICE, SHRINK_OPTIONS)).toEqual({
      ...DEFAULT_SHRINK_CHOICE,
      preset: 'max',
      maxValue: '500',
      maxUnit: 'KB',
    });
  });
});
