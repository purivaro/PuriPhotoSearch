import { useEffect, useState } from 'react';
import { Eraser, RotateCcw, SquareDashedMousePointer, TriangleAlert, Undo2 } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { InlineNotice, Spinner } from '@/components/ui/Feedback';
import { cn } from '@/lib/cn';
import { formatCount, newRequestId } from '@/lib/format';
import type { FaceBox, Removal } from '@/lib/ipc/types';
import { FaceAvatar } from '../people/FaceAvatar';
import { FillModelsInstall } from './FillModelsInstall';
import { faceRemoved, personRect } from './removals';
import type { RemovalsState } from './useRemovals';
import { bodyReady, type SubjectMapState } from './useSubjectMap';

/** ลบได้มากสุดต่อรูป (เท่ากับ remove::MAX_REMOVALS ใน Rust) */
export const MAX_REMOVALS = 20;

interface RemovePanelProps {
  removals: Removal[];
  onChange: (removals: Removal[]) => void;
  faces: FaceBox[];
  /** สถานะของตัวแยกตัวคน/ตัวเติมฉากหลัง (ดาวน์โหลด) */
  subject: SubjectMapState;
  state: RemovalsState;
  drawing: boolean;
  onDrawing: (drawing: boolean) => void;
  /** ชี้ที่รายการ: แสดงกรอบนั้นบนภาพ */
  onHover: (id: string | null) => void;
}

/**
 * ลบคนหรือสิ่งของ (DECISIONS ข้อ 42): ลากกรอบบนภาพ หรือเลือกจากใบหน้าที่แอปพบ → แอปหาคน/สิ่งของในกรอบแล้วเติมฉากหลังแทน
 * · เอาคืนได้ทีละส่วน · ไฟล์ต้นฉบับไม่ถูกแตะ
 */
