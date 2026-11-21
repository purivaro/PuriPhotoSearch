import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router';
import { toast } from 'sonner';
import { ChevronDown, CircleCheck, Download, FileText, FolderOpen, History, Minimize2, Pause, Play, RotateCcw, Sparkles, Square, TriangleAlert, Upload } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { ConfirmDialog, Dialog } from '@/components/ui/Dialog';
import { ErrorState, LoadingBlock, ProgressBar, StatusBadge } from '@/components/ui/Feedback';
import { ExportReportView } from '@/features/export/ExportDialog';
import { ShrinkReportView } from '@/features/export/ShrinkDialog';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount, formatInstant } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { JobSnapshot } from '@/lib/ipc/types';
import { isJobActive, qk } from '@/lib/queries';
import { canCancel, canPause, canResume, canRetryFailed, errorCodeLabel, jobKindLabel, jobProgressText, jobResultText, jobStageLabel, jobStateLabel, jobStateTone } from './jobText';

const FINISHED: JobSnapshot['state'][] = ['completed', 'completed_with_errors', 'cancelled', 'failed'];

/** คำสั่งควบคุมงาน: แสดงสถานะใหม่หลัง backend ยืนยันเท่านั้น (เช่น "กำลังหยุดหลังรายการปัจจุบัน") */
function useJobControls(job: JobSnapshot) {
  const qc = useQueryClient();
  const options = {
    onSuccess: () => void qc.invalidateQueries({ queryKey: ['jobs'] }),
    onError: (error: unknown) => toast.error(errorMessage(error)),
  };
  return {
    pause: useMutation({ mutationFn: () => call('pause_job', { jobId: job.id }), ...options }),
    resume: useMutation({ mutationFn: () => call('resume_job', { jobId: job.id }), ...options }),
    cancel: useMutation({ mutationFn: () => call('cancel_job', { jobId: job.id }), ...options }),
    retry: useMutation({ mutationFn: () => call('retry_failed_items', { jobId: job.id }), ...options }),
  };
}

