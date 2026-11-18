import type { AppError, AssetKind, CommandName, Commands, DataChangedEvent } from './types';

/**
 * ทางเรียก Rust ทางเดียวของ UI
 *
 * - ในแอป Tauri: `invoke(cmd, { args })` — หนึ่งคำสั่ง Tauri ต่อหนึ่งคำสั่งโดเมน (แบบเดียวกับ PuriCat)
 *   ทุกตัวเป็น wrapper บาง ๆ ไปยัง `puri_core::api::dispatch`
 * - ใน development preview บนเบราว์เซอร์: POST ไปที่ dev bridge (`cargo run -p puri-devbridge`)
 *   ซึ่งเรียก dispatch ตัวเดียวกันกับคลังทดสอบ ไม่มีข้อมูลปลอมฝั่ง UI
 */

const DEV_BRIDGE = 'http://127.0.0.1:1421';

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export class IpcError extends Error implements AppError {
  code: AppError['code'];
  retryable: boolean;
  affectedIds?: string[];
  nextAction?: AppError['nextAction'];

  constructor(e: AppError) {
    super(e.message);
    this.name = 'IpcError';
    this.code = e.code;
    this.retryable = e.retryable;
    this.affectedIds = e.affectedIds;
    this.nextAction = e.nextAction;
  }
}

function toIpcError(raw: unknown): IpcError {
  if (raw && typeof raw === 'object' && 'code' in raw && 'message' in raw) {
    return new IpcError(raw as AppError);
  }
  return new IpcError({
    code: 'INTERNAL',
    message: 'เชื่อมต่อกับส่วนประมวลผลของแอปไม่ได้ กรุณาลองใหม่',
    retryable: true,
  });
}

export async function call<K extends CommandName>(
  cmd: K,
  args: Commands[K][0],
): Promise<Commands[K][1]> {
  if (isTauri) {
    const { invoke } = await import('@tauri-apps/api/core');
    try {
      return await invoke<Commands[K][1]>(cmd, { args });
    } catch (e) {
      throw toIpcError(e);
    }
  }
  let res: Response;
  try {
    res = await fetch(`${DEV_BRIDGE}/rpc`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ cmd, args }),
    });
  } catch {
    throw toIpcError(null);
  }
  const body = await res.json();
  if (!res.ok) throw toIpcError(body);
  return body as Commands[K][1];
}

/** URL ของ asset สำหรับ `<img src>`; `version` ใช้กัน cache เมื่อข้อมูลเปลี่ยน */
export function assetUrl(kind: AssetKind, id: string, version?: string | number): string {
  const q = version === undefined ? '' : `?v=${encodeURIComponent(String(version))}`;
  if (!isTauri) return `${DEV_BRIDGE}/asset/${kind}/${id}${q}`;
  // custom protocol ของ Tauri: macOS/Linux ใช้ scheme ตรง ๆ ส่วน Windows ใช้ http://<scheme>.localhost
  const base = navigator.userAgent.includes('Windows') ? 'http://puri.localhost' : 'puri://localhost';
  return `${base}/${kind}/${id}${q}`;
}

/** URL จาก path ของ asset ที่ Rust ส่งมาแล้ว เช่น `PersonRef.coverAsset` = "face/{id}" หรือ "sample/{id}" */
export function assetPath(path: string, version?: string | number): string {
  const [kind, ...rest] = path.split('/');
  return assetUrl(kind as AssetKind, rest.join('/'), version);
}

/** เปิด native folder picker (เฉพาะในแอป Tauri); คืน null เมื่อผู้ใช้ยกเลิก */
export async function pickFolder(title: string): Promise<string | null> {
  if (!isTauri) throw new Error('pickFolder ใช้ได้เฉพาะในแอป');
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string | null>('pick_folder', { title });
}

/** ฟังการเปลี่ยนข้อมูลจาก Rust; ใน dev bridge ไม่มี event ให้ UI ใช้ polling ตามปกติ */
export async function onDataChanged(handler: (e: DataChangedEvent) => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  return listen<DataChangedEvent>('data-changed', (ev) => handler(ev.payload));
}

/** ฟังโฟลเดอร์ที่ถูกลากมาวางบนหน้าต่าง (เฉพาะในแอป Tauri) */
export async function onFolderDropped(handler: (path: string) => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { getCurrentWebview } = await import('@tauri-apps/api/webview');
  return getCurrentWebview().onDragDropEvent((ev) => {
    if (ev.payload.type !== 'drop') return;
    const [first] = ev.payload.paths;
    if (first) handler(first);
  });
}
