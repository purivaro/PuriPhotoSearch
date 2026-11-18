import logoUrl from '@/assets/logo.png';
import { cn } from '@/lib/cn';

/** น้องโพลารอยด์ถือแว่นขยาย — มาสคอตของ Puri Photo Search (ต้นฉบับและพรอมต์: `src-tauri/app-icon.png`, `app-icon.prompt.txt`) */
export function LogoMark({ className }: { className?: string }) {
  return <img src={logoUrl} alt="Puri Photo Search" width={192} height={192} draggable={false} className={cn('size-11 select-none', className)} />;
}

export function Wordmark({ compact }: { compact?: boolean }) {
  return (
    <div className="flex items-center gap-2.5">
      <LogoMark className={compact ? 'size-10' : 'size-11'} />
      {!compact && (
        <div className="leading-none">
          <div className="text-[26px] font-extrabold tracking-tight text-ink">Puri</div>
          <div className="mt-1 text-[10.5px] font-semibold tracking-[0.18em] text-ink">PHOTO SEARCH</div>
        </div>
      )}
    </div>
  );
}
