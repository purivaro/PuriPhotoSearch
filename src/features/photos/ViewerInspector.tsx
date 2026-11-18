import { useEffect, useRef, useState } from 'react';
import { Link } from 'react-router';
import { toast } from 'sonner';
import { ArrowLeftRight, Calendar, CircleAlert, CircleCheck, CloudOff, Copy, EllipsisVertical, ExternalLink, FolderOpen, Image as ImageIcon, ScanFace, UserRound, UserRoundPen, X } from 'lucide-react';
import { LevelBadge } from '@/features/faces/LevelBadge';
import { Button, IconButton } from '@/components/ui/Button';
import { ConfirmDialog } from '@/components/ui/Dialog';
import { InlineNotice, StatusBadge } from '@/components/ui/Feedback';
import { Menu, MenuContent, MenuItem, MenuTrigger, Popover, PopoverContent, PopoverTrigger } from '@/components/ui/Overlay';
import { FaceAvatar } from '@/features/people/FaceAvatar';
import { PersonPicker } from '@/features/people/PersonPicker';
import { cn } from '@/lib/cn';
import { formatBytes, formatCount, formatTakenAt, formatThaiDate } from '@/lib/format';
import type { FaceBox, Id, PhotoDetails } from '@/lib/ipc/types';
import { useFaceMutations } from './useFaceMutations';
import { FILE_MANAGER } from '@/lib/platform';

interface ViewerInspectorProps {
  details: PhotoDetails;
  selectedFaceId: Id | null;
  onSelectFace: (faceId: Id | null) => void;
  drawMode: boolean;
  onDrawModeChange: (on: boolean) => void;
  onReveal: () => void;
  onOpenOriginal: () => void;
}

