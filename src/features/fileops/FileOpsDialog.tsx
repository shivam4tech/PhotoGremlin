import { useEffect, useRef } from "react";
import { FileOpsPanel, type FileOpsTab } from "./FileOpsPanel";

export function FileOpsDialog({ photoIds, initialTab, onClose }: {
  photoIds: number[];
  initialTab: FileOpsTab;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
    return () => { if (dialog?.open) dialog.close(); };
  }, []);

  return (
    <dialog
      ref={ref}
      className="workspace-dialog"
      aria-label="Photograph file actions"
      onCancel={(event) => { event.preventDefault(); onClose(); }}
      onClick={(event) => { if (event.target === ref.current) onClose(); }}
    >
      <div className="workspace-dialog-card">
        <div className="workspace-dialog-head">
          <div><strong>File actions</strong><span>Preview every change before it touches disk</span></div>
          <button className="btn btn-ghost btn-sm" onClick={onClose} aria-label="Close file actions">✕</button>
        </div>
        <FileOpsPanel key={initialTab} photoIds={photoIds} initialTab={initialTab} />
      </div>
    </dialog>
  );
}
