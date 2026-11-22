import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Check, FileUp, Info, MoreHorizontal, Pencil, Plus, SlidersHorizontal, Trash2, TriangleAlert } from 'lucide-react';
import { useFolderChooser } from '@/app/FolderChooser';
import { Button, IconButton } from '@/components/ui/Button';
import { ConfirmDialog, Dialog } from '@/components/ui/Dialog';
import { EmptyState, ErrorState, InlineNotice, LoadingBlock } from '@/components/ui/Feedback';
import { Field, TextInput } from '@/components/ui/Form';
import { Menu, MenuContent, MenuItem, MenuTrigger } from '@/components/ui/Overlay';
import { cn } from '@/lib/cn';
import { errorMessage } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { EditMask, EditPreset } from '@/lib/ipc/types';
import { isNeutral, sameAdjust, type Adjust } from './adjust';
import { describeEdit } from './editSummary';
import { IDENTITY } from './editGeometry';
import { isPortableMask, maskIsActive } from './masks';

const PRESETS_KEY = ['edit-presets'] as const;
const collator = new Intl.Collator('th', { numeric: true, sensitivity: 'base' });

/** preset ทั้งหมด เรียงแบบพจนานุกรมไทย (สระนำ ใ ไ เ แ โ นับตามพยัญชนะ) และตัวเลขตามค่า */
export function usePresets() {
  return useQuery({
    queryKey: PRESETS_KEY,
    queryFn: () => call('list_edit_presets', {}),
    select: (list) => [...list].sort((a, b) => collator.compare(a.name, b.name)),
  });
}

/** สรุปค่าใน preset (แสง · สี · Mask ตัวคน) */
export function presetSummary(adjust: Adjust, masks: EditMask[] = []): string {
  const d = describeEdit({ ...IDENTITY, adjust });
  const subject = masks.length > 0 ? `Mask ตัวคน/ส่วนของคน${masks.length > 1 ? ` ${masks.length} อัน` : ''}` : null;
  return [d.light, d.color, subject].filter(Boolean).join(' · ');
}

interface EditPresetsProps {
  adjust: Adjust;
  /** Mask ของรูปนี้ — บันทึก preset เก็บ Mask ตัวคนที่มีผลไปด้วย */
  masks: EditMask[];
  /** ใช้ preset: แทนค่าแสงและสีทั้งหมด และเพิ่ม Mask ตัวคนของ preset */
  onApply: (preset: { adjust: Adjust; masks: EditMask[] }) => void;
  /** ชี้ค้างที่ preset: ดูตัวอย่างบนภาพก่อนใช้ (null = เลิกดู) */
  onPreview: (adjust: Adjust | null) => void;
}