/** แผงรายละเอียดภาพ: ข้อมูลไฟล์ สถานะต้นฉบับ และคนในรูปพร้อมคำสั่งแก้ระดับใบหน้า */
export function ViewerInspector({ details, selectedFaceId, onSelectFace, drawMode, onDrawModeChange, onReveal, onOpenOriginal }: ViewerInspectorProps) {
  const taken = formatTakenAt(details.takenAt);
  const eventDate = formatThaiDate(details.eventDate);
  const copyPath = async () => {
    try {
      await navigator.clipboard.writeText(details.path);
      toast.success('คัดลอกตำแหน่งไฟล์แล้ว');
    } catch {
      toast.error('คัดลอกไม่สำเร็จ เลือกข้อความแล้วคัดลอกเองได้');
    }
  };

  return (
    <div className="scroll-thin flex h-full flex-col gap-4 overflow-y-auto p-5">
      <section aria-labelledby="vi-title">
        <h2 id="vi-title" className="mb-3 text-lg font-bold text-ink">
          รายละเอียดภาพ
        </h2>
        <p className="mb-2 text-[15px] font-semibold break-all text-ink select-text" data-selectable>
          {details.filename}
        </p>
        <dl className="space-y-1.5 text-sm">
          <div className="flex items-start gap-2">
            <FolderOpen className="mt-0.5 size-4 shrink-0 text-muted" aria-hidden />
            <dt className="sr-only">กิจกรรม</dt>
            <dd className="min-w-0">
              <Link to={`/events/${details.eventId}`} className="font-semibold text-action hover:underline">
                {details.eventTitle}
              </Link>
              <span className="block text-[13px] text-muted">{eventDate ? `วันกิจกรรม ${eventDate}` : 'กิจกรรมนี้ยังไม่ระบุวันที่'}</span>
            </dd>
          </div>
          <div className="flex items-start gap-2">
            <Calendar className="mt-0.5 size-4 shrink-0 text-muted" aria-hidden />
            <dt className="sr-only">วันถ่าย</dt>
            <dd>
              {taken ? (
                <>
                  ถ่ายเมื่อ {taken}
                  <span className="block text-[13px] text-muted">จากข้อมูลในไฟล์ (EXIF) เวลาท้องถิ่นของกล้อง</span>
                </>
              ) : (
                <span className="text-muted">ไม่ทราบวันถ่าย (ไฟล์ไม่มีข้อมูลวันถ่าย)</span>
              )}
            </dd>
          </div>
          <div className="flex items-start gap-2">
            <ImageIcon className="mt-0.5 size-4 shrink-0 text-muted" aria-hidden />
            <dt className="sr-only">ขนาดภาพ</dt>
            <dd>
              {details.width > 0 ? `${formatCount(details.width)} × ${formatCount(details.height)}` : 'ยังไม่ทราบขนาดภาพ'}
              {details.fileSize > 0 ? ` · ${formatBytes(details.fileSize)}` : ''}
            </dd>
          </div>
          <div className="flex items-start gap-2">
            {details.originalAvailable ? (
              <CircleCheck className="mt-0.5 size-4 shrink-0 text-ok" aria-hidden />
            ) : (
              <CloudOff className="mt-0.5 size-4 shrink-0 text-warn" aria-hidden />
            )}
            <dt className="sr-only">สถานะต้นฉบับ</dt>
            <dd className={details.originalAvailable ? 'text-ok' : 'text-warn'}>{details.originalAvailable ? 'ต้นฉบับพร้อมใช้' : 'ภาพต้นฉบับไม่พร้อม'}</dd>
          </div>
        </dl>

        {!details.originalAvailable && (
          <InlineNotice tone="warn" className="mt-3">
            โฟลเดอร์ต้นทางไม่พร้อมใช้งาน เชื่อมต่อไดรฟ์แล้วเปิดรูปนี้อีกครั้ง ระหว่างนี้ดูภาพย่อและชื่อบุคคลได้ แต่เปิดหรือส่งออกไฟล์ต้นฉบับไม่ได้
          </InlineNotice>
        )}
        {details.decodeError && (
          <InlineNotice tone="danger" icon={<CircleAlert aria-hidden />} className="mt-3">
            อ่านไฟล์รูปนี้ไม่สำเร็จ: {details.decodeError}
          </InlineNotice>
        )}

        <div className="mt-3 flex items-start gap-1.5 rounded-xl bg-sunken px-3 py-2">
          <p className="min-w-0 flex-1 text-[12.5px] leading-relaxed break-all text-muted select-text" data-selectable>
            {details.path}
          </p>
          <IconButton label="คัดลอกตำแหน่งไฟล์" size="sm" onClick={() => void copyPath()}>
            <Copy className="size-4" aria-hidden />
          </IconButton>
        </div>

        <div className="mt-3 grid grid-cols-2 gap-2">
          <Button size="sm" icon={<FolderOpen className="size-4" aria-hidden />} onClick={onReveal} disabled={!details.originalAvailable} data-testid="viewer-reveal">
            เปิดใน {FILE_MANAGER}
          </Button>
          <Button size="sm" icon={<ExternalLink className="size-4" aria-hidden />} onClick={onOpenOriginal} disabled={!details.originalAvailable} data-testid="viewer-open-original">
            เปิดต้นฉบับ
          </Button>
        </div>
      </section>

      <section aria-labelledby="vi-people" className="border-t border-line pt-4">
        <h2 id="vi-people" className="mb-2 text-lg font-bold text-ink">
          คนในรูป ({formatCount(details.faces.length)})
        </h2>
        {details.faces.length === 0 ? (
          <p className="mb-3 text-[13.5px] text-muted">ยังไม่มีใบหน้าในรูปนี้ ถ้ามีใบหน้าที่ระบบไม่พบ กด “เพิ่มกรอบใบหน้า” แล้วลากกรอบรอบใบหน้าเพื่อบอกว่าเป็นใคร</p>
        ) : (
          <ul className="mb-3 space-y-1.5">
            {details.faces.map((face) => (
              <FaceRow key={face.id} face={face} photoId={details.id} selected={face.id === selectedFaceId} onSelect={() => onSelectFace(face.id === selectedFaceId ? null : face.id)} />
            ))}
          </ul>
        )}
        <Button
          className="w-full"
          variant={drawMode ? 'outline' : 'secondary'}
          icon={drawMode ? <X className="size-4" aria-hidden /> : <ScanFace className="size-[18px]" aria-hidden />}
          onClick={() => onDrawModeChange(!drawMode)}
          aria-pressed={drawMode}
          data-testid="viewer-add-face"
        >
          {drawMode ? 'ยกเลิกการเพิ่มกรอบ' : 'เพิ่มกรอบใบหน้า'}
        </Button>
      </section>
    </div>
  );
}

