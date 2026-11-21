import { useEffect, useMemo, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Ban, Check, Flag, Sparkles, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { ConfirmDialog, Dialog } from '@/components/ui/Dialog';
import { ErrorState, InlineNotice, LoadingBlock, ProgressBar } from '@/components/ui/Feedback';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { CullDecision, CullPreview, Id } from '@/lib/ipc/types';
import { isJobActive, qk } from '@/lib/queries';

/** ระดับการคัด 1–5 (DECISIONS ข้อ 37) — ค่าตั้งต้น 2 เก็บใกล้กับที่หลวงพี่คัดเองที่สุด */
export const CULL_LEVELS = [
  { level: 1, label: 'เบามาก', hint: 'ไม่เอาเฉพาะรูปที่แทบเหมือนกันและรูปที่เบลอชัด ๆ' },
  { level: 2, label: 'เบา', hint: 'ช่วงที่กดถ่ายต่อกันเก็บใบที่ดีที่สุด' },
  { level: 3, label: 'ปานกลาง', hint: 'รวมรูปที่คล้ายกันในไม่กี่วินาทีเป็นช่วงเดียว' },
  { level: 4, label: 'เข้ม', hint: 'รูปฉากเดียวกันในช่วงนาทีเดียวกันเก็บใบเดียว' },
  { level: 5, label: 'เข้มมาก', hint: 'เก็บน้อยที่สุด เฉพาะรูปเด่นของแต่ละฉาก' },
] as const;
export const DEFAULT_CULL_LEVEL = 2;

const MAX_GROUPS = 24;

interface CullDialogProps {
  eventId: Id;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** หลังใช้ผล: ให้หน้ากิจกรรมกรองดูรูปที่เลือกไว้ได้ */
  onShowPicked?: () => void;
}

export function CullDialog({ eventId, open, onOpenChange, onShowPicked }: CullDialogProps) {
  const qc = useQueryClient();
  const [level, setLevel] = useState(DEFAULT_CULL_LEVEL);
  const [confirmClear, setConfirmClear] = useState(false);
  const status = useQuery({
    queryKey: ['cull-status', eventId],
    queryFn: () => call('get_cull_status', { eventId }),
    enabled: open,
    refetchInterval: (q) => (q.state.data?.job && isJobActive(q.state.data.job) ? 800 : false),
  });
  const measuring = Boolean(status.data?.job && isJobActive(status.data.job));

  // เปิดหน้าต่างนี้ = อยากให้คัด: สั่งดูรูปที่ยังไม่ได้ดู (ครั้งเดียวต่อการเปิด) · `run.at` = เวลาที่สั่ง — สถานะที่อ่านก่อนเวลานั้นยังไม่รู้ว่ามีงาน
  // จึงยังไม่แสดงผลการคัด (ไม่ให้ผลที่ยังวัดไม่ครบโผล่ขึ้นมาแวบหนึ่ง)
  const [run, setRun] = useState<{ at: number; job: boolean } | null>(null);
  const asked = useRef(false);
  const analyze = useMutation({
    mutationFn: () => call('start_cull_analysis', { eventId }),
    onSuccess: (job) => {
      setRun({ at: Date.now(), job: job !== null });
      void qc.invalidateQueries({ queryKey: ['cull-status', eventId] });
      void qc.invalidateQueries({ queryKey: qk.jobsActive });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  useEffect(() => {
    if (!open) {
      asked.current = false;
      setRun(null);
      return;
    }
    const s = status.data;
    if (s && !asked.current && !s.job && s.measured < s.total) {
      asked.current = true;
      analyze.mutate();
    }
  }, [open, status.data, analyze]);
  const settled = Boolean(
    status.data && (status.data.measured >= status.data.total || (run && (!run.job || status.dataUpdatedAt > run.at))),
  );
  const ready = Boolean(status.data && status.data.total > 0 && !measuring && !analyze.isPending && settled);

  const preview = useQuery({
    queryKey: ['cull-preview', eventId, level, status.data?.measured],
    queryFn: () => call('preview_cull', { eventId, level }),
    enabled: open && ready,
    placeholderData: (prev) => prev,
  });

  const apply = useMutation({
    mutationFn: () => call('apply_cull', { eventId, level }),
    onSuccess: (r) => {
      void qc.invalidateQueries({ queryKey: ['photos'] });
      void qc.invalidateQueries({ queryKey: ['photo'] });
      void qc.invalidateQueries({ queryKey: ['cull-status', eventId] });
      onOpenChange(false);
      toast.success(`AI คัดแล้ว: เลือกไว้ ${formatCount(r.picked)} รูป · ไม่เอา ${formatCount(r.rejected)} รูป`, {
        description: 'รูปที่ไม่เอายังอยู่ครบ ไฟล์ไม่ถูกลบ · กด P / X / U ในตัวดูภาพเพื่อเปลี่ยนเองได้',
        action: onShowPicked ? { label: 'ดูรูปที่เลือก', onClick: onShowPicked } : undefined,
      });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const clear = useMutation({
    mutationFn: () => call('clear_ai_picks', { eventId }),
    onSuccess: (r) => {
      void qc.invalidateQueries({ queryKey: ['photos'] });
      void qc.invalidateQueries({ queryKey: ['photo'] });
      void qc.invalidateQueries({ queryKey: ['cull-status', eventId] });
      setConfirmClear(false);
      toast.success(`ล้างการคัดของ AI แล้ว ${formatCount(r.cleared)} รูป — การคัดที่ทำเองอยู่ครบ`);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const s = status.data;
  const p = preview.data;
  return (
    <>
      <Dialog
        open={open}
        onOpenChange={onOpenChange}
        size="xl"
        title="AI ช่วยคัดภาพ"
        description="ดูความคมและหารูปที่ถ่ายซ้ำในงานนี้ แล้วเลือกรูปที่ดีที่สุดของแต่ละช่วงให้ — ทำในเครื่อง ไม่ส่งรูปออกไปไหน รูปที่คัดเองแล้วไม่ถูกแตะ และไม่มีไฟล์ถูกลบหรือย้าย"
        footer={
          <>
            {s && s.aiFlagged > 0 && (
              <Button
                variant="ghost"
                className="mr-auto"
                icon={<Trash2 className="size-4" aria-hidden />}
                onClick={() => setConfirmClear(true)}
                data-testid="cull-clear"
              >
                ล้างการคัดของ AI ({formatCount(s.aiFlagged)})
              </Button>
            )}
            <Button onClick={() => onOpenChange(false)}>{measuring ? 'ปิด (ทำต่อเบื้องหลัง)' : 'ยกเลิก'}</Button>
            <Button
              variant="primary"
              icon={<Sparkles className="size-4" aria-hidden />}
              disabled={!ready || !p || p.decisions.length === 0}
              busy={apply.isPending}
              onClick={() => apply.mutate()}
              data-testid="cull-apply"
            >
              ใช้ผลการคัดนี้
            </Button>
          </>
        }
      >
        {status.isError ? (
          <ErrorState title="อ่านสถานะของงานนี้ไม่ได้" error={status.error} onRetry={() => void status.refetch()} compact />
        ) : !s ? (
          <LoadingBlock label="กำลังเตรียม…" />
        ) : s.total === 0 ? (
          <InlineNotice tone="warn">ยังไม่มีรูปที่เปิดได้ในงานนี้ (ไฟล์อาจย้ายไปแล้ว) จึงยังคัดไม่ได้</InlineNotice>
        ) : measuring && s.job ? (
          <div className="flex flex-col gap-3 py-6" data-testid="cull-measuring">
            <p className="text-[15px] font-semibold text-ink">กำลังดูความคมและหารูปที่ถ่ายซ้ำ…</p>
            <ProgressBar
              value={s.job.counts.done + s.job.counts.failed}
              max={s.job.counts.total}
              label="ความคืบหน้าการดูรูป"
            />
            <p className="text-sm text-muted">
              ดูแล้ว {formatCount(s.job.counts.done + s.job.counts.failed)} จาก {formatCount(s.job.counts.total)} รูป · ปิดหน้าต่างนี้ได้ งานทำต่อเบื้องหลัง
              แล้วกลับมากด “AI ช่วยคัด” อีกครั้งเมื่อเสร็จ
            </p>
          </div>
        ) : !ready ? (
          <LoadingBlock label="กำลังเริ่มดูรูปของงานนี้…" />
        ) : !p ? (
          preview.isError ? (
            <ErrorState title="คิดผลการคัดไม่สำเร็จ" error={preview.error} onRetry={() => void preview.refetch()} compact />
          ) : (
            <LoadingBlock label="กำลังคิดผลการคัด…" />
          )
        ) : (
          <CullResult preview={p} level={level} onLevel={setLevel} userFlagged={s.userFlagged} />
        )}
      </Dialog>
      <ConfirmDialog
        open={confirmClear}
        onOpenChange={setConfirmClear}
        title="ล้างการคัดของ AI ในงานนี้?"
        description={`รูป ${formatCount(s?.aiFlagged ?? 0)} รูปที่ AI เลือกไว้หรือไม่เอาจะกลับเป็น “ยังไม่คัด” · รูปที่คัดเองอยู่ครบ`}
        confirmLabel="ล้างการคัดของ AI"
        cancelLabel="ไม่ล้าง"
        busy={clear.isPending}
        onConfirm={() => clear.mutate()}
      />
    </>
  );
}

function CullResult({ preview, level, onLevel, userFlagged }: { preview: CullPreview; level: number; onLevel: (l: number) => void; userFlagged: number }) {
  const counts = preview.levels.find((l) => l.level === preview.level);
  const total = preview.decisions.length;
  const rejected = total - (counts?.keep ?? 0);
  const { groups, blurry } = useMemo(() => grouped(preview.decisions), [preview.decisions]);
  const current = CULL_LEVELS.find((l) => l.level === level);

  return (
    <div className="flex flex-col gap-5">
      <fieldset>
        <legend className="mb-2 text-sm font-semibold text-ink">ระดับการคัด</legend>
        <div role="radiogroup" aria-label="ระดับการคัด" className="grid grid-cols-5 gap-1.5" data-testid="cull-levels">
          {CULL_LEVELS.map((l) => {
            const c = preview.levels.find((x) => x.level === l.level);
            const active = l.level === level;
            return (
              <button
                key={l.level}
                type="button"
                role="radio"
                aria-checked={active}
                onClick={() => onLevel(l.level)}
                onKeyDown={(e) => {
                  if (e.key === 'ArrowRight' || e.key === 'ArrowDown') onLevel(Math.min(5, level + 1));
                  if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') onLevel(Math.max(1, level - 1));
                }}
                tabIndex={active ? 0 : -1}
                data-testid={`cull-level-${l.level}`}
                className={cn(
                  'flex flex-col items-center gap-0.5 rounded-xl border px-2 py-2.5 text-center transition-colors',
                  active ? 'border-action bg-brand-soft text-action' : 'border-line bg-surface text-ink hover:bg-sunken',
                )}
              >
                <span className="text-[14px] font-bold">{l.label}</span>
                <span className={cn('text-[12.5px]', active ? 'text-action' : 'text-muted')}>เก็บ {formatCount(c?.keep ?? 0)}</span>
              </button>
            );
          })}
        </div>
        <div className="mt-1.5 flex justify-between text-[12px] text-muted" aria-hidden>
          <span>ได้รูปมาก</span>
          <span>ได้รูปน้อย</span>
        </div>
        {current && <p className="mt-1 text-[13px] text-muted">{current.hint}</p>}
      </fieldset>

      <div className="flex flex-wrap items-center gap-x-5 gap-y-1.5 rounded-xl bg-sunken px-4 py-3 text-[14px]" data-testid="cull-summary" aria-live="polite">
        <span className="inline-flex items-center gap-1.5 font-semibold text-ink">
          <Flag className="size-4 text-ok" fill="currentColor" aria-hidden />
          เก็บ {formatCount(counts?.keep ?? 0)} รูป
        </span>
        <span className="inline-flex items-center gap-1.5 text-ink">
          <Ban className="size-4" aria-hidden />
          ไม่เอา {formatCount(rejected)} รูป
          <span className="text-muted">
            (ซ้ำ {formatCount(counts?.duplicates ?? 0)} · เบลอ {formatCount(counts?.blurry ?? 0)})
          </span>
        </span>
        {userFlagged > 0 && <span className="text-muted">รูปที่คัดเองแล้ว {formatCount(userFlagged)} รูป AI ไม่แตะ</span>}
        {preview.unmeasured > 0 && <span className="text-warn">ยังดูไม่ได้ {formatCount(preview.unmeasured)} รูป (ไม่ถูกคัด)</span>}
      </div>

      {groups.length > 0 && (
        <section>
          <h3 className="mb-2 text-sm font-semibold text-ink">รูปที่ถ่ายซ้ำ — เก็บใบที่มีเครื่องหมายถูก</h3>
          <ul className="flex flex-col gap-2" data-testid="cull-groups">
            {groups.slice(0, MAX_GROUPS).map((g) => (
              <li key={g.keep} className="flex gap-1.5 overflow-x-auto scroll-thin pb-1">
                <Thumb id={g.keep} keep />
                {g.dupes.map((d) => (
                  <Thumb key={d} id={d} />
                ))}
              </li>
            ))}
          </ul>
          {groups.length > MAX_GROUPS && <p className="mt-2 text-[13px] text-muted">และอีก {formatCount(groups.length - MAX_GROUPS)} ช่วง</p>}
        </section>
      )}
      {blurry.length > 0 && (
        <section>
          <h3 className="mb-2 text-sm font-semibold text-ink">รูปที่เบลอกว่ารูปอื่นในงานนี้มาก</h3>
          <div className="flex flex-wrap gap-1.5" data-testid="cull-blurry">
            {blurry.map((id) => (
              <Thumb key={id} id={id} />
            ))}
          </div>
        </section>
      )}
      {total > 0 && groups.length === 0 && blurry.length === 0 && (
        <InlineNotice tone="info">ไม่พบรูปซ้ำหรือรูปที่เบลอในระดับนี้ — AI จะเลือกไว้ทุกรูปที่ยังไม่ได้คัด ลองเลื่อนไปทาง “เข้ม” ถ้าอยากได้รูปน้อยลง</InlineNotice>
      )}
    </div>
  );
}

function grouped(decisions: CullDecision[]) {
  const dupes = new Map<Id, Id[]>();
  const blurry: Id[] = [];
  for (const d of decisions) {
    if (d.reason.kind === 'duplicate') dupes.set(d.reason.of, [...(dupes.get(d.reason.of) ?? []), d.photoId]);
    if (d.reason.kind === 'blur') blurry.push(d.photoId);
  }
  // เรียงช่วงตามลำดับรูปที่เก็บ (decisions เรียงตามเวลาถ่ายแล้ว)
  const order = new Map(decisions.map((d, i) => [d.photoId, i]));
  const groups = [...dupes.entries()]
    .map(([keep, ids]) => ({ keep, dupes: ids }))
    .sort((a, b) => (order.get(a.keep) ?? 0) - (order.get(b.keep) ?? 0));
  return { groups, blurry };
}

function Thumb({ id, keep }: { id: Id; keep?: boolean }) {
  return (
    <span
      className={cn('relative size-[84px] shrink-0 overflow-hidden rounded-lg bg-sunken', keep ? 'ring-2 ring-ok' : 'opacity-60')}
      data-testid={keep ? 'cull-keep' : 'cull-drop'}
      data-photo-id={id}
    >
      <img src={assetUrl('thumb', id)} alt="" loading="lazy" className="size-full object-cover" />
      {keep ? (
        <span className="absolute top-1 left-1 inline-flex size-5 items-center justify-center rounded-full bg-ok text-white">
          <Check className="size-3.5" aria-hidden />
          <span className="sr-only">เก็บ</span>
        </span>
      ) : (
        <span className="absolute top-1 left-1 inline-flex size-5 items-center justify-center rounded-full bg-black/65 text-white">
          <Ban className="size-3" aria-hidden />
          <span className="sr-only">ไม่เอา</span>
        </span>
      )}
    </span>
  );
}