export function RemovePanel({ removals, onChange, faces, subject, state, drawing, onDrawing, onHover }: RemovePanelProps) {
  const status = subject.status;
  const ready = bodyReady(status);
  const unsupported = !!status && (status.state === 'unsupported' || status.fill.state === 'unsupported');
  const full = removals.length >= MAX_REMOVALS;
  // กดลากกรอบระหว่างที่ยังไม่มีโมเดล: ถามดาวน์โหลดก่อน เสร็จแล้วเข้าโหมดลากกรอบให้เอง
  const [want, setWant] = useState(false);
  useEffect(() => {
    if (ready && want) {
      setWant(false);
      onDrawing(true);
    }
    // onDrawing เปลี่ยนทุกครั้งที่วาดใหม่ — รอเฉพาะตอนพร้อม
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, want]);
  const add = (rect: Removal['rect'], anchor: Removal['anchor']) => onChange([...removals, { id: newRequestId(), rect, anchor }]);
  const pickable = faces.filter((f) => !faceRemoved(f.sourceBbox, removals));

  return (
    <section className="space-y-3 border-t border-line pt-4" data-testid="remove-panel">
      <h3 className="flex items-center gap-1.5 text-sm font-semibold text-ink">
        <Eraser className="size-4 text-muted" aria-hidden /> ลบคนหรือสิ่งของ
        {removals.length > 0 && <span className="font-normal text-muted">· {formatCount(removals.length)} ส่วน</span>}
      </h3>
      <p className="text-[12.5px] text-muted">ลากกรอบรอบคนหรือสิ่งของที่ไม่ต้องการ แอปหาสิ่งนั้นในกรอบแล้วเติมฉากหลังแทน ไฟล์ต้นฉบับไม่ถูกแตะ</p>
      {unsupported ? (
        <p className="text-[12.5px] text-muted" data-testid="remove-unsupported">
          {status.state === 'unsupported' ? status.reason : status.fill.reason}
        </p>
      ) : (
        <>
          <Button
            size="sm"
            variant={drawing ? 'primary' : 'outline'}
            icon={<SquareDashedMousePointer className="size-4" aria-hidden />}
            disabled={!status || full}
            aria-pressed={drawing}
            onClick={() => (drawing ? onDrawing(false) : ready ? onDrawing(true) : setWant(true))}
            className="w-full"
            data-testid="remove-draw"
          >
            {drawing ? 'ลากกรอบบนภาพ… (กดอีกครั้งหรือ Esc เพื่อยกเลิก)' : full ? `ลบได้ไม่เกิน ${MAX_REMOVALS} ส่วนต่อรูป` : 'ลากกรอบรอบสิ่งที่จะลบ'}
          </Button>
          {want && !ready && (
            <FillModelsInstall
              subject={subject}
              onCancel={() => setWant(false)}
              title="ลบคน/สิ่งของต้องดาวน์โหลดก่อน (ครั้งเดียว)"
              after="เสร็จแล้วลากกรอบบนภาพได้เลย"
              testId="remove"
            />
          )}
          {ready && pickable.length > 0 && !full && (
            <div className="space-y-1.5">
              <p className="text-[12.5px] text-muted">หรือเลือกคนจากใบหน้าที่พบในรูป</p>
              <div className="flex flex-wrap gap-1.5">
                {pickable.map((f) => (
                  <button
                    key={f.id}
                    type="button"
                    onClick={() => add(personRect(f.sourceBbox), [f.sourceBbox.x + f.sourceBbox.w / 2, f.sourceBbox.y + f.sourceBbox.h / 2])}
                    title={`ลบ${f.person ? ` ${f.person.displayName}` : 'คนนี้'}ออกจากรูป`}
                    aria-label={`ลบ${f.person ? ` ${f.person.displayName}` : 'คนนี้'}ออกจากรูป`}
                    className="group relative rounded-full ring-2 ring-transparent transition hover:ring-danger focus-visible:ring-danger"
                    data-testid="remove-face"
                  >
                    <FaceAvatar asset={`face/${f.id}`} name={f.person?.displayName ?? ''} decorative className="size-10" />
                    <span className="absolute -right-0.5 -bottom-0.5 flex size-4 items-center justify-center rounded-full bg-danger text-white opacity-0 transition group-hover:opacity-100 group-focus-visible:opacity-100">
                      <Eraser className="size-2.5" aria-hidden />
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}
        </>
      )}
      {removals.length > 0 && (
        <>
          {state.phase === 'preparing' && (
            <InlineNotice tone="info" icon={<Spinner className="size-4" />}>
              <span data-testid="remove-preparing">กำลังลบ… ครั้งแรกของแต่ละส่วนราว 8 วินาที</span>
            </InlineNotice>
          )}
          {state.phase === 'failed' && (
            <InlineNotice
              tone="danger"
              icon={<TriangleAlert aria-hidden />}
              action={
                <Button size="sm" icon={<RotateCcw className="size-4" aria-hidden />} onClick={state.retry}>
                  ลองใหม่
                </Button>
              }
            >
              <span data-testid="remove-failed">{state.error}</span>
            </InlineNotice>
          )}
          {state.phase === 'needs_model' && !want && (
            <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
              <span data-testid="remove-needs-model">รูปนี้มีส่วนที่ลบ ต้องดาวน์โหลดตัวแยกตัวคนและตัวเติมฉากหลังก่อนจึงจะเห็นผล</span>
            </InlineNotice>
          )}
          <ul className="space-y-1" data-testid="remove-list">
            {removals.map((r, i) => (
              <li
                key={r.id}
                onPointerEnter={() => onHover(r.id)}
                onPointerLeave={() => onHover(null)}
                className="flex items-center gap-2 rounded-lg border border-line bg-surface px-2.5 py-1.5"
                data-testid="remove-item"
              >
                <span className="flex size-5 shrink-0 items-center justify-center rounded-full bg-sunken text-[11.5px] font-bold text-ink tabular-nums">{i + 1}</span>
                <span className={cn('min-w-0 flex-1 truncate text-[12.5px]', state.found[i] === false ? 'text-warn' : 'text-muted')}>
                  {state.found[i] === false ? 'ไม่พบคน/สิ่งของที่เด่นในกรอบ — ลบทั้งกรอบ' : 'คน/สิ่งของในกรอบ'}
                </span>
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Undo2 className="size-4" aria-hidden />}
                  onClick={() => {
                    onHover(null);
                    onChange(removals.filter((x) => x.id !== r.id));
                  }}
                  data-testid="remove-undo"
                >
                  เอาคืน
                </Button>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
