import { memo, useEffect, useMemo, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { ArrowUp, Check, ChevronRight, Copy, ImageIcon, RotateCcw, Sparkles, Square, SquarePen, TriangleAlert, Wrench, X } from 'lucide-react';
import { useRealLocation, useViewer } from '@/app/navigation';
import { Button, IconButton } from '@/components/ui/Button';
import { InlineNotice, Spinner } from '@/components/ui/Feedback';
import { Switch } from '@/components/ui/Form';
import { cn } from '@/lib/cn';
import { errorMessage, formatCount } from '@/lib/format';
import { assetUrl, call } from '@/lib/ipc/client';
import type { AiCliStatus, AiProvider } from '@/lib/ipc/types';
import { qk } from '@/lib/queries';
import { useAssistant, type ChatMessage, type ToolStep } from '@/stores/assistant';
import { useUi } from '@/stores/ui';
import { parseMarkdown, type Inline } from './markdown';

const PROVIDERS: { value: AiProvider; label: string; product: string }[] = [
  { value: 'claude', label: 'Claude', product: 'Claude Code' },
  { value: 'codex', label: 'ChatGPT', product: 'Codex ของ ChatGPT' },
  { value: 'gemini', label: 'Gemini', product: 'Gemini CLI' },
];

const TOOL_LABELS: Record<string, string> = {
  search_photos: 'ค้นรูป',
  list_events: 'ดูรายการกิจกรรม',
  list_people: 'ดูรายชื่อบุคคล',
  get_photo: 'อ่านข้อมูลรูป',
  view_photo: 'เปิดดูรูป',
  edit_photo: 'แต่งรูป',
  edit_photos: 'แต่งหลายรูป',
  revert_photos: 'ย้อนเป็นต้นฉบับ',
  list_presets: 'ดู preset',
  apply_preset: 'ใช้ preset',
  save_preset: 'บันทึก preset',
  show_photos: 'แสดงรูป',
  generate_image: 'สร้างภาพ',
};

const SUGGESTIONS = [
  'แต่งรูปที่เปิดอยู่ให้สว่างสดใสแบบเป็นธรรมชาติ ผิวคนดูดี',
  'หารูปจากกิจกรรมล่าสุด แล้วแสดงให้ดู',
  'ดูว่ามี preset อะไรบ้าง แล้วลองใช้กับรูปที่เปิดอยู่',
];

/** แผงผู้ช่วย AI ด้านขวา — คุยกับ AI ที่สมัครไว้ และให้ AI ใช้เครื่องมือของแอปแทน (DECISIONS ข้อ 22) */
export function AssistantPanel() {
  const qc = useQueryClient();
  const close = () => useUi.getState().setAssistantOpen(false);
  const { provider, tools, messages, running, setProvider, setTools, send, stop, clear } = useAssistant();
  const status = useQuery({ queryKey: ['ai-status'], queryFn: () => call('ai_status', {}), staleTime: 60_000 });
  const cli = status.data?.find((s) => s.provider === provider);
  const location = useRealLocation();
  const openPhotoId = location.pathname.startsWith('/photos/') ? decodeURIComponent(location.pathname.split('/')[2] ?? '') : null;
  const openPhoto = useQuery({ queryKey: qk.photo(openPhotoId ?? ''), queryFn: () => call('get_photo_details', { photoId: openPhotoId! }), enabled: Boolean(openPhotoId) });
  const [draft, setDraft] = useState('');
  const list = useRef<HTMLDivElement>(null);
  const product = PROVIDERS.find((p) => p.value === provider)!;
  const ready = Boolean(cli?.installed) && cli?.signedIn !== false;

  // เลื่อนลงล่างสุดเมื่อมีข้อความใหม่
  const last = messages.at(-1);
  useEffect(() => {
    list.current?.scrollTo({ top: list.current.scrollHeight });
  }, [messages.length, last?.text, last?.steps.length, last?.photos.length, last?.status]);

  const submit = (text = draft) => {
    if (!text.trim() || running || !ready) return;
    const context = openPhoto.data ? `ผู้ใช้กำลังเปิดดูรูป id=${openPhoto.data.id} (${openPhoto.data.filename}) จากกิจกรรม “${openPhoto.data.eventTitle}”` : null;
    setDraft('');
    void send(text, context, qc);
  };

  return (
    <aside aria-label="ผู้ช่วย AI" className="flex h-full w-[420px] shrink-0 flex-col border-l border-line bg-surface" data-testid="assistant-panel">
      <div data-tauri-drag-region className="flex shrink-0 items-center gap-2 border-b border-line px-4 pt-3 pb-2.5">
        <Sparkles className="size-5 text-action" aria-hidden />
        <h2 className="flex-1 text-[15px] font-bold text-ink">ผู้ช่วย AI</h2>
        <IconButton label="เริ่มคุยใหม่" size="sm" onClick={clear} disabled={messages.length === 0} data-testid="assistant-new">
          <SquarePen className="size-4" aria-hidden />
        </IconButton>
        <IconButton label="ปิดผู้ช่วย AI" size="sm" onClick={close} data-testid="assistant-close">
          <X className="size-4" aria-hidden />
        </IconButton>
      </div>

      <div className="shrink-0 space-y-2 border-b border-line px-4 py-3">
        <div role="radiogroup" aria-label="AI ที่ใช้" className="grid grid-cols-3 gap-1 rounded-control border border-line bg-sunken p-0.5">
          {PROVIDERS.map((p) => {
            const s = status.data?.find((x) => x.provider === p.value);
            const active = p.value === provider;
            return (
              <button
                key={p.value}
                type="button"
                role="radio"
                aria-checked={active}
                disabled={Boolean(running)}
                onClick={() => setProvider(p.value)}
                className={cn('flex h-8 items-center justify-center gap-1.5 rounded-[7px] text-[13px] font-semibold', active ? 'bg-surface text-action shadow-card' : 'text-muted hover:text-ink')}
                data-testid={`assistant-provider-${p.value}`}
              >
                <span aria-hidden className={cn('size-1.5 rounded-full', !s ? 'bg-line-strong' : s.installed && s.signedIn === true ? 'bg-ok' : 'bg-warn')} />
                {p.label}
              </button>
            );
          })}
        </div>
        <CliLine cli={cli} product={product.product} loading={status.isPending} onRecheck={() => void status.refetch()} rechecking={status.isFetching} />
      </div>

      <div ref={list} className="scroll-thin min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-4" data-testid="assistant-messages">
        {messages.length === 0 ? (
          <Intro product={product.product} tools={tools} onPick={(t) => submit(t)} disabled={!ready} />
        ) : (
          messages.map((m) => (m.role === 'user' ? <UserBubble key={m.id} message={m} /> : <AnswerView key={m.id} message={m} />))
        )}
      </div>

      <div className="shrink-0 space-y-2 border-t border-line px-4 pt-3 pb-4">
        {openPhoto.data && (
          <p className="flex items-center gap-1.5 truncate text-[12.5px] text-muted" data-testid="assistant-context">
            <ImageIcon className="size-3.5 shrink-0" aria-hidden />
            ส่งไปด้วย: รูปที่เปิดอยู่ <span className="truncate font-semibold text-ink">{openPhoto.data.filename}</span>
          </p>
        )}
        <div className="flex items-end gap-2 rounded-xl border border-line-strong bg-surface px-3 py-2 focus-within:border-action">
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                submit();
              }
            }}
            rows={Math.min(6, Math.max(1, draft.split('\n').length))}
            placeholder={ready ? 'สั่งงาน เช่น “แต่งรูปนี้ให้สว่างขึ้น”' : 'ยังใช้ AI นี้ไม่ได้ — ดูวิธีแก้ด้านบน'}
            disabled={!ready}
            aria-label="ข้อความถึงผู้ช่วย AI"
            className="max-h-40 min-h-6 flex-1 resize-none bg-transparent text-sm leading-6 text-ink outline-none placeholder:text-faint disabled:cursor-not-allowed"
            data-testid="assistant-input"
          />
          {running ? (
            <IconButton label="หยุด" size="sm" onClick={stop} className="bg-ink text-surface hover:bg-ink/85 hover:text-surface" data-testid="assistant-stop">
              <Square className="size-3.5 fill-current" aria-hidden />
            </IconButton>
          ) : (
            <IconButton
              label="ส่ง"
              size="sm"
              onClick={() => submit()}
              disabled={!draft.trim() || !ready}
              className="bg-action text-on-action hover:bg-action-hover hover:text-on-action disabled:bg-sunken disabled:text-faint"
              data-testid="assistant-send"
            >
              <ArrowUp className="size-4" aria-hidden />
            </IconButton>
          )}
        </div>
        <Switch
          checked={tools}
          onCheckedChange={setTools}
          label="ให้ AI ใช้เครื่องมือของแอป"
          description={tools ? 'ค้นรูป เปิดดูรูป และแต่งรูปแทนได้ · ไม่ลบหรือย้ายไฟล์ · การแก้ทุกครั้งย้อนได้' : 'AI ตอบอย่างเดียว ไม่เห็นและแก้คลังไม่ได้'}
          disabled={Boolean(running)}
          testId="assistant-tools"
        />
      </div>
    </aside>
  );
}

