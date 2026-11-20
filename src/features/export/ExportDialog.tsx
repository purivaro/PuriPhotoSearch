import { useEffect, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { CircleAlert, CircleCheck, FolderOpen, Info, TriangleAlert } from 'lucide-react';
import { useFolderChooser } from '@/app/FolderChooser';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { ErrorState, InlineNotice, LoadingBlock, ProgressBar } from '@/components/ui/Feedback';
import { jobStateLabel } from '@/features/jobs/jobText';
import { errorMessage, formatBytes, formatCount, isErrorCode, newRequestId } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { ExportPlan, ExportReport, ExportResize, Id, JobSnapshot, SelectionSpec, WatermarkSpec } from '@/lib/ipc/types';
import { qk } from '@/lib/queries';
import { useSettings } from '@/stores/settings';
import { toast } from 'sonner';
import { ResizePicker, type ResizeHints } from './ResizePicker';
import { EXPORT_OPTIONS, EXPORT_WATERMARK_OPTIONS, toResize } from './resizeChoice';
import { WatermarkRow, useWatermarks } from './WatermarkDialog';
import { toWatermarkSpec, watermarkKeyOf } from './watermarkChoice';

interface ExportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selection: SelectionSpec;
  /** จำนวนรูปที่ผู้ใช้เห็นว่าเลือกไว้ (ยอดจริงใช้จากแผนของ Rust) */
  count: number;
  /** รูปที่ใช้แสดงตัวอย่างลายน้ำ (รูปแรกที่เลือก) */
  previewPhotoId?: Id | null;
  onFinished?: () => void;
}

const FINISHED: JobSnapshot['state'][] = ['completed', 'completed_with_errors', 'cancelled', 'failed'];

/** คำอธิบายขนาดเมื่อใส่ลายน้ำ: ทุกสำเนาถูกบันทึกใหม่ ไม่มีการคัดลอกไฟล์เดิม */
const WATERMARK_HINTS: ResizeHints = {
  original: 'ขนาดภาพเท่าต้นฉบับ บันทึกใหม่เป็น JPEG คุณภาพสูงเพื่อใส่ลายน้ำ เก็บวันที่ถ่ายและข้อมูลกล้องไว้',
  long_edge: 'ย่อเฉพาะรูปที่ใหญ่กว่านี้ (รูปที่เล็กกว่าคงขนาดเดิม) · ทุกสำเนาบันทึกเป็น JPEG คุณภาพสูงพร้อมลายน้ำ เก็บวันที่ถ่ายไว้',
  max: 'ย่อเฉพาะรูปที่ไฟล์ใหญ่กว่านี้ ลดขนาดภาพก่อนแล้วใช้คุณภาพสูงสุดที่ยังไม่เกิน · ทุกสำเนามีลายน้ำ',
};

/** ตัวแทนค่าของวิธีย่อ — เทียบด้วยค่า ไม่ใช่ JSON เพราะลำดับ key ที่ Rust ส่งกลับมาไม่เหมือนที่ส่งไป */
function resizeKeyOf(r: ExportResize | null): string {
  if (!r) return '';
  if (r.mode === 'long_edge') return `edge:${r.px}`;
  if (r.mode === 'max_bytes') return `bytes:${r.bytes}`;
  return 'original';
}