/** preset ของแสงและสี: บันทึกค่าที่ปรับไว้ ใช้ซ้ำ และนำเข้า .xmp จาก Lightroom (DECISIONS ข้อ 20) */
export function EditPresets({ adjust, masks, onApply, onPreview }: EditPresetsProps) {
  // Mask ตัวคนไปกับ preset ได้ (ไม่ผูกกับรูป) · Mask ใบหน้าผูกกับใบหน้าของรูปนี้ จึงไม่เก็บ
  const subjectMasks = masks.filter((m) => isPortableMask(m) && maskIsActive(m));
  const qc = useQueryClient();
  const choose = useFolderChooser();
  const presets = usePresets();
  const [naming, setNaming] = useState<{ mode: 'save' } | { mode: 'rename'; preset: EditPreset } | null>(null);
  const [deleting, setDeleting] = useState<EditPreset | null>(null);
  const [imported, setImported] = useState<EditPreset | null>(null);

  const setList = (list: EditPreset[]) => qc.setQueryData(PRESETS_KEY, list);

  const save = useMutation({
    mutationFn: (name: string) => call('save_edit_preset', { name, adjust, masks: subjectMasks }),
    onSuccess: (preset) => {
      void qc.invalidateQueries({ queryKey: PRESETS_KEY });
      setNaming(null);
      toast.success(`บันทึก preset “${preset.name}” แล้ว`);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const rename = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => call('rename_edit_preset', { id, name }),
    onSuccess: (list) => {
      setList(list);
      setNaming(null);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const remove = useMutation({
    mutationFn: (id: string) => call('delete_edit_preset', { id }),
    onSuccess: (list, id) => {
      setList(list);
      const name = deleting?.id === id ? deleting.name : 'preset';
      setDeleting(null);
      toast.success(`ลบ “${name}” แล้ว รูปที่ใช้ไปแล้วไม่เปลี่ยน`);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const importXmp = useMutation({
    mutationFn: (path: string) => call('import_lightroom_preset', { path }),
    onSuccess: (preset) => {
      void qc.invalidateQueries({ queryKey: PRESETS_KEY });
      setImported(preset);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const startImport = async () => {
    const path = await choose({
      title: 'นำเข้า preset ของ Lightroom',
      hint: 'เลือกไฟล์ .xmp (Lightroom Classic / Lightroom / Camera Raw) — ไฟล์เดิมไม่ถูกแตะ',
      kind: 'preset',
    });
    if (path) importXmp.mutate(path);
  };

  const list = presets.data ?? [];
  const canSave = !isNeutral(adjust) || subjectMasks.length > 0;

  return (
    <div className="space-y-4" data-testid="editor-presets">
      <div className="grid grid-cols-2 gap-1.5">
        <Button size="sm" icon={<Plus className="size-4" aria-hidden />} disabled={!canSave} onClick={() => setNaming({ mode: 'save' })} data-testid="preset-save">
          บันทึกค่าปัจจุบัน
        </Button>
        <Button size="sm" icon={<FileUp className="size-4" aria-hidden />} busy={importXmp.isPending} onClick={() => void startImport()} data-testid="preset-import">
          นำเข้า .xmp
        </Button>
      </div>
      {!canSave && list.length > 0 && <p className="-mt-2 text-[12.5px] text-muted">ปรับแสงหรือสีก่อน จึงจะบันทึกเป็น preset ได้</p>}

      {presets.isPending ? (
        <LoadingBlock />
      ) : presets.isError ? (
        <ErrorState compact error={presets.error} onRetry={() => void presets.refetch()} />
      ) : list.length === 0 ? (
        <EmptyState
          compact
          icon={<SlidersHorizontal aria-hidden />}
          title="ยังไม่มี preset"
          description="ปรับแสงและสีให้ถูกใจแล้วกด “บันทึกค่าปัจจุบัน” หรือนำเข้า preset .xmp จาก Lightroom"
        />
      ) : (
        <ul className="space-y-1.5" aria-label="preset" onPointerLeave={() => onPreview(null)}>
          {list.map((preset) => {
            const active = sameAdjust(preset.adjust, adjust);
            return (
              <li key={preset.id} className={cn('group flex items-stretch rounded-xl border', active ? 'border-action bg-brand-soft' : 'border-line hover:bg-sunken')}>
                <button
                  type="button"
                  aria-pressed={active}
                  onPointerEnter={() => onPreview(preset.adjust)}
                  onFocus={() => onPreview(preset.adjust)}
                  onBlur={() => onPreview(null)}
                  onClick={() => {
                    onPreview(null);
                    onApply(preset);
                  }}
                  className="min-w-0 flex-1 rounded-l-xl px-3 py-2 text-left"
                  data-testid="preset-item"
                >
                  <span className="flex items-center gap-1.5">
                    <span className="truncate text-sm font-semibold text-ink">{preset.name}</span>
                    {active && <Check className="size-4 shrink-0 text-action" aria-label="ใช้อยู่" />}
                  </span>
                  <span className="mt-0.5 flex items-center gap-1.5 text-[12px] text-muted">
                    <span className="shrink-0 rounded bg-sunken px-1.5 py-px font-semibold text-muted group-hover:bg-surface">{preset.source === 'lightroom' ? 'Lightroom' : 'ของฉัน'}</span>
                    {preset.unsupported.length > 0 && (
                      <span className="flex items-center gap-1 text-warn" title={`นำเข้าไม่ได้: ${preset.unsupported.join(' · ')}`}>
                        <TriangleAlert className="size-3.5" aria-hidden />
                        ไม่ครบ {preset.unsupported.length} ส่วน
                      </span>
                    )}
                  </span>
                </button>
                <Menu>
                  <MenuTrigger asChild>
                    <IconButton label={`จัดการ ${preset.name}`} size="sm" className="my-auto mr-1 shrink-0" data-testid="preset-menu">
                      <MoreHorizontal className="size-4" aria-hidden />
                    </IconButton>
                  </MenuTrigger>
                  <MenuContent>
                    <MenuItem icon={<Pencil />} onSelect={() => setNaming({ mode: 'rename', preset })}>
                      เปลี่ยนชื่อ
                    </MenuItem>
                    <MenuItem icon={<Trash2 />} tone="danger" onSelect={() => setDeleting(preset)}>
                      ลบ preset
                    </MenuItem>
                  </MenuContent>
                </Menu>
              </li>
            );
          })}
        </ul>
      )}

      {list.length > 0 && (
        <p className="flex items-start gap-1.5 text-[13px] text-muted">
          <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
          ชี้ที่ preset เพื่อดูตัวอย่างบนภาพ คลิกเพื่อใช้ · ใช้ preset จะแทนค่าแสงและสีทั้งหมดของรูปนี้ ไม่แตะการตัดและหมุน (Mask ตัวคนของ preset ถูกเพิ่มเข้าไป)
        </p>
      )}

      <NameDialog
        key={naming ? (naming.mode === 'rename' ? naming.preset.id : 'save') : 'closed'}
        open={naming !== null}
        title={naming?.mode === 'rename' ? 'เปลี่ยนชื่อ preset' : 'บันทึกเป็น preset'}
        description={naming?.mode === 'save' ? presetSummary(adjust, subjectMasks) || undefined : undefined}
        initial={naming?.mode === 'rename' ? naming.preset.name : ''}
        confirmLabel={naming?.mode === 'rename' ? 'เปลี่ยนชื่อ' : 'บันทึก preset'}
        busy={save.isPending || rename.isPending}
        onOpenChange={(open) => !open && setNaming(null)}
        onSubmit={(name) => (naming?.mode === 'rename' ? rename.mutate({ id: naming.preset.id, name }) : save.mutate(name))}
      />

      <ConfirmDialog
        open={deleting !== null}
        onOpenChange={(open) => !open && setDeleting(null)}
        title={`ลบ preset “${deleting?.name ?? ''}”?`}
        description="รูปที่ใช้ preset นี้ไปแล้วไม่เปลี่ยน ลบแล้วกู้คืนไม่ได้ (ถ้านำเข้าจาก Lightroom นำเข้าใหม่ได้จากไฟล์ .xmp เดิม)"
        confirmLabel="ลบ preset"
        tone="danger"
        busy={remove.isPending}
        onConfirm={() => deleting && remove.mutate(deleting.id)}
      />

      <Dialog
        open={imported !== null}
        onOpenChange={(open) => !open && setImported(null)}
        title={`นำเข้า “${imported?.name ?? ''}” แล้ว`}
        description="ผลจะใกล้เคียงกับใน Lightroom แต่ไม่เหมือนทุกจุด เพราะสูตรของ Adobe ไม่เปิดเผย"
        footer={
          <>
            <Button onClick={() => setImported(null)}>ปิด</Button>
            <Button
              variant="primary"
              onClick={() => {
                if (imported) onApply(imported);
                setImported(null);
              }}
              data-testid="preset-imported-apply"
            >
              ใช้กับรูปนี้
            </Button>
          </>
        }
      >
        {imported && (
          <div className="space-y-3" data-testid="preset-imported">
            <div>
              <p className="text-sm font-semibold text-ink">นำเข้าได้</p>
              <p className="mt-0.5 text-[13px] text-muted">{presetSummary(imported.adjust, imported.masks)}</p>
            </div>
            {imported.unsupported.length > 0 && (
              <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
                <span className="block font-semibold">ส่วนที่แอปนี้ยังทำไม่ได้ จึงไม่ได้นำเข้า</span>
                <span className="mt-1 block">{imported.unsupported.join(' · ')}</span>
              </InlineNotice>
            )}
          </div>
        )}
      </Dialog>
    </div>
  );
}

function NameDialog({
  open,
  title,
  description,
  initial,
  confirmLabel,
  busy,
  onOpenChange,
  onSubmit,
}: {
  open: boolean;
  title: string;
  description?: string;
  initial: string;
  confirmLabel: string;
  busy: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (name: string) => void;
}) {
  const [name, setName] = useState(initial);
  const trimmed = name.trim();
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={title}
      description={description}
      size="sm"
      dismissible={!busy}
      footer={
        <>
          <Button onClick={() => onOpenChange(false)} disabled={busy}>
            ยกเลิก
          </Button>
          <Button variant="primary" type="submit" form="preset-name-form" disabled={!trimmed} busy={busy} data-testid="preset-name-confirm">
            {confirmLabel}
          </Button>
        </>
      }
    >
      <form
        id="preset-name-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (trimmed) onSubmit(trimmed);
        }}
      >
        <Field label="ชื่อ preset">
          {(props) => (
            <TextInput
              {...props}
              data-autofocus
              value={name}
              maxLength={80}
              onChange={(e) => setName(e.target.value)}
              placeholder="เช่น งานในอาคาร แสงไฟเหลือง"
              autoComplete="off"
              data-testid="preset-name-input"
            />
          )}
        </Field>
      </form>
    </Dialog>
  );
}
