import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router';
import { CircleCheck, Images, Pencil, X } from 'lucide-react';
import { useViewer, useViewerOpen } from '@/app/navigation';
import { Button, IconButton } from '@/components/ui/Button';
import { Sheet } from '@/components/ui/Dialog';
import { ErrorState, LoadingBlock, StatusBadge } from '@/components/ui/Feedback';
import { PhotoThumb } from '@/features/photos/PhotoThumb';
import { formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { Id, PersonSummary } from '@/lib/ipc/types';
import { qk } from '@/lib/queries';
import { CoappearanceList } from './CoappearanceList';
import { FaceAvatar } from './FaceAvatar';
import { PersonEditorDialog } from './PersonEditorDialog';

const PANEL_WIDTH = 364;
const MIN_MAIN_WIDTH = 720;

/** สถานะแผงดูเร็วของหน้า (S05/S08): เปิดตามคำสั่งผู้ใช้เท่านั้น ไม่เปิดเองตอนเริ่มแอป */
export function useQuickView() {
  const [personId, setPersonId] = useState<Id | null>(null);
  const trigger = useRef<HTMLElement | null>(null);

  const restoreFocus = useCallback(() => {
    const el = trigger.current;
    if (!el) return;
    // คืน focus ไปที่ตัวการ์ดต้นทาง (ปุ่มเปิดรายละเอียดของการ์ดนั้น)
    const card = el.closest<HTMLElement>('[data-testid="person-card"]');
    (card?.querySelector<HTMLElement>('[data-testid="person-card-open"]') ?? el).focus();
  }, []);

  const toggle = useCallback(
    (person: PersonSummary, source: HTMLElement) => {
      trigger.current = source;
      setPersonId((current) => (current === person.id ? null : person.id));
    },
    [],
  );

  const close = useCallback(() => {
    setPersonId(null);
    restoreFocus();
  }, [restoreFocus]);

  return { personId, toggle, close, restoreFocus };
}

/** พื้นที่กลางเหลือพอวางแผงด้านข้างหรือไม่ (ถ้าไม่พอใช้ drawer ทับ) */
function useSideBySide(): boolean {
  const [fits, setFits] = useState(true);
  useEffect(() => {
    const main = document.getElementById('app-main');
    if (!main) return;
    const measure = () => setFits(main.clientWidth - PANEL_WIDTH >= MIN_MAIN_WIDTH);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(main);
    return () => observer.disconnect();
  }, []);
  return fits;
}

interface QuickViewHostProps {
  personId: Id | null;
  onClose: () => void;
  restoreFocus: () => void;
}

/** วางแผงดูเร็วเป็น inspector ด้านขวา หรือ drawer เมื่อหน้าต่างแคบ */
export function PersonQuickViewHost({ personId, onClose, restoreFocus }: QuickViewHostProps): ReactNode {
  const sideBySide = useSideBySide();
  const viewerOpen = useViewerOpen();

  // Esc ปิดแผง เมื่อไม่มีชั้นอื่น (ตัวดูภาพ/dialog) อยู่ด้านบน
  useEffect(() => {
    if (!personId || !sideBySide || viewerOpen) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented) return;
      if (document.querySelector('[role="dialog"][data-state="open"], [data-radix-popper-content-wrapper]')) return;
      onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [personId, sideBySide, viewerOpen, onClose]);

  if (sideBySide) {
    if (!personId) return null;
    return (
      <aside
        aria-label="ดูข้อมูลบุคคล"
        data-testid="person-quickview-panel"
        className="anim-slide m-3 ml-0 flex shrink-0 flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-card"
        style={{ width: PANEL_WIDTH }}
      >
        <QuickViewContent personId={personId} onClose={onClose} />
      </aside>
    );
  }
  return (
    <Sheet
      open={Boolean(personId) && !viewerOpen}
      onOpenChange={(open) => !open && onClose()}
      label="ดูข้อมูลบุคคล"
      onCloseAutoFocus={(event) => {
        event.preventDefault();
        restoreFocus();
      }}
    >
      {personId && <QuickViewContent personId={personId} onClose={onClose} />}
    </Sheet>
  );
}

function QuickViewContent({ personId, onClose }: { personId: Id; onClose: () => void }) {
  const qc = useQueryClient();
  const viewer = useViewer();
  const [editing, setEditing] = useState(false);
  // key ผูกกับ personId: เปลี่ยนคนเร็ว ๆ ผลของคนเก่าจะไม่ถูกนำมาแสดงแทนคนใหม่
  const query = useQuery({ queryKey: qk.quickView(personId), queryFn: () => call('get_person_quick_view', { personId }) });
  const data = query.data?.person.id === personId ? query.data : undefined;

  // บันทึก "เปิดล่าสุด" เมื่อแสดงข้อมูลสำเร็จจากการสั่งดูเร็วเท่านั้น (หนึ่งครั้งต่อการเปิด)
  const recorded = useRef<Id | null>(null);
  useEffect(() => {
    if (!data || recorded.current === personId) return;
    recorded.current = personId;
    void call('record_person_view', { personId })
      .then(() => qc.invalidateQueries({ queryKey: qk.homePeople }))
      .catch(() => {
        /* ประวัติเปิดล่าสุดเป็นความสะดวก ไม่ขวางการดูข้อมูล */
      });
  }, [data, personId, qc]);

  return (
    <>
      <div className="flex shrink-0 items-center justify-between px-5 pt-4 pb-2">
        <h2 className="text-xl font-bold text-ink">ดูข้อมูลบุคคล</h2>
        <IconButton label="ปิดแผงดูข้อมูลบุคคล" data-testid="person-quickview-close" onClick={onClose}>
          <X className="size-5" aria-hidden />
        </IconButton>
      </div>
      <div className="scroll-thin min-h-0 flex-1 overflow-y-auto px-5 pb-5">
        {query.isPending || (query.isFetching && !data) ? (
          <LoadingBlock label="กำลังโหลดข้อมูลบุคคล…" />
        ) : query.isError || !data ? (
          <ErrorState error={query.error} onRetry={() => void query.refetch()} compact />
        ) : (
          <div className="space-y-5">
            <div className="flex gap-4">
              <FaceAvatar asset={data.person.coverAsset} name={data.person.displayName} shape="rounded" className="size-[112px] text-4xl" />
              <div className="min-w-0 flex-1">
                <h3 className="truncate text-xl font-bold text-ink">{data.person.displayName}</h3>
                {data.person.nickname && <p className="truncate text-[13.5px] text-muted">{data.person.nickname}</p>}
                {data.hasConfirmedFaces && (
                  <StatusBadge tone="ok" icon={<CircleCheck aria-hidden />} className="mt-1">
                    ยืนยันแล้ว
                  </StatusBadge>
                )}
                <p className="mt-1.5 text-sm text-muted">
                  {formatCount(data.person.confirmedPhotoCount)} รูป · {formatCount(data.confirmedEventCount)} กิจกรรม
                </p>
                <Button size="sm" className="mt-2" icon={<Pencil className="size-3.5" aria-hidden />} onClick={() => setEditing(true)}>
                  แก้ไขข้อมูล
                </Button>
              </div>
            </div>

            <Link
              to={`/people/${data.person.id}`}
              data-testid="person-quickview-open-all"
              className="flex h-11 w-full items-center justify-center gap-2 rounded-control bg-action text-[15px] font-semibold text-on-action shadow-card hover:bg-action-hover"
            >
              <Images className="size-[18px]" aria-hidden />
              ดูรูปทั้งหมด
            </Link>

            <section aria-labelledby="qv-recent">
              <h4 id="qv-recent" className="mb-2 text-[15px] font-bold text-ink">
                ภาพล่าสุด
              </h4>
              {data.recentPhotos.length === 0 ? (
                <p className="text-[13.5px] text-muted">ยังไม่มีรูปที่ยืนยันว่าเป็นบุคคลนี้ เลือกใบหน้าในรูปแล้วกำหนดเป็นบุคคลนี้ได้จากตัวดูภาพ</p>
              ) : (
                <ul className="grid grid-cols-2 gap-2">
                  {data.recentPhotos.slice(0, 4).map((photo) => (
                    <li key={photo.id}>
                      <button
                        type="button"
                        aria-label={`เปิดรูป ${photo.filename} จาก ${photo.eventTitle}`}
                        onClick={() => viewer.open(photo.id, { ids: data.recentPhotos.map((p) => p.id), originLabel: 'กลับ' })}
                        className="block w-full overflow-hidden rounded-xl"
                      >
                        <PhotoThumb photo={photo} className="aspect-[4/3] w-full" />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </section>

            <section className="space-y-2.5 border-t border-line pt-4 text-sm">
              <div className="flex gap-3">
                <span className="w-16 shrink-0 font-semibold text-ink">กลุ่ม</span>
                {data.person.groups.length > 0 ? (
                  <ul className="flex flex-wrap gap-1.5">
                    {data.person.groups.map((group) => (
                      <li key={group.id} className="rounded-full bg-brand-soft px-2.5 py-0.5 text-[13px] text-action">
                        {group.name}
                      </li>
                    ))}
                  </ul>
                ) : (
                  <span className="text-muted">ยังไม่ได้จัดกลุ่ม</span>
                )}
              </div>
              <div className="flex gap-3">
                <span className="w-16 shrink-0 font-semibold text-ink">หมายเหตุ</span>
                <span className={data.notes ? 'min-w-0 break-words whitespace-pre-wrap text-ink' : 'text-muted'}>{data.notes || 'ไม่มี'}</span>
              </div>
            </section>

            <section aria-labelledby="qv-co" className="border-t border-line pt-4">
              <div className="mb-2">
                <h4 id="qv-co" className="text-[15px] font-bold text-ink">
                  ปรากฏร่วมในภาพ
                </h4>
                <p className="text-[12.5px] text-muted">เลือกคนเพื่อค้นรูปที่อยู่ด้วยกัน · นับจากทุกกิจกรรม เฉพาะที่ยืนยันแล้ว</p>
              </div>
              <CoappearanceList personId={data.person.id} items={data.coappearances} />
            </section>
          </div>
        )}
      </div>
      <PersonEditorDialog open={editing} onOpenChange={setEditing} personId={personId} />
    </>
  );
}
