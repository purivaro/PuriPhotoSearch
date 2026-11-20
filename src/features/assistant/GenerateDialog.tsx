import { useEffect, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Info, Sparkles, TriangleAlert } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { InlineNotice } from '@/components/ui/Feedback';
import { Field, TextArea } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { SelectionSpec } from '@/lib/ipc/types';
import { useAssistant } from '@/stores/assistant';
import { useUi } from '@/stores/ui';

/** แม่แบบ: ชื่อ + คำสั่งตั้งต้น (แก้ต่อได้) */
export const TEMPLATES: { name: string; prompt: string }[] = [
  { name: 'ภาพวาดสีน้ำ', prompt: 'เปลี่ยนเป็นภาพวาดสีน้ำนุ่ม ๆ โทนสว่าง เห็นลายกระดาษ' },
  { name: 'ภาพวาดสีน้ำมัน', prompt: 'เปลี่ยนเป็นภาพวาดสีน้ำมันแบบอิมเพรสชันนิสม์ ฝีแปรงชัด โทนอบอุ่น' },
  { name: 'การ์ตูนน่ารัก', prompt: 'เปลี่ยนเป็นภาพการ์ตูนลายเส้นอบอุ่น สีพาสเทล ใบหน้ายังจำได้ว่าเป็นใคร' },
  { name: 'ลายเส้นดินสอ', prompt: 'เปลี่ยนเป็นภาพสเก็ตช์ดินสอขาวดำ ลายเส้นละเอียด' },
  { name: 'ขาวดำคลาสสิก', prompt: 'เปลี่ยนเป็นภาพถ่ายขาวดำแบบฟิล์มคลาสสิก คอนทราสต์นุ่ม เกรนฟิล์มบาง ๆ' },
  { name: 'ตุ๊กตาดินปั้น 3 มิติ', prompt: 'เปลี่ยนเป็นตุ๊กตาดินปั้น 3 มิติน่ารัก ฉากเหมือนของจริงขนาดจิ๋ว' },
  { name: 'โปสเตอร์งานบุญ', prompt: 'ทำเป็นภาพโปสเตอร์งานบุญที่สง่างาม โทนทองและขาว แสงนุ่ม ไม่ต้องมีตัวหนังสือ' },
  { name: 'ภาพบุคคลแสงสตูดิโอ', prompt: 'ทำเป็นภาพบุคคลแสงสตูดิโอมืออาชีพ ฉากหลังเรียบสะอาด ผิวดูเป็นธรรมชาติ คนยังเป็นคนเดิม' },
];

interface GenerateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selection: SelectionSpec;
  count: number;
}

