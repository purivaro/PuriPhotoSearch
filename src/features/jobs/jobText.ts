import { formatCount } from '@/lib/format';
import type { ErrorCode, JobSnapshot, JobState } from '@/lib/ipc/types';

export function jobKindLabel(job: JobSnapshot): string {
  if (job.kind === 'model') return 'ดาวน์โหลดตัววิเคราะห์ใบหน้า';
  return job.kind === 'export' ? 'ส่งออกสำเนา' : 'เพิ่มรูปเข้าคลัง';
}

const STATE_LABEL: Record<JobState, string> = {
  queued: 'รอคิว',
  running: 'กำลังทำ',
  pausing: 'กำลังหยุดหลังรายการปัจจุบัน',
  paused: 'พักไว้',
  blocked: 'รอแก้ไขก่อนทำต่อ',
  completed: 'เสร็จแล้ว',
  completed_with_errors: 'เสร็จบางส่วน',
  cancel_requested: 'กำลังยกเลิก',
  cancelled: 'ยกเลิกแล้ว',
  failed: 'ทำไม่สำเร็จ',
};

export function jobStateLabel(state: JobState): string {
  return STATE_LABEL[state];
}

export function jobStateTone(state: JobState): 'neutral' | 'ok' | 'warn' | 'danger' | 'brand' | 'info' {
  switch (state) {
    case 'running':
      return 'brand';
    case 'completed':
      return 'ok';
    case 'completed_with_errors':
    case 'blocked':
    case 'pausing':
    case 'cancel_requested':
      return 'warn';
    case 'failed':
      return 'danger';
    default:
      return 'neutral';
  }
}

/** ขั้นตอนปัจจุบันเป็นภาษาผู้ใช้ */
export function jobStageLabel(job: JobSnapshot): string {
  if (job.kind === 'model') return job.stage === 'verify' ? 'ตรวจและติดตั้งไฟล์' : job.stage === 'download' ? 'ดาวน์โหลด' : 'เสร็จขั้นตอน';
  if (job.kind === 'export') return 'คัดลอกไฟล์ต้นฉบับ';
  if (job.stage === 'discovery') return 'ตรวจรายการ';
  if (job.stage === 'thumbnail') return 'เตรียมข้อมูลรูป';
  return 'เสร็จขั้นตอน';
}

/** ข้อความความคืบหน้า: ระหว่างตรวจรายการยังไม่รู้ยอดรวม จึงบอกแค่ "พบแล้ว" */
export function jobProgressText(job: JobSnapshot): string {
  const { total, done, failed } = job.counts;
  if (job.kind === 'model') {
    // counts ของงานนี้เป็น MB
    if (job.stage === 'verify') return 'ดาวน์โหลดครบแล้ว กำลังตรวจและติดตั้งไฟล์';
    if (!job.totalKnown || total === 0) return done > 0 ? `ดาวน์โหลดแล้ว ${formatCount(done)} MB` : 'กำลังเริ่มดาวน์โหลด';
    return `ดาวน์โหลดแล้ว ${formatCount(done)} จาก ${formatCount(total)} MB`;
  }
  if (!job.totalKnown) return `กำลังตรวจรายการ พบแล้ว ${formatCount(total)} ไฟล์`;
  const processed = done + failed + job.counts.skipped;
  const unit = job.kind === 'export' ? 'ไฟล์' : 'รูป';
  const head = job.kind === 'export' ? 'คัดลอกแล้ว' : 'เตรียมข้อมูลรูป';
  return `${head} ${formatCount(processed)} / ${formatCount(total)} ${unit}`;
}

/** สรุปผลเมื่องานจบ ไม่ใช้คำว่าสำเร็จเมื่อมีรายการล้มเหลว */
export function jobResultText(job: JobSnapshot): string {
  const { done, failed, total } = job.counts;
  if (job.kind === 'model') {
    if (job.state === 'cancelled') return 'ยกเลิกการดาวน์โหลดแล้ว ยังไม่ได้ติดตั้งตัววิเคราะห์ใบหน้า';
    if (job.state === 'failed') return job.blockedReason || 'ดาวน์โหลดไม่สำเร็จ ลองใหม่ได้';
    return 'ติดตั้งตัววิเคราะห์ใบหน้าแล้ว';
  }
  const unit = job.kind === 'export' ? 'ไฟล์' : 'รูป';
  if (job.state === 'cancelled') return `ยกเลิกแล้ว ทำสำเร็จไว้ ${formatCount(done)} จาก ${formatCount(total)} ${unit}`;
  if (failed > 0) return `ทำสำเร็จ ${formatCount(done)} รายการ มี ${formatCount(failed)} รายการที่ต้องแก้ไข`;
  return `ทำสำเร็จ ${formatCount(done)} ${unit}`;
}

const ERROR_LABEL: Partial<Record<ErrorCode, string>> = {
  ACCESS_DENIED: 'ไม่มีสิทธิ์อ่านไฟล์',
  SOURCE_UNAVAILABLE: 'ไม่พบไฟล์ต้นทาง',
  UNSUPPORTED_FORMAT: 'ชนิดไฟล์ยังไม่รองรับ',
  DECODE_FAILED: 'ไฟล์รูปเสียหรืออ่านไม่ได้',
  DISK_FULL: 'พื้นที่ดิสก์ไม่พอ',
  REVISION_CHANGED: 'ไฟล์ถูกแก้ระหว่างทำงาน',
  MODEL_NOT_READY: 'ยังไม่ได้ตั้งค่าการวิเคราะห์ใบหน้า',
  CANCELLED: 'ถูกยกเลิก',
};

export function errorCodeLabel(code: ErrorCode): string {
  return ERROR_LABEL[code] ?? 'ข้อผิดพลาดอื่น';
}

export function canPause(job: JobSnapshot): boolean {
  // การดาวน์โหลดพักกลางทางไม่ได้: ยกเลิกแล้วเริ่มใหม่แทน
  if (job.kind === 'model') return false;
  return job.state === 'running' || job.state === 'queued';
}
export function canResume(job: JobSnapshot): boolean {
  return job.state === 'paused' || job.state === 'blocked';
}
export function canCancel(job: JobSnapshot): boolean {
  return ['queued', 'running', 'pausing', 'paused', 'blocked'].includes(job.state);
}
export function canRetryFailed(job: JobSnapshot): boolean {
  return job.kind === 'catalog' && job.counts.failed > 0 && ['completed_with_errors', 'cancelled', 'failed'].includes(job.state);
}
