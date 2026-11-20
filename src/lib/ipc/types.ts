/**
 * สัญญา IPC ระหว่าง UI กับ Rust (Blueprint หมวด 12)
 *
 * ไฟล์นี้เป็นต้นฉบับฝั่ง TypeScript ของ DTO ทุกตัวที่ `puri_core::api::dispatch` คืนกลับมา
 * ฝั่ง Rust ใช้ serde `rename_all = "camelCase"` ให้ตรงกับชื่อ field ที่นี่
 * เมื่อแก้ไฟล์นี้ต้องแก้ `crates/core/src/api.rs` และ DTO ที่เกี่ยวข้องไปพร้อมกัน
 */

export type Id = string;

// ───────────────────────── ข้อผิดพลาด ─────────────────────────

export type ErrorCode =
  | 'ACCESS_DENIED'
  | 'SOURCE_UNAVAILABLE'
  | 'MODEL_NOT_READY'
  | 'UNSUPPORTED_FORMAT'
  | 'DECODE_FAILED'
  | 'DISK_FULL'
  | 'PREVIEW_STALE'
  | 'REVISION_CHANGED'
  | 'VERSION_CONFLICT'
  | 'DATABASE_CORRUPT'
  | 'LIBRARY_TOO_NEW'
  | 'LIBRARY_LOCKED'
  | 'CANCELLED'
  | 'NOT_FOUND'
  | 'INVALID_INPUT'
  | 'DESTINATION_INSIDE_SOURCE'
  | 'INTERNAL';

export interface AppError {
  code: ErrorCode;
  /** ข้อความภาษาไทยที่แสดงผู้ใช้ได้ทันที */
  message: string;
  retryable: boolean;
  affectedIds?: Id[];
  nextAction?:
    | 'grant_access'
    | 'relink'
    | 'free_space'
    | 'install_model'
    | 'refresh_preview'
    | 'retry'
    | 'open_help';
}

// ───────────────────────── คลัง ─────────────────────────

export interface LibrarySummary {
  libraryId: Id;
  name: string;
  eventCount: number;
  photoCount: number;
  /** บุคคลที่ยังไม่ถูกเก็บ (archive) */
  personCount: number;
  /** ใบหน้าที่ตรวจพบแล้วแต่ยังไม่มีชื่อที่ยืนยัน (รอตรวจสอบ) */
  unnamedFaceCount: number;
  /** เพิ่มขึ้นทุกครั้งที่ข้อมูลในคลังเปลี่ยน ใช้ตรวจว่า cache ฝั่ง UI ยังสดหรือไม่ */
  dataRevision: number;
}

export interface Capabilities {
  appVersion: string;
  schemaVersion: number;
  platform: string;
  /** นามสกุลไฟล์ที่ build นี้ถอดรหัสได้จริง เช่น ["jpg","jpeg","png"] */
  imageFormats: string[];
  faceAnalysis: FaceAnalysisStatus;
  /** ติดตามรูปใหม่ในโฟลเดอร์ที่เชื่อมแล้วโดยอัตโนมัติ — false ในชุดงานนี้: ต้องกด "ตรวจรูปใหม่" เอง */
  folderWatch: {
    available: boolean;
    reason: string;
  };
  /** 'native' ในแอปจริง, 'dev_path_input' เมื่อรัน UI ในเบราว์เซอร์ผ่าน dev bridge */
  folderPicker: 'native' | 'dev_path_input';
  libraryPath: string;
  cachePath: string;
}

/** สถานะตัววิเคราะห์ใบหน้า (S21) — ทุกค่ามาจากสิ่งที่ตรวจพบจริงในเครื่อง */
export interface FaceAnalysisStatus {
  /** พร้อมวิเคราะห์: มีไฟล์โมเดลที่ตรวจแล้วและ build นี้รันได้ */
  available: boolean;
  /**
   * ready = พร้อมใช้ · not_installed = ยังไม่ได้ดาวน์โหลดโมเดล (กด install_face_model ครั้งเดียว) ·
   * installing = กำลังดาวน์โหลด (ดู installJobId) · unsupported = build/เครื่องนี้รันตัววิเคราะห์ไม่ได้
   */
  state: 'ready' | 'not_installed' | 'installing' | 'unsupported';
  /** ข้อความภาษาไทยอธิบายสถานะ พร้อมแสดงผู้ใช้ */
  reason: string;
  /** ชื่อชุดโมเดล เช่น "InsightFace buffalo_l" */
  modelName: string;
  /** ขนาดที่ต้องดาวน์โหลด (MB) */
  downloadSizeMb: number;
  /** ที่มาของไฟล์ (โดเมน/โครงการ) สำหรับแสดงก่อนดาวน์โหลด */
  downloadSource: string;
  /** เงื่อนไขการใช้ของโมเดล ต้องแสดงใกล้ปุ่มดาวน์โหลด */
  licenseNote: string;
  installJobId: Id | null;
  /** โฟลเดอร์ที่เก็บไฟล์โมเดลในเครื่อง */
  modelPath: string;
}

// ───────────────────────── เพิ่มโฟลเดอร์ (S02–S03) ─────────────────────────

export type SourceMode = 'single_event' | 'child_events';

