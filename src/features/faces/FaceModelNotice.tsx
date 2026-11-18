import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { CircleCheck, Download, HardDrive, ScanFace, TriangleAlert, X } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { ProgressBar } from '@/components/ui/Feedback';
import { jobProgressText } from '@/features/jobs/jobText';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { FaceAnalysisStatus, Id, JobSnapshot } from '@/lib/ipc/types';
import { isJobActive, qk, useCapabilities } from '@/lib/queries';

interface FaceModelNoticeProps {
  /** card = กล่องเต็ม (หน้าเริ่มต้น/ตรวจสอบใบหน้า/ตั้งค่า), bar = แถบเดียว (หน้าหลัก/ตรวจรายการ) */
  variant?: 'card' | 'bar';
  /** แสดงบรรทัด "พร้อมใช้" เมื่อ state = ready (ปกติไม่แสดงอะไร) */
  showReady?: boolean;
  className?: string;
}

/**
 * ตัวติดตั้งตัววิเคราะห์ใบหน้าแบบคลิกเดียว (S21 ขั้นพื้นฐาน)
 * ผู้ใช้กดปุ่มเดียว แอปดาวน์โหลดโมเดลเอง แล้ววิเคราะห์รูปในคลังต่อให้ ไม่ต้องเลือกไฟล์หรือโฟลเดอร์
 * ทุกข้อความสถานะมาจาก `capabilities.faceAnalysis` — ไม่อ้างว่าพร้อมก่อน Rust ยืนยัน
 */
