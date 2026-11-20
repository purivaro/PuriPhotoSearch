import { Image as ImageIcon } from 'lucide-react';
import { IconButton } from '@/components/ui/Button';
import { THUMB_SIZES, useSettings } from '@/stores/settings';

const LABELS = ['เล็กมาก', 'เล็ก', 'กลาง', 'ใหญ่', 'ใหญ่มาก'];

/** แถบปรับขนาดรูปในกริด ๕ ระดับ จำค่าไว้ใช้ทุกหน้าที่แสดงรูป */
export function ThumbSizeSlider() {
  const level = useSettings((s) => s.thumbSize);
  const setLevel = useSettings((s) => s.setThumbSize);
  const max = THUMB_SIZES.length - 1;
  return (
    <div className="flex shrink-0 items-center gap-0.5" data-testid="thumb-size">
      <IconButton size="sm" label="รูปเล็กลง" onClick={() => setLevel(level - 1)} disabled={level === 0} data-testid="thumb-size-smaller">
        <ImageIcon className="size-3.5" aria-hidden />
      </IconButton>
      <input
        type="range"
        min={0}
        max={max}
        step={1}
        value={level}
        onChange={(e) => setLevel(Number(e.target.value))}
        aria-label="ขนาดรูป"
        aria-valuetext={LABELS[level]}
        title={`ขนาดรูป: ${LABELS[level]}`}
        data-testid="thumb-size-range"
        className="h-1.5 w-24 cursor-pointer accent-[var(--action)]"
      />
      <IconButton size="sm" label="รูปใหญ่ขึ้น" onClick={() => setLevel(level + 1)} disabled={level === max} data-testid="thumb-size-larger">
        <ImageIcon className="size-5" aria-hidden />
      </IconButton>
    </div>
  );
}