export interface EventDraft {
  draftId: Id;
  /** whole = ทั้งโฟลเดอร์ที่เลือกเป็นงานเดียว, folder = โฟลเดอร์ย่อยชั้นแรก, root_loose = รูปที่วางตรงโฟลเดอร์แม่ */
  kind: 'whole' | 'folder' | 'root_loose';
  folderName: string;
  displayPath: string;
  suggestedTitle: string;
  /** YYYY-MM-DD (ค.ศ.) หรือ null เมื่อวันที่กำกวม/ไม่มี */
  suggestedDate: string | null;
  dateSource: 'folder_name' | 'none';
  /** จำนวนไฟล์รูปที่รองรับ */
  photoCount: number;
  /** ยังไม่อยู่ในคลัง */
  newCount: number;
  /** อยู่ในคลังแล้ว (จะไม่ถูกเพิ่มซ้ำ) */
  existingCount: number;
  /** ไฟล์ภาพชนิดที่ build นี้ยังไม่รองรับ */
  unsupportedCount: number;
  readable: boolean;
  unreadableReason: string | null;
  /** มีกิจกรรมที่ผูกกับโฟลเดอร์นี้อยู่แล้ว */
  existingEventId: Id | null;
  existingEventTitle: string | null;
  defaultSelected: boolean;
  /** มีรูปตัวอย่างให้แสดงผ่าน asset `draft/{previewId}/{draftId}` หรือไม่ */
  hasSample: boolean;
}

export interface ImportPreview {
  previewId: Id;
  rootPath: string;
  rootName: string;
  suggestedMode: SourceMode;
  /** โหมดงานเดียว: ทั้งโฟลเดอร์เป็นหนึ่งกิจกรรม */
  single: EventDraft;
  /** โหมดหลายงาน: โฟลเดอร์ย่อยชั้นแรก */
  children: EventDraft[];
  /** โหมดหลายงาน: รูปที่วางตรงโฟลเดอร์แม่ (null เมื่อไม่มี) */
  rootLoose: EventDraft | null;
  /** สรุปชนิดไฟล์ภาพที่ข้าม เช่น {"heic": 12} */
  unsupportedByExt: Record<string, number>;
  /** ISO-8601 UTC */
  expiresAt: string;
}

export interface CommitImportArgs {
  previewId: Id;
  /** UUID ที่ UI สร้างครั้งเดียวต่อการกดยืนยัน ใช้กันสร้างซ้ำเมื่อกดซ้ำ/retry */
  requestId: Id;
  mode: SourceMode;
  /** เฉพาะรายการที่ผู้ใช้เลือก */
  drafts: { draftId: Id; title: string; eventDate: string | null }[];
  /** เก็บความตั้งใจไว้ใช้เมื่อมีตัววิเคราะห์ใบหน้า */
  analyzeFaces: boolean;
  watch: boolean;
}

export interface ImportCommitResult {
  eventIds: Id[];
  jobIds: Id[];
  createdEvents: number;
  reusedEvents: number;
}

// ───────────────────────── กิจกรรม (S12–S13) ─────────────────────────

export interface EventSummary {
  id: Id;
  title: string;
  eventDate: string | null;
  dateSource: 'folder_name' | 'user' | 'none';
  photoCount: number;
  /** จำนวนรูปที่ภาพย่อพร้อมแล้ว (ข้อมูลรูปพร้อม) */
  thumbReadyCount: number;
  /** จำนวนรูปที่เตรียมข้อมูลไม่สำเร็จ */
  failedCount: number;
  /** จำนวนรูปที่วิเคราะห์ใบหน้าเสร็จแล้ว (รวมรูปที่วิเคราะห์แล้วไม่พบใบหน้า) */
  faceAnalyzedCount: number;
  coverPhotoId: Id | null;
  /** โฟลเดอร์ต้นทางเข้าถึงได้ ณ เวลาที่ถาม */
  sourceAvailable: boolean;
  folderPath: string;
  activeJobId: Id | null;
  createdAt: string;
  recordVersion: number;
}

export interface PersonCount {
  person: PersonRef;
  photoCount: number;
}

export interface EventDetail extends EventSummary {
  notes: string;
  /** บุคคลที่ยืนยันแล้วในงานนี้ เรียงตามจำนวนรูป */
  people: PersonCount[];
  jobs: JobSnapshot[];
}

export interface UpdateEventArgs {
  eventId: Id;
  expectedVersion: number;
  title?: string;
  /** undefined = ไม่แก้, null = ล้างวันที่ */
  eventDate?: string | null;
  notes?: string;
}

// ───────────────────────── รูป / ค้นหา (S06, S13, S15) ─────────────────────────

export type PersonMode = 'all' | 'any';

export interface SearchParams {
  text?: string;
  personIds: Id[];
  personMode: PersonMode;
  eventIds: Id[];
  date?: {
    field: 'photo_taken' | 'event_date';
    from?: string;
    to?: string;
    includeUnknown: boolean;
  };
  /** any = ทั้งหมด, has_faces = มีใบหน้าที่ระบุไว้, no_person = ยังไม่มีชื่อบุคคลในรูป */
  faceFilter: 'any' | 'has_faces' | 'no_person';
  sort: 'taken_desc' | 'taken_asc' | 'added_desc' | 'filename';
  cursor?: string;
  /** ค่าเริ่มต้น 100, เพดานฝั่ง Rust 200 */
  limit: number;
  /** นับรูปที่ AI จัดให้บุคคลนั้นด้วย (ยังไม่มีคนยืนยัน); ผลบอกแยกใน `suggestedPeople` */
  includeSuggested?: boolean;
  /** เฉพาะรูปที่ผู้ใช้แจ้งว่า "ไม่ใช่" บุคคลนี้; ผลบอกใบหน้าใน `rejectedFaces` */
  rejectedPersonId?: Id;
}

