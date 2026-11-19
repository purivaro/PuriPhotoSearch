import { useEffect, useRef, type ReactNode } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router';
import { CircleCheck, TriangleAlert, UsersRound } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { FaceModelNotice } from '@/features/faces/FaceModelNotice';
import { errorMessage, formatCount, newRequestId } from '@/lib/format';
import { call } from '@/lib/ipc/client';
import type { SetupState } from '@/lib/ipc/types';
import { invalidatePeopleData, qk, useCapabilities, useSetupState } from '@/lib/queries';
import { useUi } from '@/stores/ui';

/**
 * S01 (ส่วนเสริม) — เตรียมแอปให้พร้อมใช้: สิ่งที่ผู้ใช้ใหม่ไม่มีทางรู้ว่าต้องทำก่อน รวมไว้ในหน้าต่างเดียว
 * (1) ดาวน์โหลดตัววิเคราะห์ใบหน้า (2) นำรายชื่อจากรุ่น Python เข้า เมื่อพบฐานเดิมในเครื่อง
 * เปิดเองครั้งเดียว: ปิดแล้วจำไว้กับคลัง ทั้งสองอย่างยังทำได้จากหน้าอื่นตามเดิม
 */
export function SetupDialog() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const open = useUi((s) => s.setupOpen);
  const setOpen = useUi((s) => s.setSetupOpen);
  const setup = useSetupState();
  const model = useCapabilities().data?.faceAnalysis.state;
  const legacyPending = setup.data?.legacyPending ?? 0;
  const dismissed = setup.data?.dismissed;

  // ตัดสินครั้งเดียวต่อการเปิดแอป เมื่อรู้ทั้งสถานะตัววิเคราะห์ใบหน้าและข้อมูลรุ่นเดิมแล้ว
  const decided = useRef(false);
  useEffect(() => {
    if (decided.current || dismissed === undefined || !model) return;
    decided.current = true;
    if (!dismissed && (model === 'not_installed' || legacyPending > 0)) setOpen(true);
  }, [dismissed, model, legacyPending, setOpen]);

  // requestId เดียวต่อการกดนำเข้าหนึ่งครั้ง (ลองใหม่หลังผิดพลาดใช้ค่าเดิม)
  const requestId = useRef<string | null>(null);
  const run = useMutation({
    mutationFn: () => {
      requestId.current ??= newRequestId();
      return call('import_legacy_people', { requestId: requestId.current });
    },
    onSuccess: () => {
      requestId.current = null;
      invalidatePeopleData(qc);
    },
  });

  const close = () => {
    setOpen(false);
    run.reset();
    if (dismissed === false) {
      qc.setQueryData<SetupState>(qk.setup, (state) => state && { ...state, dismissed: true });
      // จำไม่สำเร็จ = แค่ถามอีกครั้งตอนเปิดแอปครั้งหน้า ไม่ต้องรบกวนผู้ใช้
      void call('dismiss_setup', {}).catch(() => {});
    }
  };

  const auto = model !== 'unsupported';
  const showLegacy = legacyPending > 0 || Boolean(run.data);
  const remaining = model === 'not_installed' || legacyPending > 0;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && !run.isPending && close()}
      dismissible={!run.isPending}
      size="lg"
      title="เตรียมแอปให้พร้อมใช้"
      description={
        !auto
          ? 'นำรายชื่อที่เคยตั้งไว้มาใช้ต่อในแอปนี้'
          : showLegacy
            ? 'ทำสองอย่างนี้ครั้งเดียว แล้วแอปจะหาคนในรูปให้เอง'
            : 'ทำครั้งเดียว แล้วแอปจะหาคนในรูปให้เอง'
      }
      footer={
        <>
          <p className="mr-auto text-[13px] text-muted">ทำภายหลังได้ที่หน้าตั้งค่า</p>
          <Button variant={remaining ? 'secondary' : 'primary'} disabled={run.isPending} onClick={close} data-testid="setup-close">
            {remaining ? 'ข้ามไปก่อน' : 'เสร็จแล้ว'}
          </Button>
        </>
      }
    >
      <div className="space-y-3.5 pt-1" data-testid="setup-dialog">
        {auto &&
          (model === 'ready' ? (
            <Done title="ตัววิเคราะห์ใบหน้าพร้อมใช้" testId="setup-model-ready">
              รูปที่เพิ่มเข้าคลังจะถูกวิเคราะห์เองเบื้องหลัง
            </Done>
          ) : (
            <FaceModelNotice />
          ))}

        {showLegacy &&
          (run.data ? (
            <Done
              title={`นำรายชื่อเข้าแล้ว ${formatCount(run.data.personsCreated)} คน`}
              testId="setup-legacy-done"
              action={
                <Button
                  size="sm"
                  onClick={() => {
                    close();
                    void navigate('/people');
                  }}
                  data-testid="setup-open-people"
                >
                  เปิดหน้าบุคคล
                </Button>
              }
            >
              {!auto
                ? 'เลือกชื่อเหล่านี้ได้ตอนระบุใบหน้าในรูป'
                : model === 'ready'
                  ? 'แอปจะใช้ใบหน้าตัวอย่างของแต่ละคนหารูปของเขาในคลังนี้ให้'
                  : 'เมื่อตัววิเคราะห์ใบหน้าพร้อม แอปจะใช้ใบหน้าตัวอย่างของแต่ละคนหารูปของเขาในคลังนี้ให้'}
            </Done>
          ) : (
            <section className="rounded-card border border-brand-line bg-surface p-5 text-left shadow-card" data-testid="setup-legacy">
              <div className="flex items-start gap-3.5">
                <span className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-brand-soft text-action">
                  <UsersRound className="size-6" aria-hidden />
                </span>
                <div className="min-w-0 flex-1">
                  <h3 className="text-[17px] font-bold text-ink">นำรายชื่อจาก Puri Photo Search รุ่นเดิม</h3>
                  <p className="mt-0.5 text-sm text-muted">
                    พบรายชื่อ {formatCount(legacyPending)} คนที่เคยตั้งไว้ในรุ่นเดิมบนเครื่องนี้ นำเข้าแล้วไม่ต้องตั้งชื่อใหม่
                    {auto && ' แอปจะใช้ใบหน้าตัวอย่างของแต่ละคนหารูปของเขาในคลังนี้ให้'}
                  </p>
                  <Button
                    variant="primary"
                    size="lg"
                    className="mt-3.5"
                    icon={<UsersRound className="size-[18px]" aria-hidden />}
                    busy={run.isPending}
                    onClick={() => run.mutate()}
                    data-testid="setup-legacy-import"
                  >
                    นำเข้า {formatCount(legacyPending)} คน
                  </Button>
                  {run.isError && (
                    <p role="alert" className="mt-2.5 flex items-start gap-2 text-sm text-danger" data-testid="setup-legacy-failed">
                      <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
                      นำเข้าไม่สำเร็จ: {errorMessage(run.error)} — ข้อมูลของรุ่นเดิมไม่ถูกแก้ไข กดปุ่มเพื่อลองใหม่ได้
                    </p>
                  )}
                  <p className="mt-3 text-[12.5px] leading-relaxed text-muted">
                    นำเข้าเฉพาะรายชื่อ กลุ่ม และใบหน้าตัวอย่าง ข้อมูลของรุ่นเดิมไม่ถูกแก้ไข และรุ่นเดิมยังใช้ได้ตามปกติ
                  </p>
                </div>
              </div>
            </section>
          ))}
      </div>
    </Dialog>
  );
}

function Done({ title, children, action, testId }: { title: string; children: ReactNode; action?: ReactNode; testId: string }) {
  return (
    <div className="flex items-center gap-3.5 rounded-card border border-line bg-surface px-5 py-4 text-left" data-testid={testId}>
      <span className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-ok-soft text-ok">
        <CircleCheck className="size-6" aria-hidden />
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-[15px] font-bold text-ink">{title}</p>
        <p className="text-sm text-muted">{children}</p>
      </div>
      {action}
    </div>
  );
}
