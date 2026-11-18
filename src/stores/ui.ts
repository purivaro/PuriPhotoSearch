import { create } from 'zustand';

interface UiState {
  paletteOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
}

/** สถานะชั่วคราวของหน้าจอที่หลายส่วนต้องสั่งได้ (เช่น เปิดช่องค้นหาจากทุกหน้า) */
export const useUi = create<UiState>((set) => ({
  paletteOpen: false,
  setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
}));
