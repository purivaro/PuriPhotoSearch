import { useState } from 'react';
import { Image as ImageIcon, ImageOff } from 'lucide-react';
import { cn } from '@/lib/cn';
import { assetUrl } from '@/lib/ipc/client';
import type { PhotoResult } from '@/lib/ipc/types';

/**
 * ภาพย่อของรูปหนึ่งใบ พร้อมสถานะที่ไม่ใช่ภาพ: กำลังเตรียม / อ่านไม่ได้
 * ภาพย่ออาจ crop เพื่อ layout ได้ ส่วนตัวดูภาพแสดงภาพเต็มเสมอ
 */
export function PhotoThumb({ photo, className, fit = 'cover' }: { photo: Pick<PhotoResult, 'id' | 'revisionId' | 'editId' | 'thumbStatus' | 'filename'>; className?: string; fit?: 'cover' | 'contain' }) {
  const [failedKey, setFailedKey] = useState<string | null>(null);
  // รุ่นของภาพย่อ: เปลี่ยนเมื่อไฟล์เปลี่ยนหรือแต่งใหม่
  const version = photo.editId ?? photo.revisionId;
  const key = `${photo.id}:${version}`;
  if (photo.thumbStatus !== 'ready' || failedKey === key) {
    const failed = photo.thumbStatus === 'failed' || failedKey === key;
    return (
      <div className={cn('flex flex-col items-center justify-center gap-1 bg-sunken text-faint', className)}>
        {failed ? <ImageOff className="size-6" aria-hidden /> : <ImageIcon className="size-6" aria-hidden />}
        <span className="text-[11.5px] text-muted">{failed ? 'อ่านรูปนี้ไม่ได้' : 'กำลังเตรียมภาพย่อ'}</span>
      </div>
    );
  }
  return (
    <img
      src={assetUrl('thumb', photo.id, version)}
      alt=""
      loading="lazy"
      decoding="async"
      draggable={false}
      onError={() => setFailedKey(key)}
      className={cn('bg-sunken', fit === 'cover' ? 'object-cover' : 'object-contain', className)}
    />
  );
}
