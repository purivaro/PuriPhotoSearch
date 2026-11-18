import { useState } from 'react';
import { UserRound } from 'lucide-react';
import { cn } from '@/lib/cn';
import { assetUrl } from '@/lib/ipc/client';
import type { Id } from '@/lib/ipc/types';

interface FaceAvatarProps {
  faceId: Id | null;
  /** ใช้เป็น alt และตัวอักษรแทนเมื่อยังไม่มีภาพตัวแทน */
  name: string;
  className?: string;
  shape?: 'circle' | 'rounded' | 'square';
  /** true เมื่อชื่อแสดงอยู่ข้าง ๆ แล้ว ไม่ต้องอ่านซ้ำ */
  decorative?: boolean;
}

/** ภาพใบหน้าตัวแทนของบุคคล (ครอปจากใบหน้าที่ผู้ใช้ยืนยัน) หรือสัญลักษณ์แทนเมื่อยังไม่มี */
export function FaceAvatar({ faceId, name, className, shape = 'circle', decorative }: FaceAvatarProps) {
  const [failedFor, setFailedFor] = useState<Id | null>(null);
  const radius = shape === 'circle' ? 'rounded-full' : shape === 'rounded' ? 'rounded-xl' : '';
  const initial = name.trim().charAt(0);
  if (!faceId || failedFor === faceId) {
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
      src={assetUrl('face', faceId)}
      alt={decorative ? '' : name}
      loading="lazy"
      decoding="async"
      onError={() => setFailedFor(faceId)}
      className={cn('shrink-0 bg-sunken object-cover', radius, className)}
    />
  );
}
