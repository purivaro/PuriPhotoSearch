import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate } from 'react-router';
import { toast } from 'sonner';
import { FolderPlus, HardDrive, Pin, ScanFace, Users } from 'lucide-react';
import { AddFolderButton, JobIndicator, Page, SearchTrigger, SectionTitle, useDocumentTitle } from '@/app/PageLayout';
import { EmptyState, ErrorState, LoadingBlock } from '@/components/ui/Feedback';
import { Tabs } from '@/components/ui/Form';
import { EVENT_GRID, EventCard } from '@/features/events/EventCard';
import { FaceModelNotice } from '@/features/faces/FaceModelNotice';
import { JobStrip } from '@/features/jobs/JobCard';
import { PERSON_GRID, PersonCard } from '@/features/people/PersonCard';
import { PersonQuickViewHost, useQuickView } from '@/features/people/PersonQuickView';
import { useTogglePin } from '@/features/people/usePersonActions';
import { errorMessage, formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { EventSummary, HomePeople } from '@/lib/ipc/types';
import { isJobActive, qk, useActiveJobs, useCapabilities, useEvents, useSummary } from '@/lib/queries';

const LINK = 'text-[15px] font-bold text-action hover:underline';

/** S05 — หน้าหลัก (Home Hybrid V03): ค้นหา เพิ่มโฟลเดอร์ และกลับไปยังคนที่ใช้บ่อยได้ทันที */
export function HomePage() {
  useDocumentTitle('หน้าหลัก');
  const quickView = useQuickView();
  const summary = useSummary();
  const capabilities = useCapabilities();
  const jobs = useActiveJobs();
  const pending = (jobs.data ?? []).filter((job) => isJobActive(job) || job.state === 'paused');
  const unnamedFaces = summary.data?.unnamedFaceCount ?? 0;

  return (
    <Page aside={<PersonQuickViewHost personId={quickView.personId} onClose={quickView.close} restoreFocus={quickView.restoreFocus} />}>
      <header data-tauri-drag-region className="flex flex-wrap items-start justify-between gap-4 pt-6 pb-4">
        <h1 data-tauri-drag-region className="text-[34px] leading-tight font-extrabold tracking-tight text-ink">
          คลังภาพของคุณ
        </h1>
        <div className="flex flex-col items-end gap-1.5">
          <AddFolderButton size="lg" />
          <p className="flex items-center gap-1.5 text-[13.5px] text-muted">
            <HardDrive className="size-4" aria-hidden />
            รูปต้นฉบับอยู่ในโฟลเดอร์เดิม
          </p>
        </div>
      </header>

      <div className="flex items-center gap-3">
        <SearchTrigger large className="flex-1" />
        {capabilities.data?.faceAnalysis.available && (
          <Link to="/review" className="inline-flex h-12 shrink-0 items-center gap-2 rounded-control border border-line-strong bg-surface px-4 text-[15px] font-semibold text-ink shadow-card hover:bg-sunken">
            <ScanFace className="size-5" aria-hidden /> ตรวจสอบใบหน้า
          </Link>
        )}
        <JobIndicator />
      </div>

      <div className="mt-2.5 mb-6 min-h-6 text-[15px] text-ink" data-testid="library-summary">
        {summary.isPending ? (
          <span className="text-muted">กำลังนับข้อมูลในคลัง…</span>
        ) : summary.isError ? (
          <span className="text-danger">
            โหลดสรุปคลังไม่สำเร็จ{' '}
            <button type="button" className="font-semibold underline" onClick={() => void summary.refetch()}>
              ลองใหม่
            </button>
          </span>
        ) : (
          <p className="flex flex-wrap items-center gap-x-2.5">
            <Link to="/events" className="hover:text-action hover:underline">
              <span className="font-bold">{formatCount(summary.data.eventCount)}</span> กิจกรรม
            </Link>
            <span className="text-faint" aria-hidden>
              ·
            </span>
            <Link to="/search" className="hover:text-action hover:underline">
              <span className="font-bold">{formatCount(summary.data.photoCount)}</span> รูป
            </Link>
            <span className="text-faint" aria-hidden>
              ·
            </span>
            <Link to="/people" className="hover:text-action hover:underline">
              <span className="font-bold">{formatCount(summary.data.personCount)}</span> บุคคล
            </Link>
          </p>
        )}
      </div>

      {/* ใบหน้ารอตรวจสอบ: อยู่บนสุดของเนื้อหา เพราะเป็นงานถัดไปที่ผู้ใช้ต้องทำหลังเพิ่มกิจกรรม */}
      {unnamedFaces > 0 && (
        <Link
          to="/review"
          data-testid="home-review-strip"
          className="mb-6 flex flex-wrap items-center gap-x-4 gap-y-2 rounded-card border border-brand-line bg-brand-soft px-5 py-3.5 transition-colors hover:border-brand"
        >
          <ScanFace className="size-6 shrink-0 text-action" aria-hidden />
          <span className="min-w-[200px] flex-1 text-[17px] font-bold text-action">มี {formatCount(unnamedFaces)} ใบหน้ารอตรวจสอบ</span>
          <span className="inline-flex h-9 shrink-0 items-center rounded-lg border border-action/60 bg-surface px-4 text-sm font-semibold text-action">ตรวจสอบใบหน้า</span>
        </Link>
      )}

      {/* ยังไม่ติดตั้งตัววิเคราะห์ใบหน้า: แถบเดียว กดปุ่มเดียวจบ (ไม่แสดงเมื่อพร้อมใช้แล้ว) */}
      <FaceModelNotice variant="bar" className="mb-6" />

      <HomePeopleSection quickViewPersonId={quickView.personId} onQuickView={quickView.toggle} />
      <RecentEvents />

      {pending.length > 0 && (
        <section aria-label="งานเบื้องหลัง" className="mt-6 space-y-2.5">
          {pending.slice(0, 2).map((job) => (
            <JobStrip key={job.id} job={job} />
          ))}
          {pending.length > 2 && (
            <Link to="/activity" className={LINK}>
              ดูงานทั้งหมด ({formatCount(pending.length)})
            </Link>
          )}
        </section>
      )}
    </Page>
  );
}

function HomePeopleSection({ quickViewPersonId, onQuickView }: { quickViewPersonId: string | null; onQuickView: Parameters<typeof PersonCard>[0]['onQuickView'] }) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const togglePin = useTogglePin();
  const home = useQuery({ queryKey: qk.homePeople, queryFn: () => call('list_home_people', { limit: 12 }) });
  // แท็บที่ผู้ใช้เลือกในรอบนี้; ถ้ายังไม่เลือกใช้ค่าที่คลังจำไว้/ค่าเริ่มต้นจาก backend
  const [chosen, setChosen] = useState<HomePeople['tab'] | null>(null);
  const tab = chosen ?? home.data?.tab ?? 'pinned';

  const saveTab = useMutation({
    mutationFn: (next: HomePeople['tab']) => call('set_home_people_tab', { tab: next }),
    onError: (error) => toast.error(`จำแท็บที่เลือกไม่สำเร็จ: ${errorMessage(error)}`),
    onSuccess: () => void qc.invalidateQueries({ queryKey: qk.homePeople }),
  });

  const list = home.data ? (tab === 'pinned' ? home.data.pinned : home.data.recent) : [];
  const noPeopleAtAll = home.data && home.data.pinned.length === 0 && home.data.recent.length === 0;

  return (
    <section aria-labelledby="home-people" className="mb-7">
      <div className="mb-3 flex flex-wrap items-center gap-4">
        <h2 id="home-people" className="text-[22px] font-bold text-ink">
          บุคคล
        </h2>
        <Tabs<HomePeople['tab']>
          label="รายการบุคคลบนหน้าหลัก"
          variant="pill"
          testId="home-people-tab"
          value={tab}
          onChange={(next) => {
            setChosen(next);
            saveTab.mutate(next);
          }}
          items={[
            { value: 'pinned', label: 'ปักหมุด' },
            { value: 'recent', label: 'เปิดล่าสุด' },
          ]}
        />
        <Link to="/people" className={`ml-auto ${LINK}`}>
          ดูทั้งหมด
        </Link>
      </div>

      {home.isPending ? (
        <LoadingBlock label="กำลังโหลดบุคคล…" className="py-10" />
      ) : home.isError ? (
        <ErrorState compact error={home.error} title="โหลดรายการบุคคลไม่สำเร็จ" onRetry={() => void home.refetch()} />
      ) : list.length === 0 ? (
        <div className="rounded-card border border-dashed border-line-strong bg-surface">
          {tab === 'pinned' ? (
            <EmptyState
              compact
              icon={<Pin aria-hidden />}
              title="ปักหมุดบุคคลที่ค้นบ่อย"
              description={noPeopleAtAll ? 'เมื่อตั้งชื่อคนในรูปแล้ว ปักหมุดไว้เพื่อกลับมาหาได้จากหน้านี้ทันที' : 'กดรูปหมุดบนการ์ดบุคคลเพื่อเก็บไว้ตรงนี้ แอปจะไม่ปักหมุดให้เอง'}
              action={
                <Link to="/people" className="inline-flex h-9 items-center rounded-lg border border-line-strong bg-surface px-3.5 text-sm font-semibold text-ink hover:bg-sunken">
                  เปิดหน้าบุคคล
                </Link>
              }
            />
          ) : (
            <EmptyState compact icon={<Users aria-hidden />} title="ยังไม่มีบุคคลที่เปิดล่าสุด" description="คนที่เปิดดูรายละเอียดหรือกดดูเร็วจะแสดงที่นี่" />
          )}
        </div>
      ) : (
        <ul className={PERSON_GRID} data-testid="home-people-grid">
          {list.map((person) => (
            <li key={person.id}>
              <PersonCard
                person={person}
                quickViewActive={quickViewPersonId === person.id}
                onOpen={(p) => navigate(`/people/${p.id}`)}
                onQuickView={onQuickView}
                onTogglePin={(p) => togglePin.mutate(p)}
                pinBusy={togglePin.isPending && togglePin.variables?.id === person.id}
              />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function recentFirst(events: EventSummary[]): EventSummary[] {
  return [...events].sort((a, b) => b.createdAt.localeCompare(a.createdAt) || a.id.localeCompare(b.id));
}

function RecentEvents() {
  const events = useEvents('');
  return (
    <section aria-labelledby="home-events">
      <SectionTitle
        title="กิจกรรมล่าสุด"
        hint="กิจกรรมจากโฟลเดอร์ที่เพิ่มไว้"
        action={
          <Link to="/events" className={LINK}>
            ดูทั้งหมด
          </Link>
        }
      />
      <h2 id="home-events" className="sr-only">
        กิจกรรมล่าสุด
      </h2>
      {events.isPending ? (
        <LoadingBlock label="กำลังโหลดกิจกรรม…" className="py-10" />
      ) : events.isError ? (
        <ErrorState compact error={events.error} title="โหลดกิจกรรมไม่สำเร็จ" onRetry={() => void events.refetch()} />
      ) : events.data.length === 0 ? (
        <div className="rounded-card border border-dashed border-line-strong bg-surface">
          <EmptyState
            icon={<FolderPlus aria-hidden />}
            title="เริ่มจากโฟลเดอร์กิจกรรมแรก"
            description="เลือกโฟลเดอร์ที่เก็บรูปของงาน กิจกรรมจะปรากฏที่นี่ทันที โดยรูปต้นฉบับยังอยู่ในโฟลเดอร์เดิม"
            action={<AddFolderButton size="lg" />}
          />
        </div>
      ) : (
        <ul className={EVENT_GRID} data-testid="home-event-grid">
          {recentFirst(events.data)
            .slice(0, 6)
            .map((event) => (
              <li key={event.id}>
                <EventCard event={event} />
              </li>
            ))}
        </ul>
      )}
    </section>
  );
}
