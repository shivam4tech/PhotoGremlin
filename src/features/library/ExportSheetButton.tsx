/**
 * "Contact sheet" action for the selection (Sprint 14): pick a destination
 * folder, export a printable PNG sheet of the selected photographs, and
 * surface progress + result via the Rust background job's events.
 */
import { useEffect, useState } from "react";
import { api, onProgress, toErrorMessage } from "@/lib/ipc";
import type { ContactSheetCompletePayload, ProgressPayload } from "@/types/api";
import { useAppStore } from "@/stores/appStore";

export function ExportSheetButton({ photoIds, presentation = "button" }: { photoIds: number[]; presentation?: "button" | "menu" }) {
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ProgressPayload | null>(null);
  const setNotice = useAppStore((state) => state.setNotice);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    void Promise.all([
      onProgress<ProgressPayload>("contact-sheet-progress", (next) => {
        setBusy(true);
        setProgress(next);
      }),
      onProgress<ContactSheetCompletePayload>("contact-sheet-complete", (p) => {
        setBusy(false);
        setProgress(null);
        if (p.error) {
          setNotice(p.error);
        } else if (p.files.length === 0) {
          setNotice(p.cancelled ? "Contact-sheet export cancelled." : "No sheets were written.");
        } else {
          const folder = p.files[0].split("/").slice(0, -1).join("/");
          setNotice(
            `Contact sheet written: ${p.files.length} page${p.files.length === 1 ? "" : "s"} → ${folder}`,
          );
        }
      }),
    ]).then((registered) => {
      if (cancelled) registered.forEach((unlisten) => unlisten());
      else unlisteners.push(...registered);
    });
    return () => {
      cancelled = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [setNotice]);

  const onExport = async () => {
    if (busy || photoIds.length === 0) return;
    const destDir = await api.pickFolder();
    if (!destDir) return;
    setBusy(true);
    try {
      await api.exportContactSheet(
        destDir,
        `${photoIds.length} photographs`,
        photoIds,
      );
    } catch (e) {
      setBusy(false);
      setNotice(toErrorMessage(e));
    }
  };

  if (presentation === "menu") {
    return busy ? (
      <button onClick={() => void api.stopExport()}>
        <strong>Cancel contact sheet</strong>
        <span>{progress && progress.total > 0 ? `${progress.done.toLocaleString()} of ${progress.total.toLocaleString()} pages` : "Preparing printable pages…"}</span>
      </button>
    ) : (
      <button onClick={onExport} disabled={photoIds.length === 0}>
        <strong>Contact sheet (PNG)…</strong><span>12 photographs per printable page</span>
      </button>
    );
  }

  return (
    <button
      className="btn btn-ghost btn-sm"
      onClick={busy ? () => void api.stopExport() : onExport}
      disabled={photoIds.length === 0}
      title={
        photoIds.length === 0
          ? "Select photographs first"
          : `Export a printable contact sheet of ${photoIds.length.toLocaleString()} selected photograph${photoIds.length === 1 ? "" : "s"}`
      }
    >
      {busy ? "Cancel contact sheet" : "Contact sheet"}
    </button>
  );
}
