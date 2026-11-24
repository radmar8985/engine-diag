import { describe, it, expect } from 'vitest';
import {
  fmtVal,
  getStatus,
  calcAvg,
  calcRms,
  getFccuStateLabel,
  getFccuStateColor,
} from './formatters';

describe('formatters unit tests', () => {
  describe('fmtVal', () => {
    it('formats numbers with Polish comma decimal separator', () => {
      expect(fmtVal(48.123, 1)).toBe('48,1');
      expect(fmtVal(12.5, 2)).toBe('12,50');
      expect(fmtVal(0, 0)).toBe('0');
      expect(fmtVal(-3.456, 2)).toBe('-3,46');
    });

    it('defaults null, undefined, and NaN to zero formatted string', () => {
      expect(fmtVal(null, 2)).toBe('0,00');
      expect(fmtVal(undefined, 1)).toBe('0,0');
      expect(fmtVal(Number.NaN, 2)).toBe('0,00');
    });
  });

  describe('getStatus', () => {
    const range = { min: 40, max: 52 };

    it('treats exact zero as normal (idle reading)', () => {
      expect(getStatus(0, range)).toBe('normal');
    });

    it('returns normal when comfortably within bounds', () => {
      expect(getStatus(45.5, range)).toBe('normal');
    });

    it('returns warning within 10% boundary margins', () => {
      // max * 0.9 = 46.8 -> 47.0 is warning
      expect(getStatus(47.0, range)).toBe('warning');
      // min * 1.1 = 44.0 -> 43.0 is warning
      expect(getStatus(43.0, range)).toBe('warning');
    });

    it('returns alarm when outside min/max range', () => {
      expect(getStatus(39.9, range)).toBe('alarm');
      expect(getStatus(52.1, range)).toBe('alarm');
    });
  });

  describe('calcAvg & calcRms', () => {
    it('returns 0 for empty arrays', () => {
      expect(calcAvg([])).toBe(0);
      expect(calcRms([])).toBe(0);
    });

    it('computes arithmetic mean accurately', () => {
      expect(calcAvg([10, 20, 30])).toBe(20);
      expect(calcAvg([-5, 5])).toBe(0);
    });

    it('computes root mean square accurately', () => {
      expect(calcRms([3, 4])).toBeCloseTo(Math.sqrt(12.5), 6);
      expect(calcRms([-2, 2, -2, 2])).toBeCloseTo(2, 6);
    });
  });

  describe('FCCU state labels and colors', () => {
    it('maps known FCCU state codes to Polish labels and colors', () => {
      expect(getFccuStateLabel(0)).toBe('Inicjalizacja');
      expect(getFccuStateLabel(1)).toBe('Uruchamianie');
      expect(getFccuStateLabel(2)).toBe('Włączony');
      expect(getFccuStateLabel(3)).toBe('Wyłączony');
      expect(getFccuStateLabel(99)).toBe('Brak odczytu');

      expect(getFccuStateColor(0)).toBe('#3b82f6');
      expect(getFccuStateColor(2)).toBe('#22c55e');
      expect(getFccuStateColor(99)).toBe('#71717a');
    });
  });
});
