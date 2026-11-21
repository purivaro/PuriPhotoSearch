// ต้นแบบ (exp/preview-wasm) — ให้สคริปต์วัดความเร็วที่ฉีดเข้าหน้าต่างเรียกสูตรตัวอย่างสดตัวจริงได้: TS (renderPixels) และ WASM
import { renderPixels } from '@/features/photos/render';
import wasmUrl from './preview_wasm.wasm?url';

declare global {
  interface Window {
    __puriBench?: { renderPixels: typeof renderPixels; wasm: () => Promise<WebAssembly.Exports> };
  }
}

window.__puriBench = {
  renderPixels,
  async wasm() {
    const bytes = await (await fetch(wasmUrl)).arrayBuffer();
    return (await WebAssembly.instantiate(bytes, {})).instance.exports;
  },
};