/** S04/S19 — หนึ่งงานเบื้องหลัง พร้อมความคืบหน้าจริง คำสั่งที่ใช้ได้ และรายการข้อผิดพลาด */
export function JobCard({ job, defaultExpanded = false }: { job: JobSnapshot; defaultExpanded?: boolean }) {
  const controls = useJobControls(job);
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [confirmCancel, setConfirmCancel] = useState(false);
  const [reportOpen, setReportOpen] = useState(false);
  const finished = FINISHED.includes(job.state);
  const active = isJobActive(job);
  const processed = job.counts.done + job.counts.failed + job.counts.skipped;
  const KindIcon =
    job.kind === 'export' ? Upload : job.kind === 'model' ? Download : job.kind === 'shrink' ? Minimize2 : job.kind === 'cull' ? Sparkles : FolderOpen;
  const isModel = job.kind === 'model';

  return (
    <li data-testid="job-card" data-job-id={job.id} data-job-state={job.state} className="rounded-card border border-line bg-surface shadow-card">
      <div className="flex flex-wrap items-start gap-x-4 gap-y-3 px-5 py-4">
        <span className={cn('mt-0.5 flex size-10 shrink-0 items-center justify-center rounded-xl', active ? 'bg-brand-soft text-action' : 'bg-sunken text-muted')}>
          <KindIcon className="size-5" aria-hidden />
        </span>
        <div className="min-w-[220px] flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-[16px] font-bold text-ink">{job.title}</h3>
            <StatusBadge tone={jobStateTone(job.state)} icon={job.state === 'completed' ? <CircleCheck aria-hidden /> : job.state === 'completed_with_errors' ? <TriangleAlert aria-hidden /> : undefined}>
              {jobStateLabel(job.state)}
            </StatusBadge>
            {job.recovered && !finished && (
              <StatusBadge tone="info" icon={<History aria-hidden />}>
                กู้คืนหลังปิดแอปกลางคัน
              </StatusBadge>
            )}
          </div>
          <p className="text-[13.5px] text-muted">
            {jobKindLabel(job)}
            {!finished && ` · ขั้นตอน: ${jobStageLabel(job)}`}
            {` · เริ่ม ${formatInstant(job.startedAt ?? job.createdAt) ?? '—'}`}
            {job.finishedAt ? ` · จบ ${formatInstant(job.finishedAt)}` : ''}
          </p>

          {finished ? (
            <p className="mt-1.5 text-sm font-semibold text-ink">{jobResultText(job)}</p>
          ) : (
            <div className="mt-2.5 max-w-[560px]">
              <div className="mb-1.5 flex items-baseline justify-between gap-3 text-sm">
                <span className="font-semibold text-ink">{jobProgressText(job)}</span>
                {job.counts.failed > 0 && <span className="text-warn">ผิดพลาด {formatCount(job.counts.failed)}</span>}
              </div>
              <ProgressBar
                value={processed}
                max={job.counts.total}
                indeterminate={(!job.totalKnown || (isModel && job.stage === 'verify')) && job.state === 'running'}
                label={`ความคืบหน้าของ ${job.title}`}
              />
            </div>
          )}
          {job.state === 'blocked' && job.blockedReason && <p className="mt-2 text-sm text-warn">{job.blockedReason}</p>}
          {job.state === 'paused' && <p className="mt-2 text-[13.5px] text-muted">งานนี้พักไว้ จะทำต่อเมื่อกด “ทำต่อ” เท่านั้น ส่วนที่ทำสำเร็จแล้วยังใช้ได้</p>}
        </div>

        <div className="flex shrink-0 flex-wrap items-center gap-2">
          {job.eventId && (
            <Link to={`/events/${job.eventId}`} className="inline-flex h-8 items-center rounded-lg border border-line-strong bg-surface px-3 text-[13px] font-semibold text-ink shadow-card hover:bg-sunken">
              เปิดกิจกรรม
            </Link>
          )}
          {(job.kind === 'export' || job.kind === 'shrink') && finished && (
            <Button size="sm" icon={<FileText className="size-4" aria-hidden />} onClick={() => setReportOpen(true)}>
              ดูรายงาน
            </Button>
          )}
          {canPause(job) && (
            <Button size="sm" icon={<Pause className="size-4" aria-hidden />} busy={controls.pause.isPending} onClick={() => controls.pause.mutate()} data-testid="job-pause">
              หยุดชั่วคราว
            </Button>
          )}
          {canResume(job) && (
            <Button size="sm" variant="outline" icon={<Play className="size-4" aria-hidden />} busy={controls.resume.isPending} onClick={() => controls.resume.mutate()} data-testid="job-resume">
              ทำต่อ
            </Button>
          )}
          {canRetryFailed(job) && (
            <Button size="sm" variant="outline" icon={<RotateCcw className="size-4" aria-hidden />} busy={controls.retry.isPending} onClick={() => controls.retry.mutate()} data-testid="job-retry">
              ลองใหม่เฉพาะที่ผิดพลาด
            </Button>
          )}
          {canCancel(job) && (
            <Button size="sm" variant="danger" icon={<Square className="size-3.5" aria-hidden />} onClick={() => setConfirmCancel(true)} data-testid="job-cancel">
              ยกเลิกงานนี้
            </Button>
          )}
        </div>
      </div>

      {job.errorSummary.length > 0 && (
        <div className="border-t border-line px-5 py-2.5">
          <button type="button" aria-expanded={expanded} onClick={() => setExpanded((v) => !v)} className="flex w-full items-center gap-2 rounded-lg py-1 text-left text-sm font-semibold text-warn">
            <TriangleAlert className="size-4 shrink-0" aria-hidden />
            <span className="flex-1">{job.errorSummary.map((item) => `${errorCodeLabel(item.code)} ${formatCount(item.count)} รายการ`).join(' · ')}</span>
            <ChevronDown className={cn('size-4 shrink-0 transition-transform', expanded && 'rotate-180')} aria-hidden />
          </button>
          {expanded && <JobErrors job={job} />}
        </div>
      )}

      <ConfirmDialog
        open={confirmCancel}
        onOpenChange={setConfirmCancel}
        title={isModel ? 'ยกเลิกการดาวน์โหลด?' : 'ยกเลิกส่วนที่เหลือ?'}
        description={
          isModel
            ? 'ตัววิเคราะห์ใบหน้าจะยังไม่ถูกติดตั้ง กดดาวน์โหลดใหม่ได้ภายหลัง รูปและชื่อบุคคลในคลังไม่กระทบ'
            : job.kind === 'shrink'
              ? 'รูปที่ย่อเสร็จแล้วยังเป็นไฟล์ที่ย่อ (ต้นฉบับขนาดเต็มอยู่ในถังขยะ) รูปที่เหลือไม่ถูกแตะ'
              : job.kind === 'cull'
                ? 'รูปที่ดูแล้วจะยังใช้คัดได้ รูปที่เหลือยังไม่ถูกคัด สั่งให้ AI ช่วยคัดใหม่เมื่อไรก็ทำต่อจากเดิม'
                : 'ข้อมูลที่ทำสำเร็จแล้วจะยังอยู่ การยกเลิกงานไม่ใช่การนำกิจกรรมออกจากคลัง และรูปต้นฉบับไม่ถูกแก้ไข'
        }
        confirmLabel={isModel ? 'ยกเลิกการดาวน์โหลด' : 'ยกเลิกส่วนที่เหลือ'}
        cancelLabel={isModel ? 'ดาวน์โหลดต่อ' : 'ทำงานต่อ'}
        tone="danger"
        busy={controls.cancel.isPending}
        onConfirm={() => controls.cancel.mutate(undefined, { onSettled: () => setConfirmCancel(false) })}
      />
      {job.kind === 'export' && <ExportReportDialog jobId={job.id} open={reportOpen} onOpenChange={setReportOpen} />}
      {job.kind === 'shrink' && <ShrinkReportDialog jobId={job.id} open={reportOpen} onOpenChange={setReportOpen} />}
    </li>
  );
}

