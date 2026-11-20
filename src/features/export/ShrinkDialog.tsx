import { useEffect, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { CircleAlert, CircleCheck, Info, ShieldCheck, Trash2, TriangleAlert } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { ErrorState, InlineNotice, LoadingBlock, ProgressBar } from '@/components/ui/Feedback';
import { jobStateLabel } from '@/features/jobs/jobText';
import { errorMessage, formatBytes, formatCount, isErrorCode, newRequestId } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { ExportResize, Id, JobSnapshot, SelectionSpec, ShrinkPlan, ShrinkReport } from '@/lib/ipc/types';
import { qk } from '@/lib/queries';
import { useSettings } from '@/stores/settings';
import { ResizePicker, type ResizeHints } from './ResizePicker';
import { SHRINK_OPTIONS, toResize } from './resizeChoice';

interface ShrinkDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selection: SelectionSpec;
  /** จำนวนรูปที่ผู้ใช้เห็นว่าเลือกไว้ (ยอดจริงใช้จากแผนของ Rust) */
  count: number;
}

const FINISHED: JobSnapshot['state'][] = ['completed', 'completed_with_errors', 'cancelled', 'failed'];

const HINTS: ResizeHints = {
  original: '',
  long_edge: 'ย่อเฉพาะรูปที่ใหญ่กว่านี้ แล้วบันทึกทับที่เดิมเป็น JPEG คุณภาพสูง พร้อมวันที่ถ่าย ข้อมูลกล้อง และโปรไฟล์สีเดิม',
  max: 'ย่อเฉพาะรูปที่ไฟล์ใหญ่กว่านี้ ภาพต้องเหลือด้านยาวอย่างน้อย 1280 px ถ้าย่อจนต่ำกว่านั้นจะข้ามรูปนั้นไป',
};

function resizeKeyOf(r: ExportResize | null): string {
  if (!r) return '';
  if (r.mode === 'long_edge') return `edge:${r.px}`;
  if (r.mode === 'max_bytes') return `bytes:${r.bytes}`;
  return 'original';
}