function CliLine({ cli, product, loading, onRecheck, rechecking }: { cli: AiCliStatus | undefined; product: string; loading: boolean; onRecheck: () => void; rechecking: boolean }) {
  const [copied, setCopied] = useState(false);
  if (loading) {
    return (
      <p className="flex items-center gap-2 text-[12.5px] text-muted">
        <Spinner className="size-3.5" /> กำลังหา AI ที่ติดตั้งในเครื่อง…
      </p>
    );
  }
  if (cli?.installed && cli.signedIn !== false) {
    return (
      <p className="truncate text-[12.5px] text-muted" title={cli.path ?? undefined} data-testid="assistant-cli">
        {product} {cli.version?.replace(/\s*\(.*\)$/, '').replace(/^codex-cli\s*/, '')} · {cli.signedIn ? 'ล็อกอินแล้ว (ใช้บัญชีที่สมัครไว้)' : 'ยังไม่รู้สถานะล็อกอิน จะรู้เมื่อเริ่มคุย'}
      </p>
    );
  }
  const command = cli?.loginCommand ?? '';
  return (
    <InlineNotice tone="warn" icon={<TriangleAlert aria-hidden />}>
      <span className="block font-semibold">{cli?.installed ? `ยังไม่ได้ล็อกอิน ${product}` : `ไม่พบ ${product} ในเครื่องนี้`}</span>
      {cli?.installed ? (
        <span className="mt-1 flex flex-wrap items-center gap-1.5">
          เปิด Terminal แล้วพิมพ์
          <code className="rounded bg-surface px-1.5 py-0.5 font-mono text-[12px] text-ink">{command}</code>
          <button
            type="button"
            className="inline-flex items-center gap-1 text-[12.5px] font-semibold text-action hover:underline"
            onClick={() =>
              void navigator.clipboard.writeText(command).then(
                () => {
                  setCopied(true);
                  window.setTimeout(() => setCopied(false), 1500);
                },
                () => toast.error('คัดลอกไม่สำเร็จ'),
              )
            }
          >
            {copied ? <Check className="size-3.5" aria-hidden /> : <Copy className="size-3.5" aria-hidden />}
            {copied ? 'คัดลอกแล้ว' : 'คัดลอก'}
          </button>
        </span>
      ) : (
        <span className="mt-1 block">ติดตั้งและล็อกอินด้วยบัญชีที่สมัครไว้ก่อน แอปใช้บัญชีนั้นโดยตรง ไม่ต้องใส่ API key</span>
      )}
      <button type="button" onClick={onRecheck} className="mt-1.5 text-[12.5px] font-semibold text-action hover:underline" disabled={rechecking}>
        {rechecking ? 'กำลังตรวจ…' : 'ตรวจอีกครั้ง'}
      </button>
    </InlineNotice>
  );
}

