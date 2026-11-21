import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { toast } from 'sonner';
import { formatCount } from '@/lib/format';
import { useViewer } from '@/app/navigation';
import { ErrorState, LoadingBlock } from '@/components/ui/Feedback';
import { ExportDialog } from '@/features/export/ExportDialog';
import { ShrinkDialog } from '@/features/export/ShrinkDialog';
import { ApplyPresetDialog } from './ApplyPresetDialog';
import { GenerateDialog } from '@/features/assistant/GenerateDialog';
import { MatchStyleDialog } from '@/features/assistant/MatchStyleDialog';
import { PasteEditDialog } from './PasteEditDialog';
import type { Coverage, PhotoFlag, PhotoResult } from '@/lib/ipc/types';
import { usePhotoList, type PhotoQuery } from '@/lib/queries';
import { PhotoGrid } from './PhotoGrid';
import { FLAG_LABEL, useSetPhotoFlags } from './PhotoFlags';
import { SelectionBar } from './SelectionBar';
import { ThumbSizeSlider } from './ThumbSizeSlider';
import { EMPTY_SELECTION, isSelected, selectionCount, toSelectionSpec, type Selection } from './selection';

export interface PhotoResultsMeta {
  total: number;
  loaded: number;
  coverage: Coverage | null;
  /** กำลังโหลดผลของเงื่อนไขใหม่/รีเฟรช */
  refreshing: boolean;
  /** แต่งทีละรูป: เปิดรูปแรกของผลในหน้าแต่งภาพ แล้วบันทึกไปทีละรูปตามลำดับนี้ (null = ไม่มีรูป) */
  startEditing: (() => void) | null;
}

interface PhotoResultsProps {
  query: PhotoQuery;
  /** ชื่อชุดผลสำหรับ screen reader */
  label: string;
  /** ข้อความปุ่มย้อนกลับในตัวดูภาพ */
  originLabel: string;
  showEvent?: boolean;
  /** ตัวแทนของ "เงื่อนไข": เมื่อเปลี่ยน ชุดที่เลือกไว้จะถูกล้างพร้อมแจ้งผู้ใช้ */
  selectionKey: string;
  /** แสดงเมื่อไม่มีรูปตามเงื่อนไข */
  empty: ReactNode;
  /** แถวสรุปเหนือ grid (จำนวนผล, ความครบถ้วน) */
  summary?: (meta: PhotoResultsMeta) => ReactNode;
  /** ปุ่มเพิ่มในแถบเลือกหลายรูป; `photos` คือรูปที่ติ๊กเลือกเอง หรือ null เมื่อเลือกแบบ "ผลทั้งหมด" (ไม่ได้โหลดครบ) */
  selectionActions?: (ctx: { photos: PhotoResult[] | null; clear: () => void }) => ReactNode;
  /** แสดงแถบปรับขนาดรูปในแถวหัว (ปิดได้เมื่อหน้านั้นวางแถบไว้ในแถบเครื่องมือของตัวเอง) */
  sizeControl?: boolean;
  /** "แต่งรูปของคนนี้": เมื่อผลของเงื่อนไขปัจจุบันโหลดเสร็จ เปิดรูปแรกในหน้าแต่งภาพ แล้วแจ้งว่าเปิดได้หรือไม่ (ไม่มีรูปเหลือ) */
  autoEdit?: boolean;
  onAutoEditHandled?: (opened: boolean) => void;
}

/**
 * ผลรูปหนึ่งชุด: โหลด/ผิดพลาด/ว่าง → grid → แถบเลือกหลายรูป → ส่งออก → เปิดตัวดูภาพตามลำดับผลเดิม
 * ใช้ร่วมกันในหน้ากิจกรรม บุคคล และค้นหา เพื่อให้พฤติกรรมการเลือกและเปิดรูปตรงกันทุกหน้า
 */
