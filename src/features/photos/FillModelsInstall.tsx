import { Download } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Feedback';
import { formatCount } from '@/lib/format';
import { bodyInstalling, missingForBody, missingForParts, partsInstalling, type SubjectMapState } from './useSubjectMap';

interface FillModelsInstallProps {
  subject: SubjectMapState;
  onCancel: () => void;
  /** หัวข้อของการ์ด เช่น "ตัวผอมต้องดาวน์โหลดก่อน (ครั้งเดียว)" */
  title: string;
  /** สิ่งที่แอปทำให้เองเมื่อดาวน์โหลดเสร็จ */
  after: string;
  /** คำนำหน้า data-testid: `${testId}-install` `${testId}-installing` `${testId}-install-start` */
  testId: string;
  /** ชุดโมเดล: ตัวแยกตัวคน + ตัวเติมฉากหลัง (ค่าเริ่มต้น) หรือ ตัวแยกตัวคน + ตัวแยกส่วนของคน */
  kind?: 'fill' | 'parts';
}

/**
 * ถามดาวน์โหลดตัวแยกตัวคน + ตัวเติมฉากหลัง (เฉพาะตัวที่ยังไม่มี) — ตัวผอม (ข้อ 41) และลบคน/สิ่งของ (ข้อ 42) ใช้คู่นี้
 * ระหว่างดาวน์โหลดแต่งส่วนอื่นได้
 */
export function FillModelsInstall({ subject, onCancel, title, after, testId, kind = 'fill' }: FillModelsInstallProps) {
  const status = subject.status;
  if (!status) return null;
  const missing = kind === 'fill' ? missingForBody(status) : missingForParts(status);
  const size = missing.reduce((sum, m) => sum + m.downloadSizeMb, 0);
  const label = (m: typeof status.fill) => (m === status ? 'ตัวแยกตัวคน' : m === status.fill ? 'ตัวเติมฉากหลัง' : 'ตัวแยกส่วนของคน');
  if (kind === 'fill' ? bodyInstalling(status) : partsInstalling(status)) {
    return (
      <div className="flex items-start gap-2 rounded-xl border border-line bg-sunken/60 p-3 text-[13px] text-ink" data-testid={`${testId}-installing`}>
        <Spinner className="mt-0.5 size-4 shrink-0" />
        <span>
          กำลังดาวน์โหลด ({formatCount(size)} MB) — แต่งส่วนอื่นได้ระหว่างรอ {after}
        </span>
      </div>
    );
  }
  return (
    <div className="space-y-2 rounded-xl border border-line bg-sunken/60 p-3" data-testid={`${testId}-install`}>
      <p className="text-sm font-semibold text-ink">{title}</p>
      <ul className="space-y-1.5 text-[12.5px] text-muted">
        {missing.map((m) => (
          <li key={label(m)}>
            <span className="font-semibold text-ink">{label(m)}</span> · {m.modelName} {formatCount(m.downloadSizeMb)} MB ·{' '}
            {m.licenseNote}
          </li>
        ))}
      </ul>
      <p className="text-[12px] text-muted">ทำงานบนเครื่องนี้ทั้งหมด ไม่ส่งรูปออกไปไหน · ครั้งแรกของแต่ละรูปใช้เวลาราว 10 วินาที</p>
      <div className="flex gap-1.5">
        <Button
          size="sm"
          variant="primary"
          icon={<Download className="size-4" aria-hidden />}
          onClick={kind === 'fill' ? subject.installBody : subject.installParts}
          data-testid={`${testId}-install-start`}
        >
          ดาวน์โหลด ({formatCount(size)} MB)
        </Button>
        <Button size="sm" onClick={onCancel}>
          ไว้ทีหลัง
        </Button>
      </div>
    </div>
  );
}
