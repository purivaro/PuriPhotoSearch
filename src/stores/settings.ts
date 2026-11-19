import { create } from 'zustand';

export type ThemeChoice = 'light' | 'dark' | 'system';

const THEME_KEY = 'puriphoto.theme';
const SIDEBAR_KEY = 'puriphoto.sidebarCollapsed';
const AUTO_UPDATE_KEY = 'puriphoto.autoUpdate';

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
}

/** ค่าตั้งของหน้าจอที่ไม่ใช่ข้อมูลคลัง (ธีม, การยุบ sidebar, อัปเดตแอปเอง) เก็บในเครื่องผ่าน localStorage */
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
}));
