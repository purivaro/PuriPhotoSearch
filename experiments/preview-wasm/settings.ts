import { NEUTRAL } from '../../src/features/photos/adjust.ts';
import { IDENTITY } from '../../src/features/photos/editGeometry.ts';

// งานเบา: แถบแสง/สีหลัก (แบบที่ผู้ใช้ลากบ่อยที่สุด) · งานหนัก: ทุกแถบ + Texture/Clarity/Dehaze + Mask ใบหน้า 3 หน้า
export const light = (exposure: number) => ({
  ...IDENTITY,
  adjust: { ...NEUTRAL, exposure, contrast: 15, highlights: -30, shadows: 25, whites: 10, blacks: -8, temperature: 6, tint: 3, vibrance: 15, saturation: 5 },
});
export const heavy = (exposure: number) => ({
  ...IDENTITY,
  adjust: {
    ...light(exposure).adjust,
    hue: [0, 3, -1, 7, 0, 5, 0, 0], sat: [12, -3, -13, -20, 15, -9, 0, 0], lum: [10, 30, 2, 11, 25, 0, 5, 6],
    parametric: [-5, 4, 12, 0],
    curve: [[0, 8], [64, 60], [190, 200], [255, 250]],
    levels: { inBlack: 4, inWhite: 250, gamma: 1.05, outBlack: 2, outWhite: 253 },
    grading: { shadows: { hue: 220, sat: 20, lum: -5 }, midtones: { hue: 35, sat: 10, lum: 0 }, highlights: { hue: 45, sat: 15, lum: 3 }, global: { hue: 0, sat: 0, lum: 0 }, blending: 60, balance: -10 },
    vignette: { amount: -20, midpoint: 50, feather: 60, roundness: 0 },
    texture: -20, clarity: 10, dehaze: 8,
  },
  masks: [{
    id: 'm1',
    area: { kind: 'faces', boxes: [[0.2907, 0.4097, 0.054, 0.0446], [0.4410, 0.5883, 0.0579, 0.0533], [0.8821, 0.1686, 0.1092, 0.0943]], skin: true },
    adjust: { ...NEUTRAL, exposure: 0.25, texture: -35, clarity: -10, saturation: -12, temperature: -6, tint: 4 },
    amount: 100,
    invert: false,
  }],
});
