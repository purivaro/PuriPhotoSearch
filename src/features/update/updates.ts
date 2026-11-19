import { useEffect, useRef } from 'react';
import { toast } from 'sonner';
import { create } from 'zustand';
import { isTauri, onUpdateStatus, updateCommand } from '@/lib/ipc/client';
import type { UpdateStatus } from '@/lib/ipc/types';
import { useSettings } from '@/stores/settings';

/** รอให้หน้าแรกขึ้นก่อนค่อยถามเซิร์ฟเวอร์ แล้วถามซ้ำเป็นระยะสำหรับแอปที่เปิดค้างไว้หลายวัน */
const FIRST_CHECK_MS = 5_000;
const RECHECK_MS = 6 * 60 * 60 * 1000;

interface UpdateState {
  /** null = ยังไม่ได้ถามเปลือกแอป (หรือเปิดในเบราว์เซอร์ ซึ่งไม่มีตัวอัปเดต) */
  status: UpdateStatus | null;
  dialogOpen: boolean;
  setStatus: (status: UpdateStatus) => void;
  setDialogOpen: (open: boolean) => void;
}

export const useUpdate = create<UpdateState>((set) => ({
  status: null,
  dialogOpen: false,
  setStatus: (status) => set({ status }),
  setDialogOpen: (dialogOpen) => set({ dialogOpen }),
}));

/** ถามเซิร์ฟเวอร์ว่ามีรุ่นใหม่ไหม พบแล้วเปลือกแอปดาวน์โหลดต่อเองเบื้องหลัง */
export async function checkForUpdate(): Promise<void> {
  if (!isTauri) return;
  useUpdate.getState().setStatus(await updateCommand('check'));
}

/** ต่อสถานะตัวอัปเดตเข้ากับ UI, ตรวจรุ่นใหม่เองตามค่าตั้ง และบอกครั้งเดียวเมื่อรุ่นใหม่พร้อมติดตั้ง */
export function useUpdateRuntime() {
  const autoUpdate = useSettings((s) => s.autoUpdate);
  const readyVersion = useUpdate((s) => (s.status?.state === 'ready' ? s.status.version : null));
  const announced = useRef<string | null>(null);

  useEffect(() => {
    if (!isTauri) return;
    const { setStatus } = useUpdate.getState();
    let dispose: (() => void) | undefined;
    let cancelled = false;
    void onUpdateStatus(setStatus).then((off) => {
      if (cancelled) off();
      else dispose = off;
    });
    void updateCommand('status').then((status) => {
      // event ที่มาถึงก่อนคำตอบนี้ใหม่กว่า
      if (!cancelled && !useUpdate.getState().status) setStatus(status);
    });
    return () => {
      cancelled = true;
      dispose?.();
    };
  }, []);

  useEffect(() => {
    if (!isTauri || !autoUpdate) return;
    // ตรวจเองล้มเหลวไม่ต้องรบกวนผู้ใช้: สถานะอยู่ในหน้าตั้งค่า และรอบหน้าจะลองใหม่
    const check = () => void checkForUpdate().catch(() => {});
    const first = window.setTimeout(check, FIRST_CHECK_MS);
    const repeat = window.setInterval(check, RECHECK_MS);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(repeat);
    };
  }, [autoUpdate]);

  useEffect(() => {
    if (!readyVersion || announced.current === readyVersion) return;
    announced.current = readyVersion;
    toast(`รุ่น ${readyVersion} พร้อมติดตั้งแล้ว`, {
      id: 'update-ready',
      description: 'อัปเดตเมื่อสะดวก แอปจะปิดแล้วเปิดใหม่เอง',
      duration: 20_000,
      action: { label: 'อัปเดต', onClick: () => useUpdate.getState().setDialogOpen(true) },
    });
  }, [readyVersion]);
}
