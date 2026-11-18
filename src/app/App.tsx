import { useEffect, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { HashRouter, Navigate, Route, Routes, useLocation } from 'react-router';
import { Toaster } from 'sonner';
import { ErrorState, LoadingBlock } from '@/components/ui/Feedback';
import { TooltipProvider } from '@/components/ui/Overlay';
import { EventDetailPage } from '@/features/events/EventDetailPage';
import { EventsPage } from '@/features/events/EventsPage';
import { HomePage } from '@/features/home/HomePage';
import { ImportReviewPage } from '@/features/import/ImportReviewPage';
import { ActivityPage } from '@/features/jobs/ActivityPage';
import { PeoplePage } from '@/features/people/PeoplePage';
import { PersonDetailPage } from '@/features/people/PersonDetailPage';
import { PhotoViewer } from '@/features/photos/PhotoViewer';
import { ReviewPage } from '@/features/review/ReviewPage';
import { SearchPage } from '@/features/search/SearchPage';
import { SettingsPage } from '@/features/settings/SettingsPage';
import { onFolderDropped } from '@/lib/ipc/client';
import { useJobWatcher, useSummary } from '@/lib/queries';
import { resolveTheme, useSettings } from '@/stores/settings';
import { useUi } from '@/stores/ui';
import { AppShell } from './AppShell';
import { CommandPalette } from './CommandPalette';
import { FolderChooserProvider } from './FolderChooser';
import { NotFoundPage } from './NotFoundPage';
import { RealLocationProvider, useBackgroundLocation, useStartImport, ViewerProvider } from './navigation';
import { WelcomePage } from './WelcomePage';

const queryClient = new QueryClient({
  defaultOptions: {
    // ไม่ลองซ้ำเงียบ ๆ: แสดงสาเหตุและปุ่ม "ลองใหม่" ให้ผู้ใช้เห็นแทน skeleton ค้าง
    queries: { retry: false, staleTime: 5_000, refetchOnWindowFocus: true },
    mutations: { retry: false },
  },
});

/** เปิดแอป: มีกิจกรรมแล้วไปหน้าหลัก ยังไม่มีไปหน้าเริ่มต้นใช้งาน */
function Boot() {
  const summary = useSummary();
  if (summary.isPending) {
    return (
      <div className="flex h-full items-center justify-center" data-tauri-drag-region>
        <LoadingBlock label="กำลังเปิดคลังภาพ…" />
      </div>
    );
  }
  if (summary.isError) {
    return (
      <div className="flex h-full items-center justify-center" data-tauri-drag-region>
        <ErrorState error={summary.error} title="เปิดคลังภาพไม่สำเร็จ" onRetry={() => void summary.refetch()} />
      </div>
    );
  }
  return <Navigate to={summary.data.eventCount === 0 ? '/welcome' : '/home'} replace />;
}

/** ส่วนที่ต้องอยู่ใน router: เฝ้างาน, แป้นลัดทั้งแอป, รับโฟลเดอร์ที่ลากมาวาง */
function AppRuntime() {
  useJobWatcher();
  const startImport = useStartImport();
  const setPaletteOpen = useUi((s) => s.setPaletteOpen);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
      const key = event.key.toLowerCase();
      if (key === 'k') {
        event.preventDefault();
        setPaletteOpen(!useUi.getState().paletteOpen);
      } else if (key === 'f') {
        // ⌘F: ไปช่องค้นหาของหน้าปัจจุบันถ้ามี ไม่มีก็เปิดค้นทั้งคลัง
        event.preventDefault();
        const local = document.querySelector<HTMLInputElement>('[data-page-search]');
        if (local && !local.closest('[inert]')) local.focus();
        else setPaletteOpen(true);
      } else if (key === 'o') {
        event.preventDefault();
        void startImport();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [setPaletteOpen, startImport]);

  useEffect(() => {
    let dispose: (() => void) | undefined;
    let cancelled = false;
    // ลากโฟลเดอร์มาวาง = ใช้ขั้นตรวจรายการชุดเดียวกับการเลือกจาก picker ไม่ข้ามหน้าตรวจสอบ
    void onFolderDropped((path) => void startImport(path)).then((off) => {
      if (cancelled) off();
      else dispose = off;
    });
    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [startImport]);

  return null;
}

/** วางตัวดูภาพทับพื้นที่เนื้อหาของ AppShell (เมนูซ้ายยังอยู่) โดยยังอยู่ใต้ router ตำแหน่งจริง */
function ViewerPortal({ children }: { children: ReactNode }) {
  const [host, setHost] = useState<HTMLElement | null>(null);
  useEffect(() => {
    // #app-main ถูกสร้างโดย AppShell ใน render รอบเดียวกัน จึงหาได้หลัง mount
    setHost(document.getElementById('app-main'));
  }, []);
  return host ? createPortal(children, host) : null;
}

function AppRoutes() {
  const location = useLocation();
  const background = useBackgroundLocation();
  const viewerOpen = location.pathname.startsWith('/photos/');
  return (
    <RealLocationProvider value={location}>
      <Routes location={background}>
        <Route path="/" element={<Boot />} />
        <Route path="/welcome" element={<WelcomePage />} />
        <Route element={<AppShell />}>
          <Route path="/home" element={<HomePage />} />
          <Route path="/search" element={<SearchPage />} />
          <Route path="/people" element={<PeoplePage />} />
          <Route path="/people/:personId" element={<PersonDetailPage />} />
          <Route path="/events" element={<EventsPage />} />
          <Route path="/events/:eventId" element={<EventDetailPage />} />
          <Route path="/import/review" element={<ImportReviewPage />} />
          <Route path="/review" element={<ReviewPage />} />
          <Route path="/activity" element={<ActivityPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="*" element={<NotFoundPage />} />
        </Route>
      </Routes>
      {viewerOpen && (
        <ViewerPortal>
          <Routes>
            <Route path="/photos/:photoId" element={<PhotoViewer />} />
          </Routes>
        </ViewerPortal>
      )}
    </RealLocationProvider>
  );
}

export function App() {
  const theme = useSettings((s) => s.theme);
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={350}>
        <HashRouter>
          <FolderChooserProvider>
            <ViewerProvider>
              <AppRuntime />
              <AppRoutes />
              <CommandPalette />
            </ViewerProvider>
          </FolderChooserProvider>
        </HashRouter>
        <Toaster
          position="bottom-center"
          theme={resolveTheme(theme)}
          closeButton
          toastOptions={{
            style: { fontFamily: 'var(--font-sans)', fontSize: '14px', borderRadius: '12px' },
            actionButtonStyle: { background: 'var(--action)', color: 'var(--on-action)', fontWeight: 600 },
          }}
        />
      </TooltipProvider>
    </QueryClientProvider>
  );
}
