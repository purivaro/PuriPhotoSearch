import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { toast } from 'sonner';
import { useViewer } from '@/app/navigation';
import { ErrorState, LoadingBlock } from '@/components/ui/Feedback';
import { ExportDialog } from '@/features/export/ExportDialog';
import type { Coverage } from '@/lib/ipc/types';
import { usePhotoList, type PhotoQuery } from '@/lib/queries';
import { PhotoGrid } from './PhotoGrid';
import { SelectionBar } from './SelectionBar';
import { EMPTY_SELECTION, selectionCount, toSelectionSpec, type Selection } from './selection';

export interface PhotoResultsMeta {
  total: number;
  loaded: number;
  coverage: Coverage | null;
  /** กำลังโหลดผลของเงื่อนไขใหม่/รีเฟรช */
  refreshing: boolean;
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
}

/**
 * ผลรูปหนึ่งชุด: โหลด/ผิดพลาด/ว่าง → grid → แถบเลือกหลายรูป → ส่งออก → เปิดตัวดูภาพตามลำดับผลเดิม
 * ใช้ร่วมกันในหน้ากิจกรรม บุคคล และค้นหา เพื่อให้พฤติกรรมการเลือกและเปิดรูปตรงกันทุกหน้า
 */
export function PhotoResults({ query, label, originLabel, showEvent, selectionKey, empty, summary }: PhotoResultsProps) {
  const list = usePhotoList(query);
  const viewer = useViewer();
  const [selection, setSelection] = useState<Selection>(EMPTY_SELECTION);
  const [exporting, setExporting] = useState(false);
  const selectionRef = useRef(selection);
  selectionRef.current = selection;

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

  const meta: PhotoResultsMeta = {
    total: list.total,
    loaded: list.items.length,
    coverage: list.coverage,
    refreshing: list.isFetching && !list.isFetchingNextPage,
  };

  if (list.isPending) {
    return (
      <>
        {summary?.(meta)}
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
        {summary?.(meta)}
        {empty}
      </>
    );
  }

  const ids = list.items.map((p) => p.id);
  const count = selectionCount(selection);
  return (
    <>
      {summary?.(meta)}
      <PhotoGrid
        items={list.items}
        label={label}
        selection={selection}
        onSelectionChange={setSelection}
        onOpen={(photo) => viewer.open(photo.id, { ids, originLabel, total: list.total, owner, requestMore: list.hasNextPage ? () => requestMore.current() : undefined })}
        hasMore={Boolean(list.hasNextPage)}
        loadingMore={list.isFetchingNextPage}
        onLoadMore={() => {
          if (list.hasNextPage && !list.isFetchingNextPage) void list.fetchNextPage();
        }}
        showEvent={showEvent}
        total={list.total}
      />
      {list.isError && <ErrorState compact error={list.error} title="โหลดรูปเพิ่มไม่สำเร็จ" onRetry={() => void list.fetchNextPage()} />}
      <SelectionBar selection={selection} onSelectionChange={setSelection} loadedIds={ids} total={list.total} query={query} onExport={() => setExporting(true)} />
      <ExportDialog open={exporting} onOpenChange={setExporting} selection={toSelectionSpec(selection)} count={count} />
    </>
  );
}
