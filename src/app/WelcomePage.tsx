import { Link } from 'react-router';
import { DatabaseBackup, FolderPlus, HardDrive, Info, Search, UserRoundPen } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { InlineNotice } from '@/components/ui/Feedback';
import { FaceModelNotice } from '@/features/faces/FaceModelNotice';
import { useCapabilities, useSummary } from '@/lib/queries';
import { LogoMark } from './Logo';
import { useDocumentTitle } from './PageLayout';
import { useStartImport } from './navigation';

const STEPS = [
  { icon: FolderPlus, title: 'เพิ่มโฟลเดอร์กิจกรรม', text: 'เลือกโฟลเดอร์งานที่เก็บรูปไว้อยู่แล้ว ไม่ต้องย้ายหรือจัดไฟล์ใหม่' },
  { icon: UserRoundPen, title: 'ตั้งชื่อคนในรูป', text: 'แอปหาใบหน้าและจัดกลุ่มให้ บอกครั้งเดียวว่าเป็นใคร ใช้ได้กับทุกกิจกรรม' },
  { icon: Search, title: 'ค้นรูปจากชื่อ', text: 'พิมพ์ชื่อแล้วเห็นรูปจากทุกงาน เปิดต้นฉบับหรือส่งออกสำเนาได้ทันที' },
];

/** S01 — เริ่มต้นใช้งาน: ไปถึงการเลือกโฟลเดอร์ได้จากปุ่มหลักปุ่มเดียว */
export function WelcomePage() {
  useDocumentTitle('เริ่มต้นใช้งาน');
  const startImport = useStartImport();
  const capabilities = useCapabilities();
  const summary = useSummary();
  const hasLibrary = (summary.data?.eventCount ?? 0) > 0;

  return (
    <div className="scroll-thin flex h-full flex-col overflow-y-auto bg-canvas">
      <div data-tauri-drag-region className="h-11 shrink-0" />
      <div className="mx-auto flex w-full max-w-[880px] flex-1 flex-col items-center justify-center px-8 pb-12 text-center">
        <LogoMark className="size-20" />
        <p className="mt-4 text-[13px] font-bold tracking-[0.22em] text-muted">PURI PHOTO SEARCH</p>
        <h1 className="mt-3 text-[34px] leading-tight font-extrabold tracking-tight text-ink">
          ค้นหารูปคนที่ต้องการ
          <br />
          จากกิจกรรมในเครื่อง
        </h1>

        <ol className="mt-9 grid w-full gap-4 text-left sm:grid-cols-3">
          {STEPS.map((step, index) => (
            <li key={step.title} className="rounded-card border border-line bg-surface p-5 shadow-card">
              <div className="flex items-center gap-3">
                <span className="flex size-10 items-center justify-center rounded-xl bg-brand-soft text-action">
                  <step.icon className="size-5" aria-hidden />
                </span>
                <span className="text-[13px] font-bold text-muted">ขั้นที่ {index + 1}</span>
              </div>
              <h2 className="mt-3 text-[17px] font-bold text-ink">{step.title}</h2>
              <p className="mt-1 text-sm text-muted">{step.text}</p>
            </li>
          ))}
        </ol>

        <Button variant="primary" size="lg" className="mt-9 h-13 px-8 text-[17px]" data-testid="welcome-add-folder" icon={<FolderPlus className="size-5" aria-hidden />} onClick={() => void startImport()}>
          เพิ่มโฟลเดอร์กิจกรรม
        </Button>
        <p className="mt-3 flex items-center gap-2 text-sm text-muted">
          <HardDrive className="size-4 shrink-0" aria-hidden />
          รูปต้นฉบับยังอยู่ในโฟลเดอร์เดิม แอปจะบันทึกข้อมูลสำหรับค้นหาไว้แยกต่างหาก
        </p>

        <div className="mt-6 flex flex-wrap items-center justify-center gap-x-6 gap-y-2 text-sm">
          <Link to="/settings?tab=migration" className="inline-flex items-center gap-1.5 font-semibold text-action hover:underline" data-testid="welcome-legacy">
            <DatabaseBackup className="size-4" aria-hidden />
            นำเข้าข้อมูลจากเวอร์ชัน Python
          </Link>
          {hasLibrary && (
            <Link to="/home" className="font-semibold text-action hover:underline">
              ไปหน้าหลักของคลัง
            </Link>
          )}
        </div>

        {/* ยังไม่ติดตั้งตัววิเคราะห์ใบหน้า: กดปุ่มเดียวให้แอปดาวน์โหลดเอง (เพิ่มโฟลเดอร์และดูรูปได้โดยไม่ต้องรอ) */}
        <FaceModelNotice className="mt-7 w-full max-w-[680px]" />
        {capabilities.isError && (
          <InlineNotice tone="warn" icon={<Info aria-hidden />} className="mt-7 max-w-[620px] text-left">
            ยังเชื่อมต่อส่วนประมวลผลของแอปไม่ได้ ปุ่มเพิ่มโฟลเดอร์อาจใช้ไม่ได้จนกว่าจะเชื่อมต่อสำเร็จ
          </InlineNotice>
        )}
      </div>
    </div>
  );
}
