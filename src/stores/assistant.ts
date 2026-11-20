import { create } from 'zustand';
import type { QueryClient } from '@tanstack/react-query';
import { errorMessage } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { AiEvent, AiProvider, Id, SelectionSpec } from '@/lib/ipc/types';

export interface ToolStep {
  id: number;
  name: string;
  summary: string;
  /** null = กำลังทำ */
  ok: boolean | null;
  result: string;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  /** ข้อความของผู้ใช้: สิ่งที่ส่งไปเป็นบริบท */
  context?: string | null;
  /** คำตอบ: */
  turnId?: Id;
  provider?: AiProvider;
  model?: string | null;
  steps: ToolStep[];
  photos: { title: string; photoIds: Id[] }[];
  status: 'running' | 'done' | 'error' | 'stopped';
  error?: string | null;
  /** จำนวนรูปที่ AI แก้ในรอบนี้ (ย้อนได้) */
  undoable: number;
  undone?: boolean;
}

const PROVIDER_KEY = 'puriphoto.aiProvider';
const TOOLS_KEY = 'puriphoto.aiTools';

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* ความสะดวกของ UI เท่านั้น */
  }
}

interface AssistantState {
  provider: AiProvider;
  tools: boolean;
  messages: ChatMessage[];
  /** รอบที่กำลังทำ */
  running: Id | null;
  setProvider: (p: AiProvider) => void;
  setTools: (on: boolean) => void;
  send: (text: string, context: string | null, qc: QueryClient) => Promise<void>;
  /** ให้ ChatGPT สร้างภาพจากรูปที่เลือก — ความคืบหน้าแสดงเป็นข้อความในแผงผู้ช่วย */
  generate: (args: { selection: SelectionSpec; count: number; prompt: string; template: string | null }, qc: QueryClient) => Promise<void>;
  stop: () => void;
  undo: (messageId: string, qc: QueryClient) => Promise<number>;
  clear: () => void;
}

const storedProvider = (): AiProvider => {
  const v = read(PROVIDER_KEY);
  return v === 'codex' || v === 'gemini' ? v : 'claude';
};

let seq = 0;
const nextId = () => `m${Date.now()}-${(seq += 1)}`;

