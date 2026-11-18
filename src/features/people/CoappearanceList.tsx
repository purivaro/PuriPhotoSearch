import { Link } from 'react-router';
import { coappearanceSearchPath } from '@/features/search/searchParams';
import { formatCount } from '@/lib/format';
import type { CoappearanceItem, Id } from '@/lib/ipc/types';
import { FaceAvatar } from './FaceAvatar';

/**
 * คนที่ปรากฏร่วมในภาพกับบุคคลนี้ นับจากใบหน้าที่ยืนยันแล้วเท่านั้น
 * กดแล้วเปิดผลค้นหาแบบ "มีทุกคนในภาพเดียวกัน" — เป็นการค้นหา ไม่ใช่ข้อสรุปความสัมพันธ์
 */
export function CoappearanceList({ personId, items, layout = 'row' }: { personId: Id; items: CoappearanceItem[]; layout?: 'row' | 'list' }) {
  if (items.length === 0) {
    return <p className="text-[13.5px] text-muted">ยังไม่มีรูปที่ยืนยันแล้วว่าอยู่ร่วมกับบุคคลอื่น</p>;
  }
  if (layout === 'list') {
    return (
      <ul className="space-y-1">
        {items.map((item) => (
          <li key={item.person.id}>
            <Link
              to={coappearanceSearchPath(personId, item.person.id)}
              data-testid="coappearance-item"
              className="flex items-center gap-3 rounded-xl px-2 py-1.5 hover:bg-sunken"
            >
              <FaceAvatar faceId={item.person.coverFaceId} name={item.person.displayName} decorative className="size-10" />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-semibold text-ink">{item.person.displayName}</span>
                <span className="block text-[13px] text-muted">อยู่ร่วมกัน {formatCount(item.sharedPhotoCount)} รูป</span>
              </span>
            </Link>
          </li>
        ))}
      </ul>
    );
  }
  return (
    <ul className="grid grid-cols-3 gap-2">
      {items.map((item) => (
        <li key={item.person.id}>
          <Link
            to={coappearanceSearchPath(personId, item.person.id)}
            data-testid="coappearance-item"
            title={`ค้นรูปที่อยู่ร่วมกับ ${item.person.displayName}`}
            className="flex flex-col items-center gap-1 rounded-xl px-1 py-2 text-center hover:bg-sunken"
          >
            <FaceAvatar faceId={item.person.coverFaceId} name={item.person.displayName} decorative className="size-16 text-xl" />
            <span className="w-full truncate text-[13.5px] font-semibold text-ink">{item.person.displayName}</span>
            <span className="text-[12.5px] leading-tight text-muted">อยู่ร่วมกัน {formatCount(item.sharedPhotoCount)} รูป</span>
          </Link>
        </li>
      ))}
    </ul>
  );
}