export function FaceModelNotice({ variant = 'card', showReady, className }: FaceModelNoticeProps) {
  const qc = useQueryClient();
  const capabilities = useCapabilities();
  const status = capabilities.data?.faceAnalysis;
  // งานที่เริ่มจากหน้านี้: ใช้บอกผลเมื่อดาวน์โหลดไม่สำเร็จหลังสถานะกลับเป็น not_installed
  const [startedJobId, setStartedJobId] = useState<Id | null>(null);
  const jobId = status?.installJobId ?? startedJobId;

  const job = useQuery({
    queryKey: qk.job(jobId ?? ''),
    queryFn: () => call('get_job', { jobId: jobId as Id }),
    enabled: Boolean(jobId),
    refetchInterval: (query) => (query.state.data && !isJobActive(query.state.data) ? false : 1000),
  });

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: qk.capabilities });
    void qc.invalidateQueries({ queryKey: ['jobs'] });
  };
  const install = useMutation({
    mutationFn: () => call('install_face_model', {}),
    onSuccess: (started) => {
      setStartedJobId(started.id);
      qc.setQueryData(qk.job(started.id), started);
      refresh();
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
  const cancel = useMutation({
    mutationFn: (id: Id) => call('cancel_job', { jobId: id }),
    onSuccess: refresh,
    onError: (error) => toast.error(errorMessage(error)),
  });

  if (!status) return null;
  if (status.state === 'ready') {
    if (!showReady) return null;
    return (
      <p className={cn('flex items-center gap-2 text-sm font-semibold text-ok', className)} data-testid="face-model-ready">
        <CircleCheck className="size-[18px] shrink-0" aria-hidden />
        ตัววิเคราะห์ใบหน้าพร้อมใช้ — รูปที่เพิ่มใหม่จะถูกวิเคราะห์เองเบื้องหลัง
      </p>
    );
  }

  const shell = cn(
    'rounded-card border text-left',
    variant === 'card' ? 'border-brand-line bg-surface p-5 shadow-card' : 'border-brand-line bar-brand px-4 py-3',
    className,
  );

  if (status.state === 'unsupported') {
    return (
      <div className={cn(shell, 'border-line')} data-testid="face-model-unsupported">
        <p className="flex items-start gap-2.5 text-sm text-ink">
          <ScanFace className="mt-0.5 size-[18px] shrink-0 text-muted" aria-hidden />
          <span>
            <span className="font-semibold">เครื่องนี้ยังวิเคราะห์ใบหน้าอัตโนมัติไม่ได้</span>
            <span className="block text-muted">{status.reason} ระบุใบหน้าเองได้จากตัวดูภาพ แล้วค้นจากชื่อได้ตามปกติ</span>
          </span>
        </p>
      </div>
    );
  }

  const running = job.data && isJobActive(job.data) ? job.data : null;
  if (status.state === 'installing' || running) {
    return <Installing shell={shell} status={status} job={running ?? job.data ?? null} onCancel={(id) => cancel.mutate(id)} cancelling={cancel.isPending} compact={variant === 'bar'} />;
  }

  // not_installed
  const lastFailed = job.data && (job.data.state === 'failed' || job.data.state === 'cancelled') ? job.data : null;
  const button = (
    <Button
      variant="primary"
      size={variant === 'card' ? 'lg' : 'md'}
      icon={<Download className="size-[18px]" aria-hidden />}
      busy={install.isPending}
      onClick={() => install.mutate()}
      data-testid="face-model-install"
    >
      {lastFailed ? 'ลองดาวน์โหลดอีกครั้ง' : 'ดาวน์โหลดตัววิเคราะห์ใบหน้า'} ({formatCount(status.downloadSizeMb)} MB)
    </Button>
  );
  const failure = lastFailed && (
    <p role="alert" className="flex items-start gap-2 text-sm text-danger" data-testid="face-model-failed">
      <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
      {lastFailed.state === 'cancelled' ? 'ยกเลิกการดาวน์โหลดแล้ว ยังไม่ได้ติดตั้ง' : `ดาวน์โหลดไม่สำเร็จ${lastFailed.blockedReason ? `: ${lastFailed.blockedReason}` : ''}`}
    </p>
  );
  const details = <ModelDetails status={status} />;

  if (variant === 'bar') {
    return (
      <div className={shell} data-testid="face-model-notice">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2.5">
          <ScanFace className="size-6 shrink-0 text-action" aria-hidden />
          <div className="min-w-[220px] flex-1">
            <p className="text-[15px] font-bold text-ink">ให้แอปหาใบหน้าในรูปให้เอง</p>
            <p className="text-[13.5px] text-muted">ดาวน์โหลดครั้งเดียว แล้วแอปจะวิเคราะห์รูปในคลังและเสนอชื่อให้ตรวจ</p>
          </div>
          {button}
        </div>
        {failure && <div className="mt-2">{failure}</div>}
        <div className="mt-2">{details}</div>
      </div>
    );
  }
  return (
    <div className={shell} data-testid="face-model-notice">
      <div className="flex items-start gap-3.5">
        <span className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-brand-soft text-action">
          <ScanFace className="size-6" aria-hidden />
        </span>
        <div className="min-w-0 flex-1">
          <h3 className="text-[17px] font-bold text-ink">ให้แอปหาใบหน้าในรูปให้เอง</h3>
          <p className="mt-0.5 text-sm text-muted">
            กดปุ่มเดียว แอปจะดาวน์โหลดตัววิเคราะห์ใบหน้าครั้งเดียว แล้ววิเคราะห์รูปในคลังต่อให้เอง จากนั้นแค่ตรวจและตั้งชื่อ ไม่ต้องลากกรอบทีละหน้า
          </p>
          <div className="mt-3.5 flex flex-wrap items-center gap-3">{button}</div>
          {failure && <div className="mt-2.5">{failure}</div>}
          <div className="mt-3">{details}</div>
        </div>
      </div>
    </div>
  );
}

/** ที่มา ขนาด และเงื่อนไขการใช้ของโมเดล — ต้องอยู่ใกล้ปุ่มดาวน์โหลดเสมอ */
function ModelDetails({ status }: { status: FaceAnalysisStatus }) {
  return (
    <div className="space-y-1 text-[12.5px] leading-relaxed text-muted">
      <p>
        {status.modelName} · ดาวน์โหลดจาก {status.downloadSource} · {status.licenseNote}
      </p>
      <p className="flex items-center gap-1.5">
        <HardDrive className="size-3.5 shrink-0" aria-hidden />
        รูปของคุณไม่ถูกส่งออกจากเครื่อง การวิเคราะห์ทำในเครื่องนี้ทั้งหมด
      </p>
    </div>
  );
}

function Installing({
  shell,
  status,
  job,
  onCancel,
  cancelling,
  compact,
}: {
  shell: string;
  status: FaceAnalysisStatus;
  job: JobSnapshot | null;
  onCancel: (jobId: Id) => void;
  cancelling: boolean;
  compact: boolean;
}) {
  const verifying = job?.stage === 'verify';
  const cancelRequested = job?.state === 'cancel_requested';
  return (
    <div className={shell} data-testid="face-model-installing">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <Download className="size-5 shrink-0 text-action" aria-hidden />
        <p className="shrink-0 text-[15px] font-bold text-ink" aria-live="off" data-testid="face-model-progress">
          {cancelRequested ? 'กำลังยกเลิกการดาวน์โหลด' : job ? jobProgressText(job) : 'กำลังเริ่มดาวน์โหลด'}
        </p>
        <div className="min-w-[160px] flex-1">
          <ProgressBar
            value={job?.counts.done ?? 0}
            max={job?.counts.total ?? 0}
            indeterminate={!job || !job.totalKnown || job.counts.total === 0 || verifying}
            label={`ความคืบหน้าการดาวน์โหลด ${status.modelName}`}
          />
        </div>
        {job && !verifying && !cancelRequested && (
          <Button size="sm" icon={<X className="size-3.5" aria-hidden />} busy={cancelling} onClick={() => onCancel(job.id)} data-testid="face-model-cancel">
            ยกเลิก
          </Button>
        )}
      </div>
      {!compact && <p className="mt-2 text-[13px] text-muted">ใช้แอปต่อได้ตามปกติระหว่างดาวน์โหลด เสร็จแล้วแอปจะวิเคราะห์ใบหน้าของรูปในคลังให้เอง</p>}
    </div>
  );
}
