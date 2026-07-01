import { useMemo, useState } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { useTokenUsage } from "../../hooks/useTokenUsage";
import { formatDayLabel, formatTokenCount } from "../../lib/format";
import type { Account } from "../../lib/types";
import type { TokenUsagePoint } from "../../lib/api";

type RangePreset = "today" | "7d" | "30d" | "custom";

const PRESET_LABELS: Record<RangePreset, string> = {
  today: "今天",
  "7d": "近 7 天",
  "30d": "近 30 天",
  custom: "自定义",
};

/** 按账号循环取色，不够时从头复用。 */
const PALETTE = [
  "oklch(74% 0.13 184)",
  "oklch(74% 0.15 45)",
  "oklch(76% 0.13 160)",
  "oklch(78% 0.15 300)",
  "oklch(74% 0.17 20)",
  "oklch(80% 0.13 90)",
];

function toISODate(d: Date): string {
  return d.toISOString().slice(0, 10);
}

/** 用 UTC 日期算范围——和后端按 UTC 时间戳前 10 位分桶保持一致，避免时区错位。 */
function computeRange(
  preset: RangePreset,
  customStart: string,
  customEnd: string,
): { startDate: string; endDate: string } {
  const today = toISODate(new Date());
  if (preset === "custom") {
    return { startDate: customStart || today, endDate: customEnd || today };
  }
  const daysBack = preset === "today" ? 0 : preset === "7d" ? 6 : 29;
  const start = new Date();
  start.setUTCDate(start.getUTCDate() - daysBack);
  return { startDate: toISODate(start), endDate: today };
}

/** 范围内每一天都出一行（哪怕当天没数据），保证图表横轴连续不断档。 */
function enumerateDays(start: string, end: string): string[] {
  const days: string[] = [];
  let cursor = new Date(`${start}T00:00:00Z`).getTime();
  const endMs = new Date(`${end}T00:00:00Z`).getTime();
  while (cursor <= endMs && days.length < 366) {
    days.push(toISODate(new Date(cursor)));
    cursor += 86_400_000;
  }
  return days;
}

interface DayRow {
  day: string;
  [accountId: string]: string | number;
}

interface AccountTotals {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  total: number;
}

function sumByAccount(points: TokenUsagePoint[]): Map<string, AccountTotals> {
  const totals = new Map<string, AccountTotals>();
  for (const p of points) {
    const t = totals.get(p.accountId) ?? {
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      total: 0,
    };
    t.input += p.inputTokens;
    t.output += p.outputTokens;
    t.cacheRead += p.cacheReadTokens;
    t.cacheWrite += p.cacheWriteTokens;
    t.total += p.totalTokens;
    totals.set(p.accountId, t);
  }
  return totals;
}

interface UsageDashboardProps {
  accounts: Account[];
  onClose: () => void;
}

