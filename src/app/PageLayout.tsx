import { useEffect, useLayoutEffect, useRef, type ReactNode } from 'react';
import { Link, useNavigationType } from 'react-router';
import { Activity, ChevronRight, FolderPlus, Search } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Feedback';
import { Tooltip } from '@/components/ui/Overlay';
import { jobProgressText } from '@/features/jobs/jobText';
import { cn } from '@/lib/cn';
import { isJobActive, useActiveJobs } from '@/lib/queries';
import { useUi } from '@/stores/ui';
import { useBackgroundLocation, useStartImport } from './navigation';
import { MOD_KEY } from '@/lib/platform';

const scrollMemory = new Map<string, number>();

/**
 * โครงของหนึ่งหน้า: คอลัมน์เนื้อหาที่เลื่อนได้ + แผงด้านขวา (ถ้ามี)
 * จำตำแหน่งเลื่อนต่อ history entry แล้วคืนให้เมื่อย้อนกลับมา
 */
export function Page({ children, aside, className }: { children: ReactNode; aside?: ReactNode; className?: string }) {
  const location = useBackgroundLocation();
  const navigationType = useNavigationType();
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const key = location.key;

  useLayoutEffect(() => {
    const scroller = scrollRef.current;
    const content = contentRef.current;
    if (!scroller || !content) return;
    const target = navigationType === 'POP' ? scrollMemory.get(key) : undefined;
    let observer: ResizeObserver | undefined;
    let timer: number | undefined;
    if (target) {
      // เนื้อหาโหลดทีหลัง: ลองคืนตำแหน่งทุกครั้งที่ความสูงเปลี่ยน จนถึงเป้าหรือหมดเวลา
      const restore = () => {
        scroller.scrollTop = target;
        if (scroller.scrollTop >= target - 2) observer?.disconnect();
      };
      observer = new ResizeObserver(restore);
      observer.observe(content);
      restore();
      timer = window.setTimeout(() => observer?.disconnect(), 2500);
    }
    const remember = () => scrollMemory.set(key, scroller.scrollTop);
    scroller.addEventListener('scroll', remember, { passive: true });
    return () => {
      scroller.removeEventListener('scroll', remember);
      observer?.disconnect();
      if (timer) window.clearTimeout(timer);
    };
  }, [key, navigationType]);

  return (
    <div className="flex h-full min-w-0">
      <div ref={scrollRef} data-page-scroll className="scroll-thin min-w-0 flex-1 overflow-y-auto">
        <div ref={contentRef} className={cn('mx-auto w-full max-w-[1520px] px-7 pt-1 pb-10', className)}>
          {children}
        </div>
      </div>
      {aside}
    </div>
  );
}

/** ช่องค้นหาที่เปิด ⌘K (ค้นคน กิจกรรม หรือชื่อไฟล์) */
export function SearchTrigger({ className, large }: { className?: string; large?: boolean }) {
  const openPalette = useUi((s) => s.setPaletteOpen);
  return (
    <button
      type="button"
      data-testid="global-search-trigger"
      onClick={() => openPalette(true)}
      className={cn(
        'flex min-w-0 items-center gap-2.5 rounded-control border border-line-strong bg-surface text-left text-muted shadow-card transition-colors hover:border-brand-line',
        large ? 'h-12 px-4 text-[15px]' : 'h-10 px-3 text-sm',
        className,
      )}
    >
      <Search className={cn('shrink-0', large ? 'size-5' : 'size-[18px]')} aria-hidden />
      <span className="min-w-0 flex-1 truncate">ค้นหาคน กิจกรรม หรือชื่อไฟล์…</span>
      <kbd className="shrink-0 rounded-md border border-line bg-sunken px-1.5 py-0.5 font-sans text-[11.5px] font-semibold text-muted">{MOD_KEY} K</kbd>
    </button>
  );
}

export function AddFolderButton({ size = 'md', className }: { size?: 'md' | 'lg'; className?: string }) {
  const startImport = useStartImport();
  return (
    <Button
      variant="primary"
      size={size}
      data-testid="add-folder-button"
      icon={<FolderPlus className="size-[18px]" aria-hidden />}
      onClick={() => void startImport()}
      className={className}
    >
      เพิ่มโฟลเดอร์กิจกรรม
    </Button>
  );
}

