import { describe, it, expect } from 'vitest';
import { createGrafanaChartsConfig, createCanChartConfigs } from './chart-configs';

describe('chart-configs unit tests', () => {
  it('creates all 5 Grafana telemetry charts with proper series counts', () => {
    const cfg = createGrafanaChartsConfig();
    expect(Object.keys(cfg)).toEqual(['voltages', 'currents', 'temperature', 'rpm', 'power']);
    expect(cfg.voltages.data).toHaveLength(3);
    expect(cfg.currents.data).toHaveLength(3);
    expect(cfg.temperature.data).toHaveLength(1);
    expect(cfg.rpm.data).toHaveLength(1);
    expect(cfg.power.data).toHaveLength(3);
  });

  it('creates CAN voltage and current charts with 60s window layout', () => {
    const cfg = createCanChartConfigs();
    expect(cfg.chart1.data).toHaveLength(1);
    expect(cfg.chart2.data).toHaveLength(1);
    expect(cfg.chart1.data[0].name).toContain('Napięcie FC');
    expect(cfg.chart2.data[0].name).toContain('Prąd FC');
    expect(cfg.chart1.layout.xaxis?.range).toEqual([0, 60]);
  });
});
