import { useDeferredValue, useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Command } from 'cmdk';
import { useNavigate } from 'react-router';
import { Activity, Calendar, FolderPlus, House, Image as ImageIcon, Search, Settings, Users } from 'lucide-react';
import { ErrorState, Spinner } from '@/components/ui/Feedback';
import { FaceAvatar } from '@/features/people/FaceAvatar';
import { PhotoThumb } from '@/features/photos/PhotoThumb';
import { personSearchPath, textSearchPath } from '@/features/search/searchParams';
import { formatCount, formatThaiDate } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import { useUi } from '@/stores/ui';
import { useStartImport, useViewer } from './navigation';

const ITEM = 'flex min-h-11 cursor-pointer items-center gap-3 rounded-lg px-2.5 py-1.5 text-sm text-ink data-[selected=true]:bg-selected';
const GROUP =
  '[&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:pt-2.5 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:text-[12px] [&_[cmdk-group-heading]]:font-bold [&_[cmdk-group-heading]]:text-muted';

/**
 * ⌘K — ค้นทั้งคลัง แยกผลเป็น บุคคล / กิจกรรม / ชื่อไฟล์ (Blueprint 11.1)
 * เลือกบุคคล = ไปหน้าค้นหาพร้อม chip ที่อ้าง ID; พิมพ์แล้ว Enter = ค้นข้อความจากชื่อคน กิจกรรม และไฟล์
 */