export interface PersonRef {
  id: Id;
  displayName: string;
  coverFaceId: Id | null;
  /**
   * ภาพตัวแทนสำหรับ `<img>`: path ของ asset เช่น "face/{id}" หรือ "sample/{id}" (ใบหน้าตัวอย่างที่นำเข้าจากรุ่นเดิม)
   * ใช้กับ `assetPath()` ใน client.ts; null = ยังไม่มีภาพ ให้แสดงตัวอักษรย่อ
   */
  coverAsset: string | null;
}

export interface PhotoResult {
  id: Id;
  revisionId: Id;
  filename: string;
  eventId: Id;
  eventTitle: string;
  /** เวลาท้องถิ่นจาก EXIF แบบ `YYYY-MM-DDTHH:MM:SS` (ไม่มี timezone) หรือ null */
  takenAt: string | null;
  takenSource: 'exif' | 'none';
  /** ขนาดภาพหลังหมุนตาม EXIF แล้ว (ตรงกับที่ผู้ใช้เห็น); 0 เมื่อยังเตรียมข้อมูลรูปไม่เสร็จ */
  width: number;
  height: number;
  thumbStatus: 'ready' | 'pending' | 'failed';
  /** ผลตรวจล่าสุดที่บันทึกไว้ว่าไฟล์ต้นฉบับอยู่หรือไม่ */
  available: boolean;
  confirmedPeople: PersonRef[];
  /** บุคคลที่ AI จัดให้ในรูปนี้ พร้อมใบหน้าที่เป็นเหตุ (มีเมื่อค้นแบบ `includeSuggested`) */
  suggestedPeople: { person: PersonRef; faceId: Id }[];
  /** ใบหน้าในรูปนี้ที่ผู้ใช้แจ้งว่าไม่ใช่บุคคลที่ถาม (มีเมื่อค้นด้วย `rejectedPersonId`) */
  rejectedFaces: Id[];
}

export interface Coverage {
  eligiblePhotos: number;
  /** ข้อมูลรูปพร้อม (ภาพย่อ/วันถ่าย) */
  thumbnailsReady: number;
  thumbnailsPending: number;
  thumbnailsFailed: number;
  /** รูปที่มีใบหน้ายืนยันแล้วอย่างน้อยหนึ่งใบ */
  withConfirmedPeople: number;
  /** รูปที่วิเคราะห์ใบหน้าเสร็จแล้ว (รวม "วิเคราะห์แล้ว ไม่พบใบหน้า") */
  facesAnalyzed: number;
  /** รูปที่ยังไม่ได้วิเคราะห์ใบหน้า — ผลค้นตามคนอาจยังไม่ครบ */
  facesPending: number;
}

export interface PhotoPage {
  items: PhotoResult[];
  nextCursor: string | null;
  total: number;
  coverage: Coverage;
  dataRevision: number;
}

export interface FaceBox {
  id: Id;
  /** พิกัดสัดส่วน [0,1] บนภาพที่หมุนตาม EXIF แล้ว (มุมซ้ายบน + กว้าง/สูง) */
  bbox: { x: number; y: number; w: number; h: number };
  origin: 'manual' | 'detector' | 'legacy';
  /** บุคคลที่ผู้ใช้ยืนยันแล้ว */
  person: PersonRef | null;
  /**
   * ข้อเสนอของ AI เมื่อยังไม่มีคนที่ยืนยัน: "อาจเป็น …" — ไม่นับเป็นยืนยันแล้ว และไม่แสดงเป็นเปอร์เซ็นต์
   * level: close = ใกล้เคียงมาก, check = ควรตรวจสอบ
   */
  suggestion: { person: PersonRef; level: 'close' | 'check' } | null;
  recordVersion: number;
}

export interface PhotoDetails extends PhotoResult {
  path: string;
  fileSize: number;
  eventDate: string | null;
  /** ตรวจสด ณ เวลาที่ถาม */
  originalAvailable: boolean;
  decodeError: string | null;
  faces: FaceBox[];
}

export interface SearchSuggestions {
  people: PersonSummary[];
  events: EventSummary[];
  files: PhotoResult[];
}

// ───────────────────────── บุคคล (S05, S08–S10) ─────────────────────────

export interface GroupRef {
  id: Id;
  name: string;
}

export interface GroupSummary extends GroupRef {
  memberCount: number;
}

export interface PersonSummary extends PersonRef {
  nickname: string | null;
  /** COUNT(DISTINCT photo) จากใบหน้าที่ยืนยันแล้วบน revision ปัจจุบัน */
  confirmedPhotoCount: number;
  /** รูปที่ AI จัดให้คนนี้และยังไม่มีคำยืนยันของคนนี้ในรูปนั้น */
  aiPhotoCount: number;
  pinned: boolean;
  groups: GroupRef[];
  archived: boolean;
  recordVersion: number;
}

