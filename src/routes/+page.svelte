<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
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

  let currentReadings = { voltage1: 0, voltage2: 0, voltage3: 0, current1: 0, current2: 0, current3: 0, temperature: 0, rpm: 0, power1: 0, power2: 0, power3: 0 };

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
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Voltage 2 (V)', line: { color: '#60a5fa', width: 2, dash: 'dot' } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Voltage 3 (V)', line: { color: '#93c5fd', width: 2, dash: 'dash' } }
      ],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Voltage (V)' } }
    },
    currents: {
      data: [
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Current 1 (A)', line: { color: '#10b981', width: 2 } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Current 2 (A)', line: { color: '#34d399', width: 2, dash: 'dot' } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Current 3 (A)', line: { color: '#6ee7b7', width: 2, dash: 'dash' } }
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
      data: [
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Power 1 (W)', line: { color: '#ec4899', width: 2 } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Power 2 (W)', line: { color: '#f472b6', width: 2, dash: 'dot' } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Power 3 (W)', line: { color: '#f9a8d4', width: 2, dash: 'dash' } }
      ],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Power (W)' } }
    }
  };

  // ── CAN tab chart refs ──
  let canChart1: HTMLElement;
  let canChart2: HTMLElement;
  let canChartsInitialized = false;

  const canChartConfigs = {
    chart1: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'FC Voltage (V)', line: { color: '#10b981', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'FC Voltage (V)' } }
    },
    chart2: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'FC Current (A)', line: { color: '#10b981', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'FC Current (A)' } }
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

  // ── CAN controls state (based on DBC) ──

  // FCCU Commands (1-bit each)
  let fccuCmds = {
    systemEnable: false,
    mainValve: false,
    manualPurge: false,
    fanOn: false
  };

  // FCCU Settings
  let fccuSettings = {
    fanMode: 0,         // 0=Manual, 1=Linear Temp, 2=PID
    purgeInterval: 60,  // 0-16383 seconds
    pGain: 100,         // 0-1000
    iGain: 50,          // 0-1000
    dGain: 10           // 0-1000
  };

  // MCU Lighting (1-bit each)
  let lighting = {
    headlight: false,
    positionLight: false,
    brakeLight: false,
    leftIndicator: false,
    rightIndicator: false,
    hazard: false
  };

  // CAN send status
  let canSendStatus = '';

  async function toggleFccuCmd(cmdType: string) {
    // Toggle local state
    if (cmdType === 'system_enable') fccuCmds.systemEnable = !fccuCmds.systemEnable;
    else if (cmdType === 'main_valve') fccuCmds.mainValve = !fccuCmds.mainValve;
    else if (cmdType === 'manual_purge') fccuCmds.manualPurge = !fccuCmds.manualPurge;
    else if (cmdType === 'fan_on') fccuCmds.fanOn = !fccuCmds.fanOn;

    const stateMap: Record<string, boolean> = {
      system_enable: fccuCmds.systemEnable,
      main_valve: fccuCmds.mainValve,
      manual_purge: fccuCmds.manualPurge,
      fan_on: fccuCmds.fanOn
    };

    try {
      const result = await invoke('send_fccu_command', {
        commandType: cmdType,
        state: stateMap[cmdType]
      });
      canSendStatus = result as string;
    } catch (e) {
      canSendStatus = `Błąd: ${e}`;
    }
  }

  async function sendFccuSettings() {
    try {
      const result = await invoke('send_fccu_settings', {
        fanMode: fccuSettings.fanMode,
        purgeInterval: fccuSettings.purgeInterval,
        pGain: fccuSettings.pGain,
        iGain: fccuSettings.iGain,
        dGain: fccuSettings.dGain
      });
      canSendStatus = result as string;
    } catch (e) {
      canSendStatus = `Błąd: ${e}`;
    }
  }

  async function sendLightingCmd() {
    try {
      const result = await invoke('send_lighting', {
        headlight: lighting.headlight,
        positionLight: lighting.positionLight,
        brakeLight: lighting.brakeLight,
        leftIndicator: lighting.leftIndicator,
        rightIndicator: lighting.rightIndicator,
        hazard: lighting.hazard
      });
      canSendStatus = result as string;
    } catch (e) {
      canSendStatus = `Błąd: ${e}`;
    }
  }

  // ── Init charts ──
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

  async function initCanCharts() {
    if (canChartsInitialized) return;
    await tick();
    if (!canChart1) return;
    Plotly.newPlot(canChart1, canChartConfigs.chart1.data, canChartConfigs.chart1.layout, plotlyOptions);
    Plotly.newPlot(canChart2, canChartConfigs.chart2.data, canChartConfigs.chart2.layout, plotlyOptions);
    canChartsInitialized = true;
  }

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

    // PostgreSQL stream → Grafana tab
    const unlistenPg = listen('engine-data-update', (event: any) => {
      const data = event.payload;
      if (selectedSource === 'Grafana') {
        currentReadings = data;
      }
      const now = new Date().toLocaleTimeString();

      if (selectedSource === 'Grafana' && grafanaChartsInitialized && voltChart) {
        Plotly.extendTraces(voltChart, { x: [[now], [now], [now]], y: [[data.voltage1], [data.voltage2], [data.voltage3]] }, [0, 1, 2], MAX_POINTS);
        Plotly.extendTraces(currChart, { x: [[now], [now], [now]], y: [[data.current1], [data.current2], [data.current3]] }, [0, 1, 2], MAX_POINTS);
        Plotly.extendTraces(tempChart, { x: [[now]], y: [[data.temperature]] }, [0], MAX_POINTS);
        Plotly.extendTraces(rpmChart, { x: [[now]], y: [[data.rpm]] }, [0], MAX_POINTS);
        Plotly.extendTraces(powerChart, { x: [[now], [now], [now]], y: [[data.power1], [data.power2], [data.power3]] }, [0, 1, 2], MAX_POINTS);
      }
    });

    // CAN stream → CAN tab
    const unlistenCan = listen('can-data-update', (event: any) => {
      const d = event.payload;
      const now = new Date().toLocaleTimeString();

      if (selectedSource === 'CAN') {
        // Map CAN signals to sidebar readings
        currentReadings = {
          voltage1: d.fc_output_voltage ?? 0,
          voltage2: d.sc_voltage ?? 0,
          voltage3: d.mc_supply_voltage ?? 0,
          current1: d.fc_output_current ?? 0,
          current2: d.sc_current ?? 0,
          current3: d.mc_supply_current ?? 0,
          power1: d.fc_w ?? 0,
          power2: (d.sc_voltage ?? 0) * (d.sc_current ?? 0),
          power3: (d.mc_supply_voltage ?? 0) * (d.mc_supply_current ?? 0),
          temperature: d.fc_temp ?? 0,
          rpm: d.rpm ?? 0
        };

        // Update CAN charts
        if (canChartsInitialized && canChart1) {
          Plotly.extendTraces(canChart1, { x: [[now]], y: [[d.fc_output_voltage ?? 0]] }, [0], MAX_POINTS);
          Plotly.extendTraces(canChart2, { x: [[now]], y: [[d.fc_output_current ?? 0]] }, [0], MAX_POINTS);

          chart1History.push(d.fc_output_voltage ?? 0);
          chart2History.push(d.fc_output_current ?? 0);
          if (chart1History.length > MAX_POINTS) chart1History.shift();
          if (chart2History.length > MAX_POINTS) chart2History.shift();

          canStats = {
            chart1: { newest: d.fc_output_voltage ?? 0, average: calcAvg(chart1History), rms: calcRms(chart1History) },
            chart2: { newest: d.fc_output_current ?? 0, average: calcAvg(chart2History), rms: calcRms(chart2History) }
          };
        }
      }
    });

    return () => {
      unlistenPg.then(u => u());
      unlistenCan.then(u => u());
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
        <div class="metric-card status-{getStatus(currentReadings.voltage3, thresholds.voltage)}">
          <div class="metric-header"><span class="metric-label">Voltage 3</span><div class="indicator"></div></div>
          <div class="metric-value">{currentReadings.voltage3.toFixed(1)} <span class="unit">V</span></div>
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
        <div class="metric-card status-{getStatus(currentReadings.current3, thresholds.current)}">
          <div class="metric-header"><span class="metric-label">Current 3</span><div class="indicator"></div></div>
          <div class="metric-value">{currentReadings.current3.toFixed(1)} <span class="unit">A</span></div>
        </div>
      </div>

      <div class="metric-group">
        <div class="metric-card status-{getStatus(currentReadings.power1, thresholds.power)}">
          <div class="metric-header"><span class="metric-label">Power 1</span><div class="indicator"></div></div>
          <div class="metric-value">{currentReadings.power1.toFixed(0)} <span class="unit">W</span></div>
        </div>
        <div class="metric-card status-{getStatus(currentReadings.power2, thresholds.power)}">
          <div class="metric-header"><span class="metric-label">Power 2</span><div class="indicator"></div></div>
          <div class="metric-value">{currentReadings.power2.toFixed(0)} <span class="unit">W</span></div>
        </div>
        <div class="metric-card status-{getStatus(currentReadings.power3, thresholds.power)}">
          <div class="metric-header"><span class="metric-label">Power 3</span><div class="indicator"></div></div>
          <div class="metric-value">{currentReadings.power3.toFixed(0)} <span class="unit">W</span></div>
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
        <!-- Chart 1: FC Voltage with stats -->
        <div class="can-chart-row">
          <div class="can-chart-wrapper">
            <div class="chart-container compact">
              <h3 class="chart-title">FC Voltage</h3>
              <div class="canvas-wrapper-can"><div class="plotly-chart" bind:this={canChart1}></div></div>
            </div>
          </div>
          <div class="can-stats">
            <div class="stat-item"><span class="stat-label">Najnowsza</span><span class="stat-value">{canStats.chart1.newest.toFixed(2)} V</span></div>
            <div class="stat-item"><span class="stat-label">Średnia</span><span class="stat-value">{canStats.chart1.average.toFixed(2)} V</span></div>
            <div class="stat-item"><span class="stat-label">Skuteczna</span><span class="stat-value">{canStats.chart1.rms.toFixed(2)} V</span></div>
          </div>
        </div>

        <!-- Chart 2: FC Current with stats -->
        <div class="can-chart-row">
          <div class="can-chart-wrapper">
            <div class="chart-container compact">
              <h3 class="chart-title">FC Current</h3>
              <div class="canvas-wrapper-can"><div class="plotly-chart" bind:this={canChart2}></div></div>
            </div>
          </div>
          <div class="can-stats">
            <div class="stat-item"><span class="stat-label">Najnowsza</span><span class="stat-value">{canStats.chart2.newest.toFixed(2)} A</span></div>
            <div class="stat-item"><span class="stat-label">Średnia</span><span class="stat-value">{canStats.chart2.average.toFixed(2)} A</span></div>
            <div class="stat-item"><span class="stat-label">Skuteczna</span><span class="stat-value">{canStats.chart2.rms.toFixed(2)} A</span></div>
          </div>
        </div>

        <div class="can-separator"></div>

        <!-- ═══ CAN CONTROLS ═══ -->

        <!-- FCCU Commands — instant toggle buttons -->
        <div class="can-control-section">
          <h3 class="control-section-title">FCCU Commands</h3>
          <div class="fccu-btn-grid">
            <button class="fccu-btn {fccuCmds.systemEnable ? 'active' : ''}" on:click={() => toggleFccuCmd('system_enable')}>
              System Enable <span class="fccu-btn-state">{fccuCmds.systemEnable ? 'ON' : 'OFF'}</span>
            </button>
            <button class="fccu-btn {fccuCmds.mainValve ? 'active' : ''}" on:click={() => toggleFccuCmd('main_valve')}>
              Main Valve <span class="fccu-btn-state">{fccuCmds.mainValve ? 'ON' : 'OFF'}</span>
            </button>
            <button class="fccu-btn {fccuCmds.manualPurge ? 'active' : ''}" on:click={() => toggleFccuCmd('manual_purge')}>
              Manual Purge <span class="fccu-btn-state">{fccuCmds.manualPurge ? 'ON' : 'OFF'}</span>
            </button>
            <button class="fccu-btn {fccuCmds.fanOn ? 'active' : ''}" on:click={() => toggleFccuCmd('fan_on')}>
              Fan On <span class="fccu-btn-state">{fccuCmds.fanOn ? 'ON' : 'OFF'}</span>
            </button>
          </div>
        </div>

        <!-- FCCU Settings -->
        <div class="can-control-section">
          <h3 class="control-section-title">FCCU Settings</h3>
          <div class="settings-grid">
            <div class="setting-row">
              <label class="setting-label">Fan Mode</label>
              <select class="can-dropdown" bind:value={fccuSettings.fanMode}>
                <option value={0}>Manual</option>
                <option value={1}>Linear Temp</option>
                <option value={2}>PID</option>
              </select>
            </div>
            <div class="setting-row">
              <label class="setting-label">Purge Interval</label>
              <div class="input-with-unit">
                <input type="number" class="can-input" bind:value={fccuSettings.purgeInterval} min="0" max="16383" />
                <span class="input-unit">s</span>
              </div>
            </div>
            <div class="setting-row">
              <label class="setting-label">P Gain</label>
              <input type="number" class="can-input" bind:value={fccuSettings.pGain} min="0" max="1000" />
            </div>
            <div class="setting-row">
              <label class="setting-label">I Gain</label>
              <input type="number" class="can-input" bind:value={fccuSettings.iGain} min="0" max="1000" />
            </div>
            <div class="setting-row">
              <label class="setting-label">D Gain</label>
              <input type="number" class="can-input" bind:value={fccuSettings.dGain} min="0" max="1000" />
            </div>
          </div>
          <button class="can-send-btn" on:click={sendFccuSettings}>Wyślij Settings</button>
        </div>

        <!-- MCU Lighting -->
        <div class="can-control-section">
          <h3 class="control-section-title">Lighting</h3>
          <div class="checkbox-grid">
            <label class="toggle-label">
              <input type="checkbox" class="toggle-input" bind:checked={lighting.headlight} />
              <span class="toggle-switch"></span>
              <span>Headlight</span>
            </label>
            <label class="toggle-label">
              <input type="checkbox" class="toggle-input" bind:checked={lighting.positionLight} />
              <span class="toggle-switch"></span>
              <span>Position Light</span>
            </label>
            <label class="toggle-label">
              <input type="checkbox" class="toggle-input" bind:checked={lighting.brakeLight} />
              <span class="toggle-switch"></span>
              <span>Brake Light</span>
            </label>
            <label class="toggle-label">
              <input type="checkbox" class="toggle-input" bind:checked={lighting.leftIndicator} />
              <span class="toggle-switch"></span>
              <span>Left Indicator</span>
            </label>
            <label class="toggle-label">
              <input type="checkbox" class="toggle-input" bind:checked={lighting.rightIndicator} />
              <span class="toggle-switch"></span>
              <span>Right Indicator</span>
            </label>
            <label class="toggle-label">
              <input type="checkbox" class="toggle-input" bind:checked={lighting.hazard} />
              <span class="toggle-switch"></span>
              <span>Hazard</span>
            </label>
          </div>
          <button class="can-send-btn" on:click={sendLightingCmd}>Wyślij Lighting</button>
        </div>

        <!-- Status bar -->
        {#if canSendStatus}
          <div class="can-status-bar">{canSendStatus}</div>
        {/if}
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
  .metric-group { display: flex; gap: 6px; }
  .metric-card { background-color: #27272a; border-radius: 8px; padding: 10px 10px; display: flex; flex-direction: column; gap: 4px; flex: 1; border: 1px solid transparent; transition: all 0.3s ease; min-width: 0; }
  .metric-header { display: flex; justify-content: space-between; align-items: center; }
  .indicator { width: 8px; height: 8px; border-radius: 50%; background-color: #71717a; box-shadow: 0 0 0px transparent; transition: all 0.3s ease; }
  .status-normal .indicator { background-color: #10b981; }
  .status-warning .indicator { background-color: #f59e0b; box-shadow: 0 0 8px #f59e0b; }
  .status-alarm { border-color: #ef444433; background-color: #450a0a33; }
  .status-alarm .indicator { background-color: #ef4444; box-shadow: 0 0 12px #ef4444; animation: blink 1s infinite; }
  @keyframes blink { 0% { opacity: 1; } 50% { opacity: 0.3; } 100% { opacity: 1; } }
  .metric-label { font-size: 0.7rem; color: #a1a1aa; }
  .metric-value { font-size: 1.1rem; color: #ffffff; font-weight: 500; }
  .unit { font-size: 0.75rem; color: #71717a; font-weight: normal; }

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

  /* ── CAN control sections ── */
  .can-control-section {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 12px;
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .control-section-title { font-size: 0.9rem; color: #a1a1aa; margin: 0; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; }

  /* ── Toggle switches ── */
  .checkbox-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .toggle-label { display: flex; align-items: center; gap: 10px; cursor: pointer; font-size: 0.9rem; color: #e4e4e7; }
  .toggle-input { display: none; }
  .toggle-switch {
    position: relative;
    width: 40px;
    height: 22px;
    background-color: #3f3f46;
    border-radius: 11px;
    transition: background-color 0.2s;
    flex-shrink: 0;
  }
  .toggle-switch::after {
    content: '';
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    background-color: #71717a;
    border-radius: 50%;
    transition: all 0.2s;
  }
  .toggle-input:checked + .toggle-switch { background-color: #22c55e; }
  .toggle-input:checked + .toggle-switch::after { left: 21px; background-color: #ffffff; }

  /* ── Settings grid ── */
  .settings-grid { display: flex; flex-direction: column; gap: 10px; }
  .setting-row { display: flex; align-items: center; gap: 12px; }
  .setting-label { font-size: 0.85rem; color: #a1a1aa; width: 120px; flex-shrink: 0; }
  .input-with-unit { display: flex; align-items: center; gap: 6px; flex: 1; }
  .input-unit { font-size: 0.85rem; color: #71717a; }

  .can-input {
    flex: 1;
    background-color: #27272a;
    border: 1px solid #3f3f46;
    border-radius: 8px;
    padding: 8px 12px;
    color: #e4e4e7;
    font-size: 0.9rem;
    outline: none;
    transition: border-color 0.2s;
    max-width: 160px;
  }
  .can-input:focus { border-color: #3b82f6; }

  .can-dropdown {
    flex: 1;
    background-color: #27272a;
    border: 1px solid #3f3f46;
    border-radius: 8px;
    padding: 8px 12px;
    color: #e4e4e7;
    font-size: 0.9rem;
    outline: none;
    cursor: pointer;
    appearance: auto;
    transition: border-color 0.2s;
    max-width: 160px;
  }
  .can-dropdown:focus { border-color: #3b82f6; }

  .can-send-btn {
    align-self: flex-end;
    background-color: #dc2626;
    color: #ffffff;
    border: none;
    border-radius: 8px;
    padding: 10px 24px;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
    transition: background-color 0.2s;
    white-space: nowrap;
  }
  .can-send-btn:hover { background-color: #b91c1c; }
  .can-send-btn:active { background-color: #991b1b; }

  .can-status-bar {
    background-color: #14532d;
    border: 1px solid #22c55e33;
    color: #86efac;
    border-radius: 8px;
    padding: 8px 14px;
    font-size: 0.85rem;
  }

  /* ── FCCU instant toggle buttons ── */
  .fccu-btn-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .fccu-btn {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background-color: #27272a;
    color: #a1a1aa;
    border: 1px solid #3f3f46;
    border-radius: 8px;
    padding: 12px 16px;
    font-size: 0.9rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
  }
  .fccu-btn:hover { border-color: #52525b; }
  .fccu-btn.active {
    background-color: #22c55e;
    color: #ffffff;
    border-color: #16a34a;
  }
  .fccu-btn.active:hover { background-color: #16a34a; }
  .fccu-btn-state {
    font-size: 0.75rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    opacity: 0.8;
  }
</style>
