// ทดสอบเส้นทางหลักในหน้าต่างจริงของ Puri Photo Search (debug build ฉีดไฟล์นี้หลังหน้าโหลดเสร็จ — ดู e2e/run.sh)
// ทุกขั้นขับผ่าน DOM จริง → IPC จริง → Rust จริง → คลัง SQLite และไฟล์รูปจริงในโฟลเดอร์ชั่วคราว
//   เลือกโฟลเดอร์ → ตรวจรายการ → เพิ่มกิจกรรม → แสดงรูป → ตั้งชื่อบุคคล → ค้นหารูป → เปิดต้นฉบับ → ส่งออกสำเนา
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
    expect(text(tid('import-review')).includes('.arw'), 'รายงานไฟล์ที่ยังไม่รองรับ');
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
