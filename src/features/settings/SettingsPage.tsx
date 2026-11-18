import { useMutation } from '@tanstack/react-query';
import { useSearchParams } from 'react-router';
import { CircleCheck, CircleX, DatabaseBackup, HardDrive, Info, Monitor, Moon, ScanFace, Sun, TriangleAlert } from 'lucide-react';
import { Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { Button } from '@/components/ui/Button';
import { ErrorState, InlineNotice, LoadingBlock, StatusBadge } from '@/components/ui/Feedback';
import { Tabs } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { Capabilities, LegacyReport } from '@/lib/ipc/types';
import { useCapabilities, useSummary } from '@/lib/queries';
import { useSettings, type ThemeChoice } from '@/stores/settings';

type SettingsTab = 'general' | 'library' | 'ai' | 'migration';

const CARD = 'rounded-card border border-line bg-surface p-6 shadow-card';

/** S20–S21 (ขั้นพื้นฐาน) — ตั้งค่า: แท็บรองแนวนอนลำดับเดียวกันทุกหมวด */
export function SettingsPage() {
  useDocumentTitle('ตั้งค่า');
  const [params, setParams] = useSearchParams();
  const tab = (params.get('tab') as SettingsTab | null) ?? 'general';
  const capabilities = useCapabilities();

  return (
    <Page>
      <PageHeader title="ตั้งค่า" subtitle="ข้อมูลสำหรับค้นหาเก็บแยกจากไฟล์รูปจริง" />
      <Tabs<SettingsTab>
        label="หมวดตั้งค่า"
        testId="settings-tab"
        value={tab}
        onChange={(value) => setParams(value === 'general' ? {} : { tab: value }, { replace: true })}
        items={[
          { value: 'general', label: 'ทั่วไป' },
          { value: 'library', label: 'คลังข้อมูลและสำรอง' },
          { value: 'ai', label: 'AI และความเป็นส่วนตัว' },
          { value: 'migration', label: 'ย้ายข้อมูลจากรุ่นเดิม' },
        ]}
        className="mb-5"
      />
      {capabilities.isPending ? (
        <LoadingBlock label="กำลังโหลดข้อมูลของแอป…" />
      ) : capabilities.isError ? (
        <ErrorState error={capabilities.error} title="โหลดข้อมูลของแอปไม่สำเร็จ" onRetry={() => void capabilities.refetch()} />
      ) : (
        <div className="max-w-[820px] space-y-4">
          {tab === 'general' && <GeneralTab capabilities={capabilities.data} />}
          {tab === 'library' && <LibraryTab capabilities={capabilities.data} />}
          {tab === 'ai' && <AiTab capabilities={capabilities.data} />}
          {tab === 'migration' && <MigrationTab />}
        </div>
      )}
    </Page>
  );
}

const THEMES: { value: ThemeChoice; label: string; icon: typeof Sun }[] = [
  { value: 'system', label: 'ตามระบบ', icon: Monitor },
  { value: 'light', label: 'สว่าง', icon: Sun },
  { value: 'dark', label: 'มืด', icon: Moon },
];

const SHORTCUTS: [string, string][] = [
  ['⌘ K', 'ค้นหาคน กิจกรรม หรือชื่อไฟล์จากทุกหน้า'],
  ['⌘ O', 'เพิ่มโฟลเดอร์กิจกรรม'],
  ['⌘ F', 'ไปที่ช่องค้นหาของหน้าปัจจุบัน'],
  ['Space', 'บนการ์ดบุคคล: เปิด/ปิดดูเร็ว'],
  ['← / →', 'ในตัวดูภาพ: รูปก่อนหน้า / ถัดไป'],
  ['+ / − / 0', 'ในตัวดูภาพ: ขยาย / ย่อ / พอดีหน้าต่าง'],
  ['Shift + คลิก', 'ใน grid รูป: เลือกเป็นช่วง'],
  ['⌘ A', 'ใน grid รูป: เลือกทุกรูปที่แสดงอยู่'],
  ['Esc', 'ปิดชั้นบนสุด หรือยกเลิกรูปที่เลือก (ไม่ยกเลิกงานเบื้องหลัง)'],
];

function GeneralTab({ capabilities }: { capabilities: Capabilities }) {
  const theme = useSettings((s) => s.theme);
  const setTheme = useSettings((s) => s.setTheme);
  return (
    <>
      <section className={CARD}>
        <h2 className="text-lg font-bold text-ink">การแสดงผล</h2>
        <p className="mt-0.5 text-sm text-muted">ธีมมีผลกับหน้าจอของแอปเท่านั้น ไม่เปลี่ยนสีของรูป</p>
        <div role="radiogroup" aria-label="ธีม" className="mt-3 flex flex-wrap gap-2.5">
          {THEMES.map((item) => {
            const active = theme === item.value;
            return (
              <button
                key={item.value}
                type="button"
                role="radio"
                aria-checked={active}
                onClick={() => setTheme(item.value)}
                data-testid={`theme-${item.value}`}
                className={cn(
                  'inline-flex h-11 items-center gap-2 rounded-control border px-4 text-sm font-semibold',
                  active ? 'border-brand bg-brand-soft text-action' : 'border-line-strong bg-surface text-ink hover:bg-sunken',
                )}
              >
                <item.icon className="size-[18px]" aria-hidden />
                {item.label}
              </button>
            );
          })}
        </div>
      </section>

      <section className={CARD}>
        <h2 className="text-lg font-bold text-ink">แป้นพิมพ์ลัด</h2>
        <dl className="mt-3 divide-y divide-line">
          {SHORTCUTS.map(([keys, text]) => (
            <div key={keys} className="flex items-center gap-4 py-2 text-sm">
              <dt className="w-[130px] shrink-0">
                <kbd className="rounded-md border border-line bg-sunken px-2 py-1 font-sans text-[12.5px] font-semibold text-ink">{keys}</kbd>
              </dt>
              <dd className="text-ink">{text}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section className={CARD}>
        <h2 className="text-lg font-bold text-ink">เกี่ยวกับแอปนี้</h2>
        <InfoRows
          rows={[
            ['รุ่นแอป', capabilities.appVersion],
            ['รุ่นฐานข้อมูลของคลัง', String(capabilities.schemaVersion)],
            ['ระบบที่ใช้งาน', capabilities.platform],
            ['ชนิดไฟล์รูปที่เปิดได้', capabilities.imageFormats.map((f) => `.${f}`).join('  ') || 'ไม่มี'],
          ]}
        />
        <p className="mt-2 text-[13px] text-muted">ไฟล์ภาพชนิดอื่นจะถูกนับและรายงานว่า “ไม่รองรับ” ตอนเพิ่มโฟลเดอร์ ไม่ถูกแก้ไขหรือซ่อนหาย</p>
      </section>
    </>
  );
}

function InfoRows({ rows }: { rows: [string, string][] }) {
  return (
    <dl className="mt-3 divide-y divide-line">
      {rows.map(([label, value]) => (
        <div key={label} className="grid grid-cols-[190px_1fr] gap-3 py-2.5 text-sm">
          <dt className="font-semibold text-ink">{label}</dt>
          <dd className="min-w-0 break-all text-ink select-text" data-selectable>
            {value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function LibraryTab({ capabilities }: { capabilities: Capabilities }) {
  const summary = useSummary();
  return (
    <>
      <section className={CARD}>
        <h2 className="flex items-center gap-2 text-lg font-bold text-ink">
          <HardDrive className="size-5 text-muted" aria-hidden /> คลังข้อมูลในเครื่องนี้
        </h2>
        {summary.isError ? (
          <ErrorState compact error={summary.error} onRetry={() => void summary.refetch()} />
        ) : (
          <InfoRows
            rows={[
              ['ชื่อคลัง', summary.data?.name ?? 'กำลังโหลด…'],
              ['ข้อมูลในคลัง', summary.data ? `${formatCount(summary.data.eventCount)} กิจกรรม · ${formatCount(summary.data.photoCount)} รูป · ${formatCount(summary.data.personCount)} บุคคล` : 'กำลังโหลด…'],
              ['ตำแหน่งฐานข้อมูล', capabilities.libraryPath],
              ['ตำแหน่งภาพย่อ (cache)', capabilities.cachePath],
            ]}
          />
        )}
        <p className="mt-2 text-[13px] text-muted">ฐานข้อมูลเก็บชื่อบุคคล การยืนยัน และข้อมูลกิจกรรม ส่วนภาพย่อสร้างใหม่ได้เมื่อโฟลเดอร์ต้นทางพร้อม — ไฟล์รูปต้นฉบับอยู่ในโฟลเดอร์เดิมเสมอ</p>
      </section>
      <InlineNotice tone="info" icon={<Info aria-hidden />}>
        <p className="font-semibold">การสำรองและกู้คืนยังไม่มีในรุ่นนี้</p>
        <p>ระหว่างนี้ หากต้องการเก็บสำเนา ให้ปิดแอปก่อนแล้วคัดลอกโฟลเดอร์ฐานข้อมูลตามตำแหน่งด้านบน สำเนานั้นไม่รวมไฟล์รูปต้นฉบับ</p>
      </InlineNotice>
    </>
  );
}

function AiTab({ capabilities }: { capabilities: Capabilities }) {
  const ai = capabilities.faceAnalysis;
  return (
    <>
      <section className={CARD}>
        <div className="flex flex-wrap items-center gap-3">
          <h2 className="flex items-center gap-2 text-lg font-bold text-ink">
            <ScanFace className="size-5 text-muted" aria-hidden /> การวิเคราะห์ใบหน้า
          </h2>
          {ai.available ? (
            <StatusBadge tone="ok" icon={<CircleCheck aria-hidden />}>
              พร้อมใช้
            </StatusBadge>
          ) : (
            <StatusBadge tone="warn" icon={<CircleX aria-hidden />}>
              ยังไม่พร้อมในรุ่นนี้
            </StatusBadge>
          )}
        </div>
        <p className="mt-2 text-sm text-ink">{ai.reason || (ai.available ? 'ตรวจใบหน้าและเสนอชื่อในเครื่องนี้' : 'รุ่นนี้ยังไม่มีการวิเคราะห์ใบหน้าอัตโนมัติ')}</p>
        {!ai.available && (
          <p className="mt-2 text-sm text-muted">การเพิ่มโฟลเดอร์ ดูรูป ค้นจากชื่อไฟล์/กิจกรรม และการระบุใบหน้าเองในตัวดูภาพใช้ได้ตามปกติ ชื่อที่ระบุเองค้นได้ทันที</p>
        )}
      </section>
      <section className={CARD}>
        <h2 className="text-lg font-bold text-ink">ความเป็นส่วนตัว</h2>
        <ul className="mt-2 list-disc space-y-1.5 pl-5 text-sm text-ink">
          <li>รูป ชื่อบุคคล และข้อมูลใบหน้าประมวลผลและเก็บในเครื่องนี้เท่านั้น ไม่มีบัญชีผู้ใช้และไม่มีการส่งข้อมูลออกทางอินเทอร์เน็ต</li>
          <li>ข้อมูลในเครื่องไม่ได้ถูกเข้ารหัสเพิ่มโดยแอป ความปลอดภัยขึ้นกับการตั้งค่าของเครื่อง</li>
          <li>แอปไม่ย้าย ไม่เปลี่ยนชื่อ ไม่แก้ไข และไม่ลบไฟล์รูปต้นฉบับ</li>
        </ul>
      </section>
    </>
  );
}

function MigrationTab() {
  const inspect = useMutation({ mutationFn: () => call('preview_legacy_import', {}) });
  return (
    <>
      <section className={CARD}>
        <h2 className="flex items-center gap-2 text-lg font-bold text-ink">
          <DatabaseBackup className="size-5 text-muted" aria-hidden /> ตรวจข้อมูลจากเวอร์ชัน Python เดิม
        </h2>
        <p className="mt-1.5 text-sm text-ink">ตรวจฐานข้อมูลของ Puri Photo Search รุ่นเดิมในเครื่องนี้แบบอ่านอย่างเดียว เพื่อดูว่ามีข้อมูลอะไรบ้าง ฐานเดิมไม่ถูกแก้ไข</p>
        <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />} className="mt-3">
          รุ่นนี้ทำได้เฉพาะการตรวจและรายงาน (dry-run) ยังนำข้อมูลเข้าคลังใหม่ไม่ได้ รายชื่อและข้อมูลในรุ่นเดิมยังอยู่ครบและเปิดด้วยโปรแกรมเดิมได้ตามปกติ
        </InlineNotice>
        <Button variant="primary" className="mt-4" busy={inspect.isPending} onClick={() => inspect.mutate()} data-testid="legacy-inspect">
          {inspect.data ? 'ตรวจอีกครั้ง' : 'ตรวจข้อมูลเดิม (อ่านอย่างเดียว)'}
        </Button>
        {inspect.isError && (
          <InlineNotice tone="danger" icon={<TriangleAlert aria-hidden />} className="mt-3">
            {errorMessage(inspect.error)}
          </InlineNotice>
        )}
      </section>
      {inspect.data && <LegacyReportView report={inspect.data} />}
    </>
  );
}

function LegacyReportView({ report }: { report: LegacyReport }) {
  if (!report.recognized) {
    return (
      <section className={CARD} data-testid="legacy-report">
        <h2 className="text-lg font-bold text-ink">ไม่พบฐานข้อมูลรุ่นเดิมที่อ่านได้</h2>
        <p className="mt-1 text-sm break-all text-muted select-text" data-selectable>
          ตำแหน่งที่ตรวจ: {report.path}
        </p>
        <NotesList notes={report.notes} />
      </section>
    );
  }
  const stats: [string, number, string?][] = [
    ['บุคคล', report.persons, report.duplicateNames > 0 ? `ชื่อซ้ำ ${formatCount(report.duplicateNames)} ชื่อ` : undefined],
    ['กลุ่ม', report.groups],
    ['ใบหน้าตัวอย่างของบุคคล', report.referenceSamples],
    ['โฟลเดอร์กิจกรรม', report.eventFolders],
    ['รูป', report.photos, report.photosMissingOnDisk > 0 ? `ไม่พบไฟล์ในเครื่อง ${formatCount(report.photosMissingOnDisk)} รูป` : undefined],
    ['ใบหน้าที่ตรวจพบ', report.faces, report.embeddingDim ? `เวกเตอร์ ${report.embeddingDim} มิติ` : undefined],
    ['คู่ที่ผู้ใช้ปฏิเสธ', report.rejections],
    ['ใบหน้าที่ผู้ใช้ทิ้ง', report.dismissedFaces],
  ];
  return (
    <section className={CARD} data-testid="legacy-report">
      <div className="flex flex-wrap items-center gap-2.5">
        <h2 className="text-lg font-bold text-ink">ผลการตรวจ</h2>
        <StatusBadge tone="info">อ่านอย่างเดียว · ยังไม่ได้นำเข้า</StatusBadge>
      </div>
      <p className="mt-1 text-[13px] break-all text-muted select-text" data-selectable>
        {report.path}
      </p>
      <dl className="mt-4 grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-4">
        {stats.map(([label, value, note]) => (
          <div key={label}>
            <dt className="text-[13px] text-muted">{label}</dt>
            <dd className="text-xl font-bold text-ink">{formatCount(value)}</dd>
            {note && <dd className="text-[12.5px] text-warn">{note}</dd>}
          </div>
        ))}
      </dl>
      <NotesList notes={report.notes} />
    </section>
  );
}

function NotesList({ notes }: { notes: string[] }) {
  if (notes.length === 0) return null;
  return (
    <div className="mt-4 border-t border-line pt-3">
      <h3 className="text-sm font-bold text-ink">ข้อสังเกตจากการตรวจ</h3>
      <ul className="mt-1.5 list-disc space-y-1 pl-5 text-sm text-ink">
        {notes.map((note) => (
          <li key={note}>{note}</li>
        ))}
      </ul>
    </div>
  );
}
