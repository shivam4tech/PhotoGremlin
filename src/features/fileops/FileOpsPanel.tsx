import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import type { FileOpPlan, OperationSummary, PlanItem } from "@/types/api";
import {
  fileBase,
  flaggedResults,
  previewHeadline,
  progressLabel,
  resultHeadline,
} from "@/features/fileops/format";

export type FileOpsTab = "rename" | "move" | "copy" | "trash" | "delete-permanently";

const TEMPLATES = [
  { label: "date_name_seq", value: "{date}_{name}_{sequence}" },
  { label: "name_seq", value: "{name}_{sequence}" },
  { label: "date_camera_seq", value: "{date}_{camera}_{sequence}" },
  { label: "date_iso_seq", value: "{date}_{iso}_{sequence}" },
];

/**
 * File operations (Sprints 7 and 32): rename / move / copy / trash /
 * permanent delete for the
 * photographs currently marked "selected". Preview-first: every action builds
 * a plan the backend returns, the user inspects it, confirms, and only then
 * does anything touch disk. Destructive actions get a native confirmation.
 */
export function FileOpsPanel({ photoIds, initialTab = "rename" }: { photoIds: number[]; initialTab?: FileOpsTab }) {
  const operating = useAppStore((s) => s.operating);
  const opProgress = useAppStore((s) => s.opProgress);
  const opSummary = useAppStore((s) => s.opSummary);
  const store = useAppStore.getState;

  const [tab, setTab] = useState<FileOpsTab>(initialTab);
  const [template, setTemplate] = useState(TEMPLATES[0].value);
  const [groupName, setGroupName] = useState("");
  const [destDir, setDestDir] = useState<string | null>(null);
  const [onCollision, setOnCollision] = useState<"skip" | "avoid-by-renaming">("skip");
  const [plan, setPlan] = useState<FileOpPlan | null>(null);
  const [planning, setPlanning] = useState(false);
  const [starting, setStarting] = useState(false);
  const [localErr, setLocalErr] = useState<string | null>(null);

  // Any change to the inputs invalidates the preview.
  useEffect(() => {
    setPlan(null);
    setLocalErr(null);
  }, [tab, template, groupName, destDir, onCollision, photoIds.length]);

  const canPlan = !operating && !planning;
  const destRequired = tab === "move" || tab === "copy";
  const previewEnabled = canPlan && (!destRequired || !!destDir) && (tab !== "rename" || template.trim().length > 0);

  async function preview() {
    if (!previewEnabled) return;
    setPlanning(true);
    setLocalErr(null);
    try {
      const p =
        tab === "rename"
          ? await api.planGroupRename(photoIds, template, groupName)
          : tab === "trash"
            ? await api.planTrash(photoIds)
            : tab === "delete-permanently"
              ? await api.planPermanentDelete(photoIds)
              : await api.planMoveCopy(photoIds, destDir as string, tab, onCollision);
      setPlan(p);
    } catch (e) {
      setLocalErr(toErrorMessage(e));
      setPlan(null);
    } finally {
      setPlanning(false);
    }
  }

  const okCount = plan?.items.filter((i) => i.ok).length ?? 0;
  const canStart = !!plan && !plan.aborted && okCount > 0 && !operating && !starting;

  async function start() {
    if (!canStart) return;
    setStarting(true);
    setLocalErr(null);
    try {
      if (tab === "trash") {
        const yes = await ask(
          `Move ${okCount.toLocaleString()} photograph${okCount === 1 ? "" : "s"} to the OS trash? They stay restorable from your system trash.`,
          { title: "Move to trash", kind: "warning" },
        );
        if (!yes) return;
        await api.startTrash(photoIds);
      } else if (tab === "delete-permanently") {
        const yes = await ask(
          `Permanently delete ${okCount.toLocaleString()} photograph${okCount === 1 ? "" : "s"}? This removes the original file${okCount === 1 ? "" : "s"} and cannot be undone. Use system trash if you may need to restore ${okCount === 1 ? "it" : "them"}.`,
          { title: "Permanently delete photographs", kind: "warning" },
        );
        if (!yes) return;
        await api.startPermanentDelete(photoIds);
      } else if (tab === "rename") {
        await api.startGroupRename(photoIds, template, groupName);
      } else {
        await api.startMoveCopy(photoIds, destDir as string, tab, onCollision);
      }
      store().setOperating(true);
      store().setOpSummary(null);
      store().setOpProgress({ total: plan!.items.length, done: 0, stage: tab, current: null });
      setPlan(null);
    } catch (e) {
      setLocalErr(toErrorMessage(e));
    } finally {
      setStarting(false);
    }
  }

  async function stop() {
    try {
      await api.stopOperation();
    } catch (e) {
      store().setError(toErrorMessage(e));
    }
  }

  async function pickDest() {
    const p = await api.pickFolder();
    if (p) setDestDir(p);
  }

  if (photoIds.length === 0) return null;

  return (
    <div className="fileops">
      <div className="fileops-head">
        <span>
          <strong>{photoIds.length.toLocaleString()}</strong> selected photograph{photoIds.length === 1 ? "" : "s"}
        </span>
        <span className="faint">Every file action is previewed before anything touches disk.</span>
        <span className="spacer" />
        {opSummary && !operating && (
          <button className="btn btn-ghost btn-sm" onClick={() => store().setOpSummary(null)}>
            Dismiss results
          </button>
        )}
      </div>

      <div className="fileops-tabs">
        {(["rename", "move", "copy", "trash", "delete-permanently"] as FileOpsTab[]).map((t) => (
          <button
            key={t}
            className={`fileops-tab${tab === t ? " is-on" : ""}${t === "trash" || t === "delete-permanently" ? " is-danger" : ""}`}
            onClick={() => setTab(t)}
            disabled={operating}
          >
            {t === "rename"
              ? "Rename"
              : t === "move"
                ? "Move"
                : t === "copy"
                  ? "Copy"
                  : t === "trash"
                    ? "Trash"
                    : "Delete permanently"}
          </button>
        ))}
      </div>

      <div className="fileops-body">
        {tab === "rename" && (
          <div className="field-col">
            <label>
              Name pattern
              <input
                className="input mono"
                value={template}
                onChange={(e) => setTemplate(e.target.value)}
                placeholder="{date}_{name}_{sequence}"
                disabled={operating}
              />
            </label>
            <div className="field-row">
              {TEMPLATES.map((t) => (
                <button
                  key={t.value}
                  className={`btn btn-ghost btn-sm${template === t.value ? " is-on" : ""}`}
                  onClick={() => setTemplate(t.value)}
                  disabled={operating}
                >
                  {t.label}
                </button>
              ))}
            </div>
            <span className="field-hint mono">{template}</span>
            <label>
              Group name <span className="faint">({"{name}"})</span>
              <input
                className="input"
                value={groupName}
                onChange={(e) => setGroupName(e.target.value)}
                placeholder="Wedding"
                disabled={operating}
              />
            </label>
            <span className="field-hint">
              Tokens: {"{name} {original} {date} {time} {camera} {lens} {focal} {iso} {sequence}".split(" ").join(" · ")} — the file's own extension is always kept.
            </span>
          </div>
        )}

        {(tab === "move" || tab === "copy") && (
          <div className="field-col">
            <label>
              {tab === "move" ? "Move to folder" : "Copy to folder"}
              <span className="field-row">
                <input className="input mono" value={destDir ?? ""} readOnly placeholder="Choose a destination folder…" />
                <button className="btn btn-sm" onClick={pickDest} disabled={operating}>
                  Choose…
                </button>
              </span>
            </label>
            <span className="field-row">
              <span className="faint">If a file already exists there:</span>
              <label className="radio">
                <input
                  type="radio"
                  checked={onCollision === "skip"}
                  onChange={() => setOnCollision("skip")}
                  disabled={operating}
                />
                Skip that file
              </label>
              <label className="radio">
                <input
                  type="radio"
                  checked={onCollision === "avoid-by-renaming"}
                  onChange={() => setOnCollision("avoid-by-renaming")}
                  disabled={operating}
                />
                Avoid by renaming (IMG_0001-1.jpg)
              </label>
            </span>
            {tab === "move" && (
              <span className="field-hint">Move takes the files out of this library's folder; nothing is ever overwritten.</span>
            )}
          </div>
        )}

        {tab === "trash" && (
          <div className="field-col">
            <span className="field-hint field-warn">
              Photos go to your <strong>system trash</strong>. This is the recommended removal option because you can restore them from the OS trash.
            </span>
          </div>
        )}

        {tab === "delete-permanently" && (
          <div className="field-col">
            <span className="field-hint field-warn">
              <strong>Permanent deletion cannot be undone.</strong> The original files are removed without going to system trash. A second native confirmation is required after preview.
            </span>
          </div>
        )}

        {!operating && !opSummary && (
          <div className="fileops-actions">
            <button className="btn" onClick={preview} disabled={!previewEnabled || planning}>
              {planning ? "Previewing…" : "Preview"}
            </button>
            {plan && (
              <button className={`btn${tab === "trash" || tab === "delete-permanently" ? " btn-danger" : " btn-primary"}`} onClick={start} disabled={!canStart}>
                {starting
                  ? "Starting…"
                  : tab === "trash"
                    ? `Move ${okCount.toLocaleString()} to trash`
                    : tab === "delete-permanently"
                      ? `Delete ${okCount.toLocaleString()} permanently`
                      : `${tab === "rename" ? "Rename" : tab === "move" ? "Move" : "Copy"} ${okCount.toLocaleString()} file${okCount === 1 ? "" : "s"}`}
              </button>
            )}
          </div>
        )}

        {operating && (
          <div className="fileops-progress">
            <span className="faint mono">
              {progressLabel(opProgress, opSummary)}
              {opProgress?.current ? ` — ${opProgress.current}` : ""}
            </span>
            <button className="btn btn-sm btn-danger" onClick={stop}>
              Stop
            </button>
          </div>
        )}

        {localErr && (
          <div className="field-err" role="alert">
            {localErr}
          </div>
        )}

        {plan && !operating && <PreviewList plan={plan} onDismiss={() => setPlan(null)} />}

        {opSummary && !operating && <ResultSummary summary={opSummary} />}
      </div>
    </div>
  );
}