function Intro({ product, tools, onPick, disabled }: { product: string; tools: boolean; onPick: (text: string) => void; disabled: boolean }) {
  return (
    <div className="space-y-4 pt-2" data-testid="assistant-intro">
      <div className="space-y-1.5 text-[13.5px] text-ink">
        <p className="font-semibold">สั่งให้ AI ทำงานในคลังรูปแทนได้</p>
        <ul className="list-disc space-y-1 pl-5 text-[13px] text-muted">
          {tools ? (
            <>
              <li>ค้นรูปตามคน กิจกรรม วันที่ เปิดดูรูป แล้วแต่งแสง สี เส้นโค้ง Levels และ Color Grading ให้</li>
              <li>ใช้หรือบันทึก preset · แต่งหลายรูปพร้อมกัน</li>
              <li>ไม่ลบ ไม่ย้าย ไม่ส่งออกไฟล์ · ไฟล์ต้นฉบับไม่ถูกแตะ · ย้อนการแก้ของแต่ละรอบได้ด้วยปุ่มเดียว</li>
            </>
          ) : (
            <li>ตอนนี้ปิดเครื่องมือของแอปไว้ AI จึงตอบได้อย่างเดียว</li>
          )}
          <li>ข้อความ{tools ? ' และรูปที่ AI เปิดดู' : ''}จะถูกส่งไปยัง {product} ภายใต้บัญชีที่ล็อกอินไว้</li>
        </ul>
      </div>
      <div className="space-y-1.5">
        {SUGGESTIONS.map((s) => (
          <button
            key={s}
            type="button"
            disabled={disabled}
            onClick={() => onPick(s)}
            className="block w-full rounded-xl border border-line px-3 py-2 text-left text-[13px] text-ink hover:border-brand-line hover:bg-sunken disabled:opacity-50"
            data-testid="assistant-suggestion"
          >
            {s}
          </button>
        ))}
      </div>
    </div>
  );
}

