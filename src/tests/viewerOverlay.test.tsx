// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Viewer } from "@/features/viewer/Viewer";
import type { PhotoSummary } from "@/types/api";

vi.mock("@/lib/ipc", () => ({
  api: {
    getPhotoFull: vi.fn(() => new Promise(() => {})),
    getThumbnail: vi.fn(() => new Promise(() => {})),
  },
  toErrorMessage: (error: unknown) => String(error),
}));

vi.mock("@/stores/appStore", () => ({
  useAppStore: (selector: (state: { updateMarks: ReturnType<typeof vi.fn> }) => unknown) =>
    selector({ updateMarks: vi.fn() }),
}));

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

let root: Root;
let container: HTMLDivElement;

beforeEach(() => {
  container = document.createElement("div");
  container.style.transform = "translateX(0)";
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

describe("photo viewer overlay", () => {
  it("portals outside transformed view containers", async () => {
    const ordered = [{ id: 7 }] as PhotoSummary[];

    await act(async () => {
      root.render(
        <Viewer
          photoId={7}
          ordered={ordered}
          onClose={vi.fn()}
          onNavigate={vi.fn()}
        />,
      );
    });

    const backdrop = document.body.querySelector<HTMLElement>(".viewer-backdrop");
    expect(backdrop?.parentElement).toBe(document.body);
    expect(container.querySelector(".viewer-backdrop")).toBeNull();
    expect(backdrop?.querySelector('[role="dialog"][aria-modal="true"]')).not.toBeNull();
  });
});
