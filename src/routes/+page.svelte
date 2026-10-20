<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import Plotly from 'plotly.js-dist-min';

  let selectedSource = 'Grafana';
  const sources = ['Grafana', 'CAN'];

  const thresholds = {
    voltage: { min: 40, max: 52 },
    current: { min: 0, max: 15 },
    temperature: { min: 0, max: 75 },
    rpm: { min: 0, max: 6500 },
    power: { min: 0, max: 1000 }
  };

  let currentReadings = { voltage1: 0, voltage2: 0, current1: 0, current2: 0, temperature: 0, rpm: 0, power: 0 };

  const MAX_POINTS = 30;

  function getStatus(value: number, config: { min: number, max: number }) {
    if (value === 0) return 'normal';
    if (value < config.min || value > config.max) return 'alarm';
    if (value > config.max * 0.9 || value < config.min * 1.1) return 'warning';
    return 'normal';
  }

  const baseLayout = {
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

  const plotlyOptions = { responsive: true, displayModeBar: false };

  // ── Grafana tab chart refs ──
  let voltChart: HTMLElement;
  let currChart: HTMLElement;
  let tempChart: HTMLElement;
  let rpmChart: HTMLElement;
  let powerChart: HTMLElement;
  let grafanaChartsInitialized = false;

  const grafanaCharts = {
    voltages: {
      data: [
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Voltage 1 (V)', line: { color: '#3b82f6', width: 2 } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Voltage 2 (V)', line: { color: '#60a5fa', width: 2, dash: 'dot' } }
      ],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Voltage (V)' } }
    },
    currents: {
      data: [
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Current 1 (A)', line: { color: '#10b981', width: 2 } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Current 2 (A)', line: { color: '#34d399', width: 2, dash: 'dot' } }
      ],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Current (A)' } }
    },
    temperature: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Temperature (°C)', line: { color: '#f59e0b', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Temp (°C)' } }
    },
    rpm: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'RPM', line: { color: '#8b5cf6', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'RPM' } }
    },
    power: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Power (W)', line: { color: '#ec4899', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Power (W)' } }
    }
  };

  // ── CAN tab chart refs ──
  let canChart1: HTMLElement;
  let canChart2: HTMLElement;
  let canChartsInitialized = false;

  const canChartConfigs = {
    chart1: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Voltage 1 (V)', line: { color: '#10b981', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Voltage 1 (V)' } }
    },
    chart2: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Current 1 (A)', line: { color: '#10b981', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Current 1 (A)' } }
    }
  };

  // CAN tab statistics
  let canStats = {
    chart1: { newest: 0, average: 0, rms: 0 },
    chart2: { newest: 0, average: 0, rms: 0 }
  };
  let chart1History: number[] = [];
  let chart2History: number[] = [];

  function calcAvg(arr: number[]): number {
    if (arr.length === 0) return 0;
    return arr.reduce((a, b) => a + b, 0) / arr.length;
  }
  function calcRms(arr: number[]): number {
    if (arr.length === 0) return 0;
    return Math.sqrt(arr.reduce((a, b) => a + b * b, 0) / arr.length);
  }

  // CAN tab control form
  let canInputs = ['', '', ''];
  let canDropdowns = [1, 1, 1];

  function handleCanSend() {
    console.log('CAN send:', { inputs: canInputs, dropdowns: canDropdowns });
    // TODO: send via CAN bus through Tauri command
    alert(`Wysłano magistralą CAN:\nWartości: ${canInputs.join(', ')}\nDropdowny: ${canDropdowns.join(', ')}`);
  }

  // ── Init Grafana charts ──
  async function initGrafanaCharts() {
    if (grafanaChartsInitialized) return;
    await tick();
    if (!voltChart) return;
    Plotly.newPlot(voltChart, grafanaCharts.voltages.data, grafanaCharts.voltages.layout, plotlyOptions);
    Plotly.newPlot(currChart, grafanaCharts.currents.data, grafanaCharts.currents.layout, plotlyOptions);
    Plotly.newPlot(tempChart, grafanaCharts.temperature.data, grafanaCharts.temperature.layout, plotlyOptions);
    Plotly.newPlot(rpmChart, grafanaCharts.rpm.data, grafanaCharts.rpm.layout, plotlyOptions);
    Plotly.newPlot(powerChart, grafanaCharts.power.data, grafanaCharts.power.layout, plotlyOptions);
    grafanaChartsInitialized = true;
  }

  // ── Init CAN charts ──
  async function initCanCharts() {
    if (canChartsInitialized) return;
    await tick();
    if (!canChart1) return;
    Plotly.newPlot(canChart1, canChartConfigs.chart1.data, canChartConfigs.chart1.layout, plotlyOptions);
    Plotly.newPlot(canChart2, canChartConfigs.chart2.data, canChartConfigs.chart2.layout, plotlyOptions);
    canChartsInitialized = true;
  }

  // ── Switch source ──
  async function switchSource(source: string) {
    selectedSource = source;
    if (source === 'Grafana') {
      grafanaChartsInitialized = false;
      await initGrafanaCharts();
    } else if (source === 'CAN') {
      canChartsInitialized = false;
      await initCanCharts();
    }
  }

  onMount(() => {
    initGrafanaCharts();

    const unlistenPromise = listen('engine-data-update', (event: any) => {
      const data = event.payload;
      currentReadings = data;
      const now = new Date().toLocaleTimeString();

      // Update Grafana charts (if visible)
      if (selectedSource === 'Grafana' && grafanaChartsInitialized && voltChart) {
        Plotly.extendTraces(voltChart, { x: [[now], [now]], y: [[data.voltage1], [data.voltage2]] }, [0, 1], MAX_POINTS);
        Plotly.extendTraces(currChart, { x: [[now], [now]], y: [[data.current1], [data.current2]] }, [0, 1], MAX_POINTS);
        Plotly.extendTraces(tempChart, { x: [[now]], y: [[data.temperature]] }, [0], MAX_POINTS);
        Plotly.extendTraces(rpmChart, { x: [[now]], y: [[data.rpm]] }, [0], MAX_POINTS);
        Plotly.extendTraces(powerChart, { x: [[now]], y: [[data.power]] }, [0], MAX_POINTS);
      }

      // Update CAN charts + stats (if visible)
      if (selectedSource === 'CAN' && canChartsInitialized && canChart1) {
        Plotly.extendTraces(canChart1, { x: [[now]], y: [[data.voltage1]] }, [0], MAX_POINTS);
        Plotly.extendTraces(canChart2, { x: [[now]], y: [[data.current1]] }, [0], MAX_POINTS);

        // Track history for stats
        chart1History.push(data.voltage1);
        chart2History.push(data.current1);
        if (chart1History.length > MAX_POINTS) chart1History.shift();
        if (chart2History.length > MAX_POINTS) chart2History.shift();

        canStats = {
          chart1: { newest: data.voltage1, average: calcAvg(chart1History), rms: calcRms(chart1History) },
          chart2: { newest: data.current1, average: calcAvg(chart2History), rms: calcRms(chart2History) }
        };
      }
    });

    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  });