/** สถานะการคุยกับผู้ช่วย AI (อยู่ในหน่วยความจำจนปิดแอป — ไม่บันทึกลงดิสก์) */
export const useAssistant = create<AssistantState>((set, get) => {
  const patch = (id: string, f: (m: ChatMessage) => ChatMessage) => set((s) => ({ messages: s.messages.map((m) => (m.id === id ? f(m) : m)) }));

  const apply = (id: string, e: AiEvent, qc: QueryClient) => {
    switch (e.kind) {
      case 'started':
        patch(id, (m) => ({ ...m, model: e.model }));
        break;
      case 'answer':
        patch(id, (m) => ({ ...m, text: e.text }));
        break;
      case 'tool':
        patch(id, (m) => ({ ...m, steps: [...m.steps, { id: e.id, name: e.name, summary: e.summary, ok: null, result: '' }] }));
        break;
      case 'toolDone':
        patch(id, (m) => ({ ...m, steps: m.steps.map((s) => (s.id === e.id ? { ...s, ok: e.ok, result: e.result } : s)) }));
        break;
      case 'photos':
        patch(id, (m) => ({ ...m, photos: [...m.photos, { title: e.title, photoIds: e.photoIds }] }));
        break;
      case 'changed':
        void qc.invalidateQueries({ queryKey: ['photos'] });
        void qc.invalidateQueries({ queryKey: ['photo'] });
        void qc.invalidateQueries({ queryKey: ['edit-presets'] });
        void qc.invalidateQueries({ queryKey: ['events'] });
        void qc.invalidateQueries({ queryKey: ['summary'] });
        break;
      case 'finished':
        patch(id, (m) => ({ ...m, status: e.stopped ? 'stopped' : e.ok ? 'done' : 'error', error: e.error, undoable: e.undoable }));
        break;
    }
  };

  /** อ่านเหตุการณ์ของรอบจนจบ แล้วอัปเดตข้อความคำตอบ */
  const follow = async (messageId: string, turnId: Id, qc: QueryClient) => {
    patch(messageId, (m) => ({ ...m, turnId }));
    set({ running: turnId });
    let seen = 0;
    try {
      for (;;) {
        const r = await call('ai_events', { turnId, after: seen });
        for (const e of r.events) apply(messageId, e, qc);
        seen += r.events.length;
        if (r.done && r.events.length === 0) break;
        await new Promise((resolve) => setTimeout(resolve, r.events.length > 0 ? 120 : 350));
      }
    } catch (e) {
      patch(messageId, (m) => ({ ...m, status: 'error', error: errorMessage(e) }));
    } finally {
      set({ running: null });
    }
  };

  return {
    provider: storedProvider(),
    tools: read(TOOLS_KEY) !== 'false',
    messages: [],
    running: null,
    setProvider: (provider) => {
      write(PROVIDER_KEY, provider);
      set({ provider });
    },
    setTools: (tools) => {
      write(TOOLS_KEY, String(tools));
      set({ tools });
    },
    send: async (text, context, qc) => {
      const { provider, tools, messages, running } = get();
      if (running || !text.trim()) return;
      // บทสนทนาที่ส่ง: คำถามทุกข้อ และคำตอบที่ไม่ล้ม
      const history = messages
        .filter((m) => m.role === 'user' || (m.status !== 'error' && m.text.trim()))
        .map((m) => ({ role: m.role, text: m.text }));
      const user: ChatMessage = { id: nextId(), role: 'user', text: text.trim(), context, steps: [], photos: [], status: 'done', undoable: 0 };
      const answer: ChatMessage = { id: nextId(), role: 'assistant', text: '', provider, steps: [], photos: [], status: 'running', undoable: 0 };
      set({ messages: [...messages, user, answer] });
      let turnId: Id;
      try {
        turnId = (await call('ai_start', { provider, messages: [...history, { role: 'user', text: user.text }], context, tools })).turnId;
      } catch (e) {
        patch(answer.id, (m) => ({ ...m, status: 'error', error: errorMessage(e) }));
        return;
      }
      await follow(answer.id, turnId, qc);
    },
    generate: async ({ selection, count, prompt, template }, qc) => {
      if (get().running) throw new Error('รอให้ผู้ช่วย AI ทำงานที่ค้างอยู่เสร็จก่อน');
      const user: ChatMessage = {
        id: nextId(),
        role: 'user',
        text: `สร้างภาพด้วย AI จาก ${count} รูป${template ? ` (แม่แบบ “${template}”)` : ''}\n${prompt}`,
        steps: [],
        photos: [],
        status: 'done',
        undoable: 0,
      };
      const answer: ChatMessage = { id: nextId(), role: 'assistant', text: '', provider: 'codex', model: null, steps: [], photos: [], status: 'running', undoable: 0 };
      set((s) => ({ messages: [...s.messages, user, answer] }));
      let turnId: Id;
      try {
        turnId = (await call('ai_generate', { selection, prompt, template })).turnId;
      } catch (e) {
        patch(answer.id, (m) => ({ ...m, status: 'error', error: errorMessage(e) }));
        return;
      }
      await follow(answer.id, turnId, qc);
    },
    stop: () => {
      const turnId = get().running;
      if (turnId) void call('ai_cancel', { turnId }).catch(() => undefined);
    },
    undo: async (messageId, qc) => {
      const m = get().messages.find((x) => x.id === messageId);
      if (!m?.turnId) return 0;
      const restored = await call('ai_undo', { turnId: m.turnId });
      patch(messageId, (x) => ({ ...x, undone: true }));
      void qc.invalidateQueries({ queryKey: ['photos'] });
      void qc.invalidateQueries({ queryKey: ['photo'] });
      return restored;
    },
    clear: () => {
      get().stop();
      set({ messages: [] });
    },
  };
});
