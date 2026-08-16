import { EmptyState } from "@/components/EmptyState";

export function CollectionsView() {
  return (
    <EmptyState glyph="▣" title="No collections yet">
      <p>
        Collections are manually curated sets — the photographs you deliberately group
        together. They stay separate from saved views, which are dynamic filters.
      </p>
    </EmptyState>
  );
}
