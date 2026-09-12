// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "@/lib/ipc";
import { useFilteredPhotos, type FilteredPhotosState } from "@/hooks/useFilteredPhotos";
import type { PhotoPage, PhotoSummary } from "@/types/api";

vi.mock("@/lib/ipc", () => ({ api: { listFilteredPhotos: vi.fn() }, toErrorMessage: String }));
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
let root: Root;
let container: HTMLDivElement;
let state: FilteredPhotosState;
const photo = (id: number) => ({ id } as PhotoSummary);
function pendingPage() {
  let resolve!: (page: PhotoPage) => void;
  const promise = new Promise<PhotoPage>((done) => { resolve = done; });
  return { promise, resolve };
}
function Harness({ filter = "", enabled = true }) {
  state = useFilteredPhotos(enabled, filter, 1);
  return null;
}
beforeEach(() => {
  vi.resetAllMocks();
  container = document.createElement("div"); document.body.append(container); root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); });

describe("live filter loading", () => {
  it("retains photos while loading and ignores obsolete responses", async () => {
    vi.mocked(api.listFilteredPhotos).mockResolvedValueOnce({ photos: [photo(1)], total: 1 });
    await act(async () => root.render(<Harness />));
    const old = pendingPage(); const latest = pendingPage();
    vi.mocked(api.listFilteredPhotos).mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise);
    await act(async () => root.render(<Harness filter="old" />));
    expect(state.photos[0].id).toBe(1); expect(state.loading).toBe(true);
    await act(async () => root.render(<Harness filter="latest" />));
    await act(async () => latest.resolve({ photos: [photo(3)], total: 1 }));
    await act(async () => old.resolve({ photos: [photo(2)], total: 20 }));
    expect(state.photos.map((item) => item.id)).toEqual([3]);
    expect(state.total).toBe(1); expect(state.loading).toBe(false);
  });
  it("guards duplicate load-more requests and reloads from page zero", async () => {
    vi.mocked(api.listFilteredPhotos).mockResolvedValueOnce({ photos: [photo(1)], total: 200 });
    await act(async () => root.render(<Harness />));
    const next = pendingPage();
    vi.mocked(api.listFilteredPhotos).mockReturnValueOnce(next.promise);
    await act(async () => { state.loadMore(); state.loadMore(); });
    expect(api.listFilteredPhotos).toHaveBeenCalledTimes(2);
    await act(async () => next.resolve({ photos: [photo(2)], total: 200 }));
    expect(state.photos.map((item) => item.id)).toEqual([1, 2]);
    vi.mocked(api.listFilteredPhotos).mockResolvedValueOnce({ photos: [photo(4)], total: 1 });
    await act(async () => state.reload());
    expect(api.listFilteredPhotos).toHaveBeenLastCalledWith("", 0, 96);
    expect(state.photos.map((item) => item.id)).toEqual([4]);
  });
  it("does not restore data when an in-flight request completes after closing a project", async () => {
    const page = pendingPage(); vi.mocked(api.listFilteredPhotos).mockReturnValueOnce(page.promise);
    await act(async () => root.render(<Harness />));
    await act(async () => root.render(<Harness enabled={false} />));
    await act(async () => page.resolve({ photos: [photo(1)], total: 1 }));
    expect(state.photos).toEqual([]); expect(state.total).toBe(0); expect(state.loading).toBe(false);
  });
});
