import { CircleHelp, Sparkles } from 'lucide-react';
import { StatusBadge } from '@/components/ui/Feedback';
import { levelLabel } from '@/features/review/reviewModel';

/** ป้ายระดับของข้อเสนอ AI: ต่างกันทั้งไอคอนและข้อความ ไม่ใช่สีอย่างเดียว และไม่แสดงเป็นเปอร์เซ็นต์ */
export function LevelBadge({ level, className }: { level: 'close' | 'check'; className?: string }) {
  return (
    <StatusBadge tone={level === 'close' ? 'brand' : 'warn'} icon={level === 'close' ? <Sparkles aria-hidden /> : <CircleHelp aria-hidden />} className={className}>
      {levelLabel(level)}
    </StatusBadge>
  );
}