export interface PersonEventCount {
  eventId: Id;
  title: string;
  eventDate: string | null;
  photoCount: number;
  coverPhotoId: Id | null;
}

export interface CoappearanceItem {
  person: PersonSummary;
  sharedPhotoCount: number;
}

export interface PersonDetail extends PersonSummary {
  aliases: string[];
  notes: string;
  confirmedEventCount: number;
  hasConfirmedFaces: boolean;
  events: PersonEventCount[];
  coappearances: CoappearanceItem[];
  /** รูปที่ผู้ใช้แจ้งว่าไม่ใช่คนนี้ (คืนได้) */
  rejectedPhotoCount: number;
}

export interface PersonQuickViewData {
  person: PersonSummary;
  confirmedEventCount: number;
  hasConfirmedFaces: boolean;
  notes: string;
  /** ภาพกิจกรรมล่าสุดของคนนี้ (สูงสุด 4) */
  recentPhotos: PhotoResult[];
  coappearances: CoappearanceItem[];
  scope: 'all_events';
  dataRevision: number;
}

export interface HomePeople {
  /** แท็บที่ควรแสดง: ค่าที่ผู้ใช้เลือกไว้ หรือค่าเริ่มต้นตามหมวด 6.5 */
  tab: 'pinned' | 'recent';
  pinned: PersonSummary[];
  recent: PersonSummary[];
}

export interface SavePersonArgs {
  /** ไม่ส่ง = สร้างใหม่ */
  personId?: Id;
  /** ต้องส่งเมื่อแก้ไข */
  expectedVersion?: number;
  requestId: Id;
  displayName: string;
  nickname?: string | null;
  notes?: string;
  aliases?: string[];
  groupIds?: Id[];
}

// ───────────────────────── ใบหน้า (S15) ─────────────────────────

export interface CreateManualFaceArgs {
  requestId: Id;
  photoId: Id;
  bbox: { x: number; y: number; w: number; h: number };
  /** เลือกอย่างใดอย่างหนึ่ง: คนเดิม หรือสร้างคนใหม่ด้วยชื่อ */
  personId?: Id;
  newPersonName?: string;
}

export interface FaceMutationResult {
  operationId: Id;
  faces: FaceBox[];
  /** บุคคลที่สร้างใหม่ในคำสั่งนี้ (ถ้ามี) */
  createdPerson: PersonRef | null;
  dataRevision: number;
}

// ───────────────────────── ตรวจสอบใบหน้า (S16) ─────────────────────────

/** ใบหน้าหนึ่งใบในคิวตรวจ: ครอปดูได้จาก asset `face/{faceId}` และเปิดภาพบริบทได้จาก photoId */
export interface ReviewFace {
  faceId: Id;
  photoId: Id;
  filename: string;
  eventId: Id;
  eventTitle: string;
  /** มีเฉพาะในกลุ่ม "AI เสนอชื่อ" */
  level: 'close' | 'check' | null;
}

/** กลุ่มใบหน้าที่ AI เสนอว่าอาจเป็นบุคคลที่มีอยู่ */
export interface SuggestedGroup {
  person: PersonSummary;
  faceCount: number;
  photoCount: number;
  /** ใบหน้าตัวอย่างของกลุ่ม (สูงสุด 8) */
  sampleFaceIds: Id[];
}

/** กลุ่มใบหน้าที่ยังไม่ทราบชื่อ — เป็นกลุ่มชั่วคราวเพื่อช่วยตรวจ ไม่ใช่บุคคล */
export interface UnknownCluster {
  /** ใช้ได้ภายใน reviewRevision เดียวกันเท่านั้น */
  clusterId: Id;
  faceCount: number;
  sampleFaceIds: Id[];
}

export interface ReviewOverview {
  /** เปลี่ยนเมื่อคำยืนยัน/ใบหน้าเปลี่ยน; clusterId ของ revision เก่าใช้ไม่ได้ (ได้ PREVIEW_STALE) */
  reviewRevision: number;
  /** ใบหน้าที่ยังไม่มีชื่อทั้งหมดในขอบเขต */
  unnamedFaceCount: number;
  suggested: SuggestedGroup[];
  /** กลุ่มที่มีอย่างน้อยสองใบหน้า เรียงใหญ่ไปเล็ก (สูงสุด 200 กลุ่ม) */
  clusters: UnknownCluster[];
  /** ใบหน้าที่ยังไม่เข้ากลุ่มใด (ตั้งชื่อได้จากตัวดูภาพ) */
  singleFaceCount: number;
  /** ใบหน้าที่ยังไม่ได้นำมาจัดกลุ่มรอบนี้ (คิวจัดกลุ่มครั้งละ 12,000 ใบที่ชัดที่สุด) */
  unexaminedFaceCount: number;
  /** รูปในขอบเขตที่ยังไม่ได้วิเคราะห์ใบหน้า — คิวนี้อาจมีเพิ่มภายหลัง */
  photosPendingAnalysis: number;
}

export interface ReviewGroupFaces {
  reviewRevision: number;
  faces: ReviewFace[];
  /** จำนวนทั้งหมดของกลุ่ม (faces ถูกจำกัดที่ 500) */
  total: number;
}

