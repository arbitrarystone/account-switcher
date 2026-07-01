/** 秒 → 友好时长（如「3 小时 20 分」）。 */
export function formatDuration(sec: number): string {
  if (sec <= 0) return "0 秒";
  if (sec < 60) return `${sec} 秒`;
  const m = Math.floor(sec / 60);
  if (m < 60) return `${m} 分钟`;
  const h = Math.floor(m / 60);
  return `${h} 小时 ${m % 60} 分`;
}

/** token 数 → 紧凑展示（如 850 / 1.2k / 12k / 3.4M）。 */
export function formatTokenCount(n: number): string {
  const abs = Math.abs(n);
  if (abs < 1000) return String(n);
  if (abs < 1_000_000) {
    const k = n / 1000;
    return `${abs < 10_000 ? k.toFixed(1) : Math.round(k)}k`;
  }
  const m = n / 1_000_000;
  return `${abs < 10_000_000 ? m.toFixed(1) : Math.round(m)}M`;
}

/** `YYYY-MM-DD` → 图表/明细表用的短日期标签（如 `06/30`）。 */
export function formatDayLabel(day: string): string {
  const parts = day.split("-");
  return parts.length === 3 ? `${parts[1]}/${parts[2]}` : day;
}
