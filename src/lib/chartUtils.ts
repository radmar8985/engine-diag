import type { PlotLayout, PlotlyConfig } from 'plotly.js-dist-min';

export const MAX_POINTS = 60;

export function fmtVal(v: number | undefined | null, decimals: number = 2): string {
  if (v === undefined || v === null || isNaN(v)) return (0).toFixed(decimals).replace('.', ',');
  return v.toFixed(decimals).replace('.', ',');
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
