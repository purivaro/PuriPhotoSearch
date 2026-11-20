// ตัวอ่าน Markdown ขนาดเล็กสำหรับคำตอบของ AI (ยกมาจาก PuriCat src/features/agent/markdown.ts — ตัดส่วน SQL ออก)
// ได้เป็นข้อมูลธรรมดาให้ React วาด จึงไม่มีอะไรจากคำตอบถูกแทรกเป็น HTML

export interface Inline {
  kind: 'text' | 'code' | 'strong';
  text: string;
}

export type Block =
  | { kind: 'code'; lang: string; text: string; /** still streaming: no closing fence yet */ open: boolean }
  | { kind: 'paragraph'; inline: Inline[] }
  | { kind: 'heading'; level: number; inline: Inline[] }
  | { kind: 'list'; ordered: boolean; items: Inline[][] }
  | { kind: 'table'; header: Inline[][]; rows: Inline[][][] };

export function parseInline(text: string): Inline[] {
  const out: Inline[] = [];
  const pattern = /`([^`\n]+)`|\*\*([^*\n]+)\*\*/g;
  let at = 0;
  for (let m = pattern.exec(text); m; m = pattern.exec(text)) {
    if (m.index > at) out.push({ kind: 'text', text: text.slice(at, m.index) });
    out.push(m[1] !== undefined ? { kind: 'code', text: m[1] } : { kind: 'strong', text: m[2]! });
    at = m.index + m[0].length;
  }
  if (at < text.length) out.push({ kind: 'text', text: text.slice(at) });
  return out;
}

const FENCE = /^\s*(```+|~~~+)\s*([\w+#.-]*)\s*$/;
const HEADING = /^(#{1,6})\s+(.*)$/;
const ITEM = /^\s*(?:([-*+])|(\d+)[.)])\s+(.*)$/;
const TABLE_RULE = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/;

const cells = (line: string) =>
  line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((c) => parseInline(c.trim()));

export function parseMarkdown(source: string): Block[] {
  const lines = source.replace(/\r\n?/g, '\n').split('\n');
  const blocks: Block[] = [];
  let paragraph: string[] = [];
  const flush = () => {
    if (paragraph.length) blocks.push({ kind: 'paragraph', inline: parseInline(paragraph.join('\n')) });
    paragraph = [];
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    const fence = FENCE.exec(line);
    if (fence) {
      flush();
      const marker = fence[1]![0]!;
      const body: string[] = [];
      let open = true;
      for (i++; i < lines.length; i++) {
        const close = FENCE.exec(lines[i]!);
        if (close && close[1]![0] === marker && close[2] === '') {
          open = false;
          break;
        }
        body.push(lines[i]!);
      }
      blocks.push({ kind: 'code', lang: fence[2]!.toLowerCase(), text: body.join('\n'), open });
      continue;
    }
    if (line.trim() === '') {
      flush();
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      blocks.push({ kind: 'heading', level: heading[1]!.length, inline: parseInline(heading[2]!.trim()) });
      continue;
    }
    if (line.includes('|') && TABLE_RULE.test(lines[i + 1] ?? '')) {
      flush();
      const header = cells(line);
      const rows: Inline[][][] = [];
      for (i += 2; i < lines.length && lines[i]!.includes('|') && lines[i]!.trim() !== ''; i++) rows.push(cells(lines[i]!));
      i--;
      blocks.push({ kind: 'table', header, rows });
      continue;
    }
    const item = ITEM.exec(line);
    if (item) {
      flush();
      const ordered = item[2] !== undefined;
      const items: string[] = [item[3]!];
      for (i++; i < lines.length; i++) {
        const next = ITEM.exec(lines[i]!);
        if (next && (next[2] !== undefined) === ordered) items.push(next[3]!);
        // An indented line continues the item above it.
        else if (!next && /^\s+\S/.test(lines[i]!) && !FENCE.test(lines[i]!)) items[items.length - 1] += `\n${lines[i]!.trim()}`;
        else break;
      }
      i--;
      blocks.push({ kind: 'list', ordered, items: items.map(parseInline) });
      continue;
    }
    paragraph.push(line);
  }
  flush();
  return blocks;
}
