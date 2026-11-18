import { IpcError } from './ipc/client';

const TH_MONTHS_SHORT = ['ม.ค.', 'ก.พ.', 'มี.ค.', 'เม.ย.', 'พ.ค.', 'มิ.ย.', 'ก.ค.', 'ส.ค.', 'ก.ย.', 'ต.ค.', 'พ.ย.', 'ธ.ค.'];

const numberFormat = new Intl.NumberFormat('en-US');

/** 12375 → "12,375" */
export function formatCount(n: number): string {
  return numberFormat.format(n);
}

/** แยก `YYYY-MM-DD` โดยไม่ผ่าน Date (กันวันเลื่อนจาก timezone) */
export function parseIsoDate(iso: string): { y: number; m: number; d: number } | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!match) return null;
  const y = Number(match[1]);
  const m = Number(match[2]);
  const d = Number(match[3]);
  if (m < 1 || m > 12 || d < 1 || d > 31) return null;
  return { y, m, d };
}

/** "2026-11-18" → "18 พ.ย. 2569" (แสดงปี พ.ศ.; ข้อมูลเก็บเป็น ค.ศ.) */
export function formatThaiDate(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const p = parseIsoDate(iso);
  if (!p) return null;
  return `${p.d} ${TH_MONTHS_SHORT[p.m - 1]} ${p.y + 543}`;
}

/** "2026-11-18T09:05:00" → "18 พ.ย. 2569 09:05" (เวลาท้องถิ่นจาก EXIF ไม่แปลง timezone) */
export function formatTakenAt(local: string | null | undefined): string | null {
  if (!local) return null;
  const date = formatThaiDate(local);
  if (!date) return null;
  const time = /T(\d{2}):(\d{2})/.exec(local);
  return time ? `${date} ${time[1]}:${time[2]}` : date;
}

/** เวลา UTC (ISO-8601) ของงานเบื้องหลัง → เวลาท้องถิ่นของเครื่อง */
export function formatInstant(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  const hh = String(date.getHours()).padStart(2, '0');
  const mm = String(date.getMinutes()).padStart(2, '0');
  return `${date.getDate()} ${TH_MONTHS_SHORT[date.getMonth()]} ${date.getFullYear() + 543} ${hh}:${mm}`;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 KB';
  const units = ['KB', 'MB', 'GB', 'TB'];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value >= 100 || unit === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

/** ตัดกลาง path ยาวให้เห็นทั้งต้นและปลาย */
export function middleEllipsis(text: string, max = 56): string {
  if (text.length <= max) return text;
  const keep = max - 1;
  const head = Math.ceil(keep * 0.4);
  const tail = keep - head;
  return `${text.slice(0, head)}…${text.slice(text.length - tail)}`;
}

/** ข้อความภาษาไทยสำหรับแสดงผู้ใช้จาก error ใด ๆ */
export function errorMessage(error: unknown): string {
  if (error instanceof IpcError) return error.message;
  return 'เกิดข้อผิดพลาดที่ไม่คาดคิด กรุณาลองใหม่อีกครั้ง';
}

export function isErrorCode(error: unknown, code: IpcError['code']): boolean {
  return error instanceof IpcError && error.code === code;
}

export function newRequestId(): string {
  return crypto.randomUUID();
}
