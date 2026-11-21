// ต้นแบบ (exp/preview-wasm · DECISIONS ข้อ 30): วัดความเร็วตัวอย่างสดในหน้าต่างแอปจริง — TS · WASM · Rust ผ่าน IPC (แกนเดียว / ทุกแกน)
(async () => {
  if (window.__benchStarted) return;
  window.__benchStarted = true;
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const out = { ua: navigator.userAgent, dpr: devicePixelRatio, results: {} };
  const report = (stage) => invoke('e2e_report', { text: JSON.stringify({ stage, ...out }, null, 1) });
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const frame = () => new Promise((r) => requestAnimationFrame(() => r(performance.now())));
  const stats = (t) => {
    const s = [...t].sort((a, b) => a - b);
    return { n: t.length, mean: +(t.reduce((a, b) => a + b, 0) / t.length).toFixed(1), p95: +s[Math.min(s.length - 1, Math.floor(s.length * 0.95))].toFixed(1), min: +s[0].toFixed(1) };
  };
  const LIGHT = {"rotate": 0, "straighten": 0, "crop": null, "adjust": {"exposure": 0.3, "contrast": 15, "highlights": -30, "shadows": 25, "whites": 10, "blacks": -8, "texture": 0, "clarity": 0, "dehaze": 0, "temperature": 6, "tint": 3, "vibrance": 15, "saturation": 5, "hue": [0, 0, 0, 0, 0, 0, 0, 0], "sat": [0, 0, 0, 0, 0, 0, 0, 0], "lum": [0, 0, 0, 0, 0, 0, 0, 0], "parametric": [0, 0, 0, 0], "parametricSplit": null, "curve": [], "curveRed": [], "curveGreen": [], "curveBlue": [], "levels": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "levelsRed": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "levelsGreen": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "levelsBlue": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "grading": {"shadows": {"hue": 0, "sat": 0, "lum": 0}, "midtones": {"hue": 0, "sat": 0, "lum": 0}, "highlights": {"hue": 0, "sat": 0, "lum": 0}, "global": {"hue": 0, "sat": 0, "lum": 0}, "blending": 50, "balance": 0}, "vignette": {"amount": 0, "midpoint": 50, "feather": 50, "roundness": 0}}};
  const HEAVY = {"rotate": 0, "straighten": 0, "crop": null, "adjust": {"exposure": 0.3, "contrast": 15, "highlights": -30, "shadows": 25, "whites": 10, "blacks": -8, "texture": -20, "clarity": 10, "dehaze": 8, "temperature": 6, "tint": 3, "vibrance": 15, "saturation": 5, "hue": [0, 3, -1, 7, 0, 5, 0, 0], "sat": [12, -3, -13, -20, 15, -9, 0, 0], "lum": [10, 30, 2, 11, 25, 0, 5, 6], "parametric": [-5, 4, 12, 0], "parametricSplit": null, "curve": [[0, 8], [64, 60], [190, 200], [255, 250]], "curveRed": [], "curveGreen": [], "curveBlue": [], "levels": {"inBlack": 4, "inWhite": 250, "gamma": 1.05, "outBlack": 2, "outWhite": 253}, "levelsRed": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "levelsGreen": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "levelsBlue": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "grading": {"shadows": {"hue": 220, "sat": 20, "lum": -5}, "midtones": {"hue": 35, "sat": 10, "lum": 0}, "highlights": {"hue": 45, "sat": 15, "lum": 3}, "global": {"hue": 0, "sat": 0, "lum": 0}, "blending": 60, "balance": -10}, "vignette": {"amount": -20, "midpoint": 50, "feather": 60, "roundness": 0}}, "masks": [{"id": "m1", "area": {"kind": "faces", "boxes": [[0.2907, 0.4097, 0.054, 0.0446], [0.441, 0.5883, 0.0579, 0.0533], [0.8821, 0.1686, 0.1092, 0.0943]], "skin": true}, "adjust": {"exposure": 0.25, "contrast": 0, "highlights": 0, "shadows": 0, "whites": 0, "blacks": 0, "texture": -35, "clarity": -10, "dehaze": 0, "temperature": -6, "tint": 4, "vibrance": 0, "saturation": -12, "hue": [0, 0, 0, 0, 0, 0, 0, 0], "sat": [0, 0, 0, 0, 0, 0, 0, 0], "lum": [0, 0, 0, 0, 0, 0, 0, 0], "parametric": [0, 0, 0, 0], "parametricSplit": null, "curve": [], "curveRed": [], "curveGreen": [], "curveBlue": [], "levels": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "levelsRed": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "levelsGreen": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "levelsBlue": {"inBlack": 0, "inWhite": 255, "gamma": 1, "outBlack": 0, "outWhite": 255}, "grading": {"shadows": {"hue": 0, "sat": 0, "lum": 0}, "midtones": {"hue": 0, "sat": 0, "lum": 0}, "highlights": {"hue": 0, "sat": 0, "lum": 0}, "global": {"hue": 0, "sat": 0, "lum": 0}, "blending": 50, "balance": 0}, "vignette": {"amount": 0, "midpoint": 50, "feather": 50, "roundness": 0}}, "amount": 100, "invert": false}]};
  const withExposure = (s, e) => ({ ...s, adjust: { ...s.adjust, exposure: e } });
  try {
    await report('start');
    for (let i = 0; i < 400 && !window.__puriBench; i++) await sleep(50);
    const B = window.__puriBench;
    if (!B) throw new Error('ไม่มี __puriBench');
    // ภาพต้นทาง: รูปจริง (MOT03115 ภาพที่แต่ง) ย่อเป็น 1600×2400
    const [W, H] = [1600, 2400];
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.src = 'puri://localhost/photo/01a1fcd2-29e8-753e-82cc-7ca53cc764b2';
    await report('loading-image');
    await img.decode();
    await report('image-ok');
    const source = document.createElement('canvas');
    source.width = W;
    source.height = H;
    source.getContext('2d').drawImage(img, 0, 0, W, H);
    // canvas ที่แสดงบนจอจริง (backing store เต็มขนาด แสดงย่อ)
    const view = document.createElement('canvas');
    view.width = W;
    view.height = H;
    Object.assign(view.style, { position: 'fixed', left: '0', top: '0', width: '400px', height: '600px', zIndex: 99999, background: '#000' });
    document.body.appendChild(view);
    const ctx = view.getContext('2d', { willReadFrequently: true });
    const base = source.getContext('2d').getImageData(0, 0, W, H).data;
    const FULL = { x: 0, y: 0, w: 1, h: 1 };
    const job = (s) => JSON.stringify({ settings: s, profile: 'display', sourceW: W, sourceH: H });

    // WASM: หน่วยความจำของภาพจองครั้งเดียว
    await report('loading-wasm');
    const ex = await B.wasm();
    await report('wasm-ok');
    const ptr = ex.alloc(W * H * 4);
    const jptr = ex.alloc(1 << 16);
    const enc = new TextEncoder();
    // Rust: ส่งภาพต้นทางไปเก็บครั้งเดียว
    const t0 = performance.now();
    await invoke('bench_preview_source', new Uint8Array(base.buffer.slice(0)), { headers: { 'x-w': String(W), 'x-h': String(H) } });
    out.uploadSourceMs = +(performance.now() - t0).toFixed(1);

    // แต่ละวิธี: วาดภาพที่แต่งลง canvas ที่แสดง (คืนเวลาที่ใช้แบบ synchronous ส่วน IPC รอผล)
    const methods = {
      ts: async (s) => {
        ctx.drawImage(source, 0, 0);
        const d = ctx.getImageData(0, 0, W, H);
        B.renderPixels(d.data, W, H, s, 'display', { view: FULL, sourceW: W, sourceH: H, whole: true });
        ctx.putImageData(d, 0, 0);
      },
      wasm: async (s) => {
        ctx.drawImage(source, 0, 0);
        const d = ctx.getImageData(0, 0, W, H);
        new Uint8Array(ex.memory.buffer, ptr, W * H * 4).set(d.data);
        const j = enc.encode(job(s));
        new Uint8Array(ex.memory.buffer, jptr, j.length).set(j);
        if (ex.render(ptr, W, H, jptr, j.length) !== 0) throw new Error('wasm render');
        ctx.putImageData(new ImageData(new Uint8ClampedArray(ex.memory.buffer, ptr, W * H * 4), W, H), 0, 0);
      },
      ipc1: async (s) => {
        const buf = await invoke('bench_preview_render', { job: job(s), parallel: false });
        ctx.putImageData(new ImageData(new Uint8ClampedArray(buf), W, H), 0, 0);
      },
      ipcN: async (s) => {
        const buf = await invoke('bench_preview_render', { job: job(s), parallel: true });
        ctx.putImageData(new ImageData(new Uint8ClampedArray(buf), W, H), 0, 0);
      },
    };

    // ตรวจว่าทุกวิธีได้ภาพเดียวกัน (ต่างไม่เกิน 1)
    const pixels = async (m, s) => {
      await methods[m](s);
      return ctx.getImageData(0, 0, W, H).data;
    };
    out.sameImage = {};
    for (const [wname, ws] of [['light', LIGHT], ['heavy', HEAVY]]) {
      const ref = await pixels('ts', ws);
      for (const m of ['wasm', 'ipc1', 'ipcN']) {
        const got = await pixels(m, ws);
        let max = 0;
        for (let i = 0; i < got.length; i++) max = Math.max(max, Math.abs(got[i] - ref[i]));
        out.sameImage[`${wname}/${m}`] = max;
      }
    }
    await report('checked');

    const RUNS = 20;
    for (const [wname, ws] of [['light', LIGHT], ['heavy', HEAVY]]) {
      for (const m of ['ts', 'wasm', 'ipc1', 'ipcN']) {
        // ทีละครั้ง: จากเปลี่ยนค่าถึงภาพขึ้นจอ (เฟรมถัดไป)
        const work = [];
        const screen = [];
        for (let k = 0; k < RUNS + 2; k++) {
          await frame();
          const start = performance.now();
          await methods[m](withExposure(ws, 0.3 + k * 0.01));
          const done = performance.now();
          const shown = await frame();
          if (k >= 2) {
            work.push(done - start);
            screen.push(shown - start);
          }
        }
        // ลากแถบต่อเนื่อง 3 วินาที: ค่าเปลี่ยนตามเวลา (60 ครั้ง/วินาที) · TS/WASM วาดค่าล่าสุดทุกเฟรม (บล็อกหน้าต่างระหว่างคิด)
        // · IPC มีคำขอค้างได้หนึ่งอัน ผลกลับมาแล้วส่งค่าล่าสุดต่อทันที (ทิ้งค่าเก่าที่ข้ามไป)
        const DRAG = 3000;
        const begin = performance.now();
        const valueAt = (t) => Math.floor((t - begin) / 16.7);
        const lat = [];
        let frames = 0;
        if (m === 'ts' || m === 'wasm') {
          while (performance.now() - begin < DRAG) {
            const t = performance.now();
            const v = valueAt(t);
            await methods[m](withExposure(ws, 0.3 + (v % 40) * 0.01));
            const shown = await frame();
            lat.push(shown - (begin + v * 16.7));
            frames += 1;
          }
        } else {
          while (performance.now() - begin < DRAG) {
            const v = valueAt(performance.now());
            await methods[m](withExposure(ws, 0.3 + (v % 40) * 0.01));
            const shown = await frame();
            lat.push(shown - (begin + v * 16.7));
            frames += 1;
          }
        }
        const secs = (performance.now() - begin) / 1000;
        out.results[`${wname}/${m}`] = { work: stats(work), toScreen: stats(screen), drag: { fps: +(frames / secs).toFixed(1), latency: stats(lat) } };
        await report(`${wname}/${m}`);
      }
    }
    await report('done');
  } catch (e) {
    out.error = `${e?.name ?? ''} ${e?.message ?? e} ${e?.stack ?? ''}`;
    await report('FAILED');
  }
})();
