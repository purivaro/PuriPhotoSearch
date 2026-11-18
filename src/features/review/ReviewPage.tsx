import { Link } from 'react-router';
import { CircleCheck, ScanFace } from 'lucide-react';
import { Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { ErrorState, InlineNotice, LoadingBlock } from '@/components/ui/Feedback';
import { useCapabilities } from '@/lib/queries';

/**
 * S16 — ตรวจสอบใบหน้า
 * รุ่นนี้ยังไม่มีตัววิเคราะห์ใบหน้าอัตโนมัติ จึงไม่มีคิวข้อเสนอของ AI ให้ตรวจ
 * หน้านี้บอกสถานะตามจริงและพาไปยังวิธีระบุใบหน้าเองที่ใช้งานได้ ไม่วางปุ่มที่ยังทำงานไม่ได้
 */
export function ReviewPage() {
  useDocumentTitle('ตรวจสอบใบหน้า');
  const capabilities = useCapabilities();
  return (
    <Page>
      <PageHeader title="ตรวจสอบใบหน้า" subtitle="ตั้งชื่อและแก้ไขตัวตนของใบหน้าในรูป" />
      {capabilities.isPending ? (
        <LoadingBlock />
      ) : capabilities.isError ? (
        <ErrorState error={capabilities.error} onRetry={() => void capabilities.refetch()} />
      ) : (
        <div className="max-w-[760px] space-y-4">
          <InlineNotice tone="info" icon={<ScanFace aria-hidden />}>
            <p className="font-semibold">{capabilities.data.faceAnalysis.available ? 'การวิเคราะห์ใบหน้าพร้อมใช้ แต่คิวตรวจสอบยังไม่เปิดในรุ่นนี้' : 'รุ่นนี้ยังไม่มีการวิเคราะห์ใบหน้าอัตโนมัติ'}</p>
            <p>{capabilities.data.faceAnalysis.reason || 'จึงยังไม่มีรายการใบหน้าที่ระบบเสนอให้ตรวจ'} รูปที่เพิ่มแล้วยังดู ค้นจากชื่อไฟล์/กิจกรรม และระบุใบหน้าเองได้ตามปกติ</p>
          </InlineNotice>

          <section className="rounded-card border border-line bg-surface p-6 shadow-card">
            <h2 className="text-lg font-bold text-ink">ระบุใบหน้าเองได้ตอนนี้</h2>
            <ol className="mt-3 space-y-2.5 text-sm text-ink">
              {[
                'เปิดกิจกรรม แล้วคลิกรูปที่ต้องการเพื่อเปิดตัวดูภาพ',
                'กด “เพิ่มกรอบใบหน้า” แล้วลากกรอบรอบใบหน้า',
                'เลือกบุคคลเดิม หรือพิมพ์ชื่อเพื่อสร้างบุคคลใหม่ — ค้นรูปนั้นจากชื่อได้ทันที',
                'ถ้าระบุผิด เลือกใบหน้าแล้วกด “เปลี่ยนบุคคล” หรือ “ไม่ใช่บุคคลนี้” ได้เฉพาะใบหน้านั้น',
              ].map((step, index) => (
                <li key={step} className="flex gap-3">
                  <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-brand-soft text-[13px] font-bold text-action">{index + 1}</span>
                  <span>{step}</span>
                </li>
              ))}
            </ol>
            <div className="mt-5 flex flex-wrap gap-2.5">
              <Link to="/events" className="inline-flex h-10 items-center rounded-control bg-action px-4 text-sm font-semibold text-on-action hover:bg-action-hover" data-testid="review-open-events">
                เปิดกิจกรรมเพื่อระบุใบหน้า
              </Link>
              <Link to="/search?face=no_person" className="inline-flex h-10 items-center rounded-control border border-line-strong bg-surface px-4 text-sm font-semibold text-ink shadow-card hover:bg-sunken">
                ดูรูปที่ยังไม่มีชื่อบุคคล
              </Link>
            </div>
          </section>

          <p className="flex items-start gap-2 text-[13.5px] text-muted">
            <CircleCheck className="mt-0.5 size-4 shrink-0 text-ok" aria-hidden />
            ชื่อที่ระบุเองเป็นข้อมูลยืนยันของผู้ใช้ เมื่อมีการวิเคราะห์ใบหน้าอัตโนมัติในรุ่นถัดไป ระบบจะไม่เขียนทับชื่อที่ยืนยันไว้
          </p>
        </div>
      )}
    </Page>
  );
}