export interface SetupState {
  /** บุคคลในฐานของรุ่น Python บนเครื่องนี้ที่ยังไม่ได้นำเข้า (0 = ไม่พบฐานเดิม หรือนำเข้าครบแล้ว) */
  legacyPending: number;
  dismissed: boolean;
}

export interface LegacyImportResult {
  personsCreated: number;
  /** มีอยู่แล้วจากการนำเข้าครั้งก่อน (ไม่สร้างซ้ำ) */
  personsSkipped: number;
  groupsCreated: number;
  samplesImported: number;
  /** ใบหน้าตัวอย่างที่ยังรอสร้างเวกเตอร์ด้วยโมเดลปัจจุบัน (ทำเองเมื่อโมเดลพร้อม) */
  samplesPendingEmbedding: number;
  notes: string[];
}

// ───────────────────────── งานเบื้องหลัง (S04, S19) ─────────────────────────

export type JobState =
  | 'queued'
  | 'running'
  | 'pausing'
  | 'paused'
  | 'blocked'
  | 'completed'
  | 'completed_with_errors'
  | 'cancel_requested'
  | 'cancelled'
  | 'failed';

export interface JobSnapshot {
  id: Id;
  /** model = ดาวน์โหลดตัววิเคราะห์ใบหน้า (counts เป็น MB) */
  kind: 'catalog' | 'export' | 'model' | 'shrink';
  state: JobState;
  /** catalog: discovery | thumbnail | done; export: copy | done; model: download | verify | done */
  stage: string;
  title: string;
  eventId: Id | null;
  counts: { total: number; done: number; failed: number; skipped: number };
  /** false ระหว่าง discovery: `total` เป็นเพียง "พบแล้ว" */
  totalKnown: boolean;
  /** งานนี้ถูกกู้กลับมาหลังแอปปิดกลางคัน */
  recovered: boolean;
  blockedReason: string | null;
  errorSummary: { code: ErrorCode; count: number }[];
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
  updatedAt: string;
}

export interface JobErrorItem {
  photoId: Id | null;
  filename: string;
  code: ErrorCode;
  message: string;
}

// ───────────────────────── ส่งออก (S18) ─────────────────────────

export type SelectionSpec =
  | { kind: 'explicit'; photoIds: Id[] }
  | { kind: 'snapshot'; selectionToken: string; excludedPhotoIds: Id[] };

export interface SelectionSnapshot {
  selectionToken: string;
  count: number;
  dataRevision: number;
}

/** วิธีสร้างสำเนา: คัดลอกไฟล์เดิม หรือย่อเป็น JPEG (รูปที่เล็กพออยู่แล้วคัดลอกไฟล์เดิม) */
export type ExportResize = { mode: 'original' } | { mode: 'long_edge'; px: number } | { mode: 'max_bytes'; bytes: number };

export type WatermarkAnchor =
  | 'top_left'
  | 'top_center'
  | 'top_right'
  | 'center_left'
  | 'center'
  | 'center_right'
  | 'bottom_left'
  | 'bottom_center'
  | 'bottom_right';

/** ลายน้ำที่ใส่ทุกสำเนาของงานส่งออก */
export interface WatermarkSpec {
  id: Id;
  anchor: WatermarkAnchor;
  /** ด้านยาวของโลโก้ เป็น % ของด้านสั้นของภาพ (5–100) */
  scale: number;
  /** ความทึบ % (5–100) */
  opacity: number;
}

/** โลโก้ที่เก็บไว้ในคลัง (รูป: asset `watermark/{id}`) */
export interface WatermarkInfo {
  id: Id;
  name: string;
  width: number;
  height: number;
  /** มีส่วนโปร่งใส — ไม่มีจะเห็นเป็นกรอบสี่เหลี่ยมทับรูป */
  transparent: boolean;
}

export interface ExportPlan {
  planId: Id;
  destination: string;
  /** N: จำนวนรูปที่เลือก */
  inputCount: number;
  /** M: จำนวนไฟล์ที่จะสร้าง (โหมด flat: เท่ากับรูปที่ต้นฉบับพร้อม) */
  plannedOutputCount: number;
  unavailableInputs: number;
  /** ไฟล์ที่ชื่อชนและจะถูกเติมเลขท้าย */
  renamedCount: number;
  /** ขนาดรวมโดยประมาณ (โหมดขนาดไฟล์: เพดาน) */
  estimatedBytes: number;
  /** ขนาดรวมของต้นฉบับที่จะส่งออก */
  sourceBytes: number;
  resize: ExportResize;
  /** ไฟล์ที่จะถูกย่อ ที่เหลือเล็กพออยู่แล้วจึงคัดลอกไฟล์เดิม (หรือบันทึกใหม่ขนาดเดิมเมื่อใส่ลายน้ำ) */
  resizedCount: number;
  watermark: WatermarkSpec | null;
  sampleNames: { source: string; output: string }[];
  canStart: boolean;
  blockedReason: string | null;
}

export interface ExportReportItem {
  photoId: Id;
  filename: string;
  status: 'copied' | 'failed' | 'cancelled' | 'unavailable' | 'planned';
  outputPath: string | null;
  error: string | null;
}

