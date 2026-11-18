import { useEffect, useState } from 'react';
import { Outlet } from 'react-router';
import { useSettings } from '@/stores/settings';
import { useViewerOpen } from './navigation';
import { Sidebar } from './Sidebar';

const NARROW = '(max-width: 1180px)';

function useNarrowWindow(): boolean {
  const [narrow, setNarrow] = useState(() => window.matchMedia(NARROW).matches);
  useEffect(() => {
    const media = window.matchMedia(NARROW);
    const onChange = () => setNarrow(media.matches);
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  }, []);
  return narrow;
}

/** โครงแอป: เมนูซ้าย + เนื้อหา; ตัวดูภาพวางทับเนื้อหาโดยหน้าที่มายังอยู่ข้างหลัง */
export function AppShell() {
  const narrow = useNarrowWindow();
  const preference = useSettings((s) => s.sidebarCollapsed);
  const viewerOpen = useViewerOpen();

  return (
    <div className="flex h-full bg-canvas">
      <Sidebar collapsed={preference ?? narrow} />
      <main id="app-main" className="relative min-w-0 flex-1">
        {/* ระหว่างเปิดตัวดูภาพ หน้าเบื้องหลังยังอยู่ แต่ไม่รับ focus/คลิก */}
        <div className="h-full" inert={viewerOpen || undefined} aria-hidden={viewerOpen || undefined}>
          <Outlet />
        </div>
        {/* ตัวดูภาพถูกวางลงใน #app-main ผ่าน portal จาก App (ต้องเห็นตำแหน่งจริงของ router) */}
      </main>
    </div>
  );
}
