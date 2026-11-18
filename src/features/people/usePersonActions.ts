import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { errorMessage } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { PersonSummary } from '@/lib/ipc/types';

/** ปักหมุด/เลิกปักหมุด: รอผลจาก backend ก่อนแสดงว่าสำเร็จ และใช้ preference ชุดเดียวทุกหน้า */
export function useTogglePin() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (person: PersonSummary) => call('set_person_pinned', { personId: person.id, pinned: !person.pinned }),
    onSuccess: (updated) => {
      void qc.invalidateQueries({ queryKey: ['people'] });
      toast.success(updated.pinned ? `ปักหมุด ${updated.displayName} แล้ว` : `เลิกปักหมุด ${updated.displayName} แล้ว`);
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

export function useSetArchived() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: { personId: string; archived: boolean }) => call('set_person_archived', args),
    onSuccess: (updated) => {
      void qc.invalidateQueries({ queryKey: ['people'] });
      void qc.invalidateQueries({ queryKey: ['summary'] });
      toast.success(updated.archived ? `เก็บ ${updated.displayName} ออกจากรายชื่อหลักแล้ว ข้อมูลในภาพยังอยู่` : `นำ ${updated.displayName} กลับมาในรายชื่อหลักแล้ว`);
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}
