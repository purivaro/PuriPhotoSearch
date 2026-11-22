import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { errorMessage } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { SubjectStatus } from '@/lib/ipc/types';

/**
 * Mask ตัวคน (DECISIONS ข้อ 40) ฝั่งหน้าแต่งภาพ: สถานะของตัวแยกตัวคน + ภาพน้ำหนักของรูปที่เปิดอยู่
 * Rust คิดภาพน้ำหนักครั้งแรกราว 7 วินาทีแล้วเก็บไว้ ตัวอย่างสดใช้ภาพที่คิดแล้ว — คิดเสร็จ `token` เปลี่ยนให้วาดใหม่
 */
export const SUBJECT_STATUS_KEY = ['subject-status'] as const;

export type SubjectPhase =
  /** รูปนี้ไม่มี Mask ตัวคน */
  | 'idle'
  /** ต้องดาวน์โหลดตัวแยกก่อน (หรือเครื่องนี้ใช้ไม่ได้ — ดู status.state) */
  | 'needs_model'
  | 'preparing'
  | 'ready'
  | 'failed';

export interface SubjectMapState {
  status: SubjectStatus | undefined;
  phase: SubjectPhase;
  /** พบตัวคนในรูป (null = ยังไม่รู้) */
  found: boolean | null;
  error: string | null;
  /** เปลี่ยนเมื่อภาพน้ำหนักพร้อม */
  token: number;
  install: () => void;
  installing: boolean;
  retry: () => void;
}

export function useSubjectStatus() {
  return useQuery({
    queryKey: SUBJECT_STATUS_KEY,
    queryFn: () => call('get_subject_status', {}),
    // ระหว่างดาวน์โหลดถามซ้ำจนเสร็จ
    refetchInterval: (q) => (q.state.data?.state === 'installing' ? 1500 : false),
  });
}

export function useSubjectMap(photoId: string, needed: boolean): SubjectMapState {
  const qc = useQueryClient();
  const status = useSubjectStatus();
  const ready = status.data?.state === 'ready';
  const map = useQuery({
    queryKey: ['subject-map', photoId],
    queryFn: () => call('prepare_subject', { photoId }),
    enabled: needed && ready,
    staleTime: Infinity,
    retry: false,
  });
  const install = useMutation({
    mutationFn: () => call('install_subject_model', {}),
    onSettled: () => void qc.invalidateQueries({ queryKey: SUBJECT_STATUS_KEY }),
    onError: (e) => toast.error(errorMessage(e)),
  });

  let phase: SubjectPhase = 'idle';
  if (needed) {
    if (status.isPending) phase = 'preparing';
    else if (!ready) phase = 'needs_model';
    else if (map.isError) phase = 'failed';
    else if (map.data) phase = 'ready';
    else phase = 'preparing';
  }
  return {
    status: status.data,
    phase,
    found: map.data?.found ?? null,
    error: map.isError ? errorMessage(map.error) : null,
    token: map.dataUpdatedAt,
    install: () => install.mutate(),
    installing: install.isPending || status.data?.state === 'installing',
    retry: () => void map.refetch(),
  };
}