/** สถานะงานเบื้องหลังส่วนกลาง → ศูนย์งานและประวัติ */
export function JobIndicator() {
  const { data } = useActiveJobs();
  const active = (data ?? []).filter(isJobActive);
  const first = active[0];
  if (!first) {
    return (
      <Tooltip label="งานและประวัติ">
        <Link
          to="/activity"
          aria-label="งานและประวัติ"
          data-testid="job-indicator"
          className="inline-flex size-10 shrink-0 items-center justify-center rounded-control border border-line-strong bg-surface text-muted shadow-card hover:bg-sunken hover:text-ink"
        >
          <Activity className="size-[18px]" aria-hidden />
        </Link>
      </Tooltip>
    );
  }
  const text = active.length > 1 ? `กำลังทำ ${active.length} งาน` : jobProgressText(first);
  return (
    <Link
      to="/activity"
      data-testid="job-indicator"
      className="inline-flex h-10 max-w-[280px] shrink-0 items-center gap-2 rounded-control border border-brand-line bg-brand-soft px-3 text-[13px] font-semibold text-action hover:brightness-[0.98]"
    >
      <Spinner className="size-4 shrink-0" />
      <span className="truncate" aria-live="off">
        {text}
      </span>
    </Link>
  );
}

export interface Crumb {
  label: string;
  to?: string;
}

interface PageHeaderProps {
  title?: ReactNode;
  subtitle?: ReactNode;
  breadcrumb?: Crumb[];
  /** ปุ่มเฉพาะหน้า วางซ้ายของเครื่องมือกลาง */
  actions?: ReactNode;
  /** ซ่อนปุ่มเพิ่มโฟลเดอร์ (เช่นระหว่างขั้นตอนเพิ่มโฟลเดอร์เอง) */
  hideAddFolder?: boolean;
}

/** แถบบนของหน้า: ชื่อหน้าชัด + ค้นหา/เพิ่มโฟลเดอร์/สถานะงาน; พื้นที่ว่างลากหน้าต่างได้ */
export function PageHeader({ title, subtitle, breadcrumb, actions, hideAddFolder }: PageHeaderProps) {
  return (
    <header data-tauri-drag-region className="@container/header sticky top-0 z-20 -mx-7 mb-5 flex min-h-[72px] flex-wrap items-center gap-x-4 gap-y-2 bg-canvas/95 px-7 pt-4 pb-3 backdrop-blur-sm">
      <div data-tauri-drag-region className="min-w-[220px] flex-1">
        {breadcrumb && breadcrumb.length > 0 && (
          <nav aria-label="ตำแหน่งหน้า" className="flex items-center gap-1.5 text-[15px] text-muted">
            {breadcrumb.map((crumb, index) => (
              <span key={`${crumb.label}-${index}`} className="flex min-w-0 items-center gap-1.5">
                {index > 0 && <ChevronRight className="size-4 shrink-0 text-faint" aria-hidden />}
                {crumb.to ? (
                  <Link to={crumb.to} className="shrink-0 rounded hover:text-action hover:underline">
                    {crumb.label}
                  </Link>
                ) : (
                  <span className="truncate font-semibold text-ink">{crumb.label}</span>
                )}
              </span>
            ))}
          </nav>
        )}
        {title && <h1 className="truncate text-[26px] font-extrabold tracking-tight text-ink">{title}</h1>}
        {subtitle && <p className="truncate text-sm text-muted">{subtitle}</p>}
      </div>
      <div className="flex shrink-0 items-center gap-2.5">
        {actions}
        {/* ช่องค้นหาแสดงเมื่อแถบบนกว้างพอ (ดูจากความกว้างของแถบเอง ไม่ใช่ของหน้าต่าง เพราะอาจมีแผงขวาเปิดอยู่); ⌘K ใช้ได้เสมอ */}
        <SearchTrigger className="hidden w-[280px] @[900px]/header:flex @[1100px]/header:w-[360px]" />
        {!hideAddFolder && <AddFolderButton />}
        <JobIndicator />
      </div>
    </header>
  );
}

/** หัวข้อส่วนย่อยในหน้า */
export function SectionTitle({ title, hint, action, className }: { title: string; hint?: ReactNode; action?: ReactNode; className?: string }) {
  return (
    <div className={cn('mb-3 flex items-end justify-between gap-3', className)}>
      <div className="min-w-0">
        <h2 className="text-xl font-bold text-ink">{title}</h2>
        {hint && <p className="text-[13.5px] text-muted">{hint}</p>}
      </div>
      {action}
    </div>
  );
}

/** ใช้ title ของหน้าต่าง/แท็บให้ตรงกับหน้าที่เปิด */
export function useDocumentTitle(title: string | null | undefined) {
  useEffect(() => {
    document.title = title ? `${title} — Puri Photo Search` : 'Puri Photo Search';
  }, [title]);
}
