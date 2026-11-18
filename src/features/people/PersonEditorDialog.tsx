import { useMemo, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router';
import { toast } from 'sonner';
import { Plus, TriangleAlert, X } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { ConfirmDialog, Dialog } from '@/components/ui/Dialog';
import { ErrorState, InlineNotice, LoadingBlock } from '@/components/ui/Feedback';
import { Field, TextArea, TextInput } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { errorMessage, isErrorCode, newRequestId } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { Id, PersonDetail } from '@/lib/ipc/types';
import { invalidatePeopleData, qk, useGroups, usePeople } from '@/lib/queries';
import { FaceAvatar } from './FaceAvatar';

interface PersonEditorDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** ไม่ส่ง = เพิ่มบุคคลใหม่ */
  personId?: Id;
  onSaved?: (person: PersonDetail) => void;
}

interface FormValues {
  displayName: string;
  nickname: string;
  aliases: string[];
  groupIds: Id[];
  notes: string;
}

const EMPTY: FormValues = { displayName: '', nickname: '', aliases: [], groupIds: [], notes: '' };

function valuesOf(person: PersonDetail): FormValues {
  return {
    displayName: person.displayName,
    nickname: person.nickname ?? '',
    aliases: person.aliases,
    groupIds: person.groups.map((g) => g.id),
    notes: person.notes,
  };
}

/** S10 — เพิ่มหรือแก้ไขบุคคล: ตั้งชื่อได้เร็ว รายละเอียดเสริมไม่บังคับ */
export function PersonEditorDialog({ open, onOpenChange, personId, onSaved }: PersonEditorDialogProps) {
  const existing = useQuery({
    queryKey: qk.person(personId ?? ''),
    queryFn: () => call('get_person', { personId: personId as Id }),
    enabled: open && Boolean(personId),
  });
  const [dirty, setDirty] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [busy, setBusy] = useState(false);

  const requestClose = (next: boolean) => {
    if (next) return;
    if (busy) return;
    // ถามเฉพาะเมื่อมีการแก้จริง
    if (dirty) setConfirmDiscard(true);
    else onOpenChange(false);
  };

  const title = personId ? 'แก้ไขบุคคล' : 'เพิ่มบุคคล';
  return (
    <>
      <Dialog open={open} onOpenChange={requestClose} title={title} size="md" dismissible={!busy}>
        {personId && existing.isPending ? (
          <LoadingBlock label="กำลังโหลดข้อมูลบุคคล…" className="py-10" />
        ) : personId && existing.isError ? (
          <ErrorState error={existing.error} onRetry={() => void existing.refetch()} compact />
        ) : (
          <PersonForm
            key={existing.data ? `${existing.data.id}:${existing.data.recordVersion}` : 'new'}
            person={existing.data}
            onDirtyChange={setDirty}
            onBusyChange={setBusy}
            onCancel={() => requestClose(false)}
            onReload={() => void existing.refetch()}
            onSaved={(person) => {
              setDirty(false);
              onOpenChange(false);
              onSaved?.(person);
            }}
          />
        )}
      </Dialog>
      <ConfirmDialog
        open={confirmDiscard}
        onOpenChange={setConfirmDiscard}
        title="ทิ้งการแก้ไขที่ยังไม่บันทึก?"
        description="ข้อมูลที่พิมพ์ไว้ในฟอร์มนี้จะไม่ถูกบันทึก"
        confirmLabel="ทิ้งการแก้ไข"
        cancelLabel="กลับไปแก้ต่อ"
        tone="danger"
        onConfirm={() => {
          setConfirmDiscard(false);
          setDirty(false);
          onOpenChange(false);
        }}
      />
    </>
  );
}

