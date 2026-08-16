export function StatCard({
  label,
  value,
  sub,
}: {
  label: string;
  value: string | number;
  sub?: string;
}) {
  return (
    <div className="stat-card">
      <div className="label">{label}</div>
      <div className="value">{typeof value === "number" ? value.toLocaleString() : value}</div>
      {sub && <div className="sub">{sub}</div>}
    </div>
  );
}
