import type { PlotLayout, PlotlyConfig } from 'plotly.js-dist-min';

export const MAX_POINTS = 60;
export const CAN_WINDOW_MS = 60_000;
export const CAN_MAX_POINTS = 3000;
export const CAN_FLOW_MAX_POINTS = 6000;

export interface TimedSample {
  t: number;
  v: number;
}

/**
 * Formats epoch milliseconds into local 'YYYY-MM-DD HH:mm:ss.SSS' string for Plotly date axes.
 */
export function formatPlotlyTimestamp(ms: number): string {
  const d = new Date(ms);
  const pad2 = (n: number) => String(n).padStart(2, '0');
  const pad3 = (n: number) => String(n).padStart(3, '0');
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}.${pad3(d.getMilliseconds())}`;
}

/**
 * Retains only samples within `[nowMs - windowMs, nowMs]` (capped at `maxPoints`).
 */
export function pruneWindowSamples(
  samples: TimedSample[],
  nowMs: number,
  windowMs: number = CAN_WINDOW_MS,
  maxPoints: number = CAN_MAX_POINTS
): TimedSample[] {
  const cutoff = nowMs - windowMs;
  const filtered = samples.filter((s) => s.t >= cutoff);
  if (filtered.length > maxPoints) {
    return filtered.slice(filtered.length - maxPoints);
  }
  return filtered;
}

export function fmtVal(v: number | undefined | null, decimals: number = 2): string {
  const n = v === undefined || v === null || isNaN(v) ? 0 : v;
  return n.toFixed(decimals).replace('.', ',');
}

export function getStatus(value: number, config: { min: number; max: number }): string {
  if (value === 0) return 'normal';
  if (value < config.min || value > config.max) return 'alarm';
  if (value > config.max * 0.9 || value < config.min * 1.1) return 'warning';
  return 'normal';
}

export function calcAvg(arr: number[]): number {
  if (arr.length === 0) return 0;
  return arr.reduce((a, b) => a + b, 0) / arr.length;
}

export function calcRms(arr: number[]): number {
  if (arr.length === 0) return 0;
  return Math.sqrt(arr.reduce((a, b) => a + b * b, 0) / arr.length);
}

export const baseLayout: PlotLayout = {
  margin: { t: 10, r: 10, l: 40, b: 30 },
  paper_bgcolor: 'transparent',
  plot_bgcolor: 'transparent',
  font: { color: '#a1a1aa', size: 11, family: 'system-ui, sans-serif' },
  xaxis: { gridcolor: '#27272a', zeroline: false, tickfont: { color: '#71717a' } },
  yaxis: { gridcolor: '#27272a', zeroline: false, tickfont: { color: '#71717a' } },
  showlegend: true,
  legend: { orientation: 'h', y: -0.2, x: 0.5, xanchor: 'center' },
  hovermode: 'x unified'
};

export const plotlyOptions: PlotlyConfig = { responsive: true, displayModeBar: false };
