import { EmptyState } from "@/components/EmptyState";

export function SavedViewsView() {
  return (
    <EmptyState glyph="≣" title="No saved views yet">
      <p>
        A saved view stores a filter — not a list of images. Save “Sharp portraits, ISO
        below 1600” once, and it keeps working as your library changes.
      </p>
    </EmptyState>
  );
}
