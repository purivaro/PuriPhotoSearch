import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { errorMessage } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { ModelStatus, SubjectStatus } from '@/lib/ipc/types';

/**
 * Mask ตัวคน (DECISIONS ข้อ 40) และตัวผอม (ข้อ 41) ฝั่งหน้าแต่งภาพ: สถานะของตัวแยกตัวคน/ตัวเติมฉากหลัง + ภาพน้ำหนักของรูปที่เปิดอยู่
 * Rust คิดภาพน้ำหนักครั้งแรกราว 7 วินาที (ตัวผอมเติมฉากหลังอีกราว 2 วินาที) แล้วเก็บไว้ ตัวอย่างสดใช้ที่คิดแล้ว —
 * คิดเสร็จ `token` เปลี่ยนให้วาดใหม่
 */
export const SUBJECT_STATUS_KEY = ['subject-status'] as const;

export type SubjectPhase =
  /** รูปนี้ไม่มี Mask ตัวคนหรือตัวผอม */
  | 'idle'
  /** ต้องดาวน์โหลดก่อน (หรือเครื่องนี้ใช้ไม่ได้ — ดู status.state / status.fill.state) */
  | 'needs_model'
  | 'preparing'
  | 'ready'
  | 'failed';

export interface SubjectNeeds {
  /** มี Mask ตัวคน */
  mask: boolean;
  /** มีตัวผอม (ต้องใช้ตัวเติมฉากหลังด้วย) */
  body: boolean;
  /** แต่งจากภาพ RAW คู่ (ฉากหลังที่เติมต้องเป็นสีของภาพนั้น) */
  fromRaw: boolean;
  /** มี Mask ส่วนของคน (ต้องใช้ตัวแยกส่วนของคนด้วย) */
  parts?: boolean;
}

export interface SubjectMapState {
  status: SubjectStatus | undefined;
  phase: SubjectPhase;
  /** รูปนี้ใช้ตัวผอม (ข้อความและการดาวน์โหลดรวมตัวเติมฉากหลัง) */
  body: boolean;
  /** รูปนี้มี Mask ส่วนของคน (ข้อความและการดาวน์โหลดรวมตัวแยกส่วนของคน) */
  parts: boolean;
  /** พบตัวคนในรูป (null = ยังไม่รู้) */
  found: boolean | null;
  error: string | null;
  /** เปลี่ยนเมื่อภาพน้ำหนักพร้อม */
  token: number;
  /** ดาวน์โหลดสิ่งที่รูปนี้ต้องใช้ */
  install: () => void;
  /** ดาวน์โหลดตัวแยกตัวคน + ตัวเติมฉากหลัง (ก่อนตั้งตัวผอม) */
  installBody: () => void;
  /** ดาวน์โหลดตัวแยกตัวคน + ตัวแยกส่วนของคน (ก่อนเพิ่ม Mask ส่วนของคน) */
  installParts: () => void;
  installing: boolean;
  retry: () => void;
}

/** โมเดลที่ตัวผอมต้องใช้แต่ยังไม่มี (ตัวแยกตัวคน ตัวเติมฉากหลัง) */
export function missingForBody(status: SubjectStatus | undefined): ModelStatus[] {
  if (!status) return [];
  return [status, status.fill].filter((m) => m.state !== 'ready');
}

/** โมเดลที่รูปนี้ต้องใช้แต่ยังไม่มี (ตัวแยกตัวคนเสมอ + ตัวเติมฉากหลังถ้ามีตัวผอม + ตัวแยกส่วนของคนถ้ามี Mask ส่วนของคน) */
export function missingFor(status: SubjectStatus | undefined, needs: { body: boolean; parts: boolean }): ModelStatus[] {
  if (!status) return [];
  return [status, ...(needs.body ? [status.fill] : []), ...(needs.parts ? [status.parts] : [])].filter((m) => m.state !== 'ready');
}

/** โมเดลที่ Mask ส่วนของคนต้องใช้แต่ยังไม่มี (ตัวแยกตัวคน ตัวแยกส่วนของคน) */
export function missingForParts(status: SubjectStatus | undefined): ModelStatus[] {
  if (!status) return [];
  return [status, status.parts].filter((m) => m.state !== 'ready');
}

/** Mask ส่วนของคนใช้ได้บนเครื่องนี้ */
export const partsReady = (status: SubjectStatus | undefined) => status?.state === 'ready' && status.parts.state === 'ready';

/** กำลังดาวน์โหลดตัวใดตัวหนึ่งที่ Mask ส่วนของคนใช้ */
export const partsInstalling = (status: SubjectStatus | undefined) =>
  status?.state === 'installing' || status?.parts.state === 'installing';

/** กำลังดาวน์โหลดตัวใดตัวหนึ่งที่ตัวผอมใช้ */
export const bodyInstalling = (status: SubjectStatus | undefined) =>
  status?.state === 'installing' || status?.fill.state === 'installing';

/** ตัวผอมใช้ได้บนเครื่องนี้ */
export const bodyReady = (status: SubjectStatus | undefined) => status?.state === 'ready' && status.fill.state === 'ready';

export function useSubjectStatus() {
  return useQuery({
    queryKey: SUBJECT_STATUS_KEY,
    queryFn: () => call('get_subject_status', {}),
    // ระหว่างดาวน์โหลดถามซ้ำจนเสร็จ
    refetchInterval: (q) =>
      q.state.data?.state === 'installing' || q.state.data?.fill.state === 'installing' || q.state.data?.parts.state === 'installing' ? 1500 : false,
  });
}

export function useSubjectMap(photoId: string, needs: SubjectNeeds): SubjectMapState {
  const qc = useQueryClient();
  const status = useSubjectStatus();
  const parts = needs.parts ?? false;
  const needed = needs.mask || needs.body || parts;
  const ready = (needs.body ? bodyReady(status.data) : status.data?.state === 'ready') && (!parts || partsReady(status.data));
  const map = useQuery({
    queryKey: ['subject-map', photoId, needs.body ? (needs.fromRaw ? 'fill-raw' : 'fill') : 'map', parts ? 'parts' : ''],
    queryFn: () => call('prepare_subject', { photoId, fill: needs.body, fromRaw: needs.fromRaw, parts }),
    enabled: needed && ready,
    staleTime: Infinity,
    retry: false,
  });
  const install = useMutation({
    mutationFn: (what: { fill?: boolean; parts?: boolean }) => call('install_subject_model', what),
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
    body: needs.body,
    parts,
    found: map.data?.found ?? null,
    error: map.isError ? errorMessage(map.error) : null,
    token: map.dataUpdatedAt,
    install: () => install.mutate({ fill: needs.body, parts }),
    installBody: () => install.mutate({ fill: true }),
    installParts: () => install.mutate({ parts: true }),
    installing:
      install.isPending ||
      status.data?.state === 'installing' ||
      (needs.body && status.data?.fill.state === 'installing') ||
      (parts && status.data?.parts.state === 'installing'),
    retry: () => void map.refetch(),
  };
}
