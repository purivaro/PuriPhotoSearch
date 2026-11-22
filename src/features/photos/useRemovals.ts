import { useQuery } from '@tanstack/react-query';
import { errorMessage } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { Removal } from '@/lib/ipc/types';
import { bodyReady, useSubjectStatus } from './useSubjectMap';

/**
 * ลบคน/สิ่งของ (DECISIONS ข้อ 42) ฝั่งหน้าแต่งภาพ: Rust หาสิ่งที่เด่นในแต่ละกรอบแล้วเติมฉากหลัง (ครั้งแรกราว 8 วินาทีต่อกรอบ) แล้วเก็บไว้
 * ตัวอย่างสดใช้ที่คิดแล้ว — คิดเสร็จ `token` เปลี่ยนให้วาดใหม่
 */
export interface RemovalsState {
  /** idle = ไม่มีส่วนที่ลบ · needs_model = ต้องดาวน์โหลดตัวแยกตัวคน/ตัวเติมฉากหลังก่อน */
  phase: 'idle' | 'needs_model' | 'preparing' | 'ready' | 'failed';
  /** ต่อกรอบ (ลำดับเดียวกับรายการ): พบคน/สิ่งที่เด่นในกรอบ — false = ลบทั้งกรอบ */
  found: boolean[];
  error: string | null;
  token: number;
  retry: () => void;
}

export function useRemovals(photoId: string, removals: Removal[], fromRaw: boolean): RemovalsState {
  const status = useSubjectStatus();
  const ready = bodyReady(status.data);
  // ภาพที่เติมขึ้นกับกรอบและจุดยึดเท่านั้น (รหัสของรายการไม่เกี่ยว)
  const parts = removals.map((r) => ({ rect: r.rect, anchor: r.anchor }));
  const query = useQuery({
    queryKey: ['removals', photoId, fromRaw, JSON.stringify(parts)],
    queryFn: () => call('prepare_removals', { photoId, fromRaw, removals }),
    enabled: parts.length > 0 && ready,
    staleTime: Infinity,
    retry: false,
    // ระหว่างเพิ่มกรอบใหม่ ผลของกรอบเดิมยังใช้แสดงได้
    placeholderData: (previous) => previous,
  });
  let phase: RemovalsState['phase'] = 'idle';
  if (parts.length > 0) {
    if (status.isPending) phase = 'preparing';
    else if (!ready) phase = 'needs_model';
    else if (query.isError) phase = 'failed';
    else if (query.data && !query.isPlaceholderData) phase = 'ready';
    else phase = 'preparing';
  }
  return {
    phase,
    found: query.isPlaceholderData ? [] : (query.data?.found ?? []),
    error: query.isError ? errorMessage(query.error) : null,
    token: query.isPlaceholderData ? 0 : query.dataUpdatedAt,
    retry: () => void query.refetch(),
  };
}
