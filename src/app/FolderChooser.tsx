import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from 'react';
import { toast } from 'sonner';
import { Wrench } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Dialog } from '@/components/ui/Dialog';
import { StatusBadge } from '@/components/ui/Feedback';
import { Field, TextInput } from '@/components/ui/Form';
import { pickFolder } from '@/lib/ipc/client';
import { useCapabilities } from '@/lib/queries';

interface ChooseOptions {
  title: string;
  hint?: string;
}

type ChooseFolder = (options: ChooseOptions) => Promise<string | null>;

const FolderChooserContext = createContext<ChooseFolder | null>(null);

/**
 * เลือกโฟลเดอร์ผ่าน native picker ของระบบ
 * เมื่อรัน UI ในเบราว์เซอร์กับ dev bridge (ไม่มี picker) จะใช้กล่องพิมพ์ path ของแอปแทน และติดป้าย "โหมดพัฒนา"
 */
export function FolderChooserProvider({ children }: { children: ReactNode }) {
  const { data: capabilities } = useCapabilities();
  const [request, setRequest] = useState<ChooseOptions | null>(null);
  const [path, setPath] = useState('');
  const resolver = useRef<((value: string | null) => void) | null>(null);

  const finish = useCallback((value: string | null) => {
    resolver.current?.(value);
    resolver.current = null;
    setRequest(null);
    setPath('');
  }, []);

  const choose = useCallback<ChooseFolder>(
    async (options) => {
      if (capabilities?.folderPicker === 'native') {
        try {
          return await pickFolder(options.title);
        } catch {
          toast.error('เปิดหน้าต่างเลือกโฟลเดอร์ไม่สำเร็จ กรุณาลองใหม่');
          return null;
        }
      }
      resolver.current?.(null);
      return new Promise<string | null>((resolve) => {
        resolver.current = resolve;
        setRequest(options);
      });
    },
    [capabilities?.folderPicker],
  );

  const value = useMemo(() => choose, [choose]);
  const trimmed = path.trim();

  return (
    <FolderChooserContext.Provider value={value}>
      {children}
      <Dialog
        open={request !== null}
        onOpenChange={(open) => !open && finish(null)}
        title={request?.title ?? ''}
        description={request?.hint}
        footer={
          <>
            <Button onClick={() => finish(null)}>ยกเลิก</Button>
            <Button variant="primary" form="dev-folder-form" type="submit" disabled={!trimmed}>
              ใช้โฟลเดอร์นี้
            </Button>
          </>
        }
      >
        <form
          id="dev-folder-form"
          onSubmit={(event) => {
            event.preventDefault();
            if (trimmed) finish(trimmed);
          }}
          className="space-y-3"
        >
          <StatusBadge tone="info" icon={<Wrench aria-hidden />}>
            โหมดพัฒนา
          </StatusBadge>
          <p className="text-sm text-muted">
            หน้านี้กำลังรันในเบราว์เซอร์ จึงไม่มีหน้าต่างเลือกโฟลเดอร์ของระบบ ในแอปจริงจะเปิดหน้าต่างเลือกโฟลเดอร์ให้โดยอัตโนมัติ
          </p>
          <Field label="ตำแหน่งโฟลเดอร์บนเครื่อง">
            {(props) => (
              <TextInput
                {...props}
                data-autofocus
                data-testid="dev-folder-input"
                value={path}
                onChange={(event) => setPath(event.target.value)}
                placeholder="/Users/…/Photos/2569/691118 งานอบรม AI"
                spellCheck={false}
                autoComplete="off"
              />
            )}
          </Field>
        </form>
      </Dialog>
    </FolderChooserContext.Provider>
  );
}

export function useFolderChooser(): ChooseFolder {
  const value = useContext(FolderChooserContext);
  if (!value) throw new Error('useFolderChooser ต้องอยู่ใน FolderChooserProvider');
  return value;
}
