export function ProgressBar({
  value,
  max,
  label,
}: {
  value: number;
  max: number;
  label?: string;
}) {
  const pct = max > 0 ? Math.min(100, (value / max) * 100) : 0;
  return (
    <div>
      <div className="progress-track">
        <div className="progress-fill" style={{ width: `${pct}%` }} />
      </div>
      {label && (
        <div className="faint" style={{ fontSize: 11.5, marginTop: 6 }} data-tid={label}>
          {label}
        </div>
      )}
    </div>
  );
}
