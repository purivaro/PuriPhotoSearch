import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import type { PersonChoice } from '@/features/people/PersonPicker';
import { errorMessage, newRequestId } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { FaceBox, Id } from '@/lib/ipc/types';
import { invalidatePeopleData, qk } from '@/lib/queries';

function choiceArgs(choice: PersonChoice): { personId?: Id; newPersonName?: string } {
  return choice.kind === 'existing' ? { personId: choice.person.id } : { newPersonName: choice.name };
}

function choiceName(choice: PersonChoice): string {
  return choice.kind === 'existing' ? choice.person.displayName : choice.name;
}

/** คำสั่งแก้ตัวตนระดับใบหน้าในรูปหนึ่งใบ: แสดงผลสำเร็จหลัง backend ยืนยันเท่านั้น */
export function useFaceMutations(photoId: Id) {
  const qc = useQueryClient();
  const refresh = () => {
    void qc.invalidateQueries({ queryKey: qk.photo(photoId) });
    invalidatePeopleData(qc);
  };
  const onError = (error: unknown) => toast.error(errorMessage(error));

  const createManual = useMutation({
    mutationFn: (args: { requestId: Id; bbox: FaceBox['bbox']; choice: PersonChoice }) =>
      call('create_manual_face', { requestId: args.requestId, photoId, bbox: args.bbox, ...choiceArgs(args.choice) }),
    onSuccess: (_result, args) => {
      refresh();
      toast.success(`บันทึกใบหน้าของ ${choiceName(args.choice)} แล้ว ค้นรูปนี้จากชื่อได้ทันที`);
    },
    onError,
  });

  const assign = useMutation({
    mutationFn: (args: { faceId: Id; choice: PersonChoice }) =>
      call('assign_faces', { requestId: newRequestId(), faceIds: [args.faceId], ...choiceArgs(args.choice) }),
    onSuccess: (_result, args) => {
      refresh();
      toast.success(`กำหนดใบหน้านี้เป็น ${choiceName(args.choice)} แล้ว`);
    },
    onError,
  });

  /** "ใช่" ต่อข้อเสนอของ AI: กลายเป็นคำยืนยันของผู้ใช้เฉพาะใบหน้านี้ */
  const confirmSuggestion = useMutation({
    mutationFn: (face: FaceBox) => {
      if (!face.suggestion) throw new Error('ใบหน้านี้ไม่มีข้อเสนอให้ยืนยัน');
      return call('assign_faces', { requestId: newRequestId(), faceIds: [face.id], personId: face.suggestion.person.id });
    },
    onSuccess: (_result, face) => {
      refresh();
      toast.success(`ยืนยันว่าใบหน้านี้คือ ${face.suggestion?.person.displayName ?? 'บุคคลที่เสนอ'} แล้ว`);
    },
    onError,
  });

  /** "ไม่ใช่" ต่อข้อเสนอของ AI: จำไว้ไม่เสนอคู่นี้ซ้ำ ใบหน้ายังอยู่ให้ตั้งชื่อเป็นคนอื่น */
  const rejectSuggestion = useMutation({
    mutationFn: (face: FaceBox) => {
      if (!face.suggestion) throw new Error('ใบหน้านี้ไม่มีข้อเสนอให้ปฏิเสธ');
      return call('reject_suggestions', { faceIds: [face.id], personId: face.suggestion.person.id });
    },
    onSuccess: (_result, face) => {
      refresh();
      toast.success(`จะไม่เสนอ ${face.suggestion?.person.displayName ?? 'บุคคลนี้'} ให้ใบหน้านี้อีก ตั้งชื่อเป็นคนอื่นได้`);
    },
    onError,
  });

  const unassign = useMutation({
    mutationFn: (face: FaceBox) => call('unassign_faces', { faceIds: [face.id] }),
    onSuccess: (_result, face) => {
      refresh();
      const previous = face.person;
      toast.success(previous ? `เอา ${previous.displayName} ออกจากใบหน้านี้แล้ว กรอบใบหน้ายังอยู่` : 'เอาชื่อออกจากใบหน้านี้แล้ว', {
        // ย้อนกลับได้จริงด้วยการกำหนดบุคคลเดิมกลับเข้าไป
        action: previous
          ? {
              label: 'เลิกทำ',
              onClick: () => {
                void call('assign_faces', { requestId: newRequestId(), faceIds: [face.id], personId: previous.id })
                  .then(() => {
                    refresh();
                    toast.success(`คืน ${previous.displayName} ให้ใบหน้านี้แล้ว`);
                  })
                  .catch(onError);
              },
            }
          : undefined,
      });
    },
    onError,
  });

  const ignore = useMutation({
    mutationFn: (face: FaceBox) => call('ignore_faces', { faceIds: [face.id] }),
    onSuccess: () => {
      refresh();
      toast.success('เอากรอบนี้ออกแล้ว');
    },
    onError,
  });

  const setPersonCover = useMutation({
    mutationFn: (args: { personId: Id; faceId: Id }) => call('set_person_cover', args),
    onSuccess: (person) => {
      refresh();
      toast.success(`ใช้ใบหน้านี้เป็นภาพตัวแทนของ ${person.displayName} แล้ว`);
    },
    onError,
  });

  return { createManual, assign, confirmSuggestion, rejectSuggestion, unassign, ignore, setPersonCover };
}