/** ย่อไฟล์ต้นฉบับในที่เดิม: ตรวจแผน → ยืนยัน → ย่อและแทนที่ (ต้นฉบับขนาดเต็มไปถังขยะ) → รายงานรายรูป */
export function ShrinkDialog({ open, onOpenChange, selection, count }: ShrinkDialogProps) {
  const qc = useQueryClient();
  const [plan, setPlan] = useState<ShrinkPlan | null>(null);
  const [jobId, setJobId] = useState<Id | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const requestId = useRef(newRequestId());
  const choice = useSettings((s) => s.shrinkResize);
  const setChoice = useSettings((s) => s.setShrinkResize);
  const parsed = toResize(choice, SHRINK_OPTIONS);
  const resize = 'resize' in parsed ? parsed.resize : null;
  const resizeError = 'error' in parsed ? parsed.error : null;

  useEffect(() => {
    if (open) return;
    setPlan(null);
    setJobId(null);
    setConfirmed(false);
    requestId.current = newRequestId();
  }, [open]);

  const preview = useMutation({
    mutationFn: (r: ExportResize) => call('preview_shrink', { selection, resize: r }),
    onSuccess: (result) => setPlan(result),
  });

  // ตรวจแผนเมื่อเปิด และทุกครั้งที่เปลี่ยนขนาด (รอพิมพ์เสร็จสักครู่)
  const resizeKey = resizeKeyOf(resize);
  const planResizeKey = plan ? resizeKeyOf(plan.resize) : null;
  const previewMutate = preview.mutate;
  useEffect(() => {
    if (!open || !resize || resizeKey === planResizeKey || jobId) return;
    const timer = window.setTimeout(() => previewMutate(resize), plan ? 450 : 0);
    return () => window.clearTimeout(timer);
    // resizeKey แทน object `resize` ที่สร้างใหม่ทุก render
  }, [open, resizeKey, planResizeKey, jobId, previewMutate]);

  const start = useMutation({
    mutationFn: (planId: Id) => call('start_shrink', { planId, requestId: requestId.current }),
    onSuccess: (job) => {
      setJobId(job.id);
      void qc.invalidateQueries({ queryKey: ['jobs'] });
    },
  });

  const job = useQuery({
    queryKey: qk.job(jobId ?? ''),
    queryFn: () => call('get_job', { jobId: jobId as Id }),
    enabled: Boolean(jobId),
    refetchInterval: (query) => (query.state.data && FINISHED.includes(query.state.data.state) ? false : 700),
  });
  const finished = Boolean(job.data && FINISHED.includes(job.data.state));
  const report = useQuery({
    queryKey: ['shrink-report', jobId],
    queryFn: () => call('get_shrink_report', { jobId: jobId as Id }),
    enabled: Boolean(jobId) && finished,
  });
  const cancel = useMutation({ mutationFn: () => call('cancel_job', { jobId: jobId as Id }), onSuccess: () => void job.refetch() });

  const stale = isErrorCode(start.error, 'PREVIEW_STALE');
  let body: React.ReactNode;
  let footer: React.ReactNode;

  if (jobId) {
    const snapshot = job.data;
    if (!snapshot) {
      body = job.isError ? <ErrorState compact error={job.error} onRetry={() => void job.refetch()} /> : <LoadingBlock label="กำลังเริ่มย่อ…" className="py-10" />;
      footer = <Button onClick={() => onOpenChange(false)}>ปิด</Button>;
    } else if (!finished) {
      const processed = snapshot.counts.done + snapshot.counts.failed + snapshot.counts.skipped;
      body = (
        <div className="space-y-3 py-2" data-testid="shrink-progress">
          <p className="text-[15px] font-semibold text-ink" aria-live="polite">
            {snapshot.state === 'cancel_requested' ? 'กำลังยกเลิกหลังรูปชุดที่กำลังย่อ…' : `ย่อแล้ว ${formatCount(processed)} จาก ${formatCount(snapshot.counts.total)} รูป`}
          </p>
          <ProgressBar value={processed} max={snapshot.counts.total} label="ความคืบหน้าการย่อไฟล์ต้นฉบับ" />
          <p className="text-[13.5px] text-muted">
            สถานะ: {jobStateLabel(snapshot.state)} · ปิดหน้าต่างนี้ได้ งานจะทำต่อเบื้องหลังและดูผลได้ที่ “งานและประวัติ”
          </p>
        </div>
      );
      footer = (
        <>
          <Button variant="danger" busy={cancel.isPending} disabled={snapshot.state === 'cancel_requested'} onClick={() => cancel.mutate()}>
            ยกเลิกส่วนที่เหลือ
          </Button>
          <Button onClick={() => onOpenChange(false)}>ซ่อนหน้าต่าง</Button>
        </>
      );
    } else {
      body = report.isPending ? (
        <LoadingBlock label="กำลังสรุปผล…" className="py-10" />
      ) : report.isError ? (
        <ErrorState compact error={report.error} title="โหลดรายงานไม่สำเร็จ" onRetry={() => void report.refetch()} />
      ) : (
        <ShrinkReportView report={report.data} />
      );
      footer = (
        <Button variant="primary" onClick={() => onOpenChange(false)} data-testid="shrink-done">
          เสร็จ
        </Button>
      );
    }
  } else {
    const outdated = !plan || preview.isPending || resizeKeyOf(plan.resize) !== resizeKey;
    const skipped = plan ? [
      plan.alreadySmall > 0 && `${formatCount(plan.alreadySmall)} รูปเล็กพออยู่แล้ว`,
      plan.notJpeg > 0 && `${formatCount(plan.notJpeg)} รูปไม่ใช่ไฟล์ JPEG`,
      plan.unavailable > 0 && `${formatCount(plan.unavailable)} รูปที่ไฟล์ต้นฉบับไม่พร้อม`,
    ].filter(Boolean) : [];
    const saving = plan && plan.sourceBytes > 0 ? Math.max(0, plan.sourceBytes - plan.estimatedBytes) : 0;
    body = (
      <div className="space-y-4" data-testid="shrink-plan" aria-busy={preview.isPending || undefined}>
        <p className="text-sm text-muted">
          แทนไฟล์ต้นฉบับของ {formatCount(count)} รูปที่เลือกด้วยไฟล์ที่ย่อแล้ว ในโฟลเดอร์เดิมและชื่อเดิม เพื่อคืนพื้นที่ดิสก์
        </p>
        <ResizePicker value={choice} onChange={setChoice} error={resizeError} options={SHRINK_OPTIONS} hints={HINTS} label="ย่อเหลือ" />
        {preview.isError ? (
          <InlineNotice tone="danger" icon={<CircleAlert aria-hidden />}>
            {errorMessage(preview.error)}
          </InlineNotice>
        ) : !plan ? (
          <LoadingBlock label="กำลังตรวจรูปที่เลือก…" className="py-6" />
        ) : (
          <>
            <dl className={`grid grid-cols-2 gap-x-6 gap-y-2.5 text-sm transition-opacity sm:grid-cols-3 ${outdated ? 'opacity-50' : ''}`} data-testid="shrink-plan-stats">
              <Stat label="รูปที่เลือก" value={`${formatCount(plan.inputCount)} รูป`} />
              <Stat label="จะย่อ" value={`${formatCount(plan.plannedCount)} รูป`} />
              <Stat
                label={plan.resize.mode === 'max_bytes' ? 'ขนาดรวมหลังย่อไม่เกิน' : 'ขนาดรวมหลังย่อโดยประมาณ'}
                value={plan.plannedCount > 0 ? formatBytes(plan.estimatedBytes) : '—'}
                note={plan.plannedCount > 0 ? `จาก ${formatBytes(plan.sourceBytes)}` : undefined}
              />
            </dl>
            {skipped.length > 0 && !outdated && (
              <InlineNotice tone="info" icon={<Info aria-hidden />}>
                ไม่แตะ {skipped.join(' · ')}
              </InlineNotice>
            )}
            {plan.plannedCount === 0 && !outdated ? (
              <InlineNotice tone="ok" icon={<CircleCheck aria-hidden />}>
                ไม่มีรูปที่ต้องย่อในขนาดนี้ ลองเลือกขนาดที่เล็กลง
              </InlineNotice>
            ) : (
              <>
                <InlineNotice tone="ok" icon={<ShieldCheck aria-hidden />}>
                  ใบหน้า ชื่อบุคคล กิจกรรม และวันที่ถ่ายอยู่ครบ ไม่ต้องวิเคราะห์ใบหน้าใหม่
                </InlineNotice>
                <InlineNotice tone="warn" icon={<Trash2 aria-hidden />}>
                  ไฟล์ต้นฉบับขนาดเต็ม {formatCount(plan.plannedCount)} ไฟล์จะถูกย้ายไปถังขยะ กู้คืนได้จนกว่าจะล้างถังขยะ
                  {saving > 0 && ` · ล้างถังขยะแล้วจะได้พื้นที่คืนราว ${formatBytes(saving)}`}
                </InlineNotice>
                <label className="flex cursor-pointer items-start gap-2.5 rounded-xl border border-line px-3.5 py-2.5 text-sm" data-testid="shrink-confirm-check">
                  <input type="checkbox" className="mt-0.5 size-4 shrink-0 accent-[var(--action)]" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />
                  <span>
                    เข้าใจแล้ว ให้แทนไฟล์ต้นฉบับด้วยไฟล์ที่ย่อ
                    <span className="block text-[13px] text-muted">โฟลเดอร์ที่ซิงก์กับคลาวด์หรือเครื่องอื่น จะได้รับไฟล์ที่ย่อด้วย</span>
                  </span>
                </label>
              </>
            )}
          </>
        )}
        {start.isError && (
          <InlineNotice
            tone={stale ? 'warn' : 'danger'}
            icon={<TriangleAlert aria-hidden />}
            action={
              stale ? (
                <Button size="sm" disabled={!resize} onClick={() => resize && preview.mutate(resize)}>
                  ตรวจแผนใหม่
                </Button>
              ) : undefined
            }
          >
            {stale ? 'ข้อมูลเปลี่ยนหลังจากตรวจแผนนี้ กรุณาตรวจแผนใหม่ก่อนเริ่ม' : errorMessage(start.error)}
          </InlineNotice>
        )}
      </div>
    );
    footer = (
      <>
        <Button onClick={() => onOpenChange(false)}>ยกเลิก</Button>
        <Button
          variant="primary"
          data-testid="shrink-confirm"
          disabled={!plan || outdated || plan.plannedCount === 0 || !confirmed}
          busy={start.isPending}
          onClick={() => plan && start.mutate(plan.planId)}
        >
          {plan && plan.plannedCount > 0 ? `ย่อ ${formatCount(plan.plannedCount)} รูป` : 'ย่อ'}
        </Button>
      </>
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="ย่อไฟล์ต้นฉบับ" size="lg" footer={footer} dismissible={!start.isPending}>
      {body}
    </Dialog>
  );
}

function Stat({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div>
      <dt className="text-[13px] text-muted">{label}</dt>
      <dd className="text-lg font-bold text-ink">
        {value}
        {note && <span className="ml-1.5 text-[13px] font-normal text-muted">{note}</span>}
      </dd>
    </div>
  );
}

const ITEM_STATUS: Record<ShrinkReport['items'][number]['status'], string> = {
  planned: 'ยังไม่ได้ทำ',
  replacing: 'ค้างระหว่างแทนที่ จะทำต่อเมื่อเปิดแอปครั้งหน้า',
  shrunk: 'ย่อแล้ว',
  skipped: 'ข้าม',
  unavailable: 'ไฟล์ต้นฉบับไม่พร้อม',
  failed: 'ย่อไม่สำเร็จ',
  cancelled: 'ถูกยกเลิก',
};

/** รายงานผล: ทุกรูปที่เลือกมีผลของตัวเอง รูปที่ข้ามบอกเหตุผล */
export function ShrinkReportView({ report }: { report: ShrinkReport }) {
  const problems = report.failed + report.cancelled + report.pending;
  const others = report.items.filter((item) => item.status !== 'shrunk');
  const saved = report.sourceBytes > 0 ? Math.round((1 - report.outputBytes / report.sourceBytes) * 100) : 0;
  return (
    <div className="space-y-4" data-testid="shrink-report">
      {problems === 0 ? (
        <InlineNotice tone="ok" icon={<CircleCheck aria-hidden />}>
          <span className="font-semibold">ย่อแล้ว {formatCount(report.shrunk)} รูป</span> จาก {formatCount(report.inputCount)} รูปที่เลือก
        </InlineNotice>
      ) : (
        <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
          <span className="font-semibold">
            ย่อแล้ว {formatCount(report.shrunk)} รูป มี {formatCount(problems)} รูปที่ยังไม่ได้ย่อ
          </span>
        </InlineNotice>
      )}
      <dl className="grid grid-cols-2 gap-x-6 gap-y-2 text-sm sm:grid-cols-4">
        <Stat label="ย่อแล้ว" value={formatCount(report.shrunk)} />
        <Stat label="ข้าม" value={formatCount(report.skipped + report.unavailable)} />
        <Stat label="ไม่สำเร็จ" value={formatCount(report.failed + report.pending)} />
        <Stat label="ถูกยกเลิก" value={formatCount(report.cancelled)} />
      </dl>
      {report.shrunk > 0 && (
        <div className="rounded-xl border border-line bg-canvas px-4 py-3 text-sm" data-testid="shrink-report-size">
          <p className="text-muted">
            ขนาดรวม {formatBytes(report.sourceBytes)} → <span className="font-semibold text-ink">{formatBytes(report.outputBytes)}</span>
            {saved > 0 ? ` (เล็กลง ${saved}%)` : ''}
          </p>
          <p className="mt-1 flex items-start gap-1.5 text-[13px] text-muted">
            <Trash2 className="mt-0.5 size-4 shrink-0" aria-hidden />
            ต้นฉบับขนาดเต็มอยู่ในถังขยะ ล้างถังขยะเมื่อแน่ใจแล้ว จึงจะได้พื้นที่คืน {formatBytes(Math.max(0, report.sourceBytes - report.outputBytes))}
          </p>
        </div>
      )}
      {others.length > 0 && (
        <div>
          <p className="mb-1.5 text-sm font-semibold text-ink">รูปที่ไม่ได้ย่อ ({formatCount(others.length)})</p>
          <ul className="scroll-thin max-h-56 divide-y divide-line overflow-y-auto rounded-xl border border-line">
            {others.map((item) => (
              <li key={item.photoId} className="flex flex-wrap items-baseline gap-x-3 px-3 py-2 text-[13.5px]">
                <span className="min-w-0 flex-1 truncate font-medium text-ink">{item.filename}</span>
                <span className="text-muted">{item.note ?? ITEM_STATUS[item.status]}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
