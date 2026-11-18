import { Link } from 'react-router';
import { Compass } from 'lucide-react';
import { EmptyState } from '@/components/ui/Feedback';
import { Page, PageHeader } from './PageLayout';

export function NotFoundPage() {
  return (
    <Page>
      <PageHeader title="ไม่พบหน้านี้" />
      <EmptyState
        icon={<Compass aria-hidden />}
        title="ไม่พบหน้าที่ต้องการ"
        description="ลิงก์นี้อาจเก่าหรือรายการถูกนำออกจากคลังแล้ว"
        action={
          <Link to="/home" className="inline-flex h-10 items-center rounded-control bg-action px-4 text-sm font-semibold text-on-action hover:bg-action-hover">
            กลับหน้าหลัก
          </Link>
        }
      />
    </Page>
  );
}