function UserBubble({ message }: { message: ChatMessage }) {
  return (
    <div className="flex justify-end">
      <p className="max-w-[85%] rounded-2xl rounded-br-md bg-brand-soft px-3.5 py-2 text-sm whitespace-pre-wrap text-ink" data-testid="assistant-user">
        {message.text}
      </p>
    </div>
  );
}

function Steps({ steps, running }: { steps: ToolStep[]; running: boolean }) {
  const [open, setOpen] = useState(false);
  if (steps.length === 0) return null;
  const current = running ? [...steps].reverse().find((s) => s.ok === null) : undefined;
  return (
    <div className="rounded-xl border border-line text-[12.5px]" data-testid="assistant-steps">
      <button type="button" aria-expanded={open} onClick={() => setOpen((v) => !v)} className="flex w-full items-center gap-1.5 px-2.5 py-1.5 text-left text-muted hover:text-ink">
        <ChevronRight className={cn('size-3.5 transition-transform', open && 'rotate-90')} aria-hidden />
        <Wrench className="size-3.5" aria-hidden />
        <span className="min-w-0 flex-1 truncate">
          {current ? `กำลัง${TOOL_LABELS[current.name] ?? current.name}…` : `ใช้เครื่องมือของแอป ${formatCount(steps.length)} ครั้ง`}
        </span>
        {current && <Spinner className="size-3.5" />}
      </button>
      {open && (
        <ol className="space-y-1.5 border-t border-line px-2.5 py-2">
          {steps.map((s) => (
            <li key={s.id} className="min-w-0">
              <p className="flex items-center gap-1.5">
                {s.ok === null ? <Spinner className="size-3" /> : s.ok ? <Check className="size-3.5 text-ok" aria-label="สำเร็จ" /> : <X className="size-3.5 text-danger" aria-label="ไม่สำเร็จ" />}
                <span className="font-semibold text-ink">{TOOL_LABELS[s.name] ?? s.name}</span>
                <span className="min-w-0 truncate text-muted">{s.summary}</span>
              </p>
              {s.result && <pre className="mt-1 max-h-32 overflow-auto rounded-md bg-sunken px-2 py-1 font-sans text-[11.5px] whitespace-pre-wrap text-muted">{s.result}</pre>}
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

function PhotoStrip({ title, photoIds, version }: { title: string; photoIds: string[]; version: string }) {
  const viewer = useViewer();
  return (
    <div className="space-y-1.5" data-testid="assistant-photos">
      {title && <p className="text-[12.5px] font-semibold text-muted">{title}</p>}
      <div className="grid grid-cols-4 gap-1.5">
        {photoIds.map((id) => (
          <button
            key={id}
            type="button"
            onClick={() => viewer.open(id, { ids: photoIds, originLabel: 'กลับ' })}
            className="aspect-square overflow-hidden rounded-lg bg-sunken outline-none focus-visible:ring-2 focus-visible:ring-[var(--action)]"
            aria-label="เปิดดูรูป"
          >
            <img src={assetUrl('thumb', id, version)} alt="" loading="lazy" className="size-full object-cover" />
          </button>
        ))}
      </div>
    </div>
  );
}

const AnswerView = memo(function AnswerView({ message }: { message: ChatMessage }) {
  const qc = useQueryClient();
  const undo = useAssistant((s) => s.undo);
  const [undoing, setUndoing] = useState(false);
  const running = message.status === 'running';
  const product = PROVIDERS.find((p) => p.value === message.provider)?.label;
  return (
    <div className="space-y-2" data-testid="assistant-answer" data-status={message.status}>
      <p className="text-[12px] font-semibold text-faint">
        {product}
        {message.model ? ` · ${message.model}` : ''}
      </p>
      <Steps steps={message.steps} running={running} />
      {message.text ? <Markdown text={message.text} /> : running && message.steps.length === 0 ? <p className="flex items-center gap-2 text-sm text-muted"><Spinner className="size-4" /> กำลังคิด…</p> : null}
      {message.photos.map((p, i) => (
        <PhotoStrip key={i} title={p.title} photoIds={p.photoIds} version={`${message.turnId}-${message.status}`} />
      ))}
      {message.status === 'error' && (
        <InlineNotice tone="danger" icon={<TriangleAlert aria-hidden />}>
          {message.error ?? 'AI ทำรอบนี้ไม่สำเร็จ'}
        </InlineNotice>
      )}
      {message.status === 'stopped' && <p className="text-[12.5px] text-muted">หยุดก่อนตอบจบ</p>}
      {message.undoable > 0 && !running && (
        <div className="flex items-center gap-2 rounded-xl bg-sunken px-3 py-2 text-[12.5px]">
          <span className="flex-1 text-muted">{message.undone ? `ย้อนการแก้ ${formatCount(message.undoable)} รูปแล้ว` : `AI แก้ ${formatCount(message.undoable)} รูปในรอบนี้`}</span>
          {!message.undone && (
            <Button
              size="sm"
              variant="ghost"
              icon={<RotateCcw className="size-3.5" aria-hidden />}
              busy={undoing}
              onClick={() => {
                setUndoing(true);
                undo(message.id, qc)
                  .then((n) => toast.success(`ย้อนการแก้ ${formatCount(n)} รูปแล้ว`))
                  .catch((e) => toast.error(errorMessage(e)))
                  .finally(() => setUndoing(false));
              }}
              data-testid="assistant-undo"
            >
              ย้อนการแก้
            </Button>
          )}
        </div>
      )}
    </div>
  );
});

function InlineText({ inline }: { inline: Inline[] }) {
  return (
    <>
      {inline.map((part, i) =>
        part.kind === 'code' ? (
          <code key={i} className="rounded bg-sunken px-1 py-px font-mono text-[12px]">
            {part.text}
          </code>
        ) : part.kind === 'strong' ? (
          <strong key={i} className="font-semibold">
            {part.text}
          </strong>
        ) : (
          <span key={i}>{part.text}</span>
        ),
      )}
    </>
  );
}

function Markdown({ text }: { text: string }) {
  const blocks = useMemo(() => parseMarkdown(text), [text]);
  return (
    <div className="space-y-2 text-sm leading-relaxed break-words text-ink select-text" data-testid="assistant-text">
      {blocks.map((block, i) => {
        switch (block.kind) {
          case 'code':
            return (
              <pre key={i} className="overflow-x-auto rounded-lg bg-sunken px-3 py-2 font-mono text-[12px]">
                {block.text}
              </pre>
            );
          case 'heading':
            return (
              <p key={i} className="font-semibold">
                <InlineText inline={block.inline} />
              </p>
            );
          case 'list': {
            const List = block.ordered ? 'ol' : 'ul';
            return (
              <List key={i} className={cn('space-y-1 pl-5', block.ordered ? 'list-decimal' : 'list-disc')}>
                {block.items.map((item, n) => (
                  <li key={n} className="whitespace-pre-wrap">
                    <InlineText inline={item} />
                  </li>
                ))}
              </List>
            );
          }
          case 'table':
            return (
              <div key={i} className="overflow-x-auto rounded-lg border border-line">
                <table className="w-full border-collapse text-[12.5px]">
                  <thead className="bg-sunken text-left">
                    <tr>
                      {block.header.map((cell, c) => (
                        <th key={c} className="border-b border-line px-2 py-1.5 font-semibold whitespace-nowrap">
                          <InlineText inline={cell} />
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {block.rows.map((row, r) => (
                      <tr key={r} className="border-b border-line last:border-0">
                        {row.map((cell, c) => (
                          <td key={c} className="px-2 py-1.5 align-top">
                            <InlineText inline={cell} />
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            );
          default:
            return (
              <p key={i} className="whitespace-pre-wrap">
                <InlineText inline={block.inline} />
              </p>
            );
        }
      })}
    </div>
  );
}
