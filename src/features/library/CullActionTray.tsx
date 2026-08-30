import { useMemo, useState } from "react";
import { FileOpsDialog } from "@/features/fileops/FileOpsDialog";
import type { FileOpsTab } from "@/features/fileops/FileOpsPanel";
import { ExportSheetButton } from "./ExportSheetButton";
import { MarksPanel } from "./MarksPanel";

interface CullActionTrayProps {
  selectedIds: number[];
  rejectedCount: number;
  laterCount: number;
  shownCount: number;
  operating: boolean;
  collections: { id: number; name: string }[];
  onAddToCollection: (collectionId: number) => void;
  onKeepAllShown: () => void;
  onClearShown: () => void;
}

export function CullActionTray({
  selectedIds,
  rejectedCount,
  laterCount,
  shownCount,
  operating,
  collections,
  onAddToCollection,
  onKeepAllShown,
  onClearShown,
}: CullActionTrayProps) {
  const [fileAction, setFileAction] = useState<FileOpsTab | null>(null);
  const [collectionId, setCollectionId] = useState<number | null>(null);
  const counts = useMemo(() => [
    `${selectedIds.length.toLocaleString()} kept`,
    `${rejectedCount.toLocaleString()} rejected`,
    `${laterCount.toLocaleString()} later`,
  ].join(" · "), [selectedIds.length, rejectedCount, laterCount]);

  function chooseFileAction(event: React.MouseEvent<HTMLButtonElement>, action: FileOpsTab) {
    event.currentTarget.closest("details")?.removeAttribute("open");
    setFileAction(action);
  }

  function chooseTrayAction(event: React.MouseEvent<HTMLButtonElement>, action: () => void) {
    event.currentTarget.closest("details")?.removeAttribute("open");
    action();
  }

  return (
    <>
      <div className="cull-tray" role="region" aria-label="Cull actions">
        <div className="cull-tray-summary"><strong>Cull</strong><span>{counts}</span></div>

        {selectedIds.length > 0 && <MarksPanel photoIds={selectedIds} onApplied={() => {}} />}

        {selectedIds.length > 0 && collections.length > 0 && (
          <span className="cull-tray-collection">
            <select className="input" value={collectionId ?? ""} onChange={(event) => setCollectionId(event.target.value ? Number(event.target.value) : null)} aria-label="Collection">
              <option value="">Collection…</option>
              {collections.map((collection) => <option key={collection.id} value={collection.id}>{collection.name}</option>)}
            </select>
            <button className="btn btn-sm" disabled={collectionId === null} onClick={() => collectionId !== null && onAddToCollection(collectionId)}>Add</button>
          </span>
        )}

        <span className="spacer" />

        <details className="action-menu">
          <summary className="btn btn-sm" aria-label="Export selected photographs">Export</summary>
          <div className="action-menu-popover">
            <button className="action-menu-item" disabled={selectedIds.length === 0 || operating} onClick={(event) => chooseFileAction(event, "copy")}>
              <strong>Export originals…</strong><span>Copy kept files to a folder</span>
            </button>
            <ExportSheetButton photoIds={selectedIds} presentation="menu" />
          </div>
        </details>

        <details className="action-menu">
          <summary className="btn btn-sm btn-ghost" aria-label="More cull actions">More</summary>
          <div className="action-menu-popover action-menu-popover-right">
            <button className="action-menu-item" onClick={(event) => chooseTrayAction(event, onKeepAllShown)} disabled={operating || shownCount === 0}>
              <strong>Keep all shown</strong><span>Mark every photograph in this view as kept</span>
            </button>
            <button className="action-menu-item" onClick={(event) => chooseTrayAction(event, onClearShown)} disabled={operating || shownCount === 0}>
              <strong>Clear shown</strong><span>Remove cull decisions from this view</span>
            </button>
            <span className="action-menu-separator" />
            <button className="action-menu-item" onClick={(event) => chooseFileAction(event, "rename")} disabled={selectedIds.length === 0 || operating}>
              <strong>Rename…</strong><span>Preview filenames before applying changes</span>
            </button>
            <button className="action-menu-item" onClick={(event) => chooseFileAction(event, "move")} disabled={selectedIds.length === 0 || operating}>
              <strong>Move…</strong><span>Preview the destination and any conflicts</span>
            </button>
            <button className="action-menu-item is-danger" onClick={(event) => chooseFileAction(event, "trash")} disabled={selectedIds.length === 0 || operating}>
              <strong>Move to trash…</strong><span>Review and confirm before moving files</span>
            </button>
          </div>
        </details>
      </div>

      {fileAction && <FileOpsDialog photoIds={selectedIds} initialTab={fileAction} onClose={() => setFileAction(null)} />}
    </>
  );
}