function PreviewList({ plan, onDismiss }: { plan: FileOpPlan; onDismiss: () => void }) {
  const flagged = plan.items.filter((i) => !i.ok);
  const shown = plan.items.slice(0, 8);
  const remaining = plan.items.length - shown.length;

  return (
    <div className="preview">
      <div className="preview-head">
        <strong style={plan.aborted ? { color: "var(--danger)" } : undefined}>
          {previewHeadline(plan)}
        </strong>
        {plan.will_create_dir && !plan.aborted && (
          <span className="faint mono">creates {plan.will_create_dir}</span>
        )}
        <span className="spacer" />
        <button className="btn btn-ghost btn-sm" onClick={onDismiss}>
          Dismiss
        </button>
      </div>
      <ul className="preview-list">
        {shown.map((i) => (
          <PreviewKey key={i.photo_id} item={i} op={plan.op} />
        ))}
        {remaining > 0 && <li className="faint mono">…and {remaining.toLocaleString()} more</li>}
      </ul>
      {flagged.length > 0 && (
        <div className="preview-flag">
          {flagged.length.toLocaleString()} skipped:{" "}
          {flagged.slice(0, 5).map((i) => fileBase(i.source)).join(", ")}
          {flagged.length > 5 ? "…" : ""}
          {flagged[0].note ? ` — ${flagged[0].note}` : ""}
        </div>
      )}
    </div>
  );
}

