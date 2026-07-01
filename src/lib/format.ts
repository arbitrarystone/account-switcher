/** 秒 → 友好时长（如「3 小时 20 分」）。 */
export function formatDuration(sec: number): string {
  if (sec <= 0) return "0 秒";
  if (sec < 60) return `${sec} 秒`;
  const m = Math.floor(sec / 60);
  if (m < 60) return `${m} 分钟`;
  const h = Math.floor(m / 60);
  return `${h} 小时 ${m % 60} 分`;
}
