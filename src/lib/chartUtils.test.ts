import { describe, it, expect } from 'vitest';
import {
  MAX_POINTS,
  CAN_WINDOW_MS,
  CAN_MAX_POINTS,
  CAN_FLOW_MAX_POINTS,
  formatPlotlyTimestamp,
  pruneWindowSamples,
  fmtVal,
  getStatus,
  calcAvg,
  calcRms,
  baseLayout,
  plotlyOptions
} from './chartUtils';

describe('chartUtils unit tests', () => {
  it('exposes expected constants and plotly defaults', () => {
    expect(MAX_POINTS).toBe(60);
    expect(CAN_WINDOW_MS).toBe(60_000);
    expect(CAN_MAX_POINTS).toBe(3000);
    expect(CAN_FLOW_MAX_POINTS).toBe(6000);
    expect(baseLayout.paper_bgcolor).toBe('transparent');
    expect(plotlyOptions.responsive).toBe(true);
    expect(plotlyOptions.displayModeBar).toBe(false);
  });

  it('formats and calculates stats consistently', () => {
    expect(fmtVal(12.345, 2)).toBe('12,35');
    expect(getStatus(10, { min: 0, max: 100 })).toBe('normal');
    expect(calcAvg([2, 4, 6])).toBe(4);
    expect(calcRms([1, 1, 1, 1])).toBe(1);
  });

  it('formats local timestamps for Plotly date axes', () => {
    const sample = new Date(2026, 10, 24, 23, 5, 9, 42).getTime();
    expect(formatPlotlyTimestamp(sample)).toBe('2026-11-24 23:05:09.042');
  });

  it('prunes samples older than 60s window and caps maxPoints', () => {
    const now = 100_000;
    const samples = [
      { t: 39_999, v: 1 }, // older than 60_000ms ago -> removed
      { t: 40_000, v: 2 }, // exact boundary -> kept
      { t: 75_000, v: 3 },
      { t: 100_000, v: 4 }
    ];
    expect(pruneWindowSamples(samples, now)).toEqual([
      { t: 40_000, v: 2 },
      { t: 75_000, v: 3 },
      { t: 100_000, v: 4 }
    ]);

    expect(pruneWindowSamples(samples, now, CAN_WINDOW_MS, 2)).toEqual([
      { t: 75_000, v: 3 },
      { t: 100_000, v: 4 }
    ]);
  });
});