export interface ExportReport {
  jobId: Id;
  destination: string;
  inputCount: number;
  copied: number;
  failed: number;
  unavailable: number;
  cancelled: number;
  resize: ExportResize;
  watermark: WatermarkSpec | null;
  /** ขนาดรวมของต้นฉบับและของสำเนา เฉพาะรูปที่ส่งออกสำเร็จ */
  sourceBytes: number;
  outputBytes: number;
  items: ExportReportItem[];
}

// ───────────────────────── ย่อไฟล์ต้นฉบับในที่เดิม ─────────────────────────

/** แผนย่อไฟล์ต้นฉบับ: ต้นฉบับขนาดเต็มของรูปที่ย่อไปถังขยะ ข้อมูลในคลัง (ใบหน้า ชื่อ) อยู่ครบ */
export interface ShrinkPlan {
  planId: Id;
  resize: ExportResize;
  inputCount: number;
  plannedCount: number;
  alreadySmall: number;
  notJpeg: number;
  unavailable: number;
  /** ขนาดรวมของต้นฉบับที่จะย่อ และขนาดโดยประมาณหลังย่อ */
  sourceBytes: number;
  estimatedBytes: number;
}

export interface ShrinkReportItem {
  photoId: Id;
  filename: string;
  status: 'planned' | 'replacing' | 'shrunk' | 'skipped' | 'unavailable' | 'failed' | 'cancelled';
  /** เหตุผลที่ข้ามหรือไม่สำเร็จ */
  note: string | null;
}

export interface ShrinkReport {
  jobId: Id;
  resize: ExportResize;
  inputCount: number;
  shrunk: number;
  skipped: number;
  failed: number;
  unavailable: number;
  cancelled: number;
  /** ยังไม่ได้ทำ หรือค้างระหว่างแทนที่ (ทำต่อเมื่อเปิดแอปครั้งหน้า) */
  pending: number;
  /** ขนาดรวมก่อนและหลังย่อ ของรูปที่ย่อสำเร็จ */
  sourceBytes: number;
  outputBytes: number;
  items: ShrinkReportItem[];
}

// ───────────────────────── ย้ายข้อมูลเดิม (dry-run เท่านั้น) ─────────────────────────

/** ผลจับคู่รูปของกิจกรรมหนึ่งกับไฟล์ที่ตำแหน่งใหม่ (จับคู่จากตำแหน่งในโฟลเดอร์ + ขนาดไฟล์) */
export interface RelinkEvent {
  eventId: Id;
  title: string;
  oldFolder: string;
  newFolder: string;
  photos: number;
  matched: number;
  /** ไม่พบไฟล์ที่ตำแหน่งเดิมในโฟลเดอร์ใหม่ */
  missing: number;
  /** พบไฟล์ชื่อเดิมแต่ขนาดต่างจากที่บันทึกไว้ */
  changed: number;
}

export interface RelinkPreview {
  event: RelinkEvent;
  /** กิจกรรมอื่นจากโฟลเดอร์แม่เดียวกันที่หาไม่เจอ และพบที่ตำแหน่งใหม่แบบเดียวกัน */
  siblings: RelinkEvent[];
  siblingsNotFound: number;
}

export interface RelinkResult {
  events: number;
  photosRelinked: number;
  photosUnresolved: number;
  dataRevision: number;
}

export interface BackupCounts {
  events: number;
  photos: number;
  persons: number;
  confirmations: number;
  faces: number;
}

/** ชุดสำรองหนึ่งชุด (เนื้อหาของ manifest.json) */
export interface BackupManifest {
  format: number;
  /** manual = ผู้ใช้สั่ง, auto = แอปสำรองประจำวัน, before_restore / before_upgrade = แอปเก็บไว้ก่อนเปลี่ยนคลัง */
  kind: 'manual' | 'auto' | 'before_restore' | 'before_upgrade';
  libraryId: Id;
  libraryName: string;
  appVersion: string;
  schemaVersion: number;
  createdAt: string;
  counts: BackupCounts;
  bytes: number;
  sha256: string;
  /** สิ่งที่ไม่รวมในชุดสำรอง: originals | thumbnails | face_crops | models */
  excluded: string[];
}

export interface BackupInfo extends BackupManifest {
  /** โฟลเดอร์ของชุดสำรอง */
  path: string;
}

export interface BackupStatus {
  lastBackupAt: string | null;
  lastBackupPath: string | null;
  backupsDir: string;
  /** ชุดที่แอปเก็บเอง ใหม่สุดก่อน */
  automatic: BackupInfo[];
  /** ชุดที่จะถูกใช้เป็นคลังเมื่อเปิดแอปครั้งถัดไป */
  pendingRestore: BackupManifest | null;
}

export interface RestorePreview {
  backup: BackupInfo;
  /** จำนวนในคลังที่เปิดอยู่ ซึ่งจะถูกแทนที่ */
  current: BackupCounts;
  sameLibrary: boolean;
  needsUpgrade: boolean;
}

export interface LegacyReport {
  path: string;
  recognized: boolean;
  persons: number;
  duplicateNames: number;
  groups: number;
  referenceSamples: number;
  eventFolders: number;
  photos: number;
  photosMissingOnDisk: number;
  faces: number;
  embeddingDim: number | null;
  rejections: number;
  dismissedFaces: number;
  notes: string[];
}