/** ให้ AI (ChatGPT) สร้างภาพใหม่จากรูปที่เลือก ตามแม่แบบหรือคำสั่งของผู้ใช้ แล้วบันทึกเข้าคลัง (DECISIONS ข้อ 23) */
export function GenerateDialog({ open, onOpenChange, selection, count }: GenerateDialogProps) {
  const qc = useQueryClient();
  const generate = useAssistant((s) => s.generate);
  const running = useAssistant((s) => s.running);
  const status = useQuery({ queryKey: ['ai-status'], queryFn: () => call('ai_status', {}), staleTime: 60_000, enabled: open });
  const info = useQuery({ queryKey: ['ai-generate-info'], queryFn: () => call('ai_generate_info', {}), enabled: open });
  const codex = status.data?.find((s) => s.provider === 'codex');
  const [template, setTemplate] = useState<string | null>(TEMPLATES[0]!.name);
  const [prompt, setPrompt] = useState(TEMPLATES[0]!.prompt);
  const [busy, setBusy] = useState(false);
  const max = info.data?.maxPhotos ?? 10;
  const tooMany = count > max;
  const ready = Boolean(codex?.installed) && codex?.signedIn !== false;

  useEffect(() => {
    if (open) setBusy(false);
  }, [open]);

  const start = async () => {
    setBusy(true);
    useUi.getState().setAssistantOpen(true);
    onOpenChange(false);
    try {
      await generate({ selection, count, prompt: prompt.trim(), template }, qc);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="สร้างภาพด้วย AI"
      description={`จาก ${formatCount(count)} รูปที่เลือก · ได้ภาพใหม่รูปละหนึ่งภาพ`}
      size="lg"
      footer={
        <>
          <Button onClick={() => onOpenChange(false)}>ยกเลิก</Button>
          <Button
            variant="primary"
            icon={<Sparkles className="size-4" aria-hidden />}
            disabled={!ready || tooMany || !prompt.trim() || Boolean(running)}
            busy={busy}
            onClick={() => void start()}
            data-testid="generate-start"
          >
            สร้าง {formatCount(Math.min(count, max))} ภาพ
          </Button>
        </>
      }
    >
      <div className="space-y-4" data-testid="generate-dialog">
        {!status.isPending && !ready && (
          <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
            {codex?.installed
              ? `ยังไม่ได้ล็อกอิน ChatGPT ใน Codex — เปิด Terminal แล้วพิมพ์ ${codex.loginCommand} ก่อน`
              : 'ไม่พบ Codex ของ ChatGPT ในเครื่องนี้ — ติดตั้งแอป ChatGPT หรือ Codex CLI แล้วล็อกอินด้วยบัญชีที่สมัครไว้'}
          </InlineNotice>
        )}
        {tooMany && (
          <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
            สร้างได้ครั้งละไม่เกิน {formatCount(max)} รูป — เลือกรูปให้น้อยลงก่อน
          </InlineNotice>
        )}
        {running && (
          <InlineNotice tone="info" icon={<Info aria-hidden />}>
            ผู้ช่วย AI กำลังทำงานอื่นอยู่ รอให้เสร็จก่อน
          </InlineNotice>
        )}
        <div>
          <p className="mb-2 text-sm font-semibold text-ink">แม่แบบ</p>
          <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-4" role="radiogroup" aria-label="แม่แบบ">
            {TEMPLATES.map((t) => (
              <button
                key={t.name}
                type="button"
                role="radio"
                aria-checked={template === t.name}
                onClick={() => {
                  setTemplate(t.name);
                  setPrompt(t.prompt);
                }}
                className={cn(
                  'min-h-10 rounded-lg border px-2.5 py-1.5 text-left text-[13px] font-semibold',
                  template === t.name ? 'border-action bg-brand-soft text-action' : 'border-line text-ink hover:bg-sunken',
                )}
                data-testid="generate-template"
              >
                {t.name}
              </button>
            ))}
          </div>
        </div>
        <Field label="สิ่งที่อยากให้ AI สร้าง" hint="แก้คำสั่งของแม่แบบได้ หรือพิมพ์เองทั้งหมด">
          {(props) => (
            <TextArea
              {...props}
              value={prompt}
              rows={3}
              maxLength={2000}
              onChange={(e) => {
                setPrompt(e.target.value);
                if (TEMPLATES.find((t) => t.name === template)?.prompt !== e.target.value) setTemplate(null);
              }}
              data-testid="generate-prompt"
            />
          )}
        </Field>
        <ul className="space-y-1 text-[13px] text-muted">
          <li className="flex items-start gap-1.5">
            <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
            ส่งสำเนาย่อของแต่ละรูป (ด้านยาว 2,048 px ตามที่แต่งไว้) ไปยัง ChatGPT ภายใต้บัญชีที่ล็อกอินไว้ · ใช้เวลาราว 30–60 วินาทีต่อภาพ
          </li>
          <li className="pl-5.5">
            ภาพที่ได้เก็บใน <span className="font-semibold break-all text-ink">{info.data?.outputRoot ?? '…'}</span> แยกโฟลเดอร์ตามกิจกรรมต้นทาง และเข้าคลังเป็นกิจกรรม
            “ชื่อกิจกรรม · ภาพจาก AI” พร้อมบันทึกว่าสร้างจากรูปไหน ด้วยคำสั่งอะไร · รูปต้นฉบับไม่ถูกแตะ
          </li>
          <li className="pl-5.5">ดูความคืบหน้าได้ในแผงผู้ช่วย AI</li>
        </ul>
      </div>
    </Dialog>
  );
}
