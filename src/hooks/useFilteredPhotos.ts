import { useCallback, useEffect, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import type { PhotoSummary } from "@/types/api";

/** Page size for the grid (96 ≈ 6×16 on a 4k panel). */
export const PHOTOS_PAGE_SIZE = 96;

export interface FilteredPhotosState {
  photos: PhotoSummary[];
  total: number;
  page: number;
  loading: boolean;
  error: string | null;
  goToPage: (page: number) => void;
  reload: () => void;
}

/**
 * Loads one page of the photo index through the structured filter engine.
 * `filterJson` is the exact wire object ("" = everything). Changing the
 * filter re-loads page 0; `refreshKey` flips when a scan completes so the
 * list re-fetches itself.
 */
export function useFilteredPhotos(
  enabled: boolean,
  filterJson: string,
  refreshKey: string | number | null,
): FilteredPhotosState {
  const [photos, setPhotos] = useState<PhotoSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(
    async (p: number, fj: string) => {
      setLoading(true);
      setError(null);
      try {
        const res = await api.listFilteredPhotos(fj, p * PHOTOS_PAGE_SIZE, PHOTOS_PAGE_SIZE);
        setPhotos(res.photos);
        setTotal(res.total);
        setPage(p);
      } catch (e) {
        setPhotos([]);
        setTotal(0);
        setError(toErrorMessage(e));
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  // (Re)load page 0 when the library changes or the filter changes.
  useEffect(() => {
    if (enabled) void load(0, filterJson);
  }, [enabled, filterJson, refreshKey, load]);

  // Forget contents when the library is cleared.
  useEffect(() => {
    if (!enabled) {
      setPhotos([]);
      setTotal(0);
      setPage(0);
      setError(null);
    }
  }, [enabled]);

  const goToPage = useCallback(
    (p: number) => {
      const maxPage = Math.max(0, Math.ceil(total / PHOTOS_PAGE_SIZE) - 1);
      void load(Math.min(Math.max(0, p), maxPage), filterJson);
    },
    [load, total, filterJson],
  );

  const reload = useCallback(() => {
    void load(page, filterJson);
  }, [load, page, filterJson]);

  return { photos, total, page, loading, error, goToPage, reload };
}
