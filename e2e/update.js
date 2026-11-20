// ทดสอบการอัปเดตตัวเองในหน้าต่างจริง (debug build ฉีดไฟล์นี้ทุกครั้งที่หน้าโหลด — ดู e2e/update.sh)
// รอบแรกแอปเป็นรุ่นเก่า: ไฟล์ที่ถูกแก้ต้องถูกปฏิเสธ → ไฟล์จริงดาวน์โหลดเอง → กดอัปเดต → แอปปิดและเปิดใหม่
// รอบสองไฟล์เดียวกันนี้ถูกฉีดเข้าแอปที่เปิดใหม่ ซึ่งต้องเป็นรุ่นใหม่แล้ว
(async () => {
  if (window.__e2eStarted) return;
  window.__e2eStarted = true;

  const OLD = '__OLD__';
  const NEW = '__NEW__';
  const NOTES = '__NOTES__';
  // ไม่ใช่ LIVE: ระหว่างที่แอปถือรุ่น NEW ไว้รอผู้ใช้กด ตัวรันออกรุ่น NEWER — แอปต้องเปลี่ยนไปเสนอรุ่นล่าสุด (TARGET)
  const NEWER = '__NEWER__';
  const NEWER_NOTES = '__NEWER_NOTES__';
  const TARGET = '__TARGET__';
  // โหมดตรวจรุ่นที่เผยแพร่แล้ว: manifest และไฟล์มาจาก GitHub จริง จึงไม่มีขั้นไฟล์ที่ถูกแก้ และข้อความมีอะไรใหม่เป็นของรุ่นนั้น
  const LIVE = '__LIVE__' === '1';
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const log = [];
  const report = (stage, extra = {}) => invoke('e2e_report', { text: JSON.stringify({ stage, ...extra, log }, null, 1) });
  const stage = async (name, data = {}) => {
    log.push({ stage: name, ...data });
    await report(name);
    await sleep(900);
  };
  const waitFor = async (fn, what, timeout = 20000) => {
    const end = Date.now() + timeout;
    for (;;) {
      const v = await fn();
      if (v) return v;
      if (Date.now() > end) throw new Error(`รอไม่สำเร็จ: ${what}`);
      await sleep(100);
    }
  };
  const tid = (id) => document.querySelector(`[data-testid="${id}"]`);
  const text = (el) => el.textContent.replace(/\s+/g, ' ').trim();
  const see = (id, timeout) => waitFor(() => tid(id), id, timeout);
  const expect = (cond, message) => {
    if (!cond) throw new Error(`ไม่ตรงที่คาด: ${message}`);
  };
  const status = () => invoke('update_status');

  try {
    const first = await status();
    location.hash = '#/settings';
    const section = await see('settings-update');

    if (first.currentVersion === TARGET) {
      // รอบสอง: แอปที่เปิดขึ้นมาเองหลังติดตั้ง
      await waitFor(() => text(tid('update-current')).includes(TARGET), 'หน้าตั้งค่าแสดงรุ่นใหม่');
      await waitFor(async () => (await status()).state === 'up_to_date', 'ตรวจเองแล้วพบว่าเป็นรุ่นล่าสุด', 30000);
      expect(!tid('update-notice'), 'ไม่มีปุ่มอัปเดตค้างหลังอัปเดตแล้ว');
      await report('done', { version: first.currentVersion });
      return;
    }

    expect(first.currentVersion === OLD, `เริ่มที่รุ่น ${OLD} (ได้ ${first.currentVersion})`);
    expect(first.state !== 'unsupported', 'ตัวอัปเดตเปิดใช้เมื่อเทสต์ตั้ง PURI_UPDATE_ENDPOINT');
    await stage('opened', { version: first.currentVersion });

    // 1) แอปตรวจเองหลังเปิด: ไฟล์ที่ไม่ตรงกับลายเซ็นต้องไม่ถูกเสนอให้ติดตั้ง
    if (!LIVE) {
      await waitFor(async () => (await status()).state === 'failed', 'ไฟล์ที่ถูกแก้ถูกปฏิเสธ', 60000);
      await waitFor(() => text(section).includes('ลายเซ็น'), 'หน้าตั้งค่าบอกว่าไม่ผ่านการตรวจลายเซ็น');
      expect(!tid('update-notice') && !tid('update-open'), 'ไม่มีปุ่มติดตั้งสำหรับไฟล์ที่ไม่ผ่านลายเซ็น');
      await stage('rejected', { message: (await status()).message });
    }

    // 2) ตัวรันวางไฟล์จริงกลับ: กด "ลองตรวจใหม่" จนดาวน์โหลดผ่าน
    await waitFor(
      async () => {
        const now = await status();
        if (now.state === 'ready') return true;
        const button = tid('update-check');
        if (now.state === 'failed' && button && !button.disabled) button.click();
        await sleep(900);
        return false;
      },
      'ดาวน์โหลดรุ่นใหม่จนพร้อมติดตั้ง',
      LIVE ? 300000 : 90000,
    );
    const ready = await status();
    expect(ready.version === NEW, `พบรุ่น ${NEW} (ได้ ${ready.version})`);
    await see('update-open');
    await stage('ready', { version: ready.version, downloaded: ready.downloaded });

    if (!LIVE) {
      // 3) ระหว่างรอผู้ใช้กด ตัวรันออกรุ่นที่ใหม่กว่า (รอบแรกไฟล์ถูกแก้): แอปลองรุ่นนั้น แล้วกลับไปเสนอรุ่นที่ถือไว้
      await sleep(1500);
      const tried = await invoke('update_check');
      expect(tried.state === 'downloading' && tried.version === NEWER, `ถามซ้ำแล้วพบรุ่นที่ใหม่กว่า (ได้ ${tried.state} ${tried.version})`);
      await waitFor(async () => {
        const now = await status();
        return now.state === 'ready' && now.version === NEW;
      }, 'รุ่นที่ใหม่กว่าไม่ผ่านลายเซ็น จึงกลับไปเสนอรุ่นที่ถือไว้', 60000);
      await see('update-notice');
      await stage('kept', { version: NEW });

      // 4) ตัวรันวางไฟล์จริงของรุ่นที่ใหม่กว่า: ถามซ้ำแล้วต้องเปลี่ยนไปเสนอรุ่นล่าสุด ไม่ต้องไล่ทีละรุ่น
      await sleep(1500);
      const again = await invoke('update_check');
      expect(again.state === 'downloading' && again.version === NEWER, `เริ่มดาวน์โหลดรุ่นล่าสุด (ได้ ${again.state} ${again.version})`);
      await waitFor(async () => {
        const now = await status();
        return now.state === 'ready' && now.version === NEWER;
      }, 'รุ่นล่าสุดพร้อมติดตั้ง', 90000);
      await stage('newer', { version: NEWER });
    }

    // 5) ปุ่มในเมนูซ้าย → กล่องยืนยัน (แสดง "มีอะไรใหม่" จาก manifest) → อัปเดตและเปิดใหม่
    (await see('update-notice')).click();
    const notes = await see('update-notes');
    expect(LIVE ? text(notes).length > 0 : text(notes).includes(NEWER_NOTES), `กล่องยืนยันแสดงข้อความมีอะไรใหม่ของรุ่นล่าสุด (ได้ "${text(notes)}")`);
    await stage('confirm', { notes: text(notes) });
    (await see('update-install')).click();
    // จากนี้แอปปิดและเปิดใหม่เอง รายงานขั้นสุดท้ายมาจากรอบสอง
  } catch (e) {
    await report('FAILED', { error: String(e?.message ?? e), state: await status().catch(() => null) });
  }
})();
