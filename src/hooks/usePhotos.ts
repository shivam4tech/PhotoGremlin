import { useCallback, useEffect, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import type { PhotoSummary } from "@/types/api";

/** Tiles per page. 96 ≈ 6×16 on a 4k panel — small enough that one IPC
 * round-trip returns fast, large enough that scrolling rarely needs more. */
export const PHOTOS_PAGE_SIZE = 96;

export interface PhotosState {
  photos: PhotoSummary[];
  total: number;
  page: number;
  loading: boolean;
  error: string | null;
  goToPage: (page: number) => void;
  reload: () => void;
}

/**
 * Loads one page of the photo index and exposes page navigation.
 * `enabled` flips when the active library (re)exists so a scan completion
 * triggers a natural refetch (the view bumps `refreshKey`).
 */
export function usePhotos(enabled: boolean, refreshKey: string | number | null): PhotosState {
  const [photos, setPhotos] = useState<PhotoSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (p: number) => {
    setLoading(true);
    setError(null);
    try {
      const res = await api.listPhotos(p * PHOTOS_PAGE_SIZE, PHOTOS_PAGE_SIZE);
      setPhotos(res.photos);
      setTotal(res.total);
      setPage(p);
    } catch (e) {
      setPhotos([]);
      setError(toErrorMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  // (Re)load page 0 whenever the library changes.
  // (Re)load page 0 whenever the library changes; the (memoized)
  // refreshKey flips when a scan completes so the index refetches itself.
  useEffect(() => {
    if (enabled) void load(0);
  }, [enabled, refreshKey, load]);

  // Forget the old library's contents when the folder is cleared.
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
      void load(Math.min(Math.max(0, p), maxPage));
    },
    [load, total],
  );

  const reload = useCallback(() => {
    void load(page);
  }, [load, page]);

  return { photos, total, page, loading, error, goToPage, reload };
}