function FaceRow({ face, photoId, selected, onSelect }: { face: FaceBox; photoId: Id; selected: boolean; onSelect: () => void }) {
  const { assign, confirmSuggestion, rejectSuggestion, unassign, ignore, setPersonCover } = useFaceMutations(photoId);
  const [picking, setPicking] = useState(false);
  const [confirmIgnore, setConfirmIgnore] = useState(false);
  const rowRef = useRef<HTMLLIElement>(null);
  const person = face.person;
  // ข้อเสนอของ AI แสดงเฉพาะเมื่อยังไม่มีคนที่ผู้ใช้ยืนยัน
  const suggestion = person ? null : face.suggestion;
  const busy = assign.isPending || unassign.isPending || ignore.isPending || confirmSuggestion.isPending || rejectSuggestion.isPending;

  useEffect(() => {
    if (selected) rowRef.current?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  return (
    <li
      ref={rowRef}
      data-testid="face-row"
      data-face-id={face.id}
      data-face-state={face.person ? 'confirmed' : face.suggestion ? 'suggested' : 'unknown'}
      className={cn('rounded-xl border transition-colors', selected ? 'border-brand-line bg-brand-soft' : 'border-transparent hover:bg-sunken')}
    >
      <button type="button" onClick={onSelect} aria-expanded={selected} className="flex w-full items-center gap-3 rounded-xl px-2.5 py-2 text-left">
        {/* ครอปของใบหน้านี้เอง ไม่ใช่ภาพตัวแทนของบุคคล */}
        <FaceAvatar asset={`face/${face.id}`} name={person?.displayName ?? ''} decorative className="size-11" />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[15px] font-bold text-ink">{person ? person.displayName : suggestion ? `อาจเป็น “${suggestion.person.displayName}”` : 'ยังไม่ทราบชื่อ'}</span>
          {person ? (
            <StatusBadge tone="ok" icon={<CircleCheck aria-hidden />}>
              ยืนยันแล้ว
            </StatusBadge>
          ) : suggestion ? (
            <span className="flex flex-wrap items-center gap-1">
              <StatusBadge tone="neutral" icon={<ScanFace aria-hidden />}>
                AI เสนอ · รอตรวจสอบ
              </StatusBadge>
              <LevelBadge level={suggestion.level} />
            </span>
          ) : (
            <StatusBadge tone="neutral" icon={<ScanFace aria-hidden />}>
              รอระบุชื่อ
            </StatusBadge>
          )}
        </span>
      </button>

      {selected && (
        <div className="flex flex-wrap items-center gap-1.5 px-2.5 pb-2.5">
          {person && (
            <Link
              to={`/people/${person.id}`}
              className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-line-strong bg-surface px-2.5 text-[13px] font-semibold text-ink hover:bg-sunken"
            >
              <UserRound className="size-3.5" aria-hidden /> ดูบุคคล
            </Link>
          )}
          {suggestion && (
            <>
              <Button size="sm" variant="primary" icon={<CircleCheck className="size-3.5" aria-hidden />} busy={confirmSuggestion.isPending} disabled={busy} onClick={() => confirmSuggestion.mutate(face)} data-testid="face-suggestion-accept">
                ใช่
              </Button>
              <Button size="sm" variant="danger" icon={<X className="size-3.5" aria-hidden />} busy={rejectSuggestion.isPending} disabled={busy} onClick={() => rejectSuggestion.mutate(face)} data-testid="face-suggestion-reject">
                ไม่ใช่
              </Button>
            </>
          )}
          <Popover open={picking} onOpenChange={setPicking}>
            <PopoverTrigger asChild>
              <Button size="sm" icon={person ? <ArrowLeftRight className="size-3.5" aria-hidden /> : <UserRoundPen className="size-3.5" aria-hidden />} disabled={busy} data-testid="face-assign">
                {person ? 'เปลี่ยนบุคคล' : suggestion ? 'คนอื่น…' : 'ระบุชื่อ'}
              </Button>
            </PopoverTrigger>
            <PopoverContent align="end" aria-label={person ? 'เปลี่ยนบุคคลของใบหน้านี้' : 'ระบุชื่อใบหน้านี้'}>
              <PersonPicker
                busy={assign.isPending}
                excludeIds={person ? [person.id] : suggestion ? [suggestion.person.id] : []}
                onPick={(choice) => assign.mutate({ faceId: face.id, choice }, { onSuccess: () => setPicking(false) })}
              />
            </PopoverContent>
          </Popover>
          {person && (
            <Button size="sm" variant="danger" icon={<X className="size-3.5" aria-hidden />} busy={unassign.isPending} disabled={busy} onClick={() => unassign.mutate(face)} data-testid="face-unassign">
              ไม่ใช่บุคคลนี้
            </Button>
          )}
          <Menu>
            <MenuTrigger asChild>
              <IconButton label="คำสั่งเพิ่มเติมของใบหน้านี้" size="sm" disabled={busy}>
                <EllipsisVertical className="size-4" aria-hidden />
              </IconButton>
            </MenuTrigger>
            <MenuContent>
              {person && (
                <MenuItem icon={<ImageIcon />} onSelect={() => setPersonCover.mutate({ personId: person.id, faceId: face.id })}>
                  ใช้เป็นภาพตัวแทนของ {person.displayName}
                </MenuItem>
              )}
              <MenuItem tone="danger" icon={<X />} onSelect={() => setConfirmIgnore(true)}>
                ไม่ใช่ใบหน้า
              </MenuItem>
            </MenuContent>
          </Menu>
        </div>
      )}

      <ConfirmDialog
        open={confirmIgnore}
        onOpenChange={setConfirmIgnore}
        title="เอากรอบนี้ออกเพราะไม่ใช่ใบหน้า?"
        description={
          person
            ? `กรอบนี้จะไม่แสดงอีก และ ${person.displayName} จะไม่ถูกนับว่าอยู่ในรูปนี้จากกรอบนี้ รูปต้นฉบับและบุคคลไม่ถูกลบ`
            : 'กรอบนี้จะไม่แสดงอีก รูปต้นฉบับไม่ถูกแก้ไข'
        }
        confirmLabel="เอากรอบออก"
        tone="danger"
        busy={ignore.isPending}
        onConfirm={() => ignore.mutate(face, { onSettled: () => setConfirmIgnore(false) })}
      />
    </li>
  );
}
