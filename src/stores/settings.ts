import { create } from 'zustand';
import { DEFAULT_SHRINK_CHOICE, SHRINK_OPTIONS, parseResizeChoice, type ResizeChoice } from '@/features/export/resizeChoice';
import { parseWatermarkChoice, type WatermarkChoice } from '@/features/export/watermarkChoice';

export type ThemeChoice = 'light' | 'dark' | 'system';

const THEME_KEY = 'puriphoto.theme';
const SIDEBAR_KEY = 'puriphoto.sidebarCollapsed';
const AUTO_UPDATE_KEY = 'puriphoto.autoUpdate';
const THUMB_SIZE_KEY = 'puriphoto.thumbSize';
const EXPORT_RESIZE_KEY = 'puriphoto.exportResize';
const SHRINK_RESIZE_KEY = 'puriphoto.shrinkResize';
const EXPORT_WATERMARK_KEY = 'puriphoto.exportWatermark';

/** ความกว้างขั้นต่ำของรูปในกริดแต่ละระดับ (px) · ภาพย่อมีขนาด 512 px จึงไม่ขยายเกินนี้มากเพื่อไม่ให้ภาพแตก */
export const THUMB_SIZES = [120, 160, 212, 280, 360] as const;
export const DEFAULT_THUMB_SIZE = 2;

function storedThumbSize(): number {
  const value = Number(read(THUMB_SIZE_KEY));
  return read(THUMB_SIZE_KEY) !== null && Number.isInteger(value) && value >= 0 && value < THUMB_SIZES.length ? value : DEFAULT_THUMB_SIZE;
}

const media = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null;

export function resolveTheme(choice: ThemeChoice): 'light' | 'dark' {
  return choice === 'system' ? (media?.matches ? 'dark' : 'light') : choice;
}

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null; // โหมดส่วนตัว: ใช้ค่าเริ่มต้น
  }
}

function write(key: string, value: string | null) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    /* ค่านี้เป็นเพียงความสะดวกของ UI */
  }
}

function applyTheme(choice: ThemeChoice) {
  document.documentElement.dataset.theme = resolveTheme(choice);
}

function storedTheme(): ThemeChoice {
  const value = read(THEME_KEY);
  return value === 'light' || value === 'dark' ? value : 'system';
}

/** เรียกก่อน render ครั้งแรก เพื่อให้สีแรกที่เห็นตรงกับธีมที่เลือก */
export function bootstrapTheme() {
  applyTheme(storedTheme());
  media?.addEventListener('change', () => {
    if (useSettings.getState().theme === 'system') applyTheme('system');
  });
}

interface SettingsState {
  theme: ThemeChoice;
  /** null = ตามความกว้างหน้าต่าง, true/false = ผู้ใช้เลือกเอง */
  sidebarCollapsed: boolean | null;
  /** ตรวจและดาวน์โหลดรุ่นใหม่ของแอปเอง (ค่าเริ่มต้น เปิด) */
  autoUpdate: boolean;
  setTheme: (theme: ThemeChoice) => void;
  setAutoUpdate: (autoUpdate: boolean) => void;
  setSidebarCollapsed: (collapsed: boolean | null) => void;
  /** ระดับขนาดรูปในกริด เป็นตำแหน่งใน THUMB_SIZES */
  thumbSize: number;
  setThumbSize: (level: number) => void;
  /** ขนาดของสำเนาที่ส่งออกครั้งล่าสุด ใช้เป็นค่าตั้งต้นของครั้งถัดไป */
  exportResize: ResizeChoice;
  setExportResize: (choice: ResizeChoice) => void;
  /** ขนาดที่เลือกครั้งล่าสุดในหน้าต่าง "ย่อไฟล์ต้นฉบับ" */
  shrinkResize: ResizeChoice;
  setShrinkResize: (choice: ResizeChoice) => void;
  /** ลายน้ำที่ตั้งไว้ครั้งล่าสุดในหน้าต่างส่งออก */
  exportWatermark: WatermarkChoice;
  setExportWatermark: (choice: WatermarkChoice) => void;
}

/** ค่าตั้งของหน้าจอที่ไม่ใช่ข้อมูลคลัง (ธีม, การยุบ sidebar, อัปเดตแอปเอง, ขนาดรูปในกริด, ขนาดของสำเนาที่ส่งออก) เก็บในเครื่องผ่าน localStorage */
export const useSettings = create<SettingsState>((set) => ({
  theme: storedTheme(),
  sidebarCollapsed: read(SIDEBAR_KEY) === null ? null : read(SIDEBAR_KEY) === '1',
  setTheme: (theme) => {
    write(THEME_KEY, theme === 'system' ? null : theme);
    applyTheme(theme);
    set({ theme });
  },
  autoUpdate: read(AUTO_UPDATE_KEY) !== '0',
  setAutoUpdate: (autoUpdate) => {
    write(AUTO_UPDATE_KEY, autoUpdate ? null : '0');
    set({ autoUpdate });
  },
  setSidebarCollapsed: (sidebarCollapsed) => {
    write(SIDEBAR_KEY, sidebarCollapsed === null ? null : sidebarCollapsed ? '1' : '0');
    set({ sidebarCollapsed });
  },
  thumbSize: storedThumbSize(),
  setThumbSize: (level) => {
    const thumbSize = Math.min(THUMB_SIZES.length - 1, Math.max(0, Math.round(level)));
    write(THUMB_SIZE_KEY, thumbSize === DEFAULT_THUMB_SIZE ? null : String(thumbSize));
    set({ thumbSize });
  },
  exportResize: parseResizeChoice(read(EXPORT_RESIZE_KEY)),
  setExportResize: (exportResize) => {
    write(EXPORT_RESIZE_KEY, JSON.stringify(exportResize));
    set({ exportResize });
  },
  shrinkResize: parseResizeChoice(read(SHRINK_RESIZE_KEY), DEFAULT_SHRINK_CHOICE, SHRINK_OPTIONS),
  setShrinkResize: (shrinkResize) => {
    write(SHRINK_RESIZE_KEY, JSON.stringify(shrinkResize));
    set({ shrinkResize });
  },
  exportWatermark: parseWatermarkChoice(read(EXPORT_WATERMARK_KEY)),
  setExportWatermark: (exportWatermark) => {
    write(EXPORT_WATERMARK_KEY, JSON.stringify(exportWatermark));
    set({ exportWatermark });
  },
}));
