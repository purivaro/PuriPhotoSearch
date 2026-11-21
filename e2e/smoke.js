// ทดสอบเส้นทางหลักในหน้าต่างจริงของ Puri Photo Search (debug build ฉีดไฟล์นี้หลังหน้าโหลดเสร็จ — ดู e2e/run.sh)
// ทุกขั้นขับผ่าน DOM จริง → IPC จริง → Rust จริง → คลัง SQLite และไฟล์รูปจริงในโฟลเดอร์ชั่วคราว
//   เลือกโฟลเดอร์ → ตรวจรายการ → เพิ่มกิจกรรม → แสดงรูป → ตั้งชื่อบุคคล → ค้นหารูป → เปิดต้นฉบับ → ส่งออกสำเนา → แต่งภาพ + preset
(async () => {
  if (window.__e2eStarted) return;
  window.__e2eStarted = true;

  const invoke = window.__TAURI_INTERNALS__.invoke;
  const rpc = (cmd, args = {}) => invoke(cmd, { args });
  const PAUSE = 900; // พอให้ตัวรันถ่ายภาพหน้าต่างของแต่ละขั้น
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const log = [];
  // CSP ที่ถูกบล็อกและ error ที่ไม่มีใครจับระหว่างรัน ถือเป็นส่วนของรายงาน
  const problems = [];
  addEventListener('securitypolicyviolation', (e) => problems.push(`csp: ${e.violatedDirective} blocked ${e.blockedURI || 'inline'}`));
  addEventListener('error', (e) => problems.push(`error: ${e.message}`));
  addEventListener('unhandledrejection', (e) => problems.push(`rejection: ${e.reason?.message ?? e.reason}`));

  const report = (stage, extra = {}) => invoke('e2e_report', { text: JSON.stringify({ stage, ...extra, problems, log }, null, 1) });
  const stage = async (name, data = {}) => {
    log.push({ stage: name, ...data });
    await report(name);
    await sleep(PAUSE);
  };

  const waitFor = async (fn, what, timeout = 20000) => {
    const end = Date.now() + timeout;
    for (;;) {
      const v = await fn();
      if (v) return v;
      if (Date.now() > end) throw new Error(`รอไม่สำเร็จ: ${what}`);
      await sleep(50);
    }
  };
  const all = (sel, root = document) => [...root.querySelectorAll(sel)];
  const tid = (id, root = document) => root.querySelector(`[data-testid="${id}"]`);
  const tids = (id, root = document) => all(`[data-testid="${id}"]`, root);
  const text = (el) => el.textContent.replace(/\s+/g, ' ').trim();
  const see = (id, what = id, timeout) => waitFor(() => tid(id), what, timeout);
  const click = async (el, what) => {
    if (!el) throw new Error(`ไม่มีสิ่งให้กด: ${what}`);
    el.scrollIntoView({ block: 'nearest' });
    el.click();
    await sleep(80);
  };
  const type = async (el, value) => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, value);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(60);
  };
  const expect = (cond, message) => {
    if (!cond) throw new Error(`ไม่ตรงที่คาด: ${message}`);
  };
  const jobsIdle = () => waitFor(async () => (await rpc('list_jobs', { activeOnly: true })).length === 0, 'งานเบื้องหลังเสร็จ', 60000);
  const photoCard = (filename) => tids('photo-card').find((c) => tid('photo-card-open', c)?.getAttribute('aria-label')?.includes(filename));

  // ตัวชี้สังเคราะห์ไม่มี pointer จริงให้ capture: WebKit จะโยน NotFoundError — กลืนเฉพาะในเทสต์ ไม่แตะโค้ดของแอป
  const capture = Element.prototype.setPointerCapture;
  Element.prototype.setPointerCapture = function (id) {
    try {
      capture.call(this, id);
    } catch {
      /* synthetic pointer */
    }
  };
  /** ลากกรอบบนภาพ: พิกัดเป็นสัดส่วนของกล่องภาพที่แสดงจริง */
  const drawBox = async (x0, y0, x1, y1) => {
    const stageEl = tid('viewer-stage');
    const box = tid('viewer-image-box').getBoundingClientRect();
    const at = (fx, fy) => ({ clientX: box.left + box.width * fx, clientY: box.top + box.height * fy });
    const fire = (kind, p) =>
      stageEl.dispatchEvent(new PointerEvent(kind, { bubbles: true, cancelable: true, pointerId: 1, pointerType: 'mouse', button: 0, buttons: kind === 'pointerup' ? 0 : 1, ...p }));
    fire('pointerdown', at(x0, y0));
    await sleep(40);
    for (let i = 1; i <= 6; i++) {
      fire('pointermove', at(x0 + ((x1 - x0) * i) / 6, y0 + ((y1 - y0) * i) / 6));
      await sleep(20);
    }
    fire('pointerup', at(x1, y1));
    await sleep(120);
  };
  /** วาดกรอบแล้วตั้งชื่อ: เลือกคนเดิมถ้ามีชื่อนี้ ไม่งั้นสร้างบุคคลใหม่ */
  const labelFace = async (name, [x0, y0, x1, y1]) => {
    const before = tids('face-box').length;
    await click(await see('viewer-add-face'), 'เพิ่มกรอบใบหน้า');
    await drawBox(x0, y0, x1, y1);
    await see('face-draft-box', 'กรอบที่กำลังวาด');
    await type(await see('person-picker-input'), name);
    const choice = await waitFor(
      () => tids('person-picker-option').find((o) => text(o).includes(name)) ?? tid('person-picker-create'),
      `ตัวเลือกบุคคล “${name}”`,
    );
    await click(choice, `เลือกบุคคล ${name}`);
    await waitFor(() => tids('face-box').length === before + 1, `กรอบใบหน้าของ ${name} ถูกบันทึก`);
  };

  try {
    // ── S01: คลังว่าง → หน้าเริ่มต้น
    await waitFor(() => location.hash.startsWith('#/welcome'), 'หน้าเริ่มต้นใช้งาน');
    const caps = await rpc('get_capabilities');
    expect(caps.folderPicker === 'native', 'แอปจริงใช้ native folder picker');
    expect(caps.faceAnalysis.available === false, 'ยังไม่มี AI ต้องรายงานตามจริง');
    await see('welcome-add-folder');
    await stage('welcome');

    // ── S02–S03: เลือกโฟลเดอร์ → ตรวจรายการ
    await click(tid('welcome-add-folder'), 'เพิ่มโฟลเดอร์กิจกรรม');
    await see('import-review', 'หน้าตรวจรายการ', 30000);
    const rows = () => tids('import-draft-row');
    await waitFor(() => rows().length === 4, 'สี่แถว: สามงาน + รูปในโฟลเดอร์แม่');
    const titles = rows().map((r) => tid('import-draft-title', r).value);
    expect(titles.includes('งานอบรม AI') && titles.includes('ต้อนรับคณะญาติโยม') && titles.includes('กิจกรรมจุดประทีป'), `ชื่อที่เสนอ: ${titles}`);
    expect(text(tid('import-review')).includes('พ.ย. 2569'), 'วันที่เสนอจากชื่อโฟลเดอร์ 691118 ใต้ 2569');
    expect(text(tid('import-review')).includes('.heic'), 'รายงานไฟล์ที่ยังไม่รองรับ');
    // ไฟล์ RAW นับแยก มีสวิตช์ "รวมไฟล์ RAW" ปิดไว้ก่อนเพราะโฟลเดอร์มี JPEG ด้วย (DECISIONS ข้อ 25)
    expect(tid('import-include-raw')?.getAttribute('aria-checked') === 'false', 'สวิตช์รวมไฟล์ RAW ปิดไว้ก่อน');
    expect(text(tid('import-review')).includes('รวมไฟล์ RAW (1 ไฟล์)'), 'นับไฟล์ RAW แยก');
    expect(tid('import-analyze-faces').disabled, 'สวิตช์วิเคราะห์ใบหน้าปิดใช้งานเมื่อยังไม่มี AI');
    expect((await rpc('list_events')).length === 0, 'ยังไม่มีกิจกรรมถาวรก่อนกดเพิ่ม');
    // แก้ชื่อกิจกรรมหนึ่งงานก่อนเพิ่ม (ไม่เปลี่ยนชื่อโฟลเดอร์จริง)
    const lightsRow = rows().find((r) => tid('import-draft-title', r).value === 'กิจกรรมจุดประทีป');
    await type(tid('import-draft-title', lightsRow), 'จุดประทีป ๒๕๖๙');
    await stage('import-review', { titles });

    // ── เพิ่มกิจกรรม
    await click(tid('import-commit'), 'ยืนยันเพิ่มกิจกรรม');
    await waitFor(() => location.hash.startsWith('#/events'), 'ไปหน้ากิจกรรมหลังเพิ่มหลายงาน');
    await waitFor(() => tids('event-card').length === 4, 'การ์ดกิจกรรมสี่ใบ');
    await jobsIdle();
    const events = await rpc('list_events');
    expect(events.some((e) => e.title === 'จุดประทีป ๒๕๖๙'), 'ชื่อที่แก้ถูกใช้');
    const ai = events.find((e) => e.title === 'งานอบรม AI');
    expect(ai.photoCount === 6 && ai.thumbReadyCount === 5 && ai.failedCount === 1, `งานอบรม AI: ${JSON.stringify(ai)}`);
    expect(ai.eventDate === '2026-11-18', 'วันกิจกรรมจากชื่อโฟลเดอร์');
    await waitFor(() => all('[data-testid="event-card"] img').some((i) => i.naturalWidth > 0), 'ภาพปกโหลดผ่าน puri://');
    await stage('events', { events: events.map((e) => [e.title, e.photoCount]) });

    // ── S13: แสดงรูปของกิจกรรม
    await click(tids('event-card').find((c) => c.dataset.eventId === ai.id), 'เปิดงานอบรม AI');
    await waitFor(() => location.hash.startsWith(`#/events/${ai.id}`), 'หน้ารายละเอียดกิจกรรม');
    await waitFor(() => tids('photo-card').length === 6, 'รูปหกใบใน grid');
    await waitFor(() => all('[data-testid="photo-card"] img').filter((i) => i.naturalWidth > 0).length >= 5, 'ภาพย่อโหลดจริง');
    await stage('event-detail');

    // ── S15: ตัวดูภาพ + ระบุใบหน้าเอง
    await click(tid('photo-card-open', photoCard('IMG_0248.jpg')), 'เปิดรูป IMG_0248.jpg');
    await see('photo-viewer');
    await waitFor(() => all('[data-testid="viewer-image-box"] img').some((i) => i.naturalWidth === 1200), 'ต้นฉบับ 1200px โหลดผ่าน puri://photo');
    await labelFace('คุณกานต์', [0.2, 0.22, 0.4, 0.58]);
    await labelFace('คุณเมย์', [0.52, 0.24, 0.72, 0.6]);
    expect(tids('face-row').length === 2, 'สองคนในรูป');
    await stage('viewer-two-faces');

    // รูปถัดไปตามลำดับของ grid แล้วเลือก "คนเดิม" จากรายชื่อ
    await click(tid('viewer-next'), 'รูปถัดไป');
    await waitFor(() => tids('face-box').length === 0, 'รูปถัดไปยังไม่มีใบหน้า');
    await labelFace('คุณกานต์', [0.4, 0.22, 0.6, 0.58]);
    let people = await rpc('list_people', {});
    expect(people.length === 2, `ต้องมีสองคน ไม่สร้างคนชื่อซ้ำ: ${people.map((p) => p.displayName)}`);
    const kan = people.find((p) => p.displayName === 'คุณกานต์');
    const may = people.find((p) => p.displayName === 'คุณเมย์');
    expect(kan.confirmedPhotoCount === 2 && may.confirmedPhotoCount === 1, 'จำนวนรูปยืนยันของแต่ละคน');

    // เปิดต้นฉบับ / แสดงใน Finder: คำสั่งจริงของแอป (ตัวรันตั้ง PURI_DISABLE_OPEN ไม่ให้หน้าต่างอื่นเด้ง)
    await click(tid('viewer-open-original'), 'เปิดต้นฉบับ');
    await click(tid('viewer-reveal'), 'เปิดใน Finder');
    await sleep(300);
    await stage('viewer-labelled', { people: people.map((p) => [p.displayName, p.confirmedPhotoCount]) });

    // ปิดตัวดูภาพ: กลับหน้ากิจกรรมเดิม
    await click(tid('viewer-close'), 'ปิดตัวดูภาพ');
    await waitFor(() => !tid('photo-viewer') && location.hash.startsWith(`#/events/${ai.id}`), 'กลับหน้ากิจกรรมเดิม');

    // ── S08: บุคคล — ปักหมุด + ดูเร็ว + ปรากฏร่วมในภาพ
    await click(tid('nav-people'), 'เมนูบุคคล');
    await waitFor(() => tids('person-card').length === 2, 'การ์ดบุคคลสองใบ');
    const kanCard = () => tids('person-card').find((c) => c.dataset.personId === kan.id);
    // คนที่ปักหมุดขึ้นก่อนเสมอ: คุณเมย์ (ตามชื่ออยู่หลังคุณกานต์) ปักแล้วขึ้นมาเป็นใบแรก เอาออกแล้วกลับที่เดิม
    const firstCard = () => tids('person-card')[0]?.dataset.personId;
    expect(firstCard() === kan.id, 'เรียงตามชื่อ: คุณกานต์มาก่อน');
    const mayCard = () => tids('person-card').find((c) => c.dataset.personId !== kan.id);
    const mayId = mayCard().dataset.personId;
    await click(tid('person-pin', mayCard()), 'ปักหมุดคุณเมย์');
    await waitFor(() => firstCard() === mayId, 'คนที่ปักหมุดขึ้นเป็นใบแรก');
    await stage('people-pinned-first');
    await click(tid('person-pin', mayCard()), 'เอาหมุดคุณเมย์ออก');
    await waitFor(() => firstCard() === kan.id, 'เอาหมุดออกแล้วกลับไปเรียงตามชื่อ');
    await click(tid('person-pin', kanCard()), 'ปักหมุดคุณกานต์');
    await waitFor(() => tid('person-pin', kanCard()).getAttribute('aria-pressed') === 'true', 'สถานะปักหมุด');
    expect(location.hash.startsWith('#/people') && !location.hash.includes(kan.id), 'ปักหมุดไม่เปิดการ์ด');
    await click(tid('person-quickview', kanCard()), 'ดูเร็ว');
    const panel = await see('person-quickview-panel');
    expect(location.hash.startsWith('#/people') && !location.hash.includes(kan.id), 'ดูเร็วไม่เปลี่ยนหน้า');
    const co = await waitFor(() => tid('coappearance-item', panel), 'รายการปรากฏร่วมในภาพ');
    expect(text(co).includes('คุณเมย์'), 'คุณเมย์ปรากฏร่วมกับคุณกานต์');
    await waitFor(() => all('img', panel).some((i) => i.naturalWidth > 0), 'ภาพครอปใบหน้า/ภาพกิจกรรมในแผงโหลดจริง');
    await stage('people-quickview');

    // ── S06: กดคนร่วม → ค้นแบบ "มีทุกคนในภาพเดียวกัน"
    await click(co, 'เลือกคนร่วม');
    await waitFor(() => location.hash.startsWith('#/search'), 'หน้าค้นหา');
    await waitFor(() => tids('search-person-chip').length === 2, 'chip บุคคลสองคน');
    await waitFor(() => tids('photo-card').length === 1 && photoCard('IMG_0248.jpg'), 'AND: เฉพาะรูปที่มีทั้งสองคน');
    await stage('search-all');
    await click(tid('search-person-mode-any'), 'มีคนใดคนหนึ่ง');
    await waitFor(() => tids('photo-card').length === 2, 'OR: สองรูป ไม่ซ้ำ');
    await stage('search-any');

    // ── S18: เลือกทั้งสองรูป → ส่งออกสำเนา
    for (const card of tids('photo-card')) await click(tid('photo-card-select', card), 'เลือกรูป');
    await waitFor(() => text(tid('selection-count')).includes('2'), 'แถบเลือกบอกสองรูป');
    await click(tid('export-button'), 'ส่งออกสำเนา');
    await click(await see('export-pick-destination'), 'เลือกปลายทาง');
    const plan = await see('export-plan');
    expect(text(plan).includes('__EXPORT__'), 'แผนแสดงปลายทางที่เลือก');
    await stage('export-plan');
    await click(tid('export-confirm'), 'เริ่มส่งออก');
    const done = await see('export-report', 'รายงานส่งออก', 30000);
    await stage('export-report', { report: text(done).slice(0, 160) });
    await click(tid('export-done'), 'ปิดรายงาน');

    // ── ค้นด้วยชื่อไฟล์ข้ามกิจกรรม → เลือกผลค้นหาทั้งหมด → ส่งออกซ้ำที่เดิม (ชื่อชนต้องไม่ถูกเขียนทับ)
    location.hash = '#/search?q=IMG_0248';
    // รอผลของคำค้นใหม่จริง ๆ (ไม่ใช่การ์ดของผลก่อนหน้า) และรอให้รายการที่เลือกไว้ถูกล้างเพราะตัวกรองเปลี่ยน
    const labels = () => tids('photo-card').map((c) => tid('photo-card-open', c).getAttribute('aria-label'));
    await waitFor(() => labels().length === 2 && labels().every((l) => l.includes('IMG_0248.jpg')) && new Set(labels()).size === 2, 'IMG_0248.jpg มีในสองกิจกรรม');
    await waitFor(() => !tid('selection-bar'), 'รายการที่เลือกถูกล้างเมื่อตัวกรองเปลี่ยน');
    await click(tid('photo-card-select', tids('photo-card')[0]), 'เลือกรูปแรก');
    await click(await see('select-all-results'), 'เลือกผลค้นหาทั้งหมด');
    await waitFor(() => text(tid('selection-count')).includes('2'), 'เลือกผลทั้งหมดสองรูป');
    await click(tid('export-button'), 'ส่งออกสำเนา');
    await click(await see('export-pick-destination'), 'เลือกปลายทาง');
    await waitFor(() => tid('export-plan'), 'แผนส่งออกรอบสอง');
    await click(tid('export-confirm'), 'เริ่มส่งออกรอบสอง');
    await see('export-report', 'รายงานส่งออกรอบสอง', 30000);
    const jobs = await rpc('list_jobs', {});
    const exports = jobs.filter((j) => j.kind === 'export');
    expect(exports.length === 2 && exports.every((j) => j.state === 'completed'), `งานส่งออก: ${exports.map((j) => j.state)}`);
    const second = await rpc('get_export_report', { jobId: exports[0].id });
    expect(second.copied === 2 && second.items.some((i) => / \(\d+\)\.jpg$/.test(i.outputPath)), 'ชื่อชนถูกเติมเลขท้าย ไม่เขียนทับ');
    await stage('export-collision', { outputs: second.items.map((i) => i.outputPath.split('/').pop()) });
    await click(tid('export-done'), 'ปิดรายงาน');

    // ── S15b: แต่งภาพ (หลังส่งออก — ตัวรันตรวจว่าสำเนาที่ส่งออกตรงกับต้นฉบับทุก byte)
    // canvas ของหน้าแต่งภาพต้องอ่านพิกเซลของภาพจาก puri:// ได้ (ถ้าไม่มี CORS ภาพจะ "taint" และ getImageData โยน SecurityError)
    if (tid('selection-clear')) await click(tid('selection-clear'), 'ล้างที่เลือก');
    // รูปชื่อนี้มีสองกิจกรรม — เปิดรูปที่ระบุใบหน้าไว้ (ขั้นปรับเฉพาะส่วนใช้ใบหน้าของรูป)
    const labelled = [];
    for (const card of tids('photo-card')) {
      if ((await rpc('get_photo_details', { photoId: card.dataset.photoId })).faces.length === 2) labelled.push(card);
    }
    expect(labelled.length === 1, `รูปที่มีสองใบหน้าในผลค้นหา: ${labelled.length}`);
    await click(tid('photo-card-open', labelled[0]), 'เปิดรูปที่ระบุใบหน้าไว้');
    await see('photo-viewer');
    const editedId = await waitFor(() => location.hash.match(/photos\/([^/?]+)/)?.[1], 'รหัสรูปใน URL ของตัวดูภาพ');
    await click(await see('viewer-edit'), 'แต่งภาพ');
    await see('photo-editor');
    const canvasMean = () => {
      const c = tid('editor-preview');
      if (!c || c.width === 0) return 0;
      const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
      let sum = 0;
      for (let i = 0; i < d.length; i += 16) sum += d[i] + d[i + 1] + d[i + 2];
      return sum / (d.length / 16) / 3;
    };
    await click(tid('editor-tab-light'), 'แท็บแสงและสี');
    const dark = await waitFor(() => canvasMean() || null, 'ตัวอย่างภาพบน canvas (อ่านพิกเซลได้)', 30000);
    await type(tid('editor-exposure'), '1');
    const bright = await waitFor(() => (canvasMean() > dark + 10 ? canvasMean() : null), 'ตัวอย่างสว่างขึ้นเมื่อเพิ่มค่าแสง');
    await stage('editor-light', { mean: [Math.round(dark), Math.round(bright)] });

    // นำเข้า preset .xmp ผ่าน picker ของแอป (คำตอบที่ 4 ของ PURI_E2E_PICK) แล้วใช้กับรูปนี้
    await click(tid('editor-tab-presets'), 'แท็บ Preset');
    await click(await see('preset-import'), 'นำเข้า .xmp');
    const imported = await see('preset-imported', 'หน้าต่างผลการนำเข้า');
    // Texture นำเข้าได้แล้ว (DECISIONS ข้อ 27) · Profile ยังทำตามไม่ได้
    expect(text(imported).includes('พื้นผิว +15') && text(imported).includes('Adobe Vivid'), `บอกส่วนที่นำเข้าได้/ไม่ได้ (ได้ "${text(imported)}")`);
    await click(tid('preset-imported-apply'), 'ใช้กับรูปนี้');
    await waitFor(() => tids('preset-item').some((b) => b.getAttribute('aria-pressed') === 'true'), 'preset ที่นำเข้าถูกใช้อยู่');
    // ปรับเพิ่มแล้วบันทึกเป็น preset ของตัวเอง
    await click(tid('editor-tab-light'), 'แท็บแสงและสี');
    // preset ที่ใช้อยู่มีค่า HSL ส่วนนี้จึงเปิดเองแล้ว (เปิดเฉพาะเมื่อยังพับอยู่)
    if (tid('editor-hsl-toggle').getAttribute('aria-expanded') !== 'true') await click(tid('editor-hsl-toggle'), 'เปิดส่วน HSL');
    expect(tid('editor-hsl-toggle').getAttribute('aria-expanded') === 'true', 'ส่วน HSL เปิดเองเมื่อมีค่าที่ปรับไว้');
    await type(await see('editor-hsl-lum-1'), '45');
    await click(tid('editor-tab-presets'), 'แท็บ Preset');
    await click(await see('preset-save'), 'บันทึกค่าปัจจุบัน');
    await type(await see('preset-name-input'), 'ผิวสว่าง e2e');
    await click(tid('preset-name-confirm'), 'บันทึก preset');
    await waitFor(() => tids('preset-item').length === 2, 'preset สองอัน');
    await stage('editor-presets', { presets: tids('preset-item').map((b) => text(b)) });

    // ปรับเฉพาะส่วน (DECISIONS ข้อ 28): บริเวณใบหน้าจากกรอบที่ระบุไว้ → เปลี่ยนเฉพาะผิวหน้า ฉากหลังเท่าเดิม
    // (ผิวของรูปนี้สว่างเกือบขาวแล้วจาก preset ข้างบน "ผิวขาวใส" จึงสว่างขึ้นได้นิดเดียว — ใช้ค่าแสง −1 ของบริเวณให้เห็นผลชัด)
    const canvasAt = (fx, fy) => {
      const c = tid('editor-preview');
      const d = c.getContext('2d').getImageData(Math.round(c.width * fx) - 2, Math.round(c.height * fy) - 2, 5, 5).data;
      const sum = [0, 0, 0];
      for (let i = 0; i < d.length; i += 4) for (let k = 0; k < 3; k++) sum[k] += d[i + k];
      return sum.map((v) => v / (d.length / 4));
    };
    await click(tid('editor-tab-masks'), 'แท็บเฉพาะส่วน');
    const addFaces = await see('mask-add-faces');
    expect(text(addFaces).includes('2 คน'), `ปุ่มบอกจำนวนใบหน้า (ได้ "${text(addFaces)}")`);
    // แก้มของใบหน้าซ้าย (ใต้ตา) และฉากหลังมุมซ้ายบน
    const [skin0, back0] = [canvasAt(0.3, 0.46), canvasAt(0.06, 0.08)];
    await click(addFaces, 'เพิ่มบริเวณใบหน้าและผิว');
    await see('mask-editor', 'แผงปรับบริเวณที่เลือก');
    await sleep(1600); // ให้สีแดงที่แสดงบริเวณชั่วครู่หายไปก่อน
    await click(tids('mask-look').find((b) => text(b) === 'ผิวขาวใส'), 'จุดเริ่ม “ผิวขาวใส”');
    await type(await see('mask-exposure'), '-1');
    let last = skin0;
    const skin1 = await waitFor(
      () => {
        last = canvasAt(0.3, 0.46);
        return last[1] < skin0[1] - 20 ? last : null;
      },
      { toString: () => `ผิวหน้าในตัวอย่างมืดลงตามค่าแสงของบริเวณ (ก่อน ${skin0.map(Math.round)} · ล่าสุด ${last.map(Math.round)})` },
    );
    const back1 = canvasAt(0.06, 0.08);
    expect(back1.every((v, k) => Math.abs(v - back0[k]) <= 1), `ฉากหลังเท่าเดิม ${back0} → ${back1}`);
    await stage('editor-masks', { skin: [skin0, skin1].map((s) => s.map(Math.round)) });

    // รีทัชใบหน้า (DECISIONS ข้อ 31): หน้าผอม "มาก" ดัดเฉพาะขอบหน้า — ภาพรอบขอบหน้าเปลี่ยน กลางหน้าเท่าเดิม
    const snap = () => {
      const c = tid('editor-preview');
      return c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
    };
    const flat0 = snap();
    const centre0 = canvasAt(0.3, 0.42);
    await click(tids('retouch-thin-level').find((b) => text(b) === 'มาก'), 'หน้าผอม มาก');
    await waitFor(() => {
      const d = snap();
      for (let i = 0; i < d.length; i += 4) if (Math.abs(d[i] - flat0[i]) > 8) return true;
      return null;
    }, 'ขอบหน้าในตัวอย่างขยับ');
    const centre1 = canvasAt(0.3, 0.42);
    expect(centre1.every((v, k) => Math.abs(v - centre0[k]) <= 3), `กลางหน้าไม่ขยับ ${centre0.map(Math.round)} → ${centre1.map(Math.round)}`);
    await stage('editor-retouch');

    await click(tid('editor-save'), 'บันทึกการแต่ง');
    await waitFor(() => !tid('photo-editor'), 'ปิดหน้าแต่งภาพหลังบันทึก');
    const edit = (await rpc('get_photo_details', { photoId: editedId })).edit;
    expect(edit?.settings.adjust.exposure === 0.4 && edit.settings.adjust.lum[1] === 45, `ค่าที่บันทึก: ${JSON.stringify(edit?.settings.adjust)}`);
    const mask = edit?.settings.masks?.[0];
    expect(
      edit.settings.masks.length === 1 && mask.area.boxes.length === 2 && mask.adjust.texture === -35 && mask.adjust.exposure === -1,
      `Mask ที่บันทึก: ${JSON.stringify(mask)}`,
    );
    const retouch = edit.settings.retouch;
    expect(retouch?.thin === 80 && retouch.faces.length === 2, `รีทัชที่บันทึก: ${JSON.stringify(retouch)}`);
    await waitFor(() => all('[data-testid="viewer-image-box"] img').some((i) => i.naturalWidth > 0), 'ภาพที่แต่งแล้วโหลดในตัวดูภาพ');
    await click(tid('viewer-close'), 'ปิดตัวดูภาพ');
    await waitFor(() => !tid('photo-viewer'), 'ปิดตัวดูภาพแล้ว');

    // ใช้ preset ของตัวเองกับทุกรูปในผลค้นหาจากแถบเลือกรูป
    for (const card of tids('photo-card')) await click(tid('photo-card-select', card), 'เลือกรูป');
    await click(await see('apply-preset-button'), 'ใช้ preset');
    await see('apply-preset');
    await click(tids('apply-preset-option').find((o) => text(o).includes('ผิวสว่าง e2e')), 'เลือก preset ของตัวเอง');
    await click(tid('apply-preset-confirm'), 'ใช้กับรูปที่เลือก');
    await waitFor(() => !tid('apply-preset'), 'ปิดหน้าต่างหลังใช้ preset');
    const ids = (await rpc('query_photos', { text: 'IMG_0248', limit: 50 })).items.map((i) => i.id);
    expect(ids.length === 2, `ผลค้นหา IMG_0248 สองรูป: ${ids.length}`);
    for (const id of ids) {
      const a = (await rpc('get_photo_details', { photoId: id })).edit?.settings.adjust;
      expect(a?.lum[1] === 45 && a.exposure === 0.4, `รูป ${id} ได้ค่าของ preset: ${JSON.stringify(a)}`);
    }
    const kept = (await rpc('get_photo_details', { photoId: editedId })).edit.settings.masks;
    expect(kept?.length === 1, 'ใช้ preset (แสงและสี) แล้วการปรับเฉพาะส่วนของรูปยังอยู่');
    await stage('apply-preset-many', { photos: ids.length });
    await click(tid('selection-clear'), 'ล้างที่เลือก');

    // ── แต่งทีละรูป (DECISIONS ข้อ 32): กิจกรรม → เฉพาะรูปที่ยังไม่แต่ง → แต่งทีละรูป → บันทึก แล้วไปรูปถัดไป / ข้าม
    location.hash = `#/events/${ai.id}?ed=unedited`;
    await waitFor(() => tids('photo-card').length === 5 && !photoCard('IMG_0248.jpg'), 'เฉพาะห้ารูปที่ยังไม่แต่ง (IMG_0248 แต่งแล้ว)');
    const progress = await see('edit-progress');
    expect(text(progress).includes('แต่งแล้ว 1 จาก 6'), `ความคืบหน้าของงาน (ได้ "${text(progress)}")`);
    const [savedId, skippedId] = ['IMG_0249.jpg', 'IMG_0250.jpg'].map((f) => photoCard(f).dataset.photoId);
    await click(await see('edit-in-order'), 'แต่งทีละรูป');
    const editing = () => {
      const name = tid('photo-editor')?.querySelector('h2 + p');
      return name ? text(name) : null;
    };
    await waitFor(() => editing() === 'IMG_0249.jpg', 'หน้าแต่งภาพเปิดรูปแรกที่ยังไม่แต่งเอง');
    expect(text(tid('editor-position')).includes('อีก 4 รูป'), `บอกจำนวนที่เหลือ (ได้ "${text(tid('editor-position'))}")`);
    await click(tid('editor-rotate-right'), 'หมุนขวา');
    await click(tid('editor-save-next'), 'บันทึก แล้วไปรูปถัดไป');
    await waitFor(() => editing() === 'IMG_0250.jpg', 'หน้าแต่งภาพเปิดรูปถัดไปต่อเอง');
    expect((await rpc('get_photo_details', { photoId: savedId })).edit?.settings.rotate === 90, 'รูปแรกถูกบันทึกก่อนไปต่อ');
    await click(tid('editor-skip'), 'ข้าม');
    await waitFor(() => editing() === 'DSC_0101.jpg', 'ข้ามไปรูปถัดไป');
    expect((await rpc('get_photo_details', { photoId: skippedId })).edit === null, 'รูปที่ข้ามไม่ถูกแต่ง');
    await stage('edit-in-order');
    await click(tid('editor-cancel'), 'ปิดหน้าแต่งภาพ');
    await click(await see('viewer-close'), 'ปิดตัวดูภาพ');
    await waitFor(() => tids('photo-card').length === 4 && !photoCard('IMG_0249.jpg'), 'รูปที่แต่งแล้วหลุดจากรายการที่ยังไม่แต่ง');
    await waitFor(() => tid('edit-progress') && text(tid('edit-progress')).includes('แต่งแล้ว 2 จาก 6'), 'ความคืบหน้าเพิ่มขึ้น');

    // ── คัดก่อนแต่ง (DECISIONS ข้อ 33): ในตัวดูภาพกด P = เลือก, X = ไม่เอา แล้วไปรูปถัดไปเอง (ใช้ event.code จึงกดได้แม้แป้นเป็นภาษาไทย)
    const press = (code, key) => window.dispatchEvent(new KeyboardEvent('keydown', { code, key, bubbles: true, cancelable: true }));
    const [pickedId, rejectedId] = ['IMG_0250.jpg', 'DSC_0101.jpg'].map((f) => photoCard(f).dataset.photoId);
    await click(tid('photo-card-open', photoCard('IMG_0250.jpg')), 'เปิด IMG_0250 ในตัวดูภาพ');
    await waitFor(() => location.hash.includes(pickedId) && tid('viewer-flag-pick') && !tid('viewer-flag-pick').disabled, 'ตัวดูภาพพร้อมคัด');
    press('KeyP', 'ย'); // แป้นภาษาไทย: ตัว P คือ "ย"
    await waitFor(() => location.hash.includes(rejectedId), 'กด P แล้วไปรูปถัดไปเอง');
    await waitFor(() => tid('viewer-flag-reject') && !tid('viewer-flag-reject').disabled, 'รูปถัดไปพร้อมคัด');
    press('KeyX', 'x');
    await waitFor(async () => (await rpc('get_photo_details', { photoId: rejectedId })).flag === 'reject', 'กด X แล้วบันทึกว่าไม่เอา');
    expect((await rpc('get_photo_details', { photoId: pickedId })).flag === 'pick', 'กด P แล้วบันทึกว่าเลือก');
    await click(tid('viewer-close'), 'ปิดตัวดูภาพ');
    location.hash = `#/events/${ai.id}?fl=picked`;
    await waitFor(() => tids('photo-card').length === 1 && photoCard('IMG_0250.jpg'), 'กรองเฉพาะที่เลือกไว้');
    expect(tid('photo-card-flag', photoCard('IMG_0250.jpg'))?.dataset.flag === 'pick', 'ป้าย "เลือก" บนภาพย่อ');
    const flags = await see('flag-summary');
    expect(text(flags).includes('เลือกไว้ 1') && text(flags).includes('ไม่เอา 1'), `สรุปการคัด (ได้ "${text(flags)}")`);
    // เลือกหลายรูปแล้วล้างการคัดจากเมนูในแถบเลือก
    await click(tid('photo-card-select', photoCard('IMG_0250.jpg')), 'เลือกรูป');
    await click(await see('flag-menu-button'), 'เมนูคัดรูป');
    await click(await see('flag-menu-clear'), 'ล้างการคัด');
    await waitFor(() => tids('photo-card').length === 0, 'ล้างแล้วหลุดจากรายการที่เลือกไว้');
    expect((await rpc('get_photo_details', { photoId: pickedId })).flag === null, 'ล้างการคัดแล้ว');
    await stage('pick-reject');

    // ── S05: หน้าหลักใช้ข้อมูลจริงจากคลัง
    await click(tid('nav-home'), 'เมนูหน้าหลัก');
    await waitFor(() => tids('person-card', tid('home-people-grid') ?? document).some((c) => c.dataset.personId === kan.id), 'คนที่ปักหมุดอยู่บนหน้าหลัก');
    const summary = await rpc('get_library_summary');
    expect(summary.eventCount === 4 && summary.personCount === 2, `สรุปคลัง: ${JSON.stringify(summary)}`);
    expect(text(tid('library-summary')).includes(String(summary.photoCount)), 'จำนวนรูปบนหน้าหลักมาจากคลัง');
    await stage('home', { summary });

    expect(problems.length === 0, `มีปัญหาระหว่างรัน: ${problems.join(' | ')}`);
    await report('done', { summary });
  } catch (error) {
    await report('FAILED', { error: String(error?.message ?? error), hash: location.hash, at: log.at(-1)?.stage ?? 'เริ่มต้น' });
  }
})();
