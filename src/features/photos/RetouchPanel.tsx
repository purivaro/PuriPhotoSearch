import { useEffect, useState } from 'react';
import { Download, PersonStanding, Smile } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Feedback';
import { cn } from '@/lib/cn';
import { formatCount } from '@/lib/format';
import type { FaceBox, Retouch } from '@/lib/ipc/types';
import { SliderRow } from './AdjustControls';
import { NO_RETOUCH, RETOUCH_ITEMS, RETOUCH_LEVELS, bodyAmount, faceBoxes, retouchIsActive, type RetouchKey } from './retouch';
import { bodyInstalling, bodyReady, missingForBody, type SubjectMapState } from './useSubjectMap';

interface RetouchPanelProps {
  retouch: Retouch | null;
  faces: FaceBox[];
  onChange: (retouch: Retouch | null) => void;
  /** ตัวแยกตัวคน/ตัวเติมฉากหลัง (ตัวผอม) */
  subject: SubjectMapState;
}

/** ปุ่มระดับ ไม่/เบา/กลาง/มาก */
function Levels({ label, value, onPick, testId, disabled }: { label: string; value: number; onPick: (v: number) => void; testId: string; disabled?: boolean }) {
  return (
    <div className="flex shrink-0 gap-1" role="group" aria-label={`ระดับ${label}`}>
      {RETOUCH_LEVELS.map((level) => (
        <button
          key={level.label}
          type="button"
          onClick={() => onPick(level.value)}
          disabled={disabled}
          aria-pressed={value === level.value}
          className={cn(
            'h-7 rounded-full border px-2.5 text-[12px] font-semibold transition-colors disabled:cursor-not-allowed disabled:opacity-50',
            value === level.value ? 'border-action bg-action text-white' : 'border-line-strong bg-surface text-ink enabled:hover:border-action enabled:hover:text-action',
          )}
          data-testid={testId}
        >
          {level.label}
        </button>
      ))}
    </div>
  );
}

/**
 * รีทัชใบหน้า (DECISIONS ข้อ 31): ผิวเนียน ลดริ้วรอย หน้าเรียว หน้าผอม — ทุกใบหน้าของรูป เลือกระดับหรือเลื่อนแถบละเอียด
 * · รูปร่าง (ข้อ 41): ตัวผอม — ทุกคนในรูป ใช้ตัวแยกตัวคน + ตัวเติมฉากหลัง (ดาวน์โหลดครั้งเดียว)
 */
export function RetouchPanel({ retouch, faces, onChange, subject }: RetouchPanelProps) {
  const current = retouch ?? NO_RETOUCH;
  // ใช้ใบหน้าของรูปตอนนี้ทุกครั้งที่ปรับ (ใบหน้าที่เพิ่ง/ลบในตัวดูภาพก็ตามทัน — ตัวผอมใช้หาคอของแต่ละคน)
  const set = (key: RetouchKey | 'body', value: number) => {
    const next = { ...current, faces: faceBoxes(faces), [key]: value };
    onChange(retouchIsActive(next) ? next : null);
  };

  return (
    <div className="space-y-5">
      <section className="space-y-3" data-testid="retouch-panel">
        <h3 className="flex items-center gap-1.5 text-sm font-semibold text-ink">
          <Smile className="size-4 text-muted" aria-hidden /> รีทัชใบหน้า
          {faces.length > 0 && <span className="font-normal text-muted">· {formatCount(faces.length)} คน</span>}
        </h3>
        {faces.length === 0 ? (
          <p className="text-[12.5px] text-muted">ยังไม่มีใบหน้าในรูปนี้ — ให้แอปวิเคราะห์ใบหน้า หรือเพิ่มกรอบใบหน้าเองในตัวดูภาพก่อน</p>
        ) : (
          <>
            {RETOUCH_ITEMS.map(({ key, label, description }) => (
              <div key={key} className="space-y-1.5" data-testid={`retouch-${key}`}>
                <div className="flex items-center justify-between gap-2">
                  <span className="text-[13px] font-semibold text-ink">{label}</span>
                  <Levels label={label} value={current[key]} onPick={(v) => set(key, v)} testId={`retouch-${key}-level`} />
                </div>
                <p className="text-[12px] text-muted">{description}</p>
                <SliderRow
                  id={`retouch-${key}-slider`}
                  label="ปรับละเอียด"
                  value={current[key]}
                  min={0}
                  max={100}
                  step={1}
                  defaultValue={0}
                  format={(v) => formatCount(v)}
                  onChange={(v) => set(key, v)}
                />
              </div>
            ))}
            <p className="text-[12px] text-muted">ใช้กับทุกใบหน้าในรูป · หน้าเรียว/หน้าผอมดัดเฉพาะขอบหน้า ตา จมูก ปากอยู่ที่เดิม</p>
          </>
        )}
      </section>
      <BodySection value={bodyAmount(current)} onChange={(v) => set('body', v)} subject={subject} />
    </div>
  );
}