function JobErrors({ job }: { job: JobSnapshot }) {
  const errors = useQuery({ queryKey: [...qk.jobErrors(job.id), job.counts.failed], queryFn: () => call('list_job_errors', { jobId: job.id }) });
  if (errors.isPending) return <LoadingBlock label="กำลังโหลดรายการที่ผิดพลาด…" className="py-6" />;
  if (errors.isError) return <ErrorState compact error={errors.error} onRetry={() => void errors.refetch()} />;
  if (errors.data.length === 0) return <p className="py-3 text-sm text-muted">ไม่มีรายการผิดพลาดค้างอยู่</p>;
  return (
    <ul className="scroll-thin mt-1.5 mb-2 max-h-64 divide-y divide-line overflow-y-auto rounded-xl border border-line" data-testid="job-errors">
      {errors.data.map((item, index) => (
        <li key={`${item.photoId ?? 'x'}-${index}`} className="flex flex-wrap items-baseline gap-x-3 px-3 py-2 text-[13.5px]">
          <span className="min-w-0 flex-1 truncate font-medium text-ink">{item.filename}</span>
          <span className="shrink-0 font-semibold text-warn">{errorCodeLabel(item.code)}</span>
          <span className="w-full text-[12.5px] text-muted">{item.message}</span>
        </li>
      ))}
    </ul>
  );
}

function ExportReportDialog({ jobId, open, onOpenChange }: { jobId: string; open: boolean; onOpenChange: (open: boolean) => void }) {
  const report = useQuery({ queryKey: ['export-report', jobId], queryFn: () => call('get_export_report', { jobId }), enabled: open });
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="รายงานการส่งออกสำเนา" size="lg" footer={<Button onClick={() => onOpenChange(false)}>ปิด</Button>}>
      {report.isPending ? (
        <LoadingBlock label="กำลังโหลดรายงาน…" className="py-10" />
      ) : report.isError ? (
        <ErrorState compact error={report.error} onRetry={() => void report.refetch()} />
      ) : (
        <ExportReportView report={report.data} />
      )}
    </Dialog>
  );
}

function ShrinkReportDialog({ jobId, open, onOpenChange }: { jobId: string; open: boolean; onOpenChange: (open: boolean) => void }) {
  const report = useQuery({ queryKey: ['shrink-report', jobId], queryFn: () => call('get_shrink_report', { jobId }), enabled: open });
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="รายงานการย่อไฟล์ต้นฉบับ" size="lg" footer={<Button onClick={() => onOpenChange(false)}>ปิด</Button>}>
      {report.isPending ? (
        <LoadingBlock label="กำลังโหลดรายงาน…" className="py-10" />
      ) : report.isError ? (
        <ErrorState compact error={report.error} onRetry={() => void report.refetch()} />
      ) : (
        <ShrinkReportView report={report.data} />
      )}
    </Dialog>
  );
}

/** แถบความคืบหน้าสั้นของงานที่ยังทำอยู่ ใช้ในหน้าหลักและหน้ากิจกรรม: ไม่กั้นการใช้รูปที่พร้อมแล้ว */
export function JobStrip({ job }: { job: JobSnapshot }) {
  const processed = job.counts.done + job.counts.failed + job.counts.skipped;
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-card border border-line bg-surface px-5 py-3 shadow-card" data-testid="job-strip">
      <p className="shrink-0 text-[15px] font-bold text-ink" aria-live="off">
        {job.state === 'paused' ? `พักไว้ — ${jobProgressText(job)}` : jobProgressText(job)}
      </p>
      <div className="min-w-[140px] flex-1">
        <ProgressBar value={processed} max={job.counts.total} indeterminate={!job.totalKnown && job.state === 'running'} label={`ความคืบหน้าของ ${job.title}`} />
      </div>
      <p className="shrink-0 text-[13.5px] text-muted">
        {job.state === 'pausing' ? 'กำลังหยุดหลังรายการปัจจุบัน' : job.kind === 'model' ? 'ระหว่างนี้ใช้แอปได้ตามปกติ' : 'ใช้งานรูปที่พร้อมได้ทันที'}
      </p>
      <Link
        to={`/activity?job=${job.id}`}
        className="inline-flex h-8 shrink-0 items-center rounded-lg border border-action/60 bg-surface px-3.5 text-[13px] font-semibold text-action hover:bg-brand-soft"
        data-testid="job-strip-open"
      >
        ดูงาน
      </Link>
    </div>
  );
}
