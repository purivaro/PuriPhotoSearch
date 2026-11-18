import { NavLink } from 'react-router';
import { Folder, HardDrive, House, PanelLeft, ScanFace, Search, Settings, Users, type LucideIcon } from 'lucide-react';
import { Tooltip } from '@/components/ui/Overlay';
import { cn } from '@/lib/cn';
import { isTauri } from '@/lib/ipc/client';
import { useSettings } from '@/stores/settings';
import { Wordmark } from './Logo';
import { useBackgroundLocation } from './navigation';

interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
  /** path อื่นที่ถือว่าอยู่ในหมวดเดียวกัน */
  also?: string[];
  testId: string;
}

const NAV: NavItem[] = [
  { to: '/home', label: 'หน้าหลัก', icon: House, testId: 'nav-home' },
  { to: '/search', label: 'ค้นหารูป', icon: Search, testId: 'nav-search' },
  { to: '/people', label: 'บุคคล', icon: Users, testId: 'nav-people' },
  { to: '/events', label: 'กิจกรรม / โฟลเดอร์', icon: Folder, also: ['/import'], testId: 'nav-events' },
  { to: '/review', label: 'ตรวจสอบใบหน้า', icon: ScanFace, testId: 'nav-review' },
  { to: '/settings', label: 'ตั้งค่า', icon: Settings, testId: 'nav-settings' },
];

export function Sidebar({ collapsed }: { collapsed: boolean }) {
  // เมนูที่เลือกตามหน้าที่มาจริง แม้เปิดตัวดูภาพหรือ dialog ทับอยู่
  const background = useBackgroundLocation();
  const setCollapsed = useSettings((s) => s.setSidebarCollapsed);

  return (
    <nav
      aria-label="เมนูหลัก"
      className={cn(
        'flex h-full shrink-0 flex-col border-r border-line bg-surface transition-[width] duration-150',
        collapsed ? 'w-[72px]' : 'w-[236px]',
      )}
    >
      {/* แถบบนลากหน้าต่างได้ และเว้นที่ให้ปุ่มไฟจราจรของ macOS */}
      <div data-tauri-drag-region className={cn('shrink-0', isTauri ? 'h-11' : 'h-4')} />
      <div data-tauri-drag-region className={cn('flex shrink-0 items-center pb-5', collapsed ? 'justify-center px-2' : 'justify-between pr-2 pl-5')}>
        <Wordmark compact={collapsed} />
        {!collapsed && (
          <button
            type="button"
            aria-label="ยุบเมนู"
            title="ยุบเมนู"
            onClick={() => setCollapsed(true)}
            className="inline-flex size-8 items-center justify-center rounded-lg text-faint hover:bg-sunken hover:text-ink"
          >
            <PanelLeft className="size-[18px]" aria-hidden />
          </button>
        )}
      </div>

      <ul className={cn('flex flex-1 flex-col gap-1', collapsed ? 'px-2.5' : 'px-3')}>
        {collapsed && (
          <li>
            <Tooltip label="ขยายเมนู" side="right">
              <button
                type="button"
                aria-label="ขยายเมนู"
                onClick={() => setCollapsed(false)}
                className="mb-1 flex h-10 w-full items-center justify-center rounded-control text-faint hover:bg-sunken hover:text-ink"
              >
                <PanelLeft className="size-[18px]" aria-hidden />
              </button>
            </Tooltip>
          </li>
        )}
        {NAV.map((item) => {
          const active = [item.to, ...(item.also ?? [])].some((p) => background.pathname === p || background.pathname.startsWith(`${p}/`));
          const link = (
            <NavLink
              to={item.to}
              data-testid={item.testId}
              aria-current={active ? 'page' : undefined}
              aria-label={collapsed ? item.label : undefined}
              className={cn(
                'flex h-11 items-center rounded-control text-[15px] font-semibold transition-colors',
                collapsed ? 'justify-center' : 'gap-3 px-3',
                active ? 'bg-selected text-action' : 'text-ink hover:bg-sunken',
              )}
            >
              <item.icon className="size-[22px] shrink-0" strokeWidth={active ? 2.3 : 1.9} aria-hidden />
              {!collapsed && <span className="truncate">{item.label}</span>}
            </NavLink>
          );
          return (
            <li key={item.to}>
              {collapsed ? (
                <Tooltip label={item.label} side="right">
                  {link}
                </Tooltip>
              ) : (
                link
              )}
            </li>
          );
        })}
      </ul>

      <div className={cn('shrink-0 border-t border-line py-4', collapsed ? 'px-2.5' : 'mx-3 px-2')}>
        {collapsed ? (
          <Tooltip label="ข้อมูลอยู่ในเครื่อง ไม่มีการส่งรูปหรือชื่อบุคคลออกไปที่อื่น" side="right">
            <div className="flex h-9 items-center justify-center text-muted" role="img" aria-label="ข้อมูลอยู่ในเครื่อง">
              <HardDrive className="size-5" aria-hidden />
            </div>
          </Tooltip>
        ) : (
          <div className="flex items-center gap-2.5 text-sm text-muted" title="ไม่มีการส่งรูปหรือชื่อบุคคลออกไปที่อื่น">
            <HardDrive className="size-5 shrink-0" aria-hidden />
            ข้อมูลอยู่ในเครื่อง
          </div>
        )}
      </div>
    </nav>
  );
}
