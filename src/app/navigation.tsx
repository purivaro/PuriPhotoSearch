import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from 'react';
import { useLocation, useNavigate, type Location } from 'react-router';
import type { Id } from '@/lib/ipc/types';
import { useFolderChooser } from './FolderChooser';

// ── เพิ่มโฟลเดอร์ ───────────────────────────────────────────────

export interface ImportLocationState {
  path: string;
  /** เปลี่ยนทุกครั้งที่เลือกโฟลเดอร์ เพื่อให้ตรวจรายการใหม่แม้เลือก path เดิม */
  nonce: string;
}

/** เริ่มขั้นที่ 1 ของการเพิ่มโฟลเดอร์: เลือก (หรือรับ path ที่ลากมาวาง) แล้วไปหน้าตรวจรายการ */
export function useStartImport() {
  const choose = useFolderChooser();
  const navigate = useNavigate();
  return useCallback(
    async (droppedPath?: string) => {
      const path =
        droppedPath ??
        (await choose({
          title: 'เลือกโฟลเดอร์กิจกรรม',
          hint: 'เลือกโฟลเดอร์งานเดียว หรือโฟลเดอร์ที่รวมหลายงานก็ได้',
        }));
      if (!path) return;
      const state: ImportLocationState = { path, nonce: crypto.randomUUID() };
      navigate('/import/review', { state });
    },
    [choose, navigate],
  );
}

// ── ตัวดูภาพ (overlay ที่คงหน้าที่มาไว้ข้างหลัง) ─────────────────────

export interface ViewerSession {
  /** ลำดับรูปตามผลที่ผู้ใช้กำลังดู ใช้กับปุ่มก่อนหน้า/ถัดไป (เฉพาะที่โหลดแล้ว) */
  ids: Id[];
  /** ข้อความปุ่มย้อนกลับ เช่น "กลับไปกิจกรรม" */
  originLabel: string;
  /** จำนวนรูปทั้งหมดของชุดผล (อาจมากกว่าที่โหลดแล้ว) */
  total?: number;
  /** เจ้าของชุดผล ใช้ให้หน้าที่มาอัปเดตลำดับเมื่อโหลดรูปเพิ่ม */
  owner?: string;
  /** ขอให้หน้าที่มาโหลดรูปหน้าถัดไป (มีเมื่อยังโหลดไม่ครบ) */
  requestMore?: () => void;
  /** เปิดมาที่หน้าแต่งภาพเลย (แต่งทีละรูป — DECISIONS ข้อ 32) ตัวดูภาพใช้ครั้งเดียวแล้วล้าง */
  edit?: boolean;
}

export interface ViewerLocationState {
  backgroundLocation: Location;
  /** true เมื่อเปิดจากในแอป: ปิดด้วยการย้อน history หนึ่งขั้น */
  pushed: boolean;
}

interface ViewerContextValue {
  session: React.RefObject<ViewerSession>;
  /** เปลี่ยนทุกครั้งที่ลำดับรูปของชุดผลถูกอัปเดต */
  version: number;
  open: (photoId: Id, session: ViewerSession) => void;
  /** หน้าที่มาเรียกเมื่อชุดผลของตนโหลดรูปเพิ่ม */
  sync: (owner: string, patch: Pick<ViewerSession, 'ids' | 'total' | 'requestMore'>) => void;
  /** คืน focus ไปยังจุดที่เปิดตัวดูภาพ (เรียกเมื่อปิด) */
  restoreFocus: () => void;
}

const ViewerContext = createContext<ViewerContextValue | null>(null);

export function ViewerProvider({ children }: { children: ReactNode }) {
  const navigate = useNavigate();
  const location = useLocation();
  const session = useRef<ViewerSession>({ ids: [], originLabel: 'กลับ' });
  const locationRef = useRef(location);
  locationRef.current = location;
  const returnFocus = useRef<HTMLElement | null>(null);
  const [version, setVersion] = useState(0);

  const open = useCallback(
    (photoId: Id, next: ViewerSession) => {
      session.current = next;
      const current = locationRef.current;
      const existing = current.state as ViewerLocationState | null;
      if (!existing?.backgroundLocation) returnFocus.current = document.activeElement as HTMLElement | null;
      const state: ViewerLocationState = {
        backgroundLocation: existing?.backgroundLocation ?? current,
        pushed: true,
      };
      navigate(`/photos/${photoId}`, { state, replace: Boolean(existing?.backgroundLocation) });
    },
    [navigate],
  );

  const restoreFocus = useCallback(() => {
    const el = returnFocus.current;
    returnFocus.current = null;
    if (el && document.contains(el)) el.focus({ preventScroll: true });
  }, []);

  const sync = useCallback<ViewerContextValue['sync']>((owner, patch) => {
    if (session.current.owner !== owner) return;
    session.current = { ...session.current, ...patch };
    setVersion((v) => v + 1);
  }, []);

  const value = useMemo(() => ({ session, version, open, sync, restoreFocus }), [version, open, sync, restoreFocus]);
  return <ViewerContext.Provider value={value}>{children}</ViewerContext.Provider>;
}

export function useViewer(): ViewerContextValue {
  const value = useContext(ViewerContext);
  if (!value) throw new Error('useViewer ต้องอยู่ใน ViewerProvider');
  return value;
}

const RealLocationContext = createContext<Location | null>(null);

/**
 * หน้าพื้นหลังถูก render ด้วย `<Routes location={background}>` ซึ่งทำให้ `useLocation()` ข้างในเห็นแต่ตำแหน่งพื้นหลัง
 * ส่วนที่ต้องรู้ตำแหน่งจริง (เช่น "ตัวดูภาพเปิดอยู่หรือไม่") ใช้ค่านี้แทน
 */
export const RealLocationProvider = RealLocationContext.Provider;

export function useRealLocation(): Location {
  const fallback = useLocation();
  return useContext(RealLocationContext) ?? fallback;
}

export function useViewerOpen(): boolean {
  return useRealLocation().pathname.startsWith('/photos/');
}

/** ตำแหน่งที่ใช้ตัดสินเมนูที่เลือกและหน้าพื้นหลัง: ถ้าเปิดตัวดูภาพอยู่ให้ใช้หน้าที่มา */
export function useBackgroundLocation(): Location {
  const location = useLocation();
  const state = location.state as ViewerLocationState | null;
  return useMemo(() => {
    if (state?.backgroundLocation) return state.backgroundLocation;
    // เปิดลิงก์รูปตรง ๆ (เช่นหลัง reload): ใช้หน้าหลักเป็นพื้นหลัง
    if (location.pathname.startsWith('/photos/')) {
      return { ...location, pathname: '/home', search: '', hash: '', state: null, key: 'viewer-direct' };
    }
    return location;
  }, [location, state]);
}
