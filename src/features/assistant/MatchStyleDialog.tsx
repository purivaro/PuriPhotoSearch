import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Check, FileImage, Info, Palette, Sparkles, TriangleAlert } from 'lucide-react';
import { useFolderChooser } from '@/app/FolderChooser';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { InlineNotice, LoadingBlock } from '@/components/ui/Feedback';
import { Field, TextArea } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { Id, SelectionSpec } from '@/lib/ipc/types';
import { useAssistant, type AttachedReference } from '@/stores/assistant';
import { useUi } from '@/stores/ui';

const MAX_EDIT = 20;
const MAX_GENERATE = 10;
const PROVIDER_LABEL = { claude: 'Claude', codex: 'ChatGPT', gemini: 'Gemini' } as const;

type Mode = 'edit' | 'generate';
type Choice = { kind: 'photo'; id: Id; filename: string } | { kind: 'file'; path: string; filename: string };

interface MatchStyleDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selection: SelectionSpec;
  count: number;
}

/**
 * เลือกภาพตัวอย่าง (ref) แล้วให้ AI แต่งรูปอื่นที่เลือกให้ดูเหมือนภาพนั้น (DECISIONS ข้อ 24)
 * แบบแต่ง = ใช้เครื่องมือแต่งภาพของแอป (ไม่แตะไฟล์ ย้อนได้) · แบบสร้าง = ChatGPT สร้างภาพใหม่ในสไตล์ของภาพตัวอย่าง
 */
