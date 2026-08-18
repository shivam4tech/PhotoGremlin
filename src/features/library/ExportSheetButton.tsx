/**
 * "Contact sheet" action for the selection (Sprint 14): pick a destination
 * folder, export a printable PNG sheet of the selected photographs, and
 * surface progress + result via the Rust background job's events.
 */
import { useEffect, useState } from "react";
import { api, onProgress, toErrorMessage } from "@/lib/ipc";
import type { ContactSheetCompletePayload } from "@/types/api";
import { useAppStore } from "@/stores/appStore";

export function ExportSheetButton({ photoIds }: { photoIds: number[] }) {
  const [busy, setBusy] = useState(false);
  const setNotice = (n: string | null) => useAppStore.getState().setNotice(n);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    onProgress<ContactSheetCompletePayload>("contact-sheet-complete", (p) => {
      setBusy(false);
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
    }).then((u) => {
      unlisten = u;
    });
    return () => unlisten?.();
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

  return (
    <button
      className="btn btn-ghost btn-sm"
      onClick={onExport}
      disabled={busy || photoIds.length === 0}
      title={
        photoIds.length === 0
          ? "Select photographs first"
          : `Export a printable contact sheet of ${photoIds.length.toLocaleString()} selected photograph${photoIds.length === 1 ? "" : "s"}`
      }
    >
      {busy ? "Exporting…" : "Contact sheet"}
    </button>
  );
}