// ───────────────────────── ตารางคำสั่ง ─────────────────────────

/** ชื่อคำสั่ง → [args, result] ใช้กับ `call()` ใน `client.ts` */
export interface Commands {
  get_capabilities: [Record<string, never>, Capabilities];
  get_library_summary: [Record<string, never>, LibrarySummary];

  preview_import: [{ path: string }, ImportPreview];
  cancel_import_preview: [{ previewId: Id }, null];
  commit_import: [CommitImportArgs, ImportCommitResult];

  list_events: [{ text?: string }, EventSummary[]];
  get_event: [{ eventId: Id }, EventDetail];
  update_event: [UpdateEventArgs, EventDetail];
  /** ตรวจรูปใหม่ในโฟลเดอร์ของกิจกรรม */
  rescan_event: [{ eventId: Id; requestId: Id }, JobSnapshot];
  set_event_cover: [{ eventId: Id; photoId: Id }, EventDetail];
  reveal_event_folder: [{ eventId: Id }, null];

  query_photos: [SearchParams, PhotoPage];
  get_photo_details: [{ photoId: Id }, PhotoDetails];
  reveal_photo: [{ photoId: Id }, null];
  open_photo: [{ photoId: Id }, null];
  search_suggestions: [{ text: string; limit?: number }, SearchSuggestions];

  list_people: [{ text?: string; includeArchived?: boolean; groupId?: Id; pinnedOnly?: boolean }, PersonSummary[]];
  get_person: [{ personId: Id }, PersonDetail];
  save_person: [SavePersonArgs, PersonDetail];
  set_person_archived: [{ personId: Id; archived: boolean }, PersonSummary];
  set_person_cover: [{ personId: Id; faceId: Id }, PersonSummary];
  list_home_people: [{ limit?: number }, HomePeople];
  set_person_pinned: [{ personId: Id; pinned: boolean }, PersonSummary];
  /** รวมบุคคลที่ซ้ำ (sourceIds) เข้ากับ targetId — ย้อนกลับไม่ได้; ใบหน้าตัวอย่าง คำยืนยัน กลุ่ม ชื่ออื่น และหมุดย้ายมาที่ targetId */
  merge_persons: [{ targetId: Id; sourceIds: Id[]; requestId: Id }, { personId: Id; merged: number }];
  /** เรียกเฉพาะเมื่อผู้ใช้ตั้งใจเปิดดู (เปิด S09 หรือกดดูเร็วสำเร็จ) */
  record_person_view: [{ personId: Id }, null];
  set_home_people_tab: [{ tab: 'pinned' | 'recent' }, null];
  get_person_quick_view: [{ personId: Id }, PersonQuickViewData];
  list_groups: [Record<string, never>, GroupSummary[]];
  create_group: [{ name: string }, GroupSummary];
  delete_group: [{ groupId: Id }, null];

  create_manual_face: [CreateManualFaceArgs, FaceMutationResult];
  /** กำหนดบุคคลให้ใบหน้าที่เลือก (เปลี่ยนคน = เพิกถอนของเดิมแล้วสร้างใหม่ใน transaction เดียว) */
  assign_faces: [{ requestId: Id; faceIds: Id[]; personId?: Id; newPersonName?: string }, FaceMutationResult];
  /** "ไม่ใช่บุคคลนี้": เพิกถอนชื่อ แต่กรอบใบหน้ายังอยู่ */
  unassign_faces: [{ faceIds: Id[] }, FaceMutationResult];
  /** "ไม่ใช่ใบหน้า": เอากรอบออกจากการแสดงผล (เพิกถอนชื่อด้วย) */
  ignore_faces: [{ faceIds: Id[] }, FaceMutationResult];

  list_jobs: [{ activeOnly?: boolean; limit?: number }, JobSnapshot[]];
  get_job: [{ jobId: Id }, JobSnapshot];
  list_job_errors: [{ jobId: Id }, JobErrorItem[]];
  pause_job: [{ jobId: Id }, JobSnapshot];
  resume_job: [{ jobId: Id }, JobSnapshot];
  cancel_job: [{ jobId: Id }, JobSnapshot];
  retry_failed_items: [{ jobId: Id }, JobSnapshot];

  /** แช่แข็งผลค้นหาทั้งหมด ณ ตอนนี้ (ไม่ใช่เฉพาะหน้าที่โหลด) */
  create_selection: [SearchParams, SelectionSnapshot];
  preview_export: [{ selection: SelectionSpec; destination: string; resize?: ExportResize; watermark?: WatermarkSpec | null }, ExportPlan];
  start_export: [{ planId: Id; requestId: Id }, JobSnapshot];
  get_export_report: [{ jobId: Id }, ExportReport];
  /** แสดงโฟลเดอร์ปลายทางของงานส่งออกในตัวจัดการไฟล์ */
  reveal_export_destination: [{ jobId: Id }, null];
  list_watermarks: [Record<string, never>, WatermarkInfo[]];
  add_watermark: [{ path: string }, WatermarkInfo];
  /** คืนรายการที่เหลือ */
  delete_watermark: [{ id: Id }, WatermarkInfo[]];
  preview_shrink: [{ selection: SelectionSpec; resize: ExportResize }, ShrinkPlan];
  start_shrink: [{ planId: Id; requestId: Id }, JobSnapshot];
  get_shrink_report: [{ jobId: Id }, ShrinkReport];

