import { useEffect, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { ImagePlus, Stamp, TriangleAlert, X } from 'lucide-react';
import { useFolderChooser } from '@/app/FolderChooser';
import { Button } from '@/components/ui/Button';
import { ConfirmDialog, Dialog } from '@/components/ui/Dialog';
import { ErrorState, InlineNotice, LoadingBlock } from '@/components/ui/Feedback';
import { Switch } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { errorMessage } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { Id, WatermarkInfo } from '@/lib/ipc/types';
import { ANCHORS, OPACITY_RANGE, SCALE_RANGE, anchorLabel, watermarkRect, type WatermarkChoice } from './watermarkChoice';

export const WATERMARKS_KEY = ['watermarks'] as const;

export function useWatermarks() {
  return useQuery({ queryKey: WATERMARKS_KEY, queryFn: () => call('list_watermarks', {}) });
}

/** พื้นตารางหมากรุก ให้เห็นส่วนโปร่งใสของโลโก้ */
const CHECKER = { background: 'repeating-conic-gradient(var(--line) 0 25%, var(--surface) 0 50%) 0 0 / 12px 12px' };

/** แถวลายน้ำในหน้าต่างส่งออก: เปิด/ปิด + สรุปค่าที่ตั้ง + ปุ่มเปิดหน้าต่างตั้งค่า */
export function WatermarkRow({
  choice,
  onChange,
  error,
  previewPhotoId,
}: {
  choice: WatermarkChoice;
  onChange: (choice: WatermarkChoice) => void;
  error: string | null;
  previewPhotoId: Id | null;
}) {
  const [editing, setEditing] = useState(false);
  const list = useWatermarks();
  const logo = list.data?.find((w) => w.id === choice.id);
  const summary = logo
    ? `${logo.name} · ${anchorLabel(choice.anchor)} · ขนาด ${choice.scale}% · ทึบ ${choice.opacity}%`
    : 'ใส่โลโก้ลงทุกสำเนา ไฟล์ต้นฉบับไม่ถูกแตะ';
  return (
    <div className="rounded-xl border border-line px-4 py-3" data-testid="export-watermark">
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
        <Switch
          checked={choice.enabled}
          onCheckedChange={(enabled) => {
            onChange({ ...choice, enabled, id: choice.id ?? list.data?.[0]?.id ?? null });
            // เปิดครั้งแรกยังไม่มีโลโก้: พาไปเพิ่มเลย
            if (enabled && list.data?.length === 0) setEditing(true);
          }}
          label="ใส่ลายน้ำ"
          description={choice.enabled ? summary : 'ใส่โลโก้ลงทุกสำเนา ไฟล์ต้นฉบับไม่ถูกแตะ'}
          testId="export-watermark-toggle"
        />
        {choice.enabled && (
          <Button size="sm" icon={<Stamp className="size-4" aria-hidden />} onClick={() => setEditing(true)} data-testid="export-watermark-edit">
            ตั้งค่าลายน้ำ…
          </Button>
        )}
      </div>
      {choice.enabled && error && (
        <p role="alert" className="mt-1.5 text-[13px] text-danger">
          {error}
        </p>
      )}
      <WatermarkDialog open={editing} onOpenChange={setEditing} choice={choice} onChange={onChange} previewPhotoId={previewPhotoId} />
    </div>
  );
}

/** ตั้งค่าลายน้ำพร้อมตัวอย่างบนรูปที่เลือกจริง (สูตรตำแหน่งเดียวกับที่ Rust วาดลงสำเนา) */
function WatermarkDialog({
  open,
  onOpenChange,
  choice,
  onChange,
  previewPhotoId,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  choice: WatermarkChoice;
  onChange: (choice: WatermarkChoice) => void;
  previewPhotoId: Id | null;
}) {
  const qc = useQueryClient();
  const choose = useFolderChooser();
  const list = useWatermarks();
  const [removing, setRemoving] = useState<WatermarkInfo | null>(null);
  const logo = list.data?.find((w) => w.id === choice.id) ?? null;

  const add = useMutation({
    mutationFn: (path: string) => call('add_watermark', { path }),
    onSuccess: (info) => {
      qc.setQueryData<WatermarkInfo[]>(WATERMARKS_KEY, (old) => [...(old ?? []), info]);
      onChange({ ...choice, enabled: true, id: info.id });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const remove = useMutation({
    mutationFn: (id: Id) => call('delete_watermark', { id }),
    onSuccess: (rest, id) => {
      qc.setQueryData(WATERMARKS_KEY, rest);
      if (choice.id === id) onChange({ ...choice, id: rest[0]?.id ?? null });
      setRemoving(null);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  const pick = async () => {
    const path = await choose({
      title: 'เลือกไฟล์โลโก้',
      hint: 'ใช้ไฟล์ PNG ที่พื้นโปร่งใสจะดูดีที่สุด (JPEG ใช้ได้แต่จะเห็นเป็นกรอบสี่เหลี่ยม)',
      kind: 'image',
    });
    if (path) add.mutate(path);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="ตั้งค่าลายน้ำ"
      size="xl"
      footer={
        <Button variant="primary" onClick={() => onOpenChange(false)} data-testid="watermark-done">
          เสร็จ
        </Button>
      }
    >
      <div className="grid gap-5 md:grid-cols-[minmax(0,1fr)_272px]" data-testid="watermark-dialog">
        <WatermarkPreview photoId={previewPhotoId} logo={logo} choice={choice} />
        <div className="space-y-5">
          <section>
            <h3 className="mb-2 text-sm font-semibold text-ink">โลโก้</h3>
            {list.isPending ? (
              <LoadingBlock label="กำลังโหลดลายน้ำ…" className="py-6" />
            ) : list.isError ? (
              <ErrorState compact error={list.error} onRetry={() => void list.refetch()} />
            ) : (
              <ul className="grid grid-cols-3 gap-2" aria-label="ลายน้ำที่เก็บไว้">
                {list.data.map((w) => (
                  <li key={w.id} className="group relative min-w-0">
                    <button
                      type="button"
                      aria-pressed={w.id === choice.id}
                      onClick={() => onChange({ ...choice, enabled: true, id: w.id })}
                      title={w.name}
                      className={cn(
                        'flex w-full flex-col gap-1 rounded-lg border-2 p-1 text-left transition-colors',
                        w.id === choice.id ? 'border-action' : 'border-line hover:border-line-strong',
                      )}
                      data-testid="watermark-option"
                    >
                      <span className="flex aspect-square items-center justify-center overflow-hidden rounded-md" style={CHECKER}>
                        <img src={assetUrl('watermark', w.id)} alt="" className="max-h-full max-w-full object-contain p-1" />
                      </span>
                      <span className="truncate px-0.5 text-[12px] text-muted">{w.name}</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => setRemoving(w)}
                      aria-label={`ลบลายน้ำ ${w.name}`}
                      className="absolute -top-1.5 -right-1.5 flex size-6 items-center justify-center rounded-full border border-line bg-surface text-muted opacity-0 shadow-card transition-opacity group-hover:opacity-100 hover:text-danger focus-visible:opacity-100"
                    >
                      <X className="size-3.5" aria-hidden />
                    </button>
                  </li>
                ))}
                <li>
                  <button
                    type="button"
                    onClick={() => void pick()}
                    disabled={add.isPending}
                    className="flex aspect-square w-full flex-col items-center justify-center gap-1 rounded-lg border-2 border-dashed border-line-strong text-[12px] font-semibold text-action hover:bg-brand-soft disabled:opacity-60"
                    data-testid="watermark-add"
                  >
                    <ImagePlus className="size-5" aria-hidden />
                    {add.isPending ? 'กำลังเพิ่ม…' : 'เพิ่มโลโก้'}
                  </button>
                </li>
              </ul>
            )}
            {logo && !logo.transparent && (
              <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />} className="mt-2">
                โลโก้นี้ไม่มีพื้นโปร่งใส จะเห็นเป็นกรอบสี่เหลี่ยมทับรูป — ไฟล์ PNG พื้นใสจะดูเรียบร้อยกว่า
              </InlineNotice>
            )}
          </section>

          <section>
            <h3 id="watermark-anchor" className="mb-2 text-sm font-semibold text-ink">
              ตำแหน่ง <span className="font-normal text-muted">· {anchorLabel(choice.anchor)}</span>
            </h3>
            <div role="radiogroup" aria-labelledby="watermark-anchor" className="grid w-[132px] grid-cols-3 gap-1.5 rounded-lg border border-line bg-sunken p-1.5">
              {ANCHORS.map((a) => (
                <button
                  key={a.value}
                  type="button"
                  role="radio"
                  aria-checked={choice.anchor === a.value}
                  aria-label={a.label}
                  title={a.label}
                  onClick={() => onChange({ ...choice, anchor: a.value })}
                  className={cn(
                    'flex aspect-square items-center justify-center rounded-md border transition-colors',
                    choice.anchor === a.value ? 'border-action bg-action' : 'border-line bg-surface hover:border-line-strong',
                  )}
                  data-testid={`watermark-anchor-${a.value}`}
                >
                  <span className={cn('size-2 rounded-full', choice.anchor === a.value ? 'bg-on-action' : 'bg-line-strong')} />
                </button>
              ))}
            </div>
          </section>

          <Slider
            label="ขนาด"
            value={choice.scale}
            range={SCALE_RANGE}
            onChange={(scale) => onChange({ ...choice, scale })}
            hint="ด้านยาวของโลโก้ เทียบกับด้านสั้นของรูป ลายน้ำจึงดูเท่ากันทุกรูปและทุกขนาดสำเนา"
            testId="watermark-scale"
          />
          <Slider label="ความทึบ" value={choice.opacity} range={OPACITY_RANGE} onChange={(opacity) => onChange({ ...choice, opacity })} testId="watermark-opacity" />
        </div>
      </div>
      <ConfirmDialog
        open={removing !== null}
        onOpenChange={(open) => !open && setRemoving(null)}
        title={`ลบลายน้ำ “${removing?.name ?? ''}”?`}
        description="ลบออกจากรายการในแอปเท่านั้น ไฟล์โลโก้ต้นทางและสำเนาที่ส่งออกไปแล้วไม่ถูกแตะ"
        confirmLabel="ลบลายน้ำ"
        tone="danger"
        busy={remove.isPending}
        onConfirm={() => removing && remove.mutate(removing.id)}
      />
    </Dialog>
  );
}

function Slider({
  label,
  value,
  range,
  onChange,
  hint,
  testId,
}: {
  label: string;
  value: number;
  range: { min: number; max: number };
  onChange: (value: number) => void;
  hint?: string;
  testId: string;
}) {
  return (
    <div>
      <div className="mb-1 flex items-baseline justify-between">
        <label htmlFor={testId} className="text-sm font-semibold text-ink">
          {label}
        </label>
        <span className="text-sm font-semibold text-action tabular-nums">{value}%</span>
      </div>
      <input
        id={testId}
        type="range"
        min={range.min}
        max={range.max}
        step={1}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        aria-valuetext={`${value}%`}
        data-testid={testId}
        className="h-1.5 w-full cursor-pointer accent-[var(--action)]"
      />
      {hint && <p className="mt-1 text-[12.5px] text-muted">{hint}</p>}
    </div>
  );
}

/** รูปที่เลือกพร้อมลายน้ำวางทับ ตามขนาดที่แสดงจริงบนจอ */
function WatermarkPreview({ photoId, logo, choice }: { photoId: Id | null; logo: WatermarkInfo | null; choice: WatermarkChoice }) {
  const frame = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const el = frame.current;
    if (!el) return;
    const measure = () => setSize(el.clientWidth > 0 ? { width: el.clientWidth, height: el.clientHeight } : null);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [photoId, failed]);

  const rect = logo && size ? watermarkRect(size.width, size.height, logo.width, logo.height, choice.scale / 100, choice.anchor) : null;
  const showPhoto = photoId && !failed;
  return (
    <div className="flex min-h-[320px] items-center justify-center rounded-xl bg-viewer p-3" data-testid="watermark-preview">
      <div ref={frame} className={cn('relative', !showPhoto && 'aspect-[3/2] w-full max-w-[560px] rounded bg-[#5b6270]')}>
        {showPhoto && (
          <img
            src={assetUrl('photo', photoId)}
            alt="ตัวอย่างลายน้ำบนรูปที่เลือก"
            onLoad={() => frame.current && setSize({ width: frame.current.clientWidth, height: frame.current.clientHeight })}
            onError={() => setFailed(true)}
            className="block max-h-[460px] max-w-full rounded"
          />
        )}
        {logo && rect && (
          <img
            src={assetUrl('watermark', logo.id)}
            alt=""
            className="pointer-events-none absolute"
            style={{ left: rect.x, top: rect.y, width: rect.width, height: rect.height, opacity: choice.opacity / 100 }}
            data-testid="watermark-preview-logo"
          />
        )}
        {!logo && (
          <p className="absolute inset-0 flex items-center justify-center p-4">
            <span className="rounded-lg bg-black/60 px-3 py-1.5 text-center text-sm text-white">เพิ่มหรือเลือกโลโก้ทางขวา</span>
          </p>
        )}
      </div>
    </div>
  );
}
