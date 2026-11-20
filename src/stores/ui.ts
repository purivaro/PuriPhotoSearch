import { create } from 'zustand';

interface UiState {
  paletteOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
  /** หน้าต่าง "เตรียมแอปให้พร้อมใช้" (เปิดเองครั้งแรก และเปิดได้จากหน้าบุคคล) */
  setupOpen: boolean;
  setSetupOpen: (open: boolean) => void;
  /** แผงผู้ช่วย AI ด้านขวา */
  assistantOpen: boolean;
  setAssistantOpen: (open: boolean) => void;
}

/** สถานะชั่วคราวของหน้าจอที่หลายส่วนต้องสั่งได้ (เช่น เปิดช่องค้นหาจากทุกหน้า) */
export const useUi = create<UiState>((set) => ({
  paletteOpen: false,
  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
  setupOpen: false,
  setSetupOpen: (setupOpen) => set({ setupOpen }),
  assistantOpen: false,
  setAssistantOpen: (assistantOpen) => set({ assistantOpen }),
}));