  preview_relink: [{ eventId: Id; newFolder: string }, RelinkPreview];
  commit_relink: [{ eventId: Id; newFolder: string; includeSiblings: boolean }, RelinkResult];
  get_backup_status: [Record<string, never>, BackupStatus];
  create_backup: [{ destination: string }, BackupInfo];
  preview_restore: [{ path: string }, RestorePreview];
  stage_restore: [{ path: string }, RestorePreview];
  cancel_restore: [Record<string, never>, BackupStatus];
  preview_legacy_import: [{ path?: string }, LegacyReport];
  /** นำรายชื่อบุคคล กลุ่ม และใบหน้าตัวอย่างจากรุ่น Python เข้าคลัง (อ่านฐานเดิมอย่างเดียว รันซ้ำไม่สร้างคนซ้ำ) */
  import_legacy_people: [{ path?: string; requestId: Id }, LegacyImportResult];
  /** สิ่งที่เสนอให้ทำตอนเปิดแอปครั้งแรก (S01) นอกเหนือจากสถานะตัววิเคราะห์ใบหน้าใน get_capabilities */
  get_setup_state: [Record<string, never>, SetupState];
  /** ผู้ใช้ปิดหน้าต่างข้อเสนอแล้ว: จำไว้กับคลัง ไม่เปิดเองอีก */
  dismiss_setup: [Record<string, never>, null];

  /** ดาวน์โหลดและติดตั้งตัววิเคราะห์ใบหน้า (ครั้งเดียว) แล้ววิเคราะห์รูปที่มีในคลังต่อให้เอง */
  install_face_model: [Record<string, never>, JobSnapshot];
  /** สั่งวิเคราะห์ใบหน้าของรูปที่ยังไม่ได้วิเคราะห์ (ทั้งคลัง หรือเฉพาะกิจกรรม) */
  analyze_faces: [{ eventId?: Id }, { jobIds: Id[] }];

  get_review_overview: [{ eventId?: Id }, ReviewOverview];
  get_review_faces: [{ kind: 'person' | 'cluster'; id: Id; eventId?: Id; reviewRevision: number }, ReviewGroupFaces];
  /** "ไม่ใช่บุคคลนี้" สำหรับข้อเสนอของ AI: จำคู่ ใบหน้า–บุคคล ไว้ไม่เสนอซ้ำ ใบหน้ายังอยู่ให้เลือกคนอื่น */
  reject_suggestions: [{ faceIds: Id[]; personId: Id }, { dataRevision: number }];
  /** ย้อน "ไม่ใช่บุคคลนี้": ใบหน้ากลับไปให้ AI จัดได้ตามเดิม */
  restore_suggestions: [{ faceIds: Id[]; personId: Id }, { dataRevision: number }];
}

export type CommandName = keyof Commands;

/**
 * Asset ที่ Rust เสิร์ฟให้ `<img>` โดยรับเฉพาะ ID ทึบ (ไม่มี path จากฝั่ง UI)
 *   thumb/{photoId}            ภาพย่อ ~512px (JPEG, หมุนตาม EXIF แล้ว)
 *   photo/{photoId}            ไฟล์ต้นฉบับสำหรับตัวดูภาพ (404 เมื่อต้นทางไม่พร้อม)
 *   face/{faceId}              ภาพครอปใบหน้า ~256px
 *   draft/{previewId}/{draftId} รูปตัวอย่างในหน้าตรวจรายการก่อนเพิ่ม
 *   sample/{sampleId}          ครอปใบหน้าตัวอย่างของบุคคลที่นำเข้าจากรุ่นเดิม
 */
export type AssetKind = 'thumb' | 'photo' | 'face' | 'draft' | 'sample' | 'watermark';

/** Event ที่ Rust ส่งหลัง commit สำเร็จ (ในแอป Tauri); UI ต้องทำงานได้แม้ไม่ได้รับ (ใช้ polling) */
export interface DataChangedEvent {
  dataRevision: number;
  scopes: ('events' | 'photos' | 'people' | 'jobs')[];
}

// ───────────────────────── อัปเดตแอป ─────────────────────────

/** สถานะตัวอัปเดตของเปลือก Tauri (`src-tauri/src/updater.rs`) — ไม่ใช่คำสั่งโดเมน จึงไม่อยู่ใน `Commands` */
export interface UpdateStatus {
  /** unsupported = build นี้ไม่ตรวจหารุ่นใหม่ (รุ่นสำหรับพัฒนา) */
  state: 'unsupported' | 'idle' | 'checking' | 'up_to_date' | 'downloading' | 'ready' | 'installing' | 'failed';
  currentVersion: string;
  /** รุ่นใหม่ที่พบ */
  version: string | null;
  /** ข้อความ "มีอะไรใหม่" บรรทัดละเรื่อง */
  notes: string | null;
  downloaded: number;
  total: number | null;
  /** เหตุที่ตรวจหรือดาวน์โหลดไม่สำเร็จ */
  message: string | null;
  /** เวลาที่ตรวจสำเร็จครั้งล่าสุด (มิลลิวินาทีจาก Unix epoch) */
  checkedAt: number | null;
}
