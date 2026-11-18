import { useState } from 'react';
import { UserRound } from 'lucide-react';
import { cn } from '@/lib/cn';
import { assetPath } from '@/lib/ipc/client';

interface FaceAvatarProps {
  /** path ของ asset จาก Rust เช่น `PersonRef.coverAsset` ("face/{id}" หรือ "sample/{id}"); null = ยังไม่มีภาพ */
  asset: string | null;
  /** ใช้เป็น alt และตัวอักษรแทนเมื่อยังไม่มีภาพตัวแทน */
  name: string;
  className?: string;
  shape?: 'circle' | 'rounded' | 'square';
  /** true เมื่อชื่อแสดงอยู่ข้าง ๆ แล้ว ไม่ต้องอ่านซ้ำ */
  decorative?: boolean;
}

/** ภาพใบหน้า: ภาพตัวแทนของบุคคล (ใบหน้าที่ยืนยัน หรือใบหน้าตัวอย่างที่นำเข้าจากรุ่นเดิม) หรือครอปของใบหน้าหนึ่งใบ; ไม่มีภาพ → ตัวอักษรย่อ */
export function FaceAvatar({ asset, name, className, shape = 'circle', decorative }: FaceAvatarProps) {
  const [failedFor, setFailedFor] = useState<string | null>(null);
  const radius = shape === 'circle' ? 'rounded-full' : shape === 'rounded' ? 'rounded-xl' : '';
  const initial = name.trim().charAt(0);
  if (!asset || failedFor === asset) {
    return (
      <span
        role={decorative ? undefined : 'img'}
        aria-label={decorative ? undefined : `${name} (ยังไม่มีภาพตัวแทน)`}
        aria-hidden={decorative || undefined}
        className={cn('flex shrink-0 items-center justify-center overflow-hidden bg-brand-soft font-bold text-action select-none', radius, className)}
      >
        {initial ? <span className="text-[1.15em] leading-none">{initial}</span> : <UserRound className="size-1/2" aria-hidden />}
      </span>
    );
  }
  return (
    <img
      src={assetPath(asset)}
      alt={decorative ? '' : name}
      loading="lazy"
      decoding="async"
      onError={() => setFailedFor(asset)}
      className={cn('shrink-0 bg-sunken object-cover', radius, className)}
    />
  );
}