/** 用量统计整页：按时间段展示各账号的真实 token 用量（非仅次数）。 */
function UsageDashboard({ accounts, onClose }: UsageDashboardProps) {
  const [preset, setPreset] = useState<RangePreset>("7d");
  const [customStart, setCustomStart] = useState("");
  const [customEnd, setCustomEnd] = useState("");
  const [hiddenIds, setHiddenIds] = useState<Set<string>>(new Set());

  const { startDate, endDate } = useMemo(
    () => computeRange(preset, customStart, customEnd),
    [preset, customStart, customEnd],
  );
  const { points, loading, error } = useTokenUsage(startDate, endDate);

  const visibleAccounts = accounts.filter((a) => !hiddenIds.has(a.id));
  const visiblePoints = points.filter((p) => !hiddenIds.has(p.accountId));

  const dayRows = useMemo<DayRow[]>(() => {
    const days = enumerateDays(startDate, endDate);
    const byDay = new Map<string, DayRow>(days.map((d) => [d, { day: d }]));
    for (const p of visiblePoints) {
      const row = byDay.get(p.day);
      if (!row) continue;
      row[p.accountId] = (Number(row[p.accountId]) || 0) + p.totalTokens;
    }
    return days.map((d) => byDay.get(d) as DayRow);
  }, [startDate, endDate, visiblePoints]);

  const totalsByAccount = useMemo(() => sumByAccount(visiblePoints), [visiblePoints]);
  const grandTotal = [...totalsByAccount.values()].reduce((s, t) => s + t.total, 0);

  const colorFor = (accountId: string) => {
    const idx = accounts.findIndex((a) => a.id === accountId);
    return PALETTE[idx % PALETTE.length] ?? PALETTE[0];
  };

  const detailRows = [...visiblePoints].sort((a, b) => b.day.localeCompare(a.day));

  return (
    <div className="usage-dashboard">
      <header className="usage-head">
        <h2 className="usage-title">用量统计</h2>
        <button className="btn btn-ghost" onClick={onClose}>
          返回工作台
        </button>
      </header>

      {accounts.length === 0 ? (
        <div className="usage-empty">还没有账号，先去创建一个吧</div>
      ) : (
        <>
          <div className="usage-controls">
            <div className="usage-range-tabs" role="tablist" aria-label="时间范围">
              {(Object.keys(PRESET_LABELS) as RangePreset[]).map((p) => (
                <button
                  key={p}
                  role="tab"
                  aria-selected={preset === p}
                  className="usage-range-tab"
                  onClick={() => setPreset(p)}
                >
                  {PRESET_LABELS[p]}
                </button>
              ))}
            </div>
            {preset === "custom" && (
              <div className="usage-custom-range">
                <input
                  type="date"
                  className="field-input"
                  value={customStart}
                  max={customEnd || undefined}
                  onChange={(e) => setCustomStart(e.target.value)}
                />
                <span className="usage-range-sep">至</span>
                <input
                  type="date"
                  className="field-input"
                  value={customEnd}
                  min={customStart || undefined}
                  onChange={(e) => setCustomEnd(e.target.value)}
                />
              </div>
            )}
          </div>

          {accounts.length > 1 && (
            <div className="usage-account-chips">
              {accounts.map((a) => {
                const on = !hiddenIds.has(a.id);
                return (
                  <button
                    key={a.id}
                    className="usage-chip"
                    aria-pressed={on}
                    onClick={() =>
                      setHiddenIds((prev) => {
                        const next = new Set(prev);
                        if (on) next.add(a.id);
                        else next.delete(a.id);
                        return next;
                      })
                    }
                  >
                    <span
                      className="usage-chip-dot"
                      style={{ background: colorFor(a.id) }}
                      aria-hidden="true"
                    />
                    {a.name}
                  </button>
                );
              })}
            </div>
          )}

          {loading ? (
            <div className="usage-empty">加载中…</div>
          ) : error ? (
            <div className="usage-empty usage-empty-error">{error}</div>
          ) : visiblePoints.length === 0 ? (
            <div className="usage-empty">
              这个时间范围内还没有 token 用量数据
              <br />
              起个任务、结束会话后回来看看
            </div>
          ) : (
            <>
              <section className="usage-summary-cards">
                <div className="usage-card usage-card-total">
                  <span className="usage-card-label">合计</span>
                  <span className="usage-card-value">{formatTokenCount(grandTotal)}</span>
                </div>
                {visibleAccounts.map((a) => {
                  const t = totalsByAccount.get(a.id);
                  if (!t) return null;
                  return (
                    <div className="usage-card" key={a.id}>
                      <span
                        className="usage-card-dot"
                        style={{ background: colorFor(a.id) }}
                        aria-hidden="true"
                      />
                      <span className="usage-card-label">{a.name}</span>
                      <span className="usage-card-value">{formatTokenCount(t.total)}</span>
                      <span className="usage-card-sub">
                        输入 {formatTokenCount(t.input)} · 输出 {formatTokenCount(t.output)}
                      </span>
                    </div>
                  );
                })}
              </section>

              <div className="usage-chart">
                <ResponsiveContainer width="100%" height={260}>
                  <BarChart data={dayRows} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
                    <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" vertical={false} />
                    <XAxis
                      dataKey="day"
                      tickFormatter={(v) => formatDayLabel(String(v))}
                      stroke="var(--text-faint)"
                      fontSize={12}
                      tickLine={false}
                    />
                    <YAxis
                      tickFormatter={(v) => formatTokenCount(Number(v))}
                      stroke="var(--text-faint)"
                      fontSize={12}
                      tickLine={false}
                      axisLine={false}
                      width={48}
                    />
                    <Tooltip
                      formatter={(value) =>
                        typeof value === "number" ? formatTokenCount(value) : String(value)
                      }
                      contentStyle={{
                        background: "var(--bg-elevated)",
                        border: "1px solid var(--border)",
                        borderRadius: "var(--radius-sm)",
                        fontSize: "var(--text-sm)",
                      }}
                    />
                    <Legend wrapperStyle={{ fontSize: "var(--text-xs)" }} />
                    {visibleAccounts.map((a) => (
                      <Bar
                        key={a.id}
                        dataKey={a.id}
                        name={a.name}
                        stackId="tokens"
                        fill={colorFor(a.id)}
                        radius={2}
                      />
                    ))}
                  </BarChart>
                </ResponsiveContainer>
              </div>

              <div className="usage-table-wrap">
                <table className="usage-table">
                  <thead>
                    <tr>
                      <th>日期</th>
                      <th>账号</th>
                      <th>输入</th>
                      <th>输出</th>
                      <th>缓存读</th>
                      <th>缓存写</th>
                      <th>合计</th>
                    </tr>
                  </thead>
                  <tbody>
                    {detailRows.map((p) => (
                      <tr key={`${p.day}::${p.accountId}`}>
                        <td className="detail-mono">{p.day}</td>
                        <td>{accounts.find((a) => a.id === p.accountId)?.name ?? p.accountId}</td>
                        <td>{formatTokenCount(p.inputTokens)}</td>
                        <td>{formatTokenCount(p.outputTokens)}</td>
                        <td>{formatTokenCount(p.cacheReadTokens)}</td>
                        <td>{formatTokenCount(p.cacheWriteTokens)}</td>
                        <td>
                          <strong>{formatTokenCount(p.totalTokens)}</strong>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}

          <p className="usage-disclaimer">
            数据来源：account-switcher 的终端只是透传，看不到 API 流量本身；这里是事后按项目目录
            + 时间窗跟 claude / codex 自己写在本地的会话日志对账得到的。如果中转没有返回标准的
            usage 字段、或本地日志还没落盘，对应时段会显示为 0——不代表确认没有用量。app
            外手动跑的同项目会话也可能被计入。
          </p>
        </>
      )}
    </div>
  );
}

export default UsageDashboard;
