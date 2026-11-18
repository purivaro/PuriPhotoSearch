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
  /** เพิ่มขึ้นทุกครั้งที่ข้อมูลในคลังเปลี่ยน ใช้ตรวจว่า cache ฝั่ง UI ยังสดหรือไม่ */
  dataRevision: number;
}

export interface Capabilities {
  appVersion: string;
  schemaVersion: number;
  platform: string;
  /** นามสกุลไฟล์ที่ build นี้ถอดรหัสได้จริง เช่น ["jpg","jpeg","png"] */
  imageFormats: string[];
  faceAnalysis: {
    /** false ในชุดงานนี้: ยังไม่มีตัววิเคราะห์ใบหน้าอัตโนมัติ ใช้การระบุใบหน้าเอง */
    available: boolean;
    reason: string;
  };
  /** 'native' ในแอปจริง, 'dev_path_input' เมื่อรัน UI ในเบราว์เซอร์ผ่าน dev bridge */
  folderPicker: 'native' | 'dev_path_input';
  libraryPath: string;
  cachePath: string;
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
}

export interface PersonRef {
  id: Id;
  displayName: string;
  coverFaceId: Id | null;
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
  width: number;
  height: number;
  thumbStatus: 'ready' | 'pending' | 'failed';
  /** ผลตรวจล่าสุดที่บันทึกไว้ว่าไฟล์ต้นฉบับอยู่หรือไม่ */
  available: boolean;
  confirmedPeople: PersonRef[];
}

export interface Coverage {
  eligiblePhotos: number;
  /** ข้อมูลรูปพร้อม (ภาพย่อ/วันถ่าย) */
  thumbnailsReady: number;
  thumbnailsPending: number;
  thumbnailsFailed: number;
  /** รูปที่มีใบหน้ายืนยันแล้วอย่างน้อยหนึ่งใบ */
  withConfirmedPeople: number;
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
  person: PersonRef | null;
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
  kind: 'catalog' | 'export';
  state: JobState;
  /** catalog: discovery | thumbnail | done; export: copy | done */
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
  estimatedBytes: number;
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
  items: ExportReportItem[];
}

// ───────────────────────── ย้ายข้อมูลเดิม (dry-run เท่านั้น) ─────────────────────────

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
  preview_export: [{ selection: SelectionSpec; destination: string }, ExportPlan];
  start_export: [{ planId: Id; requestId: Id }, JobSnapshot];
  get_export_report: [{ jobId: Id }, ExportReport];

  preview_legacy_import: [{ path?: string }, LegacyReport];
}

export type CommandName = keyof Commands;

/**
 * Asset ที่ Rust เสิร์ฟให้ `<img>` โดยรับเฉพาะ ID ทึบ (ไม่มี path จากฝั่ง UI)
 *   thumb/{photoId}            ภาพย่อ ~512px (JPEG, หมุนตาม EXIF แล้ว)
 *   photo/{photoId}            ไฟล์ต้นฉบับสำหรับตัวดูภาพ (404 เมื่อต้นทางไม่พร้อม)
 *   face/{faceId}              ภาพครอปใบหน้า ~256px
 *   draft/{previewId}/{draftId} รูปตัวอย่างในหน้าตรวจรายการก่อนเพิ่ม
 */
export type AssetKind = 'thumb' | 'photo' | 'face' | 'draft';

/** Event ที่ Rust ส่งหลัง commit สำเร็จ (ในแอป Tauri); UI ต้องทำงานได้แม้ไม่ได้รับ (ใช้ polling) */
export interface DataChangedEvent {
  dataRevision: number;
  scopes: ('events' | 'photos' | 'people' | 'jobs')[];
}
