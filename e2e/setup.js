// ทดสอบหน้าต่าง "เตรียมแอปให้พร้อมใช้" ในหน้าต่างจริง (debug build ฉีดไฟล์นี้ทุกครั้งที่หน้าโหลด — ดู e2e/setup.sh)
// คลังว่าง + ยังไม่มีตัววิเคราะห์ใบหน้า + พบฐานของรุ่น Python ที่มี 3 คน:
//   เปิดแอป → หน้าต่างเปิดเอง → ข้ามไปก่อน → หน้าบุคคลยังเสนอให้นำเข้า → นำเข้า → โหลดหน้าใหม่แล้วไม่เปิดเองอีก
(async () => {
  if (window.__e2eStarted) return;
  window.__e2eStarted = true;

  const invoke = window.__TAURI_INTERNALS__.invoke;
  const rpc = (cmd, args = {}) => invoke(cmd, { args });
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  // รอบสองคือหน้าที่โหลดใหม่: บันทึกของรอบแรกเก็บไว้ใน sessionStorage
  const log = JSON.parse(sessionStorage.getItem('e2e-log') ?? '[]');
  const problems = [];
  addEventListener('securitypolicyviolation', (e) => problems.push(`csp: ${e.violatedDirective} blocked ${e.blockedURI || 'inline'}`));
  addEventListener('error', (e) => problems.push(`error: ${e.message}`));
  addEventListener('unhandledrejection', (e) => problems.push(`rejection: ${e.reason?.message ?? e.reason}`));
  const report = (stage, extra = {}) => invoke('e2e_report', { text: JSON.stringify({ stage, ...extra, problems, log }, null, 1) });
  const stage = async (name, data = {}) => {
    log.push({ stage: name, ...data });
    sessionStorage.setItem('e2e-log', JSON.stringify(log));
    await report(name);
    await sleep(2200); // พอให้ตัวรันถ่ายภาพหน้าต่างของขั้นนี้ก่อนหน้าจอเปลี่ยน
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
  const tid = (id, root = document) => root.querySelector(`[data-testid="${id}"]`);
  const tids = (id) => [...document.querySelectorAll(`[data-testid="${id}"]`)];
  const text = (el) => el.textContent.replace(/\s+/g, ' ').trim();
  const see = (id) => waitFor(() => tid(id), id);
  const gone = (id) => waitFor(() => !tid(id), `${id} หายไป`);
  const expect = (cond, message) => {
    if (!cond) throw new Error(`ไม่ตรงที่คาด: ${message}`);
  };

  try {
    if (log.length > 0) {
      // ── รอบสอง: ปิดไปแล้ว ต้องไม่เปิดเองอีก แม้ยังไม่ได้ดาวน์โหลดตัววิเคราะห์ใบหน้า
      await waitFor(async () => (await rpc('get_capabilities')).faceAnalysis.state === 'not_installed', 'ยังไม่ได้ติดตั้งตัววิเคราะห์ใบหน้า');
      await see('nav-people');
      await sleep(2500);
      expect(!tid('setup-dialog'), 'หน้าต่างเตรียมแอปไม่เปิดเองอีกหลังปิดไปแล้ว');
      await stage('reloaded');
      expect(problems.length === 0, `มีปัญหาระหว่างรัน: ${problems.join(' | ')}`);
      await report('done');
      return;
    }

    // ── เปิดแอปครั้งแรก: หน้าต่างเปิดเอง เสนอทั้งสองอย่าง
    const dialog = await see('setup-dialog');
    const install = await waitFor(() => tid('face-model-install', dialog), 'ปุ่มดาวน์โหลดตัววิเคราะห์ใบหน้าในหน้าต่าง');
    expect(/\d+ MB/.test(text(install)), `ปุ่มดาวน์โหลดบอกขนาด (ได้ "${text(install)}")`);
    expect(text(await see('setup-legacy-import')) === 'นำเข้า 3 คน', `ปุ่มนำเข้าบอกจำนวนคน (ได้ "${text(tid('setup-legacy-import'))}")`);
    expect(text(tid('setup-close')) === 'ข้ามไปก่อน', 'ยังมีสิ่งที่ไม่ได้ทำ: ปุ่มปิดคือ "ข้ามไปก่อน"');
    expect((await rpc('get_setup_state')).dismissed === false, 'ยังไม่ถูกจำว่าปิดแล้ว');
    await stage('offered');

    // ── ข้ามไปก่อน: จำไว้กับคลัง
    tid('setup-close').click();
    await gone('setup-dialog');
    await waitFor(async () => (await rpc('get_setup_state')).dismissed === true, 'จำว่าปิดหน้าต่างแล้ว');
    await stage('skipped');

    // ── หน้าบุคคลที่ยังว่าง ยังเสนอให้นำเข้า และเปิดหน้าต่างเดิมได้
    location.hash = '#/people';
    const offer = await see('people-legacy');
    expect(text(offer).includes('3 คน'), `ปุ่มในหน้าบุคคลบอกจำนวนคน (ได้ "${text(offer)}")`);
    await stage('people-empty');
    offer.click();
    await see('setup-dialog');
    (await see('setup-legacy-import')).click();
    const done = await see('setup-legacy-done');
    expect(text(done).includes('นำรายชื่อเข้าแล้ว 3 คน'), `ผลการนำเข้า (ได้ "${text(done)}")`);
    expect(tid('face-model-install') && text(tid('setup-close')) === 'ข้ามไปก่อน', 'ยังเสนอดาวน์โหลดตัววิเคราะห์ใบหน้าต่อ');
    expect((await rpc('get_setup_state')).legacyPending === 0, 'ไม่เหลือรายชื่อให้นำเข้า');
    await stage('imported');

    // ── เปิดหน้าบุคคลจากหน้าต่าง: รายชื่อมาครบ และไม่เสนอให้นำเข้าอีก
    tid('setup-open-people').click();
    await gone('setup-dialog');
    await waitFor(() => tids('person-card').length === 3, 'หน้าบุคคลมี 3 คน');
    expect(!tid('people-legacy'), 'นำเข้าครบแล้ว ไม่เสนอซ้ำ');
    const names = tids('person-card-open').map((el) => el.getAttribute('aria-label') ?? text(el));
    await stage('people', { names });

    expect(problems.length === 0, `มีปัญหาระหว่างรัน: ${problems.join(' | ')}`);
    location.reload();
  } catch (error) {
    await report('FAILED', { error: String(error?.message ?? error), hash: location.hash, at: log.at(-1)?.stage ?? 'เริ่มต้น' });
  }
})();
