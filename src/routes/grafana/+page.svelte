<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import Plotly from 'plotly.js-dist-min';
  import { baseLayout, plotlyOptions, MAX_POINTS } from '$lib/chartUtils';
  import type { EngineData } from '$lib/types';

  let voltChart: HTMLElement;
  let currChart: HTMLElement;
  let tempChart: HTMLElement;
  let rpmChart: HTMLElement;
  let powerChart: HTMLElement;
  let grafanaChartsInitialized = false;

  const grafanaCharts = {
    voltages: {
      data: [
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'U_FC (V)', line: { color: '#3b82f6', width: 2 } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'U_SC (V)', line: { color: '#60a5fa', width: 2, dash: 'dot' } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'U_MC (V)', line: { color: '#93c5fd', width: 2, dash: 'dash' } }
      ],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Napięcie (V)' } }
    },
    currents: {
      data: [
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'I_FC (A)', line: { color: '#10b981', width: 2 } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'I_SC (A)', line: { color: '#34d399', width: 2, dash: 'dot' } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'I_MC (A)', line: { color: '#6ee7b7', width: 2, dash: 'dash' } }
      ],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Prąd (A)' } }
    },
    temperature: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Temperatura (°C)', line: { color: '#f59e0b', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Temp (°C)' } }
    },
    rpm: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'RPM', line: { color: '#8b5cf6', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'RPM' } }
    },
    power: {
      data: [
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'P_FC (W)', line: { color: '#ec4899', width: 2 } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'P_SC (W)', line: { color: '#f472b6', width: 2, dash: 'dot' } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'P_MC (W)', line: { color: '#f9a8d4', width: 2, dash: 'dash' } }
      ],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Moc (W)' } }
    }
  };

  onMount(() => {
    if (voltChart) {
      Plotly.newPlot(voltChart, grafanaCharts.voltages.data, grafanaCharts.voltages.layout, plotlyOptions);
      Plotly.newPlot(currChart, grafanaCharts.currents.data, grafanaCharts.currents.layout, plotlyOptions);
      Plotly.newPlot(tempChart, grafanaCharts.temperature.data, grafanaCharts.temperature.layout, plotlyOptions);
      Plotly.newPlot(rpmChart, grafanaCharts.rpm.data, grafanaCharts.rpm.layout, plotlyOptions);
      Plotly.newPlot(powerChart, grafanaCharts.power.data, grafanaCharts.power.layout, plotlyOptions);
      grafanaChartsInitialized = true;
    }

    const unlistenPg = listen<EngineData>('engine-data-update', (event) => {
      const data = event.payload;
      const now = new Date().toLocaleTimeString();

      if (grafanaChartsInitialized && voltChart) {
        Plotly.extendTraces(voltChart, { x: [[now], [now], [now]], y: [[data.voltage1], [data.voltage2], [data.voltage3]] }, [0, 1, 2], MAX_POINTS);
        Plotly.extendTraces(currChart, { x: [[now], [now], [now]], y: [[data.current1], [data.current2], [data.current3]] }, [0, 1, 2], MAX_POINTS);
        Plotly.extendTraces(tempChart, { x: [[now]], y: [[data.temperature]] }, [0], MAX_POINTS);
        Plotly.extendTraces(rpmChart, { x: [[now]], y: [[data.rpm]] }, [0], MAX_POINTS);
        Plotly.extendTraces(powerChart, { x: [[now], [now], [now]], y: [[data.power1], [data.power2], [data.power3]] }, [0, 1, 2], MAX_POINTS);
      }
    });

    return () => {
      unlistenPg.then((u) => u());
    };
  });
</script>

<div class="chart-container"><h3 class="chart-title">Napięcia (U_FC, U_SC, U_MC)</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={voltChart}></div></div></div>
<div class="chart-container"><h3 class="chart-title">Prądy (I_FC, I_SC, I_MC)</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={currChart}></div></div></div>
<div class="chart-container"><h3 class="chart-title">Temperatura</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={tempChart}></div></div></div>
<div class="chart-container"><h3 class="chart-title">Prędkość obrotowa (RPM)</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={rpmChart}></div></div></div>
<div class="chart-container"><h3 class="chart-title">Moc (P_FC, P_SC, P_MC)</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={powerChart}></div></div></div>

<style>
  .chart-container {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 12px;
    padding: 16px 20px;
  }
  .chart-title { font-size: 0.9rem; color: #a1a1aa; margin: 0 0 10px 0; }
  .canvas-wrapper { height: 220px; width: 100%; }
  .plotly-chart { width: 100%; height: 100%; }
</style>
