import type { PlotData, PlotLayout, PlotlyConfig } from 'plotly.js-dist-min';

export const baseLayout: PlotLayout = {
  margin: { t: 10, r: 10, l: 40, b: 30 },
  paper_bgcolor: 'transparent',
  plot_bgcolor: 'transparent',
  font: { color: '#a1a1aa', size: 11, family: 'system-ui, sans-serif' },
  xaxis: { gridcolor: '#27272a', zeroline: false, tickfont: { color: '#71717a' } },
  yaxis: { gridcolor: '#27272a', zeroline: false, tickfont: { color: '#71717a' } },
  showlegend: true,
  legend: { orientation: 'h', y: -0.2, x: 0.5, xanchor: 'center' },
  hovermode: 'x unified',
};

export const plotlyOptions: PlotlyConfig = {
  responsive: true,
  displayModeBar: false,
};

export interface ChartBundle {
  data: PlotData[];
  layout: PlotLayout;
}

export function createGrafanaChartsConfig(): Record<
  'voltages' | 'currents' | 'temperature' | 'rpm' | 'power',
  ChartBundle
> {
  return {
    voltages: {
      data: [
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Voltage 1 (V)',
          line: { color: '#3b82f6', width: 2 },
        },
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Voltage 2 (V)',
          line: { color: '#60a5fa', width: 2, dash: 'dot' },
        },
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Voltage 3 (V)',
          line: { color: '#93c5fd', width: 2, dash: 'dash' },
        },
      ],
      layout: {
        ...baseLayout,
        yaxis: { ...baseLayout.yaxis, title: 'Voltage (V)' },
      },
    },
    currents: {
      data: [
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Current 1 (A)',
          line: { color: '#10b981', width: 2 },
        },
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Current 2 (A)',
          line: { color: '#34d399', width: 2, dash: 'dot' },
        },
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Current 3 (A)',
          line: { color: '#6ee7b7', width: 2, dash: 'dash' },
        },
      ],
      layout: {
        ...baseLayout,
        yaxis: { ...baseLayout.yaxis, title: 'Current (A)' },
      },
    },
    temperature: {
      data: [
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Temperature (°C)',
          line: { color: '#f59e0b', width: 2 },
        },
      ],
      layout: {
        ...baseLayout,
        yaxis: { ...baseLayout.yaxis, title: 'Temp (°C)' },
      },
    },
    rpm: {
      data: [
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'RPM',
          line: { color: '#8b5cf6', width: 2 },
        },
      ],
      layout: {
        ...baseLayout,
        yaxis: { ...baseLayout.yaxis, title: 'RPM' },
      },
    },
    power: {
      data: [
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Power 1 (W)',
          line: { color: '#ec4899', width: 2 },
        },
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Power 2 (W)',
          line: { color: '#f472b6', width: 2, dash: 'dot' },
        },
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Power 3 (W)',
          line: { color: '#f9a8d4', width: 2, dash: 'dash' },
        },
      ],
      layout: {
        ...baseLayout,
        yaxis: { ...baseLayout.yaxis, title: 'Power (W)' },
      },
    },
  };
}

export const canChartLayout: PlotLayout = {
  ...baseLayout,
  margin: { t: 8, r: 8, l: 30, b: 24 },
  showlegend: false,
  xaxis: {
    ...baseLayout.xaxis,
    title: 'Czas [s]',
    tickmode: 'linear',
    dtick: 10,
    range: [0, 60],
  },
  yaxis: { ...baseLayout.yaxis, range: [0, 4] },
};

export function createCanChartConfigs(): Record<'chart1' | 'chart2', ChartBundle> {
  return {
    chart1: {
      data: [
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Napięcie FC (V)',
          line: { color: '#3b82f6', width: 1.5 },
        },
      ],
      layout: {
        ...canChartLayout,
        yaxis: { ...canChartLayout.yaxis, title: '' },
      },
    },
    chart2: {
      data: [
        {
          x: [] as string[],
          y: [] as number[],
          type: 'scatter',
          mode: 'lines',
          name: 'Prąd FC (A)',
          line: { color: '#3b82f6', width: 1.5 },
        },
      ],
      layout: {
        ...canChartLayout,
        yaxis: { ...canChartLayout.yaxis, title: '' },
      },
    },
  };
}
