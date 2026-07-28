interface MetricCardProps {
  label: string;
  value: string;
  detail: string;
  tone?: "sage" | "amber" | "ink";
}

export function MetricCard({
  label,
  value,
  detail,
  tone = "ink",
}: MetricCardProps) {
  return (
    <article className={`metric-card metric-card--${tone}`}>
      <p>{label}</p>
      <strong>{value}</strong>
      <span>{detail}</span>
    </article>
  );
}
