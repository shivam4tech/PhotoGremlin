export function EmptyState({
  glyph,
  title,
  children,
  action,
}: {
  glyph: React.ReactNode;
  title: string;
  children?: React.ReactNode;
  action?: React.ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="glyph">{glyph}</div>
      <h2>{title}</h2>
      {children && <div>{children}</div>}
      {action}
    </div>
  );
}
