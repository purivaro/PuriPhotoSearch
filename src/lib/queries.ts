import { useEffect, useMemo, useRef } from 'react';
import { useInfiniteQuery, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { call, onDataChanged } from './ipc/client';
import type { Id, JobSnapshot, PhotoResult, SearchParams } from './ipc/types';

/** พารามิเตอร์ค้นรูปที่ไม่รวม cursor (cursor เป็นของการแบ่งหน้า) */
export type PhotoQuery = Omit<SearchParams, 'cursor'>;

export const qk = {
  capabilities: ['capabilities'] as const,
  summary: ['summary'] as const,
  setup: ['setup'] as const,
  events: (text = '') => ['events', 'list', text] as const,
  event: (id: Id) => ['events', 'detail', id] as const,
  photos: (params: PhotoQuery) => ['photos', params] as const,
  photo: (id: Id) => ['photo', id] as const,
  people: (args: object = {}) => ['people', 'list', args] as const,
  person: (id: Id) => ['people', 'detail', id] as const,
  quickView: (id: Id) => ['people', 'quick', id] as const,
  homePeople: ['people', 'home'] as const,
  groups: ['groups'] as const,
  jobsActive: ['jobs', 'active'] as const,
  jobsAll: ['jobs', 'all'] as const,
  job: (id: Id) => ['jobs', 'detail', id] as const,
  jobErrors: (id: Id) => ['jobs', 'errors', id] as const,
  reviewOverview: (eventId: Id | null) => ['review', 'overview', eventId ?? ''] as const,
  reviewFaces: (kind: 'person' | 'cluster', id: Id, eventId: Id | null, revision: number) => ['review', 'faces', kind, id, eventId ?? '', revision] as const,
};

/** หลังแก้ข้อมูลบุคคล/ใบหน้า: จำนวนรูปและรายชื่อในทุกหน้าต้องตามให้ทัน */
export function invalidatePeopleData(qc: QueryClient) {
  for (const key of [['people'], ['photos'], ['photo'], ['events'], ['summary'], ['groups'], ['review'], ['setup']]) {
    void qc.invalidateQueries({ queryKey: key });
  }
}

export function invalidateCatalog(qc: QueryClient) {
  for (const key of [['events'], ['photos'], ['photo'], ['summary'], ['jobs']]) {
    void qc.invalidateQueries({ queryKey: key });
  }
}

export function useCapabilities() {
  return useQuery({ queryKey: qk.capabilities, queryFn: () => call('get_capabilities', {}), staleTime: Infinity });
}

export function useSummary() {
  return useQuery({ queryKey: qk.summary, queryFn: () => call('get_library_summary', {}) });
}

export function useSetupState() {
  return useQuery({ queryKey: qk.setup, queryFn: () => call('get_setup_state', {}) });
}

export function useEvents(text = '') {
  return useQuery({ queryKey: qk.events(text), queryFn: () => call('list_events', text ? { text } : {}) });
}

export function useEvent(eventId: Id | undefined) {
  return useQuery({
    queryKey: qk.event(eventId ?? ''),
    queryFn: () => call('get_event', { eventId: eventId as Id }),
    enabled: Boolean(eventId),
  });
}

export function usePeople(args: { text?: string; includeArchived?: boolean; groupId?: Id; pinnedOnly?: boolean } = {}, enabled = true) {
  return useQuery({ queryKey: qk.people(args), queryFn: () => call('list_people', args), enabled });
}

export function useGroups() {
  return useQuery({ queryKey: qk.groups, queryFn: () => call('list_groups', {}) });
}

export const PHOTO_PAGE_SIZE = 100;

/** รายการรูปแบบแบ่งหน้าด้วย cursor; key ผูกกับเงื่อนไข ผลของเงื่อนไขเก่าจึงไม่ทับเงื่อนไขใหม่ */
export function usePhotoList(params: PhotoQuery, enabled = true) {
  const query = useInfiniteQuery({
    queryKey: qk.photos(params),
    queryFn: ({ pageParam, signal }) => {
      void signal;
      return call('query_photos', pageParam ? { ...params, cursor: pageParam } : params);
    },
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.nextCursor,
    enabled,
  });
  const items = useMemo(() => {
    const seen = new Set<Id>();
    const out: PhotoResult[] = [];
    for (const page of query.data?.pages ?? []) {
      for (const item of page.items) {
        // หนึ่ง photo ID แสดงครั้งเดียว แม้ข้อมูลเลื่อนระหว่างโหลดหน้า
        if (!seen.has(item.id)) {
          seen.add(item.id);
          out.push(item);
        }
      }
    }
    return out;
  }, [query.data]);
  const first = query.data?.pages[0];
  return { ...query, items, total: first?.total ?? 0, coverage: first?.coverage ?? null };
}

export function isJobActive(job: JobSnapshot): boolean {
  return ['queued', 'running', 'pausing', 'cancel_requested', 'blocked'].includes(job.state);
}

/** งานที่ยังไม่จบ; ถามถี่ขึ้นเมื่อมีงานกำลังทำ (event จาก Rust เป็นเพียงตัวเร่ง ไม่ใช่แหล่งข้อมูลหลัก) */
export function useActiveJobs() {
  return useQuery({
    queryKey: qk.jobsActive,
    queryFn: () => call('list_jobs', { activeOnly: true }),
    refetchInterval: (query) => (query.state.data?.some(isJobActive) ? 1000 : 5000),
    refetchIntervalInBackground: true,
  });
}

/**
 * เฝ้างานเบื้องหลังแล้วสั่งให้หน้าที่เกี่ยวข้องโหลดข้อมูลใหม่
 * - จำนวนของงานเปลี่ยน → สรุปคลัง/กิจกรรมตามทันที, grid รูปตามไม่ถี่กว่า 3 วินาที
 * - งานจบ → โหลดใหม่ทั้งหมดหนึ่งครั้ง
 */
export function useJobWatcher() {
  const qc = useQueryClient();
  const { data } = useActiveJobs();
  const lastSignature = useRef<string | null>(null);
  const lastActiveIds = useRef<string>('');
  const lastPhotoRefresh = useRef(0);

  useEffect(() => {
    if (!data) return;
    const signature = data.map((j) => `${j.id}:${j.state}:${j.stage}:${j.counts.total}:${j.counts.done}:${j.counts.failed}`).join('|');
    const activeIds = data.filter(isJobActive).map((j) => j.id).sort().join('|');
    if (lastSignature.current === null) {
      lastSignature.current = signature;
      lastActiveIds.current = activeIds;
      return;
    }
    if (signature === lastSignature.current) return;
    lastSignature.current = signature;
    void qc.invalidateQueries({ queryKey: ['events'] });
    void qc.invalidateQueries({ queryKey: ['summary'] });
    void qc.invalidateQueries({ queryKey: qk.jobsAll });
    void qc.invalidateQueries({ queryKey: ['jobs', 'detail'] });
    const finished = activeIds !== lastActiveIds.current;
    // งานดาวน์โหลดตัววิเคราะห์ใบหน้าเริ่ม/จบ → สถานะ AI เปลี่ยน; งานจบ → คิวตรวจสอบใบหน้าอาจมีรายการใหม่
    // (คิวตรวจสอบใบหน้าไม่ถูกโหลดใหม่เองที่นี่: หน้านั้นแจ้งผู้ใช้แล้วให้กดอัปเดต เพื่อไม่สลับรายการที่กำลังตรวจอยู่)
    if (finished || data.some((j) => j.kind === 'model')) void qc.invalidateQueries({ queryKey: qk.capabilities });
    lastActiveIds.current = activeIds;
    const now = Date.now();
    if (finished || now - lastPhotoRefresh.current > 3000) {
      lastPhotoRefresh.current = now;
      void qc.invalidateQueries({ queryKey: ['photos'] });
    }
  }, [data, qc]);

  useEffect(() => {
    let dispose: (() => void) | undefined;
    let cancelled = false;
    void onDataChanged((event) => {
      for (const scope of event.scopes) {
        if (scope === 'photos') {
          void qc.invalidateQueries({ queryKey: ['photos'] });
          void qc.invalidateQueries({ queryKey: ['photo'] });
        } else if (scope === 'people') {
          void qc.invalidateQueries({ queryKey: ['people'] });
          void qc.invalidateQueries({ queryKey: ['review'] });
        } else {
          void qc.invalidateQueries({ queryKey: [scope] });
        }
      }
      void qc.invalidateQueries({ queryKey: ['summary'] });
    }).then((off) => {
      if (cancelled) off();
      else dispose = off;
    });
    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [qc]);
}
