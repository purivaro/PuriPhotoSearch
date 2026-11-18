import { cn } from '@/lib/cn';

/** เครื่องหมายรูรับแสงสีส้มของ Puri (วาดเอง ไม่ใช้ไฟล์ภาพภายนอก) */
export function LogoMark({ className }: { className?: string }) {
  const blades = [0, 60, 120, 180, 240, 300];
  return (
    <svg viewBox="0 0 48 48" className={cn('size-11', className)} role="img" aria-label="Puri Photo Search">
      <circle cx="24" cy="24" r="23" fill="var(--brand)" />
      <g fill="#ffffff">
        {blades.map((angle) => (
          <path key={angle} transform={`rotate(${angle} 24 24)`} d="M24 4.5c5.6 0 10.6 2.3 14.2 6.1L27.4 16.9 24 4.5z" opacity="0.96" />
        ))}
      </g>
      <circle cx="24" cy="24" r="7.6" fill="var(--brand)" />
      <circle cx="24" cy="24" r="23" fill="none" stroke="#d9630c" strokeOpacity="0.35" />
    </svg>
  );
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
