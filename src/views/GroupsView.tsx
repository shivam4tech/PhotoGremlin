import { useEffect, useMemo, useRef, useState } from "react";
import { EmptyState } from "@/components/EmptyState";
import { PhotoTile } from "@/components/PhotoTile";
import { VirtualGrid } from "@/components/VirtualGrid";
import { CoverThumb } from "@/features/similarity/CoverThumb";
import {
  groupDescription,
  groupsForTab,
  mergeGroupPhotos,
  type GroupTab,
} from "@/features/similarity/groups";
import { groupLabel } from "@/features/organize/labels";
import { Viewer } from "@/features/viewer/Viewer";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import type { PhotoSummary, SimilarityGroup } from "@/types/api";

const GROUP_PHOTO_PAGE = 96;
const TABS: readonly GroupTab[] = ["all", "similar", "burst", "face"];

function tabLabel(tab: GroupTab, groups: SimilarityGroup[]): string {
  const count = groupsForTab(groups, tab).length;
  if (tab === "all") return `All ${count}`;
  if (tab === "similar") return `Similar ${count}`;
  if (tab === "burst") return `Bursts ${count}`;
  return `Face appearance ${count}`;
}

export function GroupsView() {
  const activeFolder = useAppStore((state) => state.activeFolder);
  const groups = useAppStore((state) => state.similarityGroups);
  const findingSimilar = useAppStore((state) => state.findingSimilar);
  const libraryVersion = useAppStore((state) => state.libraryVersion);
  const marksVersion = useAppStore((state) => state.marksVersion);
  const loadGroups = useAppStore((state) => state.loadSimilarityGroups);
  const setView = useAppStore((state) => state.setView);
  const [tab, setTab] = useState<GroupTab>("all");
  const [openGroup, setOpenGroup] = useState<SimilarityGroup | null>(null);
  const [photos, setPhotos] = useState<PhotoSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [viewerId, setViewerId] = useState<number | null>(null);
  const requestRef = useRef(0);

  useEffect(() => {
    setOpenGroup(null);
    setPhotos([]);
    setTotal(0);
    setError(null);
    void loadGroups();
  }, [activeFolder, loadGroups]);

  function loadGroup(group: SimilarityGroup, offset: number) {
    const request = ++requestRef.current;
    setLoading(true);
    setError(null);
    void api.groupPhotos(group.id, offset, GROUP_PHOTO_PAGE)
      .then((page) => {
        if (request !== requestRef.current) return;
        setPhotos((current) => mergeGroupPhotos(current, page.photos, offset));
        setTotal(page.total);
      })
      .catch((loadError) => {
        if (request === requestRef.current) setError(toErrorMessage(loadError));
      })
      .finally(() => {
        if (request === requestRef.current) setLoading(false);
      });
  }

  function open(group: SimilarityGroup) {
    setOpenGroup(group);
    setViewerId(null);
    setPhotos([]);
    setTotal(group.photo_count);
    loadGroup(group, 0);
  }

  useEffect(() => {
    if (openGroup) loadGroup(openGroup, 0);
    // The group id is stable across mark/file invalidations; a rebuilt group
    // set is handled by returning to the overview after the pass completes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [libraryVersion, marksVersion]);

  const allGroups = groups ?? [];
  const visibleGroups = useMemo(() => groupsForTab(allGroups, tab), [allGroups, tab]);

  if (openGroup) {
    return (
      <div className="groups-view groups-detail">
        <div className="library-toolbar">
          <button className="btn btn-sm" onClick={() => { setOpenGroup(null); setViewerId(null); }}>← All groups</button>
          <strong>{groupLabel(openGroup.group_type, total || openGroup.photo_count)}</strong>
          <span className="faint groups-detail-description">{groupDescription(openGroup.group_type)}</span>
          <span className="spacer" />
          <span className="faint mono tabular-nums">{photos.length.toLocaleString()} of {total.toLocaleString()}</span>
        </div>
        {error && <div className="groups-error" role="alert">{error}</div>}
        {loading && photos.length === 0 ? (
          <div className="library-loading">Loading grouped photographs…</div>
        ) : photos.length === 0 ? (
          <div className="groups-empty-wrap">
            <EmptyState
              glyph="◫"
              title="This group is empty"
              action={<button className="btn btn-sm" onClick={() => setOpenGroup(null)}>Back to groups</button>}
            >
              <p>Its photographs may have moved or been sent to trash.</p>
            </EmptyState>
          </div>
        ) : (
          <div className="library-grid-area">
            <VirtualGrid
              itemCount={photos.length}
              onReachEnd={() => {
                if (!loading && photos.length < total) loadGroup(openGroup, photos.length);
              }}
              render={(index) => <PhotoTile photo={photos[index]} onOpen={setViewerId} marksMode="always" />}
            />
          </div>
        )}
        {viewerId !== null && (
          <Viewer photoId={viewerId} ordered={photos} onClose={() => setViewerId(null)} onNavigate={setViewerId} />
        )}
      </div>
    );
  }

  return (
    <div className="groups-view">
      <div className="groups-toolbar">
        <div>
          <strong>Photograph groups</strong>
          <span>Deterministic local matches from visual structure and capture time</span>
        </div>
        <span className="spacer" />
        <button className="btn btn-sm" onClick={() => void loadGroups()} disabled={findingSimilar}>Refresh</button>
      </div>

      <div className="groups-overview">
        <div className="groups-tabs" role="tablist" aria-label="Group type">
          {TABS.map((item) => (
            <button
              key={item}
              type="button"
              role="tab"
              aria-selected={tab === item}
              className={`btn btn-sm${tab === item ? " btn-primary" : ""}`}
              onClick={() => setTab(item)}
            >
              {tabLabel(item, allGroups)}
            </button>
          ))}
        </div>

        {groups === null ? (
          <div className="groups-card-grid" aria-label="Loading photograph groups">
            {Array.from({ length: 6 }, (_, index) => <div className="group-card group-card-skeleton" key={index} />)}
          </div>
        ) : visibleGroups.length === 0 ? (
          <div className="groups-empty-wrap">
            <EmptyState
              glyph="⊞"
              title={allGroups.length === 0 ? "No groups yet" : "No groups of this type"}
              action={allGroups.length === 0
                ? <button className="btn btn-sm btn-primary" onClick={() => setView("library")}>Find similar photos in Library</button>
                : <button className="btn btn-sm" onClick={() => setTab("all")}>Show all groups</button>}
            >
              <p>{allGroups.length === 0 ? "Run the local grouping pass to find near-duplicates and same-moment bursts." : "Choose another group type to continue browsing."}</p>
            </EmptyState>
          </div>
        ) : (
          <div className="groups-card-grid">
            {visibleGroups.map((group) => (
              <button className="group-card" key={group.id} onClick={() => open(group)}>
                <span className="group-card-head">
                  <span className="group-label">{groupLabel(group.group_type, group.photo_count)}</span>
                  {group.session_count >= 2 && <span className="chip">{group.session_count} sessions</span>}
                </span>
                <span className="cover-strip">
                  {group.cover_photos.slice(0, 3).map((photoId) => (
                    <CoverThumb key={photoId} photoId={photoId} alt={`${group.group_type} group cover`} />
                  ))}
                  {group.photo_count > 3 && <span className="cover-more">+{group.photo_count - 3}</span>}
                </span>
                <span className="group-card-description">{groupDescription(group.group_type)}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
