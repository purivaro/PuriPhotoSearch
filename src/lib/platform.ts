/** ความต่างของระบบปฏิบัติการที่ผู้ใช้เห็น: ปุ่มลัดและชื่อตัวจัดการไฟล์ */
export const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.userAgent);

/** ปุ่มหลักของปุ่มลัด เช่น `${MOD_KEY} K` (ตัวจับปุ่มรับทั้ง ⌘ และ Ctrl อยู่แล้ว) */
export const MOD_KEY = isMac ? '⌘' : 'Ctrl';

export const FILE_MANAGER = isMac ? 'Finder' : 'File Explorer';