export function PhotoResults({ query, label, originLabel, showEvent, selectionKey, empty, summary, selectionActions, sizeControl = true, autoEdit, onAutoEditHandled }: PhotoResultsProps) {
  const list = usePhotoList(query);
  const viewer = useViewer();
  const [selection, setSelection] = useState<Selection>(EMPTY_SELECTION);
  const [exporting, setExporting] = useState(false);
  const [shrinking, setShrinking] = useState(false);
  const [pasting, setPasting] = useState(false);
  const [applyingPreset, setApplyingPreset] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [matching, setMatching] = useState(false);
  const selectionRef = useRef(selection);
  selectionRef.current = selection;
  // คัดรูปที่เลือก (คีย์ P / X / U ใน grid หรือเมนูในแถบเลือก) — ที่เลือกไว้ยังอยู่ ส่งออกต่อได้
  const setFlags = useSetPhotoFlags();
  const flagSelected = (flag: PhotoFlag | null) => {
    const count = selectionCount(selectionRef.current);
    if (count === 0) return;
    setFlags.mutate(
      { selection: toSelectionSpec(selectionRef.current), flag },
      { onSuccess: () => toast.success(flag ? `${FLAG_LABEL[flag]} ${formatCount(count)} รูปแล้ว` : `ล้างการคัด ${formatCount(count)} รูปแล้ว`, { id: 'photo-flags' }) },
    );
  };

  const lastKey = useRef(selectionKey);
  useEffect(() => {
    if (lastKey.current === selectionKey) return;
    lastKey.current = selectionKey;
    if (selectionCount(selectionRef.current) > 0) {
      // ไม่ส่งออกรูปที่ผู้ใช้มองไม่เห็นแล้วโดยไม่บอก
      setSelection(EMPTY_SELECTION);
      toast.info('ล้างรูปที่เลือกไว้แล้ว เพราะเงื่อนไขของผลเปลี่ยน');
    }
  }, [selectionKey]);

  // ตัวดูภาพที่เปิดจากชุดผลนี้เดินต่อได้เกินหน้าที่โหลดไว้: โหลดเพิ่มแล้วส่งลำดับใหม่ให้
  const owner = useId();
  const { hasNextPage, isFetchingNextPage, fetchNextPage, items, total } = list;
  const requestMore = useRef<() => void>(() => {});
  requestMore.current = () => {
    if (hasNextPage && !isFetchingNextPage) void fetchNextPage();
  };
  const sync = viewer.sync;
  useEffect(() => {
    sync(owner, { ids: items.map((p) => p.id), total, requestMore: hasNextPage ? () => requestMore.current() : undefined });
  }, [sync, owner, items, total, hasNextPage]);

  // เปิดรูปตามลำดับของผลชุดนี้ (edit = เปิดมาที่หน้าแต่งภาพ แล้ว "บันทึก แล้วไปรูปถัดไป" ได้ทั้งชุด)
  const openPhoto = (photo: PhotoResult, edit = false) =>
    viewer.open(photo.id, { ids: items.map((p) => p.id), originLabel, total, owner, requestMore: hasNextPage ? () => requestMore.current() : undefined, edit });
  const first = items[0];
  const startEditing = first ? () => openPhoto(first, true) : null;

  // "แต่งรูปของคนนี้": รอผลของเงื่อนไขใหม่ (ไม่ใช่ข้อมูลเก่าในแคช) แล้วเปิดรูปแรก
  const handled = useRef(onAutoEditHandled);
  handled.current = onAutoEditHandled;
  const settled = !list.isPending && !list.isFetching;
  useEffect(() => {
    if (!autoEdit || !settled) return;
    startEditing?.();
    handled.current?.(Boolean(startEditing));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoEdit, settled]);

  const meta: PhotoResultsMeta = {
    total: list.total,
    loaded: list.items.length,
    coverage: list.coverage,
    refreshing: list.isFetching && !list.isFetchingNextPage,
    startEditing,
  };

  // แถวหัว: สรุปผลของแต่ละหน้า + แถบปรับขนาดรูป (แสดงเมื่อมีรูปให้ปรับ)
  const header = (withSize: boolean) => {
    const content = summary?.(meta);
    if (!withSize) return content ? <div className="mb-3">{content}</div> : null;
    return (
      <div className="mb-3 flex items-center gap-x-4 gap-y-2">
        <div className="min-w-0 flex-1">{content}</div>
        <ThumbSizeSlider />
      </div>
    );
  };

  if (list.isPending) {
    return (
      <>
        {header(false)}
        <LoadingBlock label="กำลังโหลดรูป…" />
      </>
    );
  }
  if (list.isError && list.items.length === 0) {
    return <ErrorState error={list.error} title="โหลดรูปไม่สำเร็จ" onRetry={() => void list.refetch()} />;
  }
  if (list.items.length === 0) {
    return (
      <>
        {header(false)}
        {empty}
      </>
    );
  }

  const ids = list.items.map((p) => p.id);
  const count = selectionCount(selection);
  // รูปแรกที่เลือกไว้ (ในลำดับที่เห็น) ใช้เป็นตัวอย่างลายน้ำ
  const previewPhotoId = list.items.find((p) => isSelected(selection, p.id))?.id ?? null;
  return (
    <>
      {header(sizeControl)}
      <PhotoGrid
        items={list.items}
        label={label}
        selection={selection}
        onSelectionChange={setSelection}
        onOpen={(photo) => openPhoto(photo)}
        hasMore={Boolean(list.hasNextPage)}
        loadingMore={list.isFetchingNextPage}
        onLoadMore={() => {
          if (list.hasNextPage && !list.isFetchingNextPage) void list.fetchNextPage();
        }}
        showEvent={showEvent}
        total={list.total}
        onFlag={flagSelected}
      />
      {list.isError && <ErrorState compact error={list.error} title="โหลดรูปเพิ่มไม่สำเร็จ" onRetry={() => void list.fetchNextPage()} />}
      <SelectionBar
        selection={selection}
        onSelectionChange={setSelection}
        loadedIds={ids}
        total={list.total}
        query={query}
        onExport={() => setExporting(true)}
        onShrink={() => setShrinking(true)}
        onPasteEdit={() => setPasting(true)}
        onApplyPreset={() => setApplyingPreset(true)}
        onGenerate={() => setGenerating(true)}
        onMatchStyle={() => setMatching(true)}
        onFlag={flagSelected}
        actions={selectionActions?.({
          photos: selection.kind === 'explicit' ? list.items.filter((p) => selection.ids.has(p.id)) : null,
          clear: () => setSelection(EMPTY_SELECTION),
        })}
      />
      <ExportDialog open={exporting} onOpenChange={setExporting} selection={toSelectionSpec(selection)} count={count} previewPhotoId={previewPhotoId} />
      <ShrinkDialog open={shrinking} onOpenChange={setShrinking} selection={toSelectionSpec(selection)} count={count} />
      <PasteEditDialog open={pasting} onOpenChange={setPasting} selection={toSelectionSpec(selection)} count={count} />
      <ApplyPresetDialog open={applyingPreset} onOpenChange={setApplyingPreset} selection={toSelectionSpec(selection)} count={count} />
      <GenerateDialog open={generating} onOpenChange={setGenerating} selection={toSelectionSpec(selection)} count={count} />
      <MatchStyleDialog open={matching} onOpenChange={setMatching} selection={toSelectionSpec(selection)} count={count} />
    </>
  );
}
