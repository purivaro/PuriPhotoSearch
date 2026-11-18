import { useDeferredValue, useState } from 'react';
import { Command } from 'cmdk';
import { Search, UserPlus } from 'lucide-react';
import { ErrorState, Spinner } from '@/components/ui/Feedback';
import { formatCount } from '@/lib/format';
import type { Id, PersonSummary } from '@/lib/ipc/types';
import { usePeople } from '@/lib/queries';
import { FaceAvatar } from './FaceAvatar';

export type PersonChoice = { kind: 'existing'; person: PersonSummary } | { kind: 'new'; name: string };

interface PersonPickerProps {
  onPick: (choice: PersonChoice) => void;
  /** คนที่ไม่ต้องแสดง (เช่นคนที่ผูกกับใบหน้านี้อยู่แล้ว) */
  excludeIds?: Id[];
  /** อนุญาตสร้างบุคคลใหม่จากข้อความที่พิมพ์ */
  allowCreate?: boolean;
  busy?: boolean;
  placeholder?: string;
}

/**
 * ช่องเลือกบุคคล: ค้นจากชื่อ/ชื่อเล่น แสดงภาพหน้าและจำนวนรูปเพื่อแยกคนชื่อซ้ำ
 * เลือกด้วยลูกศรขึ้น/ลง + Enter ได้ ไม่ใช้ชื่อเป็นตัวตน (ส่งกลับเป็น ID)
 */
export function PersonPicker({ onPick, excludeIds = [], allowCreate = true, busy, placeholder = 'พิมพ์ชื่อเพื่อค้นหาหรือสร้างบุคคล' }: PersonPickerProps) {
  const [text, setText] = useState('');
  const deferred = useDeferredValue(text.trim());
  const people = usePeople(deferred ? { text: deferred } : {});
  const options = (people.data ?? []).filter((p) => !excludeIds.includes(p.id)).slice(0, 40);
  const name = text.trim();
  const sameName = name ? options.filter((p) => p.displayName.trim().toLocaleLowerCase() === name.toLocaleLowerCase()).length : 0;

  return (
    <Command shouldFilter={false} label="เลือกบุคคล" className="w-[320px] max-w-[calc(100vw-48px)]">
      <div className="flex items-center gap-2 border-b border-line px-3">
        <Search className="size-4 shrink-0 text-muted" aria-hidden />
        <Command.Input
          autoFocus
          data-testid="person-picker-input"
          value={text}
          onValueChange={setText}
          placeholder={placeholder}
          disabled={busy}
          className="h-11 min-w-0 flex-1 bg-transparent text-sm text-ink placeholder:text-faint focus:outline-none"
        />
        {(people.isFetching || busy) && <Spinner className="size-4" />}
      </div>
      <Command.List className="scroll-thin max-h-[280px] overflow-y-auto p-1.5">
        {people.isError && <ErrorState compact error={people.error} title="โหลดรายชื่อไม่สำเร็จ" onRetry={() => void people.refetch()} />}
        {people.isSuccess && options.length === 0 && !name && (
          <p className="px-3 py-5 text-center text-[13.5px] text-muted">ยังไม่มีบุคคลในคลัง พิมพ์ชื่อเพื่อสร้างคนแรก</p>
        )}
        {people.isSuccess && options.length === 0 && name && !allowCreate && (
          <p className="px-3 py-5 text-center text-[13.5px] text-muted">ไม่พบบุคคลที่ชื่อตรงกับ “{name}”</p>
        )}
        {options.map((person) => (
          <Command.Item
            key={person.id}
            value={person.id}
            disabled={busy}
            data-testid="person-picker-option"
            data-person-id={person.id}
            onSelect={() => onPick({ kind: 'existing', person })}
            className="flex cursor-pointer items-center gap-3 rounded-lg px-2 py-1.5 data-[disabled=true]:opacity-50 data-[selected=true]:bg-selected"
          >
            <FaceAvatar faceId={person.coverFaceId} name={person.displayName} decorative className="size-9" />
            <span className="min-w-0 flex-1">
              <span className="block truncate text-sm font-semibold text-ink">{person.displayName}</span>
              <span className="block truncate text-[12.5px] text-muted">
                {person.nickname ? `${person.nickname} · ` : ''}
                {formatCount(person.confirmedPhotoCount)} รูป
                {person.groups.length > 0 ? ` · ${person.groups.map((g) => g.name).join(', ')}` : ''}
              </span>
            </span>
          </Command.Item>
        ))}
        {allowCreate && name && (
          <Command.Item
            value="__create__"
            disabled={busy}
            data-testid="person-picker-create"
            onSelect={() => onPick({ kind: 'new', name })}
            className="mt-1 flex cursor-pointer items-center gap-3 rounded-lg border-t border-line px-2 py-2 data-[disabled=true]:opacity-50 data-[selected=true]:bg-selected"
          >
            <span className="flex size-9 shrink-0 items-center justify-center rounded-full bg-brand-soft text-action">
              <UserPlus className="size-[18px]" aria-hidden />
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-sm font-semibold text-action">สร้างบุคคลใหม่ “{name}”</span>
              {sameName > 0 && <span className="block text-[12.5px] text-muted">มีชื่อนี้อยู่แล้ว {sameName} คน — จะสร้างเป็นอีกคนหนึ่ง</span>}
            </span>
          </Command.Item>
        )}
      </Command.List>
    </Command>
  );
}
