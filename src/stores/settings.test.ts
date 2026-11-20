import { describe, expect, it } from 'vitest';
import { DEFAULT_THUMB_SIZE, THUMB_SIZES, useSettings } from './settings';

describe('ขนาดรูปในกริด', () => {
  it('เริ่มที่ขนาดกลาง และไม่เกินระดับที่มี', () => {
    expect(useSettings.getState().thumbSize).toBe(DEFAULT_THUMB_SIZE);
    useSettings.getState().setThumbSize(-3);
    expect(useSettings.getState().thumbSize).toBe(0);
    useSettings.getState().setThumbSize(99);
    expect(useSettings.getState().thumbSize).toBe(THUMB_SIZES.length - 1);
    useSettings.getState().setThumbSize(2.6);
    expect(useSettings.getState().thumbSize).toBe(3);
  });
});
