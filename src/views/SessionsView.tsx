import { EmptyState } from "@/components/EmptyState";

export function SessionsView() {
  return (
    <EmptyState glyph="◱" title="No sessions yet">
      <p>
        A session is a shoot or an imported body of work. When you scan a folder, a
        session is created for it automatically — then you can compare shots across
        sessions: sharpness, ISO, focal lengths, faces, selection ratios.
      </p>
    </EmptyState>
  );
}