function PersonForm({
  person,
  onDirtyChange,
  onBusyChange,
  onCancel,
  onReload,
  onSaved,
}: {
  person?: PersonDetail;
  onDirtyChange: (dirty: boolean) => void;
  onBusyChange: (busy: boolean) => void;
  onCancel: () => void;
  onReload: () => void;
  onSaved: (person: PersonDetail) => void;
}) {
  const qc = useQueryClient();
  const initial = useMemo(() => (person ? valuesOf(person) : EMPTY), [person]);
  const [values, setValues] = useState<FormValues>(initial);
  const [aliasDraft, setAliasDraft] = useState('');
  const [groupDraft, setGroupDraft] = useState<string | null>(null);
  const [touched, setTouched] = useState(false);
  // หนึ่ง requestId ต่อการยืนยันหนึ่งครั้ง: กดซ้ำ/ลองใหม่หลัง error ใช้ค่าเดิม
  const requestId = useRef(newRequestId());
  const groups = useGroups();

  const update = (patch: Partial<FormValues>) => {
    const next = { ...values, ...patch };
    setValues(next);
    onDirtyChange(JSON.stringify(next) !== JSON.stringify(initial));
  };

  const name = values.displayName.trim();
  const nameError = touched && !name ? 'กรุณากรอกชื่อที่ใช้แสดง' : null;

  // ชื่อซ้ำได้ แต่ต้องเห็นคนเดิมก่อนตัดสินใจ
  const lookup = usePeople({ text: name, includeArchived: true }, name.length > 0);
  const sameName = (lookup.data ?? []).filter((p) => p.id !== person?.id && p.displayName.trim().toLocaleLowerCase() === name.toLocaleLowerCase());

  const save = useMutation({
    mutationFn: () =>
      call('save_person', {
        personId: person?.id,
        expectedVersion: person?.recordVersion,
        requestId: requestId.current,
        displayName: name,
        nickname: values.nickname.trim() || null,
        notes: values.notes.trim(),
        aliases: values.aliases,
        groupIds: values.groupIds,
      }),
    onMutate: () => onBusyChange(true),
    onSettled: () => onBusyChange(false),
    onSuccess: (saved) => {
      requestId.current = newRequestId();
      qc.setQueryData(qk.person(saved.id), saved);
      invalidatePeopleData(qc);
      toast.success(person ? `บันทึกการแก้ไข ${saved.displayName} แล้ว` : `สร้างบุคคล ${saved.displayName} แล้ว`);
      onSaved(saved);
    },
  });

  const createGroup = useMutation({
    mutationFn: (groupName: string) => call('create_group', { name: groupName }),
    onSuccess: (group) => {
      void qc.invalidateQueries({ queryKey: qk.groups });
      update({ groupIds: [...values.groupIds, group.id] });
      setGroupDraft(null);
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  const addAlias = () => {
    const alias = aliasDraft.trim();
    if (!alias) return;
    if (!values.aliases.includes(alias)) update({ aliases: [...values.aliases, alias] });
    setAliasDraft('');
  };

  const conflict = isErrorCode(save.error, 'VERSION_CONFLICT');

  return (
    <form
      className="space-y-4"
      onSubmit={(event) => {
        event.preventDefault();
        setTouched(true);
        if (!name || save.isPending) return;
        save.mutate();
      }}
    >
      {person && (
        <div className="flex items-center gap-3 rounded-xl bg-sunken px-3 py-2.5">
          <FaceAvatar faceId={person.coverFaceId} name={person.displayName} shape="rounded" decorative className="size-12" />
          <p className="text-[13.5px] text-muted">
            แก้ชื่อหรือข้อมูลแล้ว รูปและใบหน้าที่ยืนยันไว้ยังผูกกับบุคคลนี้เหมือนเดิม
          </p>
        </div>
      )}

      <Field label="ชื่อที่ใช้แสดง" error={nameError}>
        {(props) => (
          <TextInput
            {...props}
            data-testid="person-name-input"
            data-autofocus
            autoFocus
            value={values.displayName}
            invalid={Boolean(nameError)}
            onChange={(event) => update({ displayName: event.target.value })}
            onBlur={() => setTouched(true)}
            placeholder="เช่น พระมหาสมชาย หรือ คุณกานต์"
          />
        )}
      </Field>

      {sameName.length > 0 && (
        <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
          <p className="font-semibold">มีบุคคลชื่อนี้อยู่แล้ว {sameName.length} คน</p>
          <p className="mb-2">ถ้าเป็นคนเดียวกันให้เปิดคนเดิม ถ้าเป็นคนละคนบันทึกชื่อซ้ำได้</p>
          <ul className="space-y-1">
            {sameName.slice(0, 3).map((p) => (
              <li key={p.id} className="flex items-center gap-2">
                <FaceAvatar faceId={p.coverFaceId} name={p.displayName} decorative className="size-7" />
                <span className="min-w-0 flex-1 truncate">
                  {p.displayName}
                  {p.nickname ? ` (${p.nickname})` : ''} · {p.confirmedPhotoCount} รูป
                </span>
                <Link to={`/people/${p.id}`} onClick={onCancel} className="shrink-0 font-semibold underline">
                  เปิดคนเดิม
                </Link>
              </li>
            ))}
          </ul>
        </InlineNotice>
      )}

      <Field label="ชื่อเล่น / ชื่อรอง" optional hint="ช่วยแยกคนชื่อซ้ำ และใช้ค้นหาได้">
        {(props) => <TextInput {...props} value={values.nickname} onChange={(event) => update({ nickname: event.target.value })} />}
      </Field>

      <Field label="ชื่ออื่นที่ใช้ค้นหา" optional hint="พิมพ์แล้วกด Enter เพื่อเพิ่มทีละชื่อ">
        {(props) => (
          <div>
            {values.aliases.length > 0 && (
              <ul className="mb-2 flex flex-wrap gap-1.5">
                {values.aliases.map((alias) => (
                  <li key={alias} className="inline-flex items-center gap-1 rounded-full bg-sunken py-0.5 pr-1 pl-3 text-[13.5px] text-ink">
                    {alias}
                    <button
                      type="button"
                      aria-label={`ลบชื่อ ${alias}`}
                      onClick={() => update({ aliases: values.aliases.filter((a) => a !== alias) })}
                      className="flex size-6 items-center justify-center rounded-full text-muted hover:bg-line hover:text-ink"
                    >
                      <X className="size-3.5" aria-hidden />
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <div className="flex gap-2">
              <TextInput
                {...props}
                value={aliasDraft}
                onChange={(event) => setAliasDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') {
                    event.preventDefault();
                    addAlias();
                  }
                }}
                placeholder="เช่น ชื่อภาษาอังกฤษ หรือฉายา"
              />
              <Button onClick={addAlias} disabled={!aliasDraft.trim()}>
                เพิ่ม
              </Button>
            </div>
          </div>
        )}
      </Field>

      <div>
        <p className="mb-1.5 flex items-baseline gap-2 text-sm font-semibold text-ink">
          กลุ่ม <span className="text-[12.5px] font-normal text-muted">ไม่บังคับ · อยู่ได้หลายกลุ่ม</span>
        </p>
        {groups.isError ? (
          <ErrorState compact error={groups.error} title="โหลดรายชื่อกลุ่มไม่สำเร็จ" onRetry={() => void groups.refetch()} />
        ) : (
          <div className="flex flex-wrap items-center gap-1.5">
            {(groups.data ?? []).map((group) => {
              const selected = values.groupIds.includes(group.id);
              return (
                <button
                  key={group.id}
                  type="button"
                  aria-pressed={selected}
                  onClick={() => update({ groupIds: selected ? values.groupIds.filter((id) => id !== group.id) : [...values.groupIds, group.id] })}
                  className={cn(
                    'h-8 rounded-full border px-3 text-[13.5px] font-semibold transition-colors',
                    selected ? 'border-brand-line bg-brand-soft text-action' : 'border-line-strong bg-surface text-muted hover:text-ink',
                  )}
                >
                  {group.name}
                </button>
              );
            })}
            {groupDraft === null ? (
              <button
                type="button"
                onClick={() => setGroupDraft('')}
                className="inline-flex h-8 items-center gap-1 rounded-full border border-dashed border-line-strong px-3 text-[13.5px] font-semibold text-muted hover:text-ink"
              >
                <Plus className="size-3.5" aria-hidden /> สร้างกลุ่ม
              </button>
            ) : (
              <span className="inline-flex items-center gap-1.5">
                <TextInput
                  autoFocus
                  aria-label="ชื่อกลุ่มใหม่"
                  value={groupDraft}
                  onChange={(event) => setGroupDraft(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault();
                      if (groupDraft.trim()) createGroup.mutate(groupDraft.trim());
                    }
                    if (event.key === 'Escape') {
                      event.stopPropagation();
                      setGroupDraft(null);
                    }
                  }}
                  placeholder="ชื่อกลุ่ม เช่น ทีมงาน"
                  className="h-8 w-44"
                />
                <Button size="sm" busy={createGroup.isPending} disabled={!groupDraft.trim()} onClick={() => createGroup.mutate(groupDraft.trim())}>
                  สร้าง
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setGroupDraft(null)}>
                  ยกเลิก
                </Button>
              </span>
            )}
          </div>
        )}
      </div>

      <Field label="หมายเหตุ" optional>
        {(props) => <TextArea {...props} value={values.notes} onChange={(event) => update({ notes: event.target.value })} rows={3} />}
      </Field>

      {!person && <p className="text-[13.5px] text-muted">สร้างบุคคลไว้ก่อนได้ แล้วค่อยเชื่อมกับใบหน้าในรูปจากตัวดูภาพภายหลัง</p>}

      {save.isError &&
        (conflict ? (
          <InlineNotice
            tone="warn"
            icon={<TriangleAlert aria-hidden />}
            action={
              <Button size="sm" onClick={onReload}>
                โหลดข้อมูลล่าสุด
              </Button>
            }
          >
            ข้อมูลนี้เปลี่ยนหลังจากที่เปิดหน้านี้ กรุณาตรวจข้อมูลล่าสุดก่อนบันทึก
          </InlineNotice>
        ) : (
          <InlineNotice tone="danger" icon={<TriangleAlert aria-hidden />}>
            {errorMessage(save.error)}
          </InlineNotice>
        ))}

      <div className="-mx-6 -mb-5 flex justify-end gap-2.5 border-t border-line px-6 py-4">
        <Button onClick={onCancel} disabled={save.isPending}>
          ยกเลิก
        </Button>
        <Button type="submit" variant="primary" busy={save.isPending} data-testid="person-save-button">
          {person ? 'บันทึกการแก้ไข' : 'สร้างบุคคล'}
        </Button>
      </div>
    </form>
  );
}
