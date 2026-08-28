import { useEffect, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import { EmptyState } from "@/components/EmptyState";
import { VirtualGrid } from "@/components/VirtualGrid";
import { PhotoTile } from "@/components/PhotoTile";
import { Viewer } from "@/features/viewer/Viewer";
import { cleanName } from "@/features/organize/labels";
import { PHOTOS_PAGE_SIZE } from "@/hooks/useFilteredPhotos";
import type { Collection, PhotoSummary } from "@/types/api";

function fmtDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

/** One opened collection: its photographs in the standard grid + viewer. */
function CollectionGrid({ col }: { col: Collection }) {
  const [photos, setPhotos] = useState<PhotoSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [viewerId, setViewerId] = useState<number | null>(null);

  function load(offset: number) {
    setLoading(true);
    setError(null);
    api
      .collectionPhotos(col.id, offset, PHOTOS_PAGE_SIZE)
      .then((res) => {
        setPhotos(res.photos);
        setTotal(res.total);
      })
      .catch((e) => setError(toErrorMessage(e)))
      .finally(() => setLoading(false));
  }

  // (Re)load when the collection changes or a file operation changed the
  // library on disk (a member may have been trashed/moved).
  const libraryVersion = useAppStore((s) => s.libraryVersion);
  const marksVersion = useAppStore((s) => s.marksVersion);
  useEffect(() => {
    load(0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [col.id, libraryVersion, marksVersion]);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div className="library-toolbar">
        <span className="mono">{col.name}</span>
        <span className="faint" style={{ fontSize: 12 }}>
          {total.toLocaleString()} photograph{total === 1 ? "" : "s"}
        </span>
        <span className="spacer" />
        <span className="faint" style={{ fontSize: 12 }}>
          Curated set — photographs are never moved or modified
        </span>
      </div>
      {error && (
        <div style={{ padding: "8px 12px", background: "var(--danger-soft)", color: "var(--danger)", fontSize: 12.5 }}>
          {error}
        </div>
      )}
      {!loading && photos.length === 0 ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <EmptyState glyph="◫" title="This collection is empty">
            <p>
              In the Library, enter culling mode, mark photographs to keep, and
              use “Add to collection” on the culling bar.
            </p>
          </EmptyState>
        </div>
      ) : (
        <div className="library-grid-area" style={{ flex: 1 }}>
          <VirtualGrid
            itemCount={photos.length}
            render={(i) => (
              <PhotoTile photo={photos[i]} onOpen={setViewerId} marksMode="always" />
            )}
          />
        </div>
      )}
      {viewerId !== null && (
        <Viewer
          photoId={viewerId}
          ordered={photos}
          onClose={() => setViewerId(null)}
          onNavigate={setViewerId}
        />
      )}
      <span className="faint" style={{ fontSize: 11.5, padding: "6px 4px" }}>
        {total > photos.length ? "Showing the first page of the collection." : ""}
      </span>
      {total > photos.length && (
        <button className="btn btn-sm" onClick={() => load(photos.length)} disabled={loading}>
          Load more
        </button>
      )}
    </div>
  );
}

/**
 * Collections (Sprint 8): manually curated sets. Creating, opening, and
 * deleting sets; membership is edited from the Library's culling bar.
 */
export function CollectionsView() {
  const activeFolder = useAppStore((s) => s.activeFolder);
  const collections = useAppStore((s) => s.collections);
  const store = useAppStore.getState;
  const [error, setError] = useState<string | null>(null);
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);
  const [openId, setOpenId] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      await store().loadCollections();
      if (!cancelled) setOpenId(null);
    })();
    return () => {
      cancelled = true;
    };
  }, [activeFolder, store]);

  async function create() {
    const name = cleanName(newName);
    if (name === null) {
      setError("Collection name must be 1–60 characters.");
      return;
    }
    setCreating(true);
    setError(null);
    try {
      const id = await api.createCollection(name, null);
      setNewName("");
      await store().loadCollections();
      setOpenId(id);
    } catch (e) {
      setError(toErrorMessage(e));
    } finally {
      setCreating(false);
    }
  }

  async function remove(id: number) {
    setError(null);
    try {
      await api.deleteCollection(id);
      if (openId === id) setOpenId(null);
      await store().loadCollections();
      store().setNotice("Collection deleted. Its photographs were not touched.");
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  if (error) {
    return (
      <div
        style={{
          padding: "10px 14px",
          borderRadius: 8,
          background: "var(--danger-soft)",
          color: "var(--danger)",
          fontSize: 12.5,
        }}
      >
        {error}
      </div>
    );
  }

  const open = collections?.find((c) => c.id === openId) ?? null;
  if (open) {
    return (
      <div>
        <div style={{ marginBottom: 10 }}>
          <button className="btn btn-sm btn-ghost" onClick={() => setOpenId(null)}>
            ← All collections
          </button>
        </div>
        <CollectionGrid col={open} />
      </div>
    );
  }

  if (!collections) {
    return <div className="faint" style={{ padding: 20 }}>Loading collections…</div>;
  }

  if (collections.length === 0) {
    return (
      <EmptyState glyph="▣" title="No collections yet">
        <p>
          Collections are manually curated sets — the photographs you deliberately
          group together. Create one below, then in the Library's culling bar add
          marked photographs to it.
        </p>
        <CreateRow
          value={newName}
          onChange={setNewName}
          onCreate={() => void create()}
          creating={creating}
        />
      </EmptyState>
    );
  }

  return (
    <div>
      <div className="card">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h3>Collections ({collections.length})</h3>
          <span className="faint" style={{ fontSize: 12 }}>
            Opening a collection shows its photographs
          </span>
        </div>
        <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
          <input
            className="input"
            style={{ flex: 1, maxWidth: 360 }}
            placeholder="New collection name…"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void create();
            }}
            aria-label="New collection name"
          />
          <button className="btn btn-primary btn-sm" onClick={() => void create()} disabled={creating}>
            {creating ? "Creating…" : "Create collection"}
          </button>
        </div>
        <table className="table" style={{ marginTop: 10 }}>
          <thead>
            <tr>
              <th>Collection</th>
              <th style={{ textAlign: "right" }}>Photographs</th>
              <th>Created</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {collections.map((c) => (
              <tr key={c.id}>
                <td style={{ fontWeight: 600 }}>
                  <button
                    className="btn btn-sm btn-ghost"
                    style={{ padding: 0, fontSize: 13, textAlign: "left" }}
                    onClick={() => setOpenId(c.id)}
                  >
                    {c.name}
                  </button>
                </td>
                <td className="mono" style={{ textAlign: "right" }}>
                  {c.photo_count.toLocaleString()}
                </td>
                <td className="faint" style={{ fontSize: 12 }}>
                  {fmtDate(c.created_at)}
                </td>
                <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                  <button className="btn btn-ghost btn-sm" onClick={() => setOpenId(c.id)}>
                    Open
                  </button>{" "}
                  <button className="btn btn-ghost btn-sm" onClick={() => void remove(c.id)}>
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function CreateRow({
  value,
  onChange,
  onCreate,
  creating,
}: {
  value: string;
  onChange: (v: string) => void;
  onCreate: () => void;
  creating: boolean;
}) {
  return (
    <div style={{ display: "flex", gap: 8, justifyContent: "center", marginTop: 6 }}>
      <input
        className="input"
        style={{ maxWidth: 320 }}
        placeholder="New collection name…"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") onCreate();
        }}
        aria-label="New collection name"
      />
      <button className="btn btn-primary btn-sm" onClick={onCreate} disabled={creating}>
        {creating ? "Creating…" : "Create collection"}
      </button>
    </div>
  );
}
