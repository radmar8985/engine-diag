import type { ThresholdRange, MetricStatus, FccuState } from '$lib/types';

/**
 * Formats a number with Polish comma decimal separator, defaulting null/undefined/NaN to 0.
 */
export function fmtVal(v: number | undefined | null, decimals: number = 2): string {
  if (v === undefined || v === null || Number.isNaN(v)) {
    return (0).toFixed(decimals).replace('.', ',');
  }
  return v.toFixed(decimals).replace('.', ',');
}

/**
 * Evaluates metric status (normal, warning, alarm) based on min/max threshold limits.
 */
export function getStatus(value: number, config: ThresholdRange): MetricStatus {
  if (value === 0) return 'normal';
  if (value < config.min || value > config.max) return 'alarm';
  if (value > config.max * 0.9 || value < config.min * 1.1) return 'warning';
  return 'normal';
}

/**
 * Calculates arithmetic average of a number array.
 */
export function calcAvg(arr: readonly number[]): number {
  if (arr.length === 0) return 0;
  return arr.reduce((acc, curr) => acc + curr, 0) / arr.length;
}

/**
 * Calculates Root Mean Square (RMS) of a number array.
 */
export function calcRms(arr: readonly number[]): number {
  if (arr.length === 0) return 0;
  return Math.sqrt(arr.reduce((acc, curr) => acc + curr * curr, 0) / arr.length);
}

/**
 * Returns human-readable Polish label for FCCU state code.
 */
export function getFccuStateLabel(state: FccuState | number): string {
  switch (state) {
    case 0:
      return 'Inicjalizacja';
    case 1:
      return 'Uruchamianie';
    case 2:
      return 'Włączony';
    case 3:
      return 'Wyłączony';
    default:
      return 'Brak odczytu';
  }
}

/**
 * Returns UI color indicator for FCCU state code.
 */
export function getFccuStateColor(state: FccuState | number): string {
  switch (state) {
    case 0:
      return '#3b82f6';
    case 1:
      return '#f59e0b';
    case 2:
      return '#22c55e';
    case 3:
      return '#f59e0b'; // Amber yellow matching mockup
    default:
      return '#71717a';
  }
}
