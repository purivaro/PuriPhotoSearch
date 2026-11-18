import { useQuery } from '@tanstack/react-query';
import { useSearchParams } from 'react-router';
import { ClipboardCheck, History } from 'lucide-react';
import { Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { EmptyState, ErrorState, LoadingBlock } from '@/components/ui/Feedback';
import { Tabs } from '@/components/ui/Form';
import { formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { JobSnapshot } from '@/lib/ipc/types';
import { qk } from '@/lib/queries';
import { JobCard } from './JobCard';

type ActivityTab = 'current' | 'history';
const CURRENT: JobSnapshot['state'][] = ['queued', 'running', 'pausing', 'paused', 'blocked', 'cancel_requested'];

/** S19 — ศูนย์งานและประวัติ: งานเบื้องหลังทั้งหมดจากสถานะที่บันทึกไว้จริง (เปิดแอปใหม่ก็ยังเห็นงานเดิม) */
export function ActivityPage() {
  useDocumentTitle('งานและประวัติ');
  const [params, setParams] = useSearchParams();
  const focusJob = params.get('job');
  const jobs = useQuery({
    queryKey: qk.jobsAll,
    queryFn: () => call('list_jobs', { limit: 200 }),
    refetchInterval: (query) => (query.state.data?.some((job) => CURRENT.includes(job.state) && job.state !== 'paused') ? 1000 : 5000),
  });

  const current = (jobs.data ?? []).filter((job) => CURRENT.includes(job.state));
  const history = (jobs.data ?? []).filter((job) => !CURRENT.includes(job.state));
  const focused = jobs.data?.find((job) => job.id === focusJob);
  const requested = params.get('tab') as ActivityTab | null;
  const tab: ActivityTab = requested ?? (focused && !CURRENT.includes(focused.state) ? 'history' : 'current');
  const list = tab === 'current' ? current : history;

  return (
    <Page>
      <PageHeader title="งานและประวัติ" subtitle="งานเบื้องหลังทำต่อขณะแอปเปิด ปิดหน้านี้ได้โดยไม่ยกเลิกงาน" />
      <Tabs<ActivityTab>
        label="หมวดของงาน"
        testId="activity-tab"
        value={tab}
        onChange={(value) => {
          const next = new URLSearchParams(params);
          next.set('tab', value);
          setParams(next, { replace: true });
        }}
        items={[
          { value: 'current', label: `กำลังทำ / รอคิว${current.length ? ` (${formatCount(current.length)})` : ''}` },
          { value: 'history', label: `ประวัติงาน${history.length ? ` (${formatCount(history.length)})` : ''}` },
        ]}
        className="mb-4"
      />
      {jobs.isPending ? (
        <LoadingBlock label="กำลังโหลดรายการงาน…" />
      ) : jobs.isError ? (
        <ErrorState error={jobs.error} title="โหลดรายการงานไม่สำเร็จ" onRetry={() => void jobs.refetch()} />
      ) : list.length === 0 ? (
        tab === 'current' ? (
          <EmptyState icon={<ClipboardCheck aria-hidden />} title="ไม่มีงานที่กำลังทำ" description="เมื่อเพิ่มโฟลเดอร์หรือส่งออกสำเนา ความคืบหน้าจะแสดงที่นี่ ดูงานที่จบแล้วได้ในแท็บประวัติงาน" />
        ) : (
          <EmptyState icon={<History aria-hidden />} title="ยังไม่มีประวัติงาน" description="งานที่เสร็จ ยกเลิก หรือทำไม่สำเร็จจะถูกเก็บไว้ที่นี่พร้อมรายละเอียดข้อผิดพลาด" />
        )
      ) : (
        <ul className="space-y-3" data-testid="job-list">
          {list.map((job) => (
            <JobCard key={job.id} job={job} defaultExpanded={job.id === focusJob} />
          ))}
        </ul>
      )}
      {jobs.isError && jobs.data && <p className="mt-3 text-[13px] text-warn">อัปเดตสถานะล่าสุดไม่สำเร็จ ข้อมูลที่เห็นอาจไม่ใช่ปัจจุบัน</p>}
    </Page>
  );
}