function PreviewKey({ item, op }: { item: PlanItem; op: FileOpPlan["op"] }) {
  return (
    <li className={item.ok ? "" : "is-bad"}>
      <span className="mono">{fileBase(item.source)}</span>
      <span className="preview-arrow">→</span>
      <span className="mono">
        {item.destination
          ? fileBase(item.destination)
          : op === "trash"
            ? "system trash"
            : op === "delete-permanently"
              ? "permanently deleted"
              : "—"}
      </span>
      {item.note && <span className="preview-note">{item.note}</span>}
    </li>
  );
}

function ResultSummary({ summary }: { summary: OperationSummary }) {
  const bad = flaggedResults(summary);
  return (
    <div className="result">
      <div className="result-head">
        <strong>{resultHeadline(summary)}</strong>
        {summary.cancelled && <span className="faint">(stopped)</span>}
        <span className="faint mono">· {(summary.elapsed_ms / 1000).toFixed(1)}s</span>
      </div>
      {bad.length > 0 && (
        <ul className="preview-list">
          {bad.slice(0, 10).map((i, idx) => (
            <li key={idx} className="is-bad">
              <span className="mono">{fileBase(i.source)}</span>
              <span className="preview-note">{i.detail ?? i.status}</span>
            </li>
          ))}
          {bad.length > 10 && <li className="faint mono">…and {bad.length - 10} more (see log)</li>}
        </ul>
      )}
      {bad.length === 0 && <div className="field-hint">Everything succeeded. The audit log keeps the full record.</div>}
    </div>
  );
}
