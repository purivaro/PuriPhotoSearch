import { useDeferredValue, useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router';
import { Pin, ScanFace, Search, SearchX, UserPlus, Users } from 'lucide-react';
import { Page, PageHeader, useDocumentTitle } from '@/app/PageLayout';
import { Button } from '@/components/ui/Button';
import { EmptyState, ErrorState, LoadingBlock } from '@/components/ui/Feedback';
import { Select, TextInput } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { formatCount } from '@/lib/format';
import type { PersonSummary } from '@/lib/ipc/types';
import { useGroups, usePeople } from '@/lib/queries';
import { PERSON_GRID, PersonCard } from './PersonCard';
import { PersonEditorDialog } from './PersonEditorDialog';
import { PersonQuickViewHost, useQuickView } from './PersonQuickView';
import { useTogglePin } from './usePersonActions';

type PeopleSort = 'name' | 'photos';

function sortPeople(people: PersonSummary[], sort: PeopleSort): PersonSummary[] {
  const byName = (a: PersonSummary, b: PersonSummary) => a.displayName.localeCompare(b.displayName, 'th') || a.id.localeCompare(b.id);
  const list = [...people];
  return sort === 'photos' ? list.sort((a, b) => b.confirmedPhotoCount - a.confirmedPhotoCount || byName(a, b)) : list.sort(byName);
}

/** S08 — รายชื่อบุคคล: เฉพาะคนที่ตั้งชื่อแล้ว ไม่ปะปนกับใบหน้าที่ยังไม่ยืนยัน */
export function PeoplePage() {
  useDocumentTitle('บุคคล');
  const navigate = useNavigate();
  const [text, setText] = useState('');
  const [groupId, setGroupId] = useState('');
  const [pinnedOnly, setPinnedOnly] = useState(false);
  const [includeArchived, setIncludeArchived] = useState(false);
  const [sort, setSort] = useState<PeopleSort>('name');
  const [creating, setCreating] = useState(false);
  const term = useDeferredValue(text.trim());
  const quickView = useQuickView();
  const togglePin = useTogglePin();

  const args = useMemo(() => {
    const out: { text?: string; includeArchived?: boolean; groupId?: string; pinnedOnly?: boolean } = {};
    if (term) out.text = term;
    if (includeArchived) out.includeArchived = true;
    if (groupId) out.groupId = groupId;
    if (pinnedOnly) out.pinnedOnly = true;
    return out;
  }, [term, includeArchived, groupId, pinnedOnly]);
  const people = usePeople(args);
  const everyone = usePeople({ includeArchived: true });
  const groups = useGroups();
  const libraryEmpty = everyone.data?.length === 0;
  const filtered = Boolean(term || groupId || pinnedOnly);
  const sorted = useMemo(() => sortPeople(people.data ?? [], sort), [people.data, sort]);

  const clearFilters = () => {
    setText('');
    setGroupId('');
    setPinnedOnly(false);
  };

  return (
    <Page aside={<PersonQuickViewHost personId={quickView.personId} onClose={quickView.close} restoreFocus={quickView.restoreFocus} />}>
      <PageHeader
        title="บุคคล"
        subtitle="คนที่ตั้งชื่อแล้ว จำนวนรูปนับเฉพาะใบหน้าที่ยืนยัน"
        actions={
          <Button icon={<UserPlus className="size-[18px]" aria-hidden />} onClick={() => setCreating(true)} data-testid="people-add">
            เพิ่มบุคคล
          </Button>
        }
      />

      {!libraryEmpty && (
        <div className="mb-4 flex flex-wrap items-center gap-2.5">
          <div className="relative min-w-[220px] flex-1 sm:max-w-[360px]">
            <Search className="pointer-events-none absolute top-1/2 left-3 size-[18px] -translate-y-1/2 text-muted" aria-hidden />
            <TextInput data-page-search data-testid="people-search" type="search" aria-label="ค้นชื่อบุคคล" value={text} onChange={(e) => setText(e.target.value)} placeholder="ค้นชื่อ ชื่อเล่น หรือชื่ออื่น" className="pl-10" />
          </div>
          {(groups.data?.length ?? 0) > 0 && (
            <Select value={groupId} onChange={(e) => setGroupId(e.target.value)} aria-label="กรองตามกลุ่ม">
              <option value="">ทุกกลุ่ม</option>
              {groups.data?.map((group) => (
                <option key={group.id} value={group.id}>
                  {group.name} ({formatCount(group.memberCount)})
                </option>
              ))}
            </Select>
          )}
          <button
            type="button"
            aria-pressed={pinnedOnly}
            onClick={() => setPinnedOnly((v) => !v)}
            data-testid="people-filter-pinned"
            className={cn(
              'inline-flex h-10 items-center gap-1.5 rounded-control border px-3.5 text-sm font-semibold',
              pinnedOnly ? 'border-brand-line bg-brand-soft text-action' : 'border-line-strong bg-surface text-ink hover:bg-sunken',
            )}
          >
            <Pin className="size-4" fill={pinnedOnly ? 'currentColor' : 'none'} aria-hidden /> ปักหมุด
          </button>
          <label className="flex h-10 cursor-pointer items-center gap-2 px-1 text-sm text-muted">
            <input type="checkbox" checked={includeArchived} onChange={(e) => setIncludeArchived(e.target.checked)} className="size-4 accent-[var(--action)]" />
            รวมคนที่เก็บไว้
          </label>
          <Select value={sort} onChange={(e) => setSort(e.target.value as PeopleSort)} aria-label="เรียงบุคคลตาม" className="ml-auto">
            <option value="name">เรียงตามชื่อ</option>
            <option value="photos">เรียงตามจำนวนรูปยืนยัน</option>
          </Select>
        </div>
      )}

      {people.isPending ? (
        <LoadingBlock label="กำลังโหลดรายชื่อบุคคล…" />
      ) : people.isError ? (
        <ErrorState error={people.error} title="โหลดรายชื่อบุคคลไม่สำเร็จ" onRetry={() => void people.refetch()} />
      ) : libraryEmpty ? (
        <EmptyState
          icon={<Users aria-hidden />}
          title="ยังไม่มีบุคคลที่ตั้งชื่อ"
          description="เปิดรูปในกิจกรรม กด “เพิ่มกรอบใบหน้า” แล้วบอกว่าเป็นใคร — หรือเพิ่มบุคคลไว้ก่อนแล้วค่อยเชื่อมกับรูปภายหลัง"
          action={
            <>
              <Link to="/events" className="inline-flex h-10 items-center rounded-control bg-action px-4 text-sm font-semibold text-on-action hover:bg-action-hover">
                เปิดกิจกรรมเพื่อระบุใบหน้า
              </Link>
              <Button icon={<UserPlus className="size-[18px]" aria-hidden />} onClick={() => setCreating(true)}>
                เพิ่มบุคคลเอง
              </Button>
            </>
          }
        />
      ) : sorted.length === 0 ? (
        <EmptyState
          icon={<SearchX aria-hidden />}
          title="ไม่พบบุคคลตามตัวกรองนี้"
          description={[term ? `คำค้น “${term}”` : null, groupId ? 'กลุ่มที่เลือก' : null, pinnedOnly ? 'เฉพาะที่ปักหมุด' : null].filter(Boolean).join(' · ') || undefined}
          action={
            <>
              {filtered && <Button onClick={clearFilters}>ล้างตัวกรอง</Button>}
              {!includeArchived && <Button onClick={() => setIncludeArchived(true)}>รวมคนที่เก็บไว้</Button>}
            </>
          }
        />
      ) : (
        <>
          <p className="mb-2.5 text-sm text-muted">{formatCount(sorted.length)} คน</p>
          <ul className={PERSON_GRID} data-testid="people-grid">
            {sorted.map((person) => (
              <li key={person.id}>
                <PersonCard
                  person={person}
                  quickViewActive={quickView.personId === person.id}
                  onOpen={(p) => navigate(`/people/${p.id}`)}
                  onQuickView={quickView.toggle}
                  onTogglePin={(p) => togglePin.mutate(p)}
                  pinBusy={togglePin.isPending && togglePin.variables?.id === person.id}
                />
              </li>
            ))}
          </ul>
        </>
      )}

      {!libraryEmpty && (
        <Link to="/review" className="mt-6 inline-flex items-center gap-2 text-sm font-semibold text-action hover:underline">
          <ScanFace className="size-4" aria-hidden /> ตั้งชื่อใบหน้าที่พบใหม่
        </Link>
      )}

      <PersonEditorDialog open={creating} onOpenChange={setCreating} onSaved={(person) => navigate(`/people/${person.id}`)} />
    </Page>
  );
}