</script>

<div class="dashboard">
  <aside class="sidebar">
    <h1 class="logo">Motor Driver Monitor</h1>

    <div class="section">
      <h2 class="section-title">Data Source</h2>
      <div class="source-buttons">
        {#each sources as source}
          <button class="source-btn {selectedSource === source ? 'active' : ''}" on:click={() => switchSource(source)}>
            {source}
          </button>
        {/each}
      </div>
    </div>

    <div class="section">
      <h2 class="section-title">Live Readings</h2>
      
      <div class="metric-group">
        <div class="metric-card status-{getStatus(currentReadings.voltage1, thresholds.voltage)}">
          <div class="metric-header"><span class="metric-label">Voltage 1</span><div class="indicator"></div></div>
          <div class="metric-value">{currentReadings.voltage1.toFixed(1)} <span class="unit">V</span></div>
        </div>
        <div class="metric-card status-{getStatus(currentReadings.voltage2, thresholds.voltage)}">
          <div class="metric-header"><span class="metric-label">Voltage 2</span><div class="indicator"></div></div>
          <div class="metric-value">{currentReadings.voltage2.toFixed(1)} <span class="unit">V</span></div>
        </div>
      </div>

      <div class="metric-group">
        <div class="metric-card status-{getStatus(currentReadings.current1, thresholds.current)}">
          <div class="metric-header"><span class="metric-label">Current 1</span><div class="indicator"></div></div>
          <div class="metric-value">{currentReadings.current1.toFixed(1)} <span class="unit">A</span></div>
        </div>
        <div class="metric-card status-{getStatus(currentReadings.current2, thresholds.current)}">
          <div class="metric-header"><span class="metric-label">Current 2</span><div class="indicator"></div></div>
          <div class="metric-value">{currentReadings.current2.toFixed(1)} <span class="unit">A</span></div>
        </div>
      </div>
      
      <div class="metric-card status-{getStatus(currentReadings.temperature, thresholds.temperature)}">
        <div class="metric-header"><span class="metric-label">Temperature</span><div class="indicator"></div></div>
        <div class="metric-value">{currentReadings.temperature.toFixed(1)} <span class="unit">°C</span></div>
      </div>

      <div class="metric-card status-{getStatus(currentReadings.rpm, thresholds.rpm)}">
        <div class="metric-header"><span class="metric-label">RPM</span><div class="indicator"></div></div>
        <div class="metric-value">{currentReadings.rpm.toFixed(0)}</div>
      </div>

      <div class="metric-card status-{getStatus(currentReadings.power, thresholds.power)}">
        <div class="metric-header"><span class="metric-label">Power</span><div class="indicator"></div></div>
        <div class="metric-value">{currentReadings.power.toFixed(0)} <span class="unit">W</span></div>
      </div>
    </div>
  </aside>

  <main class="charts-area">
    {#if selectedSource === 'Grafana'}
      <!-- ═══ GRAFANA VIEW ═══ -->
      <div class="chart-container"><h3 class="chart-title">Voltages</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={voltChart}></div></div></div>
      <div class="chart-container"><h3 class="chart-title">Currents</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={currChart}></div></div></div>
      <div class="chart-container"><h3 class="chart-title">Temperature</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={tempChart}></div></div></div>
      <div class="chart-container"><h3 class="chart-title">RPM</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={rpmChart}></div></div></div>
      <div class="chart-container"><h3 class="chart-title">Power</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={powerChart}></div></div></div>

    {:else if selectedSource === 'CAN'}
      <!-- ═══ CAN VIEW ═══ -->
      <div class="can-panel">
        <!-- Chart 1 with stats -->
        <div class="can-chart-row">
          <div class="can-chart-wrapper">
            <div class="chart-container compact">
              <h3 class="chart-title">Voltage 1</h3>
              <div class="canvas-wrapper-can"><div class="plotly-chart" bind:this={canChart1}></div></div>
            </div>
          </div>
          <div class="can-stats">
            <div class="stat-item">
              <span class="stat-label">Najnowsza</span>
              <span class="stat-value">{canStats.chart1.newest.toFixed(2)}</span>
            </div>
            <div class="stat-item">
              <span class="stat-label">Średnia</span>
              <span class="stat-value">{canStats.chart1.average.toFixed(2)}</span>
            </div>
            <div class="stat-item">
              <span class="stat-label">Skuteczna</span>
              <span class="stat-value">{canStats.chart1.rms.toFixed(2)}</span>
            </div>
          </div>
        </div>

        <!-- Chart 2 with stats -->
        <div class="can-chart-row">
          <div class="can-chart-wrapper">
            <div class="chart-container compact">
              <h3 class="chart-title">Current 1</h3>
              <div class="canvas-wrapper-can"><div class="plotly-chart" bind:this={canChart2}></div></div>
            </div>
          </div>
          <div class="can-stats">
            <div class="stat-item">
              <span class="stat-label">Najnowsza</span>
              <span class="stat-value">{canStats.chart2.newest.toFixed(2)}</span>
            </div>
            <div class="stat-item">
              <span class="stat-label">Średnia</span>
              <span class="stat-value">{canStats.chart2.average.toFixed(2)}</span>
            </div>
            <div class="stat-item">
              <span class="stat-label">Skuteczna</span>
              <span class="stat-value">{canStats.chart2.rms.toFixed(2)}</span>
            </div>
          </div>
        </div>

        <!-- Separator -->
        <div class="can-separator"></div>

        <!-- Control form -->
        <div class="can-controls">
          {#each [0, 1, 2] as i}
            <div class="control-row">
              <input
                type="number"
                class="can-input"
                placeholder="Wartość {i + 1}"
                bind:value={canInputs[i]}
              />
              <select class="can-dropdown" bind:value={canDropdowns[i]}>
                {#each Array.from({length: 10}, (_, k) => k + 1) as val}
                  <option value={val}>{val}</option>
                {/each}
              </select>
              {#if i === 2}
                <button class="can-send-btn" on:click={handleCanSend}>Wyślij</button>
              {:else}
                <div class="can-send-placeholder"></div>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </main>
</div>

<style>
  :global(body) { margin: 0; padding: 0; background-color: #09090b; color: #e4e4e7; font-family: 'Segoe UI', sans-serif; height: 100vh; overflow: hidden; }
  .dashboard { display: flex; height: 100vh; width: 100vw; }
  .sidebar { width: 300px; background-color: #18181b; border-right: 1px solid #27272a; padding: 20px; display: flex; flex-direction: column; gap: 30px; overflow-y: auto; flex-shrink: 0; }
  .logo { font-size: 1.25rem; font-weight: 600; margin: 0; color: #ffffff; }
  .section { display: flex; flex-direction: column; gap: 12px; }
  .section-title { font-size: 0.85rem; color: #a1a1aa; margin: 0 0 4px 0; font-weight: 500; }
  .source-buttons { display: flex; flex-direction: column; gap: 8px; }
  .source-btn { background-color: #27272a; color: #e4e4e7; border: none; border-radius: 8px; padding: 12px 16px; text-align: left; font-size: 0.95rem; cursor: pointer; transition: all 0.2s; }
  .source-btn.active { background-color: #3b82f6; color: #ffffff; }
  .metric-group { display: flex; gap: 8px; }
  .metric-card { background-color: #27272a; border-radius: 8px; padding: 12px 14px; display: flex; flex-direction: column; gap: 4px; flex: 1; border: 1px solid transparent; transition: all 0.3s ease; }
  .metric-header { display: flex; justify-content: space-between; align-items: center; }
  .indicator { width: 8px; height: 8px; border-radius: 50%; background-color: #71717a; box-shadow: 0 0 0px transparent; transition: all 0.3s ease; }
  .status-normal .indicator { background-color: #10b981; }
  .status-warning .indicator { background-color: #f59e0b; box-shadow: 0 0 8px #f59e0b; }
  .status-alarm { border-color: #ef444433; background-color: #450a0a33; }
  .status-alarm .indicator { background-color: #ef4444; box-shadow: 0 0 12px #ef4444; animation: blink 1s infinite; }
  @keyframes blink { 0% { opacity: 1; } 50% { opacity: 0.3; } 100% { opacity: 1; } }
  .metric-label { font-size: 0.8rem; color: #a1a1aa; }
  .metric-value { font-size: 1.3rem; color: #ffffff; font-weight: 500; }
  .unit { font-size: 0.85rem; color: #71717a; font-weight: normal; }

  /* ── Main area ── */
  .charts-area { flex: 1; padding: 20px; overflow-y: auto; display: flex; flex-direction: column; gap: 20px; }

  /* ── Grafana charts ── */
  .chart-container { background-color: #18181b; border: 1px solid #27272a; border-radius: 12px; padding: 16px 20px; }
  .chart-container.compact { padding: 12px 16px; }
  .chart-title { font-size: 0.9rem; color: #a1a1aa; margin: 0 0 10px 0; }
  .canvas-wrapper { height: 220px; width: 100%; }
  .plotly-chart { width: 100%; height: 100%; }

  /* ── CAN view ── */
  .can-panel { display: flex; flex-direction: column; gap: 16px; }

  .can-chart-row { display: flex; gap: 16px; align-items: stretch; }
  .can-chart-wrapper { flex: 1; min-width: 0; }
  .canvas-wrapper-can { height: 180px; width: 100%; }

  .can-stats { display: flex; flex-direction: column; gap: 8px; justify-content: center; width: 160px; flex-shrink: 0; }
  .stat-item { display: flex; flex-direction: column; gap: 2px; background-color: #18181b; border: 1px solid #27272a; border-radius: 8px; padding: 10px 14px; }
  .stat-label { font-size: 0.75rem; color: #71717a; text-transform: uppercase; letter-spacing: 0.05em; }
  .stat-value { font-size: 1.1rem; color: #e4e4e7; font-weight: 500; font-variant-numeric: tabular-nums; }

  .can-separator { height: 1px; background-color: #27272a; margin: 4px 0; }

  .can-controls { display: flex; flex-direction: column; gap: 12px; }
  .control-row { display: flex; gap: 12px; align-items: center; }

  .can-input {
    flex: 1;
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 10px 14px;
    color: #e4e4e7;
    font-size: 0.95rem;
    outline: none;
    transition: border-color 0.2s;
  }
  .can-input:focus { border-color: #3b82f6; }
  .can-input::placeholder { color: #52525b; }

  .can-dropdown {
    width: 120px;
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 10px 14px;
    color: #e4e4e7;
    font-size: 0.95rem;
    outline: none;
    cursor: pointer;
    appearance: auto;
    transition: border-color 0.2s;
  }
  .can-dropdown:focus { border-color: #3b82f6; }

  .can-send-btn {
    background-color: #dc2626;
    color: #ffffff;
    border: none;
    border-radius: 8px;
    padding: 10px 24px;
    font-size: 0.95rem;
    font-weight: 600;
    cursor: pointer;
    transition: background-color 0.2s;
    white-space: nowrap;
  }
  .can-send-btn:hover { background-color: #b91c1c; }
  .can-send-btn:active { background-color: #991b1b; }

  .can-send-placeholder { width: 100px; }
</style>