/** ตัวผอม: ยังไม่มีตัวแยกตัวคน/ตัวเติมฉากหลัง = ถามดาวน์โหลดก่อน แล้วตั้งระดับที่เลือกไว้ให้เองเมื่อเสร็จ */
function BodySection({ value, onChange, subject }: { value: number; onChange: (v: number) => void; subject: SubjectMapState }) {
  const status = subject.status;
  const ready = bodyReady(status);
  const unsupported = !!status && (status.state === 'unsupported' || status.fill.state === 'unsupported');
  // ระดับที่เลือกไว้ระหว่างรอดาวน์โหลด
  const [want, setWant] = useState<number | null>(null);
  const pick = (v: number) => {
    if (ready || v === 0) {
      setWant(null);
      onChange(v);
    } else {
      setWant(v);
    }
  };
  useEffect(() => {
    if (ready && want !== null) {
      onChange(want);
      setWant(null);
    }
    // onChange เปลี่ยนทุกครั้งที่วาดใหม่ — รอเฉพาะตอนพร้อม
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, want]);

  return (
    <section className="space-y-3 border-t border-line pt-4" data-testid="body-panel">
      <h3 className="flex items-center gap-1.5 text-sm font-semibold text-ink">
        <PersonStanding className="size-4 text-muted" aria-hidden /> รูปร่าง
      </h3>
      <div className="space-y-1.5" data-testid="retouch-body">
        <div className="flex items-center justify-between gap-2">
          <span className="text-[13px] font-semibold text-ink">ตัวผอม</span>
          <Levels label="ตัวผอม" value={want ?? value} onPick={pick} testId="retouch-body-level" disabled={!status || unsupported} />
        </div>
        <p className="text-[12px] text-muted">ลำตัวแคบลงทุกคนในรูป หัว แขนที่ยื่นออกไป และฉากหลังอยู่ที่เดิม ตรงที่ตัวหดเข้าไปแอปเติมฉากหลังให้</p>
        {unsupported ? (
          <p className="text-[12.5px] text-muted" data-testid="body-unsupported">
            {status.state === 'unsupported' ? status.reason : status.fill.reason}
          </p>
        ) : ready ? (
          <SliderRow
            id="retouch-body-slider"
            label="ปรับละเอียด"
            value={value}
            min={0}
            max={100}
            step={1}
            defaultValue={0}
            format={(v) => formatCount(v)}
            onChange={onChange}
          />
        ) : (
          want !== null && <BodyInstall subject={subject} onCancel={() => setWant(null)} />
        )}
      </div>
    </section>
  );
}

/** ถามดาวน์โหลดสิ่งที่ตัวผอมใช้ (เฉพาะตัวที่ยังไม่มี) */
function BodyInstall({ subject, onCancel }: { subject: SubjectMapState; onCancel: () => void }) {
  const status = subject.status;
  if (!status) return null;
  const missing = missingForBody(status);
  const size = missing.reduce((sum, m) => sum + m.downloadSizeMb, 0);
  if (bodyInstalling(status)) {
    return (
      <div className="flex items-start gap-2 rounded-xl border border-line bg-sunken/60 p-3 text-[13px] text-ink" data-testid="body-installing">
        <Spinner className="mt-0.5 size-4 shrink-0" />
        <span>กำลังดาวน์โหลด ({formatCount(size)} MB) — แต่งส่วนอื่นได้ระหว่างรอ เสร็จแล้วแอปตั้งตัวผอมตามที่เลือกไว้ให้เอง</span>
      </div>
    );
  }
  return (
    <div className="space-y-2 rounded-xl border border-line bg-sunken/60 p-3" data-testid="body-install">
      <p className="text-sm font-semibold text-ink">ตัวผอมต้องดาวน์โหลดก่อน (ครั้งเดียว)</p>
      <ul className="space-y-1.5 text-[12.5px] text-muted">
        {missing.map((m) => (
          <li key={m.modelName}>
            <span className="font-semibold text-ink">{m === status ? 'ตัวแยกตัวคน' : 'ตัวเติมฉากหลัง'}</span> · {m.modelName} {formatCount(m.downloadSizeMb)} MB ·{' '}
            {m.licenseNote}
          </li>
        ))}
      </ul>
      <p className="text-[12px] text-muted">ทำงานบนเครื่องนี้ทั้งหมด ไม่ส่งรูปออกไปไหน · ครั้งแรกของแต่ละรูปใช้เวลาราว 10 วินาที</p>
      <div className="flex gap-1.5">
        <Button size="sm" variant="primary" icon={<Download className="size-4" aria-hidden />} onClick={subject.installBody} data-testid="body-install-start">
          ดาวน์โหลด ({formatCount(size)} MB)
        </Button>
        <Button size="sm" onClick={onCancel}>
          ไว้ทีหลัง
        </Button>
      </div>
    </div>
  );
}
