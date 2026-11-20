import { create } from 'zustand';
import type { EditSettings } from '@/lib/ipc/types';

/** การแต่งที่คัดลอกไว้ (อยู่จนปิดแอป) — วางให้รูปอื่นได้แบบ Lightroom */
interface EditClipboard {
  copied: { settings: EditSettings; filename: string } | null;
  copy: (settings: EditSettings, filename: string) => void;
}

export const useEditClipboard = create<EditClipboard>((set) => ({
  copied: null,
  copy: (settings, filename) => set({ copied: { settings, filename } }),
}));