export function CommandPalette() {
  const open = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPaletteOpen);
  const navigate = useNavigate();
  const startImport = useStartImport();
  const viewer = useViewer();
  const [text, setText] = useState('');
  const term = useDeferredValue(text.trim());

  useEffect(() => {
    if (!open) setText('');
  }, [open]);

  const suggestions = useQuery({
    queryKey: ['suggestions', term],
    queryFn: () => call('search_suggestions', { text: term, limit: 6 }),
    enabled: open && term.length > 0,
    staleTime: 10_000,
  });
  // แสดงเฉพาะผลของคำที่พิมพ์อยู่ ผลของคำก่อนหน้าไม่ถูกนำมาแสดงแทน
  const data = term ? suggestions.data : undefined;

  const run = (action: () => void) => () => {
    setOpen(false);
    action();
  };

  const hasResults = Boolean(data && (data.people.length || data.events.length || data.files.length));

  return (
    <Command.Dialog
      open={open}
      onOpenChange={setOpen}
      shouldFilter={false}
      label="ค้นหาทั้งคลัง"
      overlayClassName="anim-fade fixed inset-0 z-[75] bg-scrim"
      contentClassName="anim-pop fixed top-[12vh] left-1/2 z-[76] w-[min(92vw,620px)] -translate-x-1/2 overflow-hidden rounded-2xl border border-line bg-surface shadow-pop outline-none"
    >
      <div className="flex items-center gap-3 border-b border-line px-4">
        <Search className="size-5 shrink-0 text-muted" aria-hidden />
        <Command.Input
          data-testid="global-search-input"
          value={text}
          onValueChange={setText}
          placeholder="ค้นหาคน กิจกรรม หรือชื่อไฟล์…"
          className="h-14 min-w-0 flex-1 bg-transparent text-[15px] text-ink placeholder:text-faint focus:outline-none"
        />
        {suggestions.isFetching && <Spinner className="size-4" />}
        <kbd className="rounded-md border border-line bg-sunken px-1.5 py-0.5 font-sans text-[11.5px] font-semibold text-muted">Esc</kbd>
      </div>

      <Command.List className="scroll-thin max-h-[min(60vh,480px)] overflow-y-auto p-2">
        {term && (
          <Command.Group heading="ค้นหา" className={GROUP}>
            <Command.Item value={`text:${term}`} onSelect={run(() => navigate(textSearchPath(term)))} className={ITEM} data-testid="palette-search-text">
              <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-brand-soft text-action">
                <Search className="size-[18px]" aria-hidden />
              </span>
              <span className="min-w-0 flex-1 truncate">
                ค้นรูปด้วยคำว่า <span className="font-bold">“{term}”</span>
              </span>
              <span className="shrink-0 text-[12.5px] text-muted">ชื่อคน · กิจกรรม · ชื่อไฟล์</span>
            </Command.Item>
          </Command.Group>
        )}

        {term && suggestions.isError && (
          <ErrorState compact error={suggestions.error} title="ค้นหาไม่สำเร็จ" onRetry={() => void suggestions.refetch()} />
        )}

        {data && data.people.length > 0 && (
          <Command.Group heading="บุคคล" className={GROUP}>
            {data.people.map((person) => (
              <Command.Item key={person.id} value={`person:${person.id}`} onSelect={run(() => navigate(personSearchPath(person.id)))} className={ITEM} data-testid="palette-person">
                <FaceAvatar asset={person.coverAsset} name={person.displayName} decorative className="size-9" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-semibold">{person.displayName}</span>
                  <span className="block truncate text-[12.5px] text-muted">
                    {person.nickname ? `${person.nickname} · ` : ''}
                    {formatCount(person.confirmedPhotoCount)} รูปที่ยืนยันแล้ว
                    {person.groups.length > 0 ? ` · ${person.groups.map((g) => g.name).join(', ')}` : ''}
                  </span>
                </span>
              </Command.Item>
            ))}
          </Command.Group>
        )}

        {data && data.events.length > 0 && (
          <Command.Group heading="กิจกรรม" className={GROUP}>
            {data.events.map((event) => (
              <Command.Item key={event.id} value={`event:${event.id}`} onSelect={run(() => navigate(`/events/${event.id}`))} className={ITEM} data-testid="palette-event">
                {event.coverPhotoId ? (
                  <img src={assetUrl('thumb', event.coverPhotoId)} alt="" className="h-9 w-[52px] shrink-0 rounded-md bg-sunken object-cover" />
                ) : (
                  <span className="flex h-9 w-[52px] shrink-0 items-center justify-center rounded-md bg-sunken text-faint">
                    <Calendar className="size-4" aria-hidden />
                  </span>
                )}
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-semibold">{event.title}</span>
                  <span className="block truncate text-[12.5px] text-muted">
                    {formatThaiDate(event.eventDate) ?? 'ยังไม่ระบุวันที่'} · {formatCount(event.photoCount)} รูป
                  </span>
                </span>
              </Command.Item>
            ))}
          </Command.Group>
        )}

        {data && data.files.length > 0 && (
          <Command.Group heading="ชื่อไฟล์" className={GROUP}>
            {data.files.map((photo) => (
              <Command.Item
                key={photo.id}
                value={`file:${photo.id}`}
                onSelect={run(() => viewer.open(photo.id, { ids: data.files.map((f) => f.id), originLabel: 'กลับ' }))}
                className={ITEM}
                data-testid="palette-file"
              >
                <PhotoThumb photo={photo} className="h-9 w-[52px] shrink-0 rounded-md [&>span]:hidden" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-semibold">{photo.filename}</span>
                  <span className="block truncate text-[12.5px] text-muted">{photo.eventTitle}</span>
                </span>
              </Command.Item>
            ))}
          </Command.Group>
        )}

        {term && suggestions.isSuccess && !hasResults && (
          <p className="px-3 py-4 text-center text-[13.5px] text-muted">ไม่พบคน กิจกรรม หรือไฟล์ที่ชื่อตรงกับ “{term}” — กด Enter เพื่อค้นในรูปทั้งหมด</p>
        )}

        {!term && (
          <>
            <Command.Group heading="คำสั่ง" className={GROUP}>
              <Command.Item value="add-folder" onSelect={run(() => void startImport())} className={ITEM}>
                <FolderPlus className="size-[18px] text-muted" aria-hidden /> เพิ่มโฟลเดอร์กิจกรรม
              </Command.Item>
            </Command.Group>
            <Command.Group heading="ไปที่" className={GROUP}>
              {[
                { to: '/home', label: 'หน้าหลัก', icon: House },
                { to: '/search', label: 'ค้นหารูป', icon: ImageIcon },
                { to: '/people', label: 'บุคคล', icon: Users },
                { to: '/events', label: 'กิจกรรม / โฟลเดอร์', icon: Calendar },
                { to: '/activity', label: 'งานและประวัติ', icon: Activity },
                { to: '/settings', label: 'ตั้งค่า', icon: Settings },
              ].map((item) => (
                <Command.Item key={item.to} value={`go:${item.to}`} onSelect={run(() => navigate(item.to))} className={ITEM}>
                  <item.icon className="size-[18px] text-muted" aria-hidden /> {item.label}
                </Command.Item>
              ))}
            </Command.Group>
          </>
        )}
      </Command.List>
    </Command.Dialog>
  );
}
