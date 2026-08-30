import { useEffect, useMemo, useState } from "react";
import { FileOpsDialog } from "@/features/fileops/FileOpsDialog";
import type { FileOpsTab } from "@/features/fileops/FileOpsPanel";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import type { EditorConfig } from "@/types/api";
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
  const [editor, setEditor] = useState<EditorConfig | null>(null);
  const [editorLoaded, setEditorLoaded] = useState(false);
  const [launching, setLaunching] = useState(false);
  const [launchError, setLaunchError] = useState<string | null>(null);
  const counts = useMemo(() => [
    `${selectedIds.length.toLocaleString()} kept`,
    `${rejectedCount.toLocaleString()} rejected`,
    `${laterCount.toLocaleString()} later`,
  ].join(" · "), [selectedIds.length, rejectedCount, laterCount]);

  useEffect(() => {
    let cancelled = false;
    api.getEditorConfig()
      .then((config) => { if (!cancelled) setEditor(config); })
      .catch(() => { if (!cancelled) setEditor(null); })
      .finally(() => { if (!cancelled) setEditorLoaded(true); });
    return () => { cancelled = true; };
  }, []);

  function chooseFileAction(event: React.MouseEvent<HTMLButtonElement>, action: FileOpsTab) {
    event.currentTarget.closest("details")?.removeAttribute("open");
    setFileAction(action);
  }

  function chooseTrayAction(event: React.MouseEvent<HTMLButtonElement>, action: () => void) {
    event.currentTarget.closest("details")?.removeAttribute("open");
    action();
  }

  async function openInEditor(event: React.MouseEvent<HTMLButtonElement>) {
    event.currentTarget.closest("details")?.removeAttribute("open");
    setLaunchError(null);
    if (!editor) {
      useAppStore.getState().setView("settings");
      return;
    }
    setLaunching(true);
    try {
      const result = await api.launchInEditor(selectedIds);
      useAppStore.getState().setNotice(
        `Opened ${result.launched.toLocaleString()} photograph${result.launched === 1 ? "" : "s"} in ${result.application}${result.skippedMissing ? ` · ${result.skippedMissing} missing skipped` : ""}.`,
      );
    } catch (error) {
      setLaunchError(toErrorMessage(error));
    } finally {
      setLaunching(false);
    }
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
            <button
              className="action-menu-item"
              disabled={!editorLoaded || selectedIds.length === 0 || operating || launching || Boolean(editor && selectedIds.length > editor.maxFilesPerLaunch)}
              onClick={(event) => void openInEditor(event)}
            >
              <strong>{editor ? `Open in ${editor.displayName}` : "Configure editing app…"}</strong>
              <span>{editor && selectedIds.length > editor.maxFilesPerLaunch ? `Limited to ${editor.maxFilesPerLaunch} files; export this larger set instead` : "Hand kept source files to a local desktop editor"}</span>
            </button>
            <span className="action-menu-separator" />
            <button className="action-menu-item" disabled={selectedIds.length === 0 || operating} onClick={(event) => chooseFileAction(event, "copy")}>
              <strong>Export originals…</strong><span>Copy kept files to a folder</span>
            </button>
            <ExportSheetButton photoIds={selectedIds} presentation="menu" />
          </div>
        </details>

        {launchError && <span className="cull-tray-error" role="alert">{launchError}</span>}

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