export function MatchStyleDialog({ open, onOpenChange, selection, count }: MatchStyleDialogProps) {
  const qc = useQueryClient();
  const choose = useFolderChooser();
  const { provider, running, matchLook, generate } = useAssistant();
  const photos = useQuery({ queryKey: ['selection-photos', selection], queryFn: () => call('selection_photos', { selection, limit: 31 }), enabled: open });
  const status = useQuery({ queryKey: ['ai-status'], queryFn: () => call('ai_status', {}), staleTime: 60_000, enabled: open });
  const [choice, setChoice] = useState<Choice | null>(null);
  const [mode, setMode] = useState<Mode>('edit');
  const [note, setNote] = useState('');

  useEffect(() => {
    if (open) {
      setChoice(null);
      setNote('');
    }
  }, [open]);

  const items = photos.data?.items ?? [];
  const targets = items.filter((p) => !(choice?.kind === 'photo' && choice.id === p.id));
  const total = (photos.data?.total ?? count) - (choice?.kind === 'photo' ? 1 : 0);
  const max = mode === 'edit' ? MAX_EDIT : MAX_GENERATE;
  const useProvider = mode === 'edit' ? provider : 'codex';
  const cli = status.data?.find((s) => s.provider === useProvider);
  const ready = Boolean(cli?.installed) && cli?.signedIn !== false;
  const problem = !choice
    ? null
    : total < 1
      ? 'เลือกรูปที่จะแต่งเพิ่มอีกอย่างน้อยหนึ่งรูป (นอกจากภาพตัวอย่าง)'
      : total > max
        ? `ครั้งละไม่เกิน ${formatCount(max)} รูป — เลือกให้น้อยลง`
        : null;

  const pickFile = async () => {
    const path = await choose({ title: 'เลือกภาพตัวอย่าง', hint: 'ไฟล์ JPEG หรือ PNG ที่มีโทนและสีแบบที่ต้องการ', kind: 'image' });
    if (path) setChoice({ kind: 'file', path, filename: path.split(/[\\/]/).pop() ?? path });
  };

  const start = async () => {
    if (!choice) return;
    const reference: AttachedReference =
      choice.kind === 'photo'
        ? { reference: { kind: 'photo', photoId: choice.id }, label: choice.filename }
        : { reference: { kind: 'file', path: choice.path }, label: choice.filename };
    useUi.getState().setAssistantOpen(true);
    onOpenChange(false);
    try {
      if (mode === 'edit') {
        await matchLook({ reference, targets, note }, qc);
      } else {
        await generate({ selection: { kind: 'explicit', photoIds: targets.map((t) => t.id) }, count: targets.length, prompt: note, template: null, style: reference }, qc);
      }
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="แต่งตามภาพตัวอย่าง"
      description="เลือกภาพที่มีโทนและสีแบบที่ชอบ แล้วให้ AI ทำรูปอื่นที่เลือกไว้ให้เป็นแบบนั้น"
      size="lg"
      footer={
        <>
          <Button onClick={() => onOpenChange(false)}>ยกเลิก</Button>
          <Button
            variant="primary"
            icon={mode === 'edit' ? <Palette className="size-4" aria-hidden /> : <Sparkles className="size-4" aria-hidden />}
            disabled={!choice || Boolean(problem) || !ready || Boolean(running)}
            busy={Boolean(choice) && status.isPending}
            onClick={() => void start()}
            data-testid="match-start"
          >
            {mode === 'edit' ? `แต่ง ${formatCount(Math.max(total, 0))} รูป` : `สร้าง ${formatCount(Math.max(total, 0))} ภาพ`}
          </Button>
        </>
      }
    >
      <div className="space-y-4" data-testid="match-dialog">
        <div>
          <p className="mb-2 text-sm font-semibold text-ink">1. ภาพตัวอย่าง</p>
          {photos.isPending ? (
            <LoadingBlock />
          ) : (
            <div className="grid grid-cols-4 gap-2 sm:grid-cols-6" role="radiogroup" aria-label="ภาพตัวอย่าง">
              {items.map((p) => {
                const active = choice?.kind === 'photo' && choice.id === p.id;
                return (
                  <button
                    key={p.id}
                    type="button"
                    role="radio"
                    aria-checked={active}
                    aria-label={`ใช้ ${p.filename} เป็นภาพตัวอย่าง`}
                    title={p.filename}
                    onClick={() => setChoice({ kind: 'photo', id: p.id, filename: p.filename })}
                    className={cn(
                      'relative aspect-square overflow-hidden rounded-lg bg-sunken outline-none focus-visible:ring-2 focus-visible:ring-[var(--action)]',
                      active ? 'ring-[3px] ring-[var(--action)]' : 'opacity-90 hover:opacity-100',
                    )}
                    data-testid="match-ref-photo"
                  >
                    <img src={assetUrl('thumb', p.id)} alt="" loading="lazy" className="size-full object-cover" />
                    {active && (
                      <span className="absolute inset-x-0 bottom-0 flex items-center justify-center gap-1 bg-[var(--action)] py-0.5 text-[11.5px] font-bold text-on-action">
                        <Check className="size-3" aria-hidden /> ภาพตัวอย่าง
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          )}
          <div className="mt-2 flex flex-wrap items-center gap-2 text-[13px]">
            <span className="text-muted">คลิกรูปหนึ่งให้เป็นภาพตัวอย่าง หรือ</span>
            <Button size="sm" icon={<FileImage className="size-4" aria-hidden />} onClick={() => void pickFile()} data-testid="match-ref-file">
              เลือกไฟล์ภาพจากเครื่อง…
            </Button>
            {choice?.kind === 'file' && (
              <span className="flex min-w-0 items-center gap-1 font-semibold text-action">
                <Check className="size-4 shrink-0" aria-hidden />
                <span className="truncate">{choice.filename}</span>
              </span>
            )}
          </div>
          {(photos.data?.total ?? 0) > items.length && (
            <p className="mt-1 text-[12.5px] text-muted">แสดง {formatCount(items.length)} จาก {formatCount(photos.data?.total ?? 0)} รูปที่เลือก</p>
          )}
        </div>

        <div>
          <p className="mb-2 text-sm font-semibold text-ink">2. ให้ AI ทำอะไร</p>
          <div className="grid gap-2 sm:grid-cols-2" role="radiogroup" aria-label="สิ่งที่ให้ AI ทำ">
            {(
              [
                {
                  value: 'edit' as const,
                  title: 'แต่งให้ดูเหมือนภาพตัวอย่าง',
                  detail: `${PROVIDER_LABEL[provider]} ปรับแสง สี เส้นโค้ง และ Color Grading ด้วยเครื่องมือของแอป · ไม่แตะไฟล์ ย้อนได้ทั้งรอบ`,
                },
                {
                  value: 'generate' as const,
                  title: 'สร้างภาพใหม่ตามสไตล์ภาพตัวอย่าง',
                  detail: 'ChatGPT วาดแต่ละรูปใหม่ในสไตล์ของภาพตัวอย่าง (เช่น ภาพวาด) · เข้าคลังเป็นกิจกรรม “… · ภาพจาก AI”',
                },
              ] satisfies { value: Mode; title: string; detail: string }[]
            ).map((m) => (
              <button
                key={m.value}
                type="button"
                role="radio"
                aria-checked={mode === m.value}
                onClick={() => setMode(m.value)}
                className={cn('rounded-xl border px-3.5 py-2.5 text-left', mode === m.value ? 'border-action bg-brand-soft' : 'border-line hover:bg-sunken')}
                data-testid={`match-mode-${m.value}`}
              >
                <span className={cn('block text-sm font-semibold', mode === m.value ? 'text-action' : 'text-ink')}>{m.title}</span>
                <span className="mt-0.5 block text-[12.5px] text-muted">{m.detail}</span>
              </button>
            ))}
          </div>
        </div>

        <Field label="3. บอก AI เพิ่ม" optional hint="เช่น “เอาแค่โทนสีอุ่น ๆ ไม่ต้องสว่างเท่าภาพตัวอย่าง” หรือ “ผิวคนให้ดูเป็นธรรมชาติ”">
          {(props) => <TextArea {...props} value={note} rows={2} maxLength={1000} onChange={(e) => setNote(e.target.value)} data-testid="match-note" />}
        </Field>

        {problem && (
          <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
            {problem}
          </InlineNotice>
        )}
        {!status.isPending && !ready && (
          <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
            {cli?.installed
              ? `ยังไม่ได้ล็อกอิน ${PROVIDER_LABEL[useProvider]} — เปิด Terminal แล้วพิมพ์ ${cli.loginCommand} ก่อน`
              : `ไม่พบ ${PROVIDER_LABEL[useProvider]} ในเครื่องนี้${mode === 'edit' ? ' — เลือก AI อื่นได้ในแผงผู้ช่วย AI' : ''}`}
          </InlineNotice>
        )}
        {running && (
          <InlineNotice tone="info" icon={<Info aria-hidden />}>
            ผู้ช่วย AI กำลังทำงานอื่นอยู่ รอให้เสร็จก่อน
          </InlineNotice>
        )}
        <p className="flex items-start gap-1.5 text-[13px] text-muted">
          <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
          {mode === 'edit'
            ? `ภาพตัวอย่างและรูปที่ AI เปิดดู (ย่อแล้ว) ถูกส่งไปยัง ${PROVIDER_LABEL[provider]} ภายใต้บัญชีที่ล็อกอินไว้ · ภาพตัวอย่างที่แต่งไว้ในแอป: AI เริ่มจากค่าที่แต่งนั้น แล้วปรับความสว่างและสมดุลแสงขาวให้เข้ากับแต่ละรูป · ภาพอื่น: แอปวัดโทน สี และความสว่างเป็นตัวเลขให้ AI ใช้เป็นจุดเริ่ม · ดูความคืบหน้าในแผงผู้ช่วย AI และสั่งปรับต่อได้`
            : 'ภาพตัวอย่างและแต่ละรูป (ด้านยาว 2,048 px) ถูกส่งไปยัง ChatGPT ภายใต้บัญชีที่ล็อกอินไว้ · ราว 30–60 วินาทีต่อภาพ · รูปต้นฉบับไม่ถูกแตะ'}
        </p>
      </div>
    </Dialog>
  );
}