/** S18 — ส่งออกสำเนา: เลือกปลายทาง → ตรวจแผน → คัดลอก → รายงานรายไฟล์ (ต้นฉบับไม่ถูกย้ายหรือแก้) */
export function ExportDialog({ open, onOpenChange, selection, count, previewPhotoId = null, onFinished }: ExportDialogProps) {
  const qc = useQueryClient();
  const choose = useFolderChooser();
  const [plan, setPlan] = useState<ExportPlan | null>(null);
  const [jobId, setJobId] = useState<Id | null>(null);
  const requestId = useRef(newRequestId());
  const choice = useSettings((s) => s.exportResize);
  const setChoice = useSettings((s) => s.setExportResize);
  const parsed = toResize(choice);
  const resize = 'resize' in parsed ? parsed.resize : null;
  const resizeError = 'error' in parsed ? parsed.error : null;
  const watermarkChoice = useSettings((s) => s.exportWatermark);
  const setWatermarkChoice = useSettings((s) => s.setExportWatermark);
  const watermarks = useWatermarks();
  const parsedWatermark = toWatermarkSpec(watermarkChoice, watermarks.data);
  // undefined = เปิดลายน้ำไว้แต่ยังเลือกไม่ครบ: ยังตรวจแผนไม่ได้
  const watermark = 'spec' in parsedWatermark ? parsedWatermark.spec : undefined;
  const watermarkError = 'error' in parsedWatermark ? parsedWatermark.error : null;
  const options = resize && watermark !== undefined ? { resize, watermark } : null;
  const pickerOptions = watermarkChoice.enabled ? { options: EXPORT_WATERMARK_OPTIONS, hints: WATERMARK_HINTS } : { options: EXPORT_OPTIONS };

  useEffect(() => {
    if (open) return;
    // ปิดแล้วเริ่มใหม่รอบหน้า (งานที่เริ่มไปแล้วทำต่อเบื้องหลัง)
    setPlan(null);
    setJobId(null);
    requestId.current = newRequestId();
  }, [open]);

  const preview = useMutation({
    mutationFn: (args: { destination: string; resize: ExportResize; watermark: WatermarkSpec | null }) => call('preview_export', { selection, ...args }),
    onSuccess: (result) => setPlan(result),
  });

  // เปลี่ยนขนาดหรือลายน้ำหลังเห็นแผนแล้ว: ตรวจแผนใหม่ (รอพิมพ์/เลื่อนเสร็จสักครู่) เพราะชื่อไฟล์ จำนวนที่ย่อ และขนาดรวมเปลี่ยนตาม
  const optionsKey = options ? `${resizeKeyOf(options.resize)}|${watermarkKeyOf(options.watermark)}` : '';
  const planDestination = plan?.destination;
  const planKey = plan ? `${resizeKeyOf(plan.resize)}|${watermarkKeyOf(plan.watermark)}` : null;
  const previewMutate = preview.mutate;
  useEffect(() => {
    if (!planDestination || !options || optionsKey === planKey || jobId) return;
    const timer = window.setTimeout(() => previewMutate({ destination: planDestination, ...options }), 450);
    return () => window.clearTimeout(timer);
    // ใช้ optionsKey แทน object ที่สร้างใหม่ทุก render จะได้ไม่ตรวจแผนซ้ำโดยค่าไม่เปลี่ยน
  }, [optionsKey, planDestination, planKey, jobId, previewMutate]);

  const start = useMutation({
    mutationFn: (planId: Id) => call('start_export', { planId, requestId: requestId.current }),
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
    queryKey: ['export-report', jobId],
    queryFn: () => call('get_export_report', { jobId: jobId as Id }),
    enabled: Boolean(jobId) && finished,
  });

  const notified = useRef<Id | null>(null);
  useEffect(() => {
    if (finished && jobId && notified.current !== jobId) {
      notified.current = jobId;
      onFinished?.();
    }
  }, [finished, jobId, onFinished]);

  const cancel = useMutation({ mutationFn: () => call('cancel_job', { jobId: jobId as Id }), onSuccess: () => void job.refetch() });

  const pickDestination = async () => {
    const destination = await choose({
      title: 'เลือกโฟลเดอร์ปลายทาง',
      hint: 'สำเนาจะถูกคัดลอกไปไว้ที่นี่ เลือกโฟลเดอร์ที่อยู่นอกโฟลเดอร์กิจกรรมในคลัง',
    });
    if (!destination || !options) return;
    start.reset();
    preview.mutate({ destination, ...options });
  };
  // สร้างไฟล์ใหม่ (ย่อหรือใส่ลายน้ำ) แทนการคัดลอกไฟล์เดิม
  const converting = (plan?.resize ?? resize)?.mode !== 'original' || Boolean(plan ? plan.watermark : watermark);

  const stale = isErrorCode(start.error, 'PREVIEW_STALE');
  let body: React.ReactNode;
  let footer: React.ReactNode;

  if (jobId) {
    const snapshot = job.data;
    if (!snapshot) {
      body = job.isError ? <ErrorState compact error={job.error} onRetry={() => void job.refetch()} /> : <LoadingBlock label="กำลังเริ่มส่งออก…" className="py-10" />;
      footer = <Button onClick={() => onOpenChange(false)}>ปิด</Button>;
    } else if (!finished) {
      const processed = snapshot.counts.done + snapshot.counts.failed + snapshot.counts.skipped;
      body = (
        <div className="space-y-3 py-2" data-testid="export-progress">
          <p className="text-[15px] font-semibold text-ink" aria-live="polite">
            {snapshot.state === 'cancel_requested'
              ? 'กำลังยกเลิกหลังไฟล์ที่กำลังคัดลอก…'
              : `${converting ? 'ส่งออกแล้ว' : 'คัดลอกแล้ว'} ${formatCount(processed)} จาก ${formatCount(snapshot.counts.total)} ไฟล์`}
          </p>
          <ProgressBar value={processed} max={snapshot.counts.total} label="ความคืบหน้าการส่งออก" />
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
        <LoadingBlock label="กำลังสรุปผลการส่งออก…" className="py-10" />
      ) : report.isError ? (
        <ErrorState compact error={report.error} title="โหลดรายงานการส่งออกไม่สำเร็จ" onRetry={() => void report.refetch()} />
      ) : (
        <ExportReportView report={report.data} />
      );
      footer = (
        <Button variant="primary" onClick={() => onOpenChange(false)} data-testid="export-done">
          เสร็จ
        </Button>
      );
    }
  } else if (preview.isPending && !plan) {
    body = <LoadingBlock label="กำลังตรวจแผนการส่งออก…" className="py-10" />;
    footer = <Button onClick={() => onOpenChange(false)}>ยกเลิก</Button>;
  } else if (plan) {
    // แผนที่แสดงยังไม่ตรงกับขนาดที่เลือกล่าสุด (กำลังตรวจใหม่ หรือค่าที่กรอกยังไม่ถูกต้อง): ยังไม่ให้เริ่ม
    const outdated = preview.isPending || planKey !== optionsKey;
    const kept = plan.plannedOutputCount - plan.resizedCount;
    const sizeNote = plan.resizedCount > 0 ? ` · ย่อ ${formatCount(plan.resizedCount)} รูป` : '';
    body = (
      <div className="space-y-4" data-testid="export-plan" aria-busy={preview.isPending || undefined}>
        <div className="rounded-xl border border-line bg-canvas px-4 py-3">
          <p className="text-[13px] font-semibold text-muted">โฟลเดอร์ปลายทาง</p>
          <p className="break-all text-sm text-ink select-text" data-selectable>
            {plan.destination}
          </p>
          <button type="button" onClick={() => void pickDestination()} className="mt-1 text-[13.5px] font-semibold text-action hover:underline">
            เปลี่ยนปลายทาง
          </button>
        </div>
        <ResizePicker value={choice} onChange={setChoice} error={resizeError} {...pickerOptions} />
        <WatermarkRow choice={watermarkChoice} onChange={setWatermarkChoice} error={watermarkError} previewPhotoId={previewPhotoId} />
        <dl className={`grid grid-cols-2 gap-x-6 gap-y-2.5 text-sm transition-opacity sm:grid-cols-3 ${outdated ? 'opacity-50' : ''}`} data-testid="export-plan-stats">
          <PlanStat label="รูปที่เลือก" value={`${formatCount(plan.inputCount)} รูป`} />
          <PlanStat label="ไฟล์ที่จะสร้าง" value={`${formatCount(plan.plannedOutputCount)} สำเนา`} />
          <PlanStat
            label={plan.resize.mode === 'max_bytes' && plan.resizedCount > 0 ? 'ขนาดรวมไม่เกิน' : 'ขนาดรวมโดยประมาณ'}
            value={plan.plannedOutputCount > 0 ? formatBytes(plan.estimatedBytes) : '—'}
            note={plan.resizedCount > 0 || plan.watermark ? `ต้นฉบับ ${formatBytes(plan.sourceBytes)}` : undefined}
          />
        </dl>
        {(plan.resize.mode !== 'original' || plan.watermark) && plan.plannedOutputCount > 0 && !outdated && (
          <InlineNotice tone="info" icon={<Info aria-hidden />}>
            {plan.watermark
              ? `ใส่ลายน้ำทุกสำเนา${sizeNote}`
              : plan.resizedCount === 0
                ? 'ทุกรูปเล็กกว่าขนาดที่เลือกอยู่แล้ว จะคัดลอกไฟล์เดิมโดยไม่ย่อ'
                : `จะย่อ ${formatCount(plan.resizedCount)} รูป${kept > 0 ? ` · อีก ${formatCount(kept)} รูปเล็กพออยู่แล้ว จะคัดลอกไฟล์เดิม` : ''}`}
          </InlineNotice>
        )}
        {preview.isError && (
          <InlineNotice tone="danger" icon={<CircleAlert aria-hidden />}>
            {errorMessage(preview.error)}
          </InlineNotice>
        )}
        {plan.unavailableInputs > 0 && (
          <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
            มี {formatCount(plan.unavailableInputs)} รูปที่ไฟล์ต้นฉบับไม่พร้อม จะไม่ถูกส่งออกในรอบนี้ (ไม่ใช้ภาพย่อแทน) เชื่อมต่อไดรฟ์ต้นทางแล้วส่งออกอีกครั้งได้
          </InlineNotice>
        )}
        {plan.renamedCount > 0 && (
          <InlineNotice tone="info" icon={<Info aria-hidden />}>
            มี {formatCount(plan.renamedCount)} ไฟล์ที่ชื่อชนกับไฟล์อื่น จะเติมเลขท้ายชื่อให้ ไม่เขียนทับไฟล์ที่มีอยู่
          </InlineNotice>
        )}
        {plan.sampleNames.length > 0 && (
          <div>
            <p className="mb-1 text-[13px] font-semibold text-muted">ตัวอย่างชื่อไฟล์ปลายทาง</p>
            <ul className="space-y-0.5 text-[13px]">
              {plan.sampleNames.slice(0, 5).map((sample, index) => (
                <li key={`${sample.output}-${index}`} className="flex gap-2 text-muted">
                  <span className="min-w-0 truncate">{sample.source}</span>
                  <span aria-hidden>→</span>
                  <span className="min-w-0 truncate font-medium text-ink">{sample.output}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
        {!plan.canStart && (
          <InlineNotice tone="danger" icon={<CircleAlert aria-hidden />}>
            {plan.blockedReason ?? 'ยังเริ่มส่งออกไม่ได้ กรุณาเลือกปลายทางอื่น'}
          </InlineNotice>
        )}
        {start.isError && (
          <InlineNotice
            tone={stale ? 'warn' : 'danger'}
            icon={<TriangleAlert aria-hidden />}
            action={
              stale ? (
                <Button size="sm" disabled={!options} onClick={() => options && preview.mutate({ destination: plan.destination, ...options })}>
                  ตรวจแผนใหม่
                </Button>
              ) : undefined
            }
          >
            {stale ? 'ข้อมูลเปลี่ยนหลังจากตรวจแผนนี้ กรุณาตรวจแผนใหม่ก่อนเริ่ม' : errorMessage(start.error)}
          </InlineNotice>
        )}
        <p className="flex items-center gap-1.5 text-[13px] text-muted">
          <Info className="size-4 shrink-0" aria-hidden />
          {plan.watermark ? 'สร้างสำเนาใหม่ที่ใส่ลายน้ำ' : plan.resizedCount === 0 ? 'คัดลอกไฟล์ต้นฉบับแบบไม่แปลงไฟล์' : 'สร้างสำเนาที่ย่อแล้วเป็นไฟล์ใหม่'}{' '}
          รูปต้นฉบับไม่ถูกย้าย เปลี่ยนชื่อ หรือแก้ไข
        </p>
      </div>
    );
    footer = (
      <>
        <Button onClick={() => onOpenChange(false)}>ยกเลิก</Button>
        <Button
          variant="primary"
          data-testid="export-confirm"
          disabled={!plan.canStart || plan.plannedOutputCount === 0 || outdated}
          busy={start.isPending}
          onClick={() => start.mutate(plan.planId)}
        >
          ส่งออก {formatCount(plan.plannedOutputCount)} สำเนา
        </Button>
      </>
    );
  } else {
    body = (
      <div className="space-y-4 py-1">
        <p className="text-sm text-muted">
          เลือกขนาดและโฟลเดอร์ที่จะวางสำเนาของ {formatCount(count)} รูปที่เลือก แอปจะแสดงแผนให้ตรวจก่อนเริ่ม
        </p>
        <ResizePicker value={choice} onChange={setChoice} error={resizeError} {...pickerOptions} />
        <WatermarkRow choice={watermarkChoice} onChange={setWatermarkChoice} error={watermarkError} previewPhotoId={previewPhotoId} />
        {preview.isError && (
          <InlineNotice tone="danger" icon={<CircleAlert aria-hidden />}>
            {errorMessage(preview.error)}
          </InlineNotice>
        )}
        <Button
          variant="primary"
          size="lg"
          icon={<FolderOpen className="size-5" aria-hidden />}
          disabled={!options}
          onClick={() => void pickDestination()}
          data-testid="export-pick-destination"
        >
          เลือกโฟลเดอร์ปลายทาง
        </Button>
        <p className="flex items-center gap-1.5 text-[13px] text-muted">
          <Info className="size-4 shrink-0" aria-hidden /> ปลายทางต้องอยู่นอกโฟลเดอร์กิจกรรมที่เพิ่มไว้ เพื่อไม่ให้สำเนาถูกเพิ่มกลับเข้าคลังซ้ำ
        </p>
      </div>
    );
    footer = <Button onClick={() => onOpenChange(false)}>ยกเลิก</Button>;
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="ส่งออกสำเนารูปภาพ" size="lg" footer={footer} dismissible={!start.isPending}>
      {body}
    </Dialog>
  );
}

function PlanStat({ label, value, note }: { label: string; value: string; note?: string }) {
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

const ITEM_STATUS: Record<ExportReport['items'][number]['status'], string> = {
  copied: 'คัดลอกแล้ว',
  failed: 'คัดลอกไม่สำเร็จ',
  cancelled: 'ถูกยกเลิก',
  unavailable: 'ต้นฉบับไม่พร้อม',
  planned: 'ยังไม่ได้ทำ',
};

/** รายงานผลส่งออก: ทุกรูปที่เลือกมีผลของตัวเอง ไม่ใช้คำว่า "สำเร็จ" เมื่อมีรายการไม่สำเร็จ */
export function ExportReportView({ report }: { report: ExportReport }) {
  const notDone = report.failed + report.unavailable + report.cancelled;
  const problems = report.items.filter((item) => item.status !== 'copied');
  const resized = report.resize.mode !== 'original' || report.watermark !== null;
  const saved = report.sourceBytes > 0 ? Math.round((1 - report.outputBytes / report.sourceBytes) * 100) : 0;
  return (
    <div className="space-y-4" data-testid="export-report">
      {notDone === 0 ? (
        <InlineNotice tone="ok" icon={<CircleCheck aria-hidden />}>
          <span className="font-semibold">
            {resized ? 'ส่งออกครบ' : 'คัดลอกครบ'} {formatCount(report.copied)} ไฟล์
          </span>{' '}
          จาก {formatCount(report.inputCount)} รูปที่เลือก
        </InlineNotice>
      ) : (
        <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
          <span className="font-semibold">
            ทำสำเร็จ {formatCount(report.copied)} รายการ มี {formatCount(notDone)} รายการที่ต้องแก้ไข
          </span>
        </InlineNotice>
      )}
      <dl className="grid grid-cols-2 gap-x-6 gap-y-2 text-sm sm:grid-cols-4">
        <PlanStat label={resized ? 'ส่งออกแล้ว' : 'คัดลอกแล้ว'} value={formatCount(report.copied)} />
        <PlanStat label="ไม่สำเร็จ" value={formatCount(report.failed)} />
        <PlanStat label="ต้นฉบับไม่พร้อม" value={formatCount(report.unavailable)} />
        <PlanStat label="ถูกยกเลิก" value={formatCount(report.cancelled)} />
      </dl>
      {resized && report.copied > 0 && (
        <p className="text-sm text-muted" data-testid="export-report-size">
          {report.watermark && 'ใส่ลายน้ำแล้ว · '}ขนาดรวม {formatBytes(report.sourceBytes)} →{' '}
          <span className="font-semibold text-ink">{formatBytes(report.outputBytes)}</span>
          {saved > 0 ? ` (เล็กลง ${saved}%)` : ''}
        </p>
      )}
      <div className="rounded-xl border border-line bg-canvas px-4 py-3">
        <p className="text-[13px] font-semibold text-muted">โฟลเดอร์ปลายทาง</p>
        <p className="break-all text-sm text-ink select-text" data-selectable>
          {report.destination}
        </p>
        <Button
          size="sm"
          className="mt-2"
          icon={<FolderOpen className="size-4" aria-hidden />}
          onClick={() => {
            call('reveal_export_destination', { jobId: report.jobId }).catch((e) => toast.error(errorMessage(e)));
          }}
          data-testid="export-reveal"
        >
          เปิดโฟลเดอร์ปลายทาง
        </Button>
      </div>
      {problems.length > 0 && (
        <div>
          <p className="mb-1.5 text-sm font-semibold text-ink">รายการที่ไม่ได้คัดลอก ({formatCount(problems.length)})</p>
          <ul className="scroll-thin max-h-56 divide-y divide-line overflow-y-auto rounded-xl border border-line">
            {problems.map((item) => (
              <li key={item.photoId} className="flex items-baseline gap-3 px-3 py-2 text-[13.5px]">
                <span className="min-w-0 flex-1 truncate font-medium text-ink">{item.filename}</span>
                <span className="shrink-0 text-muted">{item.error ?? ITEM_STATUS[item.status]}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
