<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Plotly from 'plotly.js-dist-min';
  import {
    type DataSource,
    type CurrentReadings,
    type MetricThresholds,
    type FccuCommandType,
    type FccuCommandsState,
    type FccuSettings,
    type LightingState,
    type CanStatistics,
    FccuState,
    FccuFanMode,
    fmtVal,
    getStatus,
    calcAvg,
    calcRms,
    getFccuStateLabel,
    getFccuStateColor,
    plotlyOptions,
    createGrafanaChartsConfig,
    createCanChartConfigs
  } from '$lib';
  import * as api from '$lib/services/api';

  let selectedSource: DataSource = 'CAN';
  const sources: readonly DataSource[] = ['Grafana', 'CAN'];

  const thresholds: MetricThresholds = {
    voltage: { min: 40, max: 52 },
    current: { min: 0, max: 15 },
    temperature: { min: 0, max: 75 },
    rpm: { min: 0, max: 6500 },
    power: { min: 0, max: 1000 }
  };

  let currentReadings: CurrentReadings = {
    voltage1: 0,
    voltage2: 0,
    voltage3: 0,
    current1: 0,
    current2: 0,
    current3: 0,
    temperature: 0,
    rpm: 0,
    power1: 0,
    power2: 0,
    power3: 0
  };

  const MAX_POINTS = 60;

  // ── Grafana tab chart refs ──
  let voltChart: HTMLElement | undefined;
  let currChart: HTMLElement | undefined;
  let tempChart: HTMLElement | undefined;
  let rpmChart: HTMLElement | undefined;
  let powerChart: HTMLElement | undefined;
  let grafanaChartsInitialized = false;

  const grafanaCharts = createGrafanaChartsConfig();

  // ── CAN tab chart refs & configs ──
  let canChart1: HTMLElement | undefined;
  let canChart2: HTMLElement | undefined;
  let canChartsInitialized = false;

  const canChartConfigs = createCanChartConfigs();

  // CAN tab statistics
  let canStats: CanStatistics = {
    chart1: { newest: 0, average: 0, rms: 0 },
    chart2: { newest: 0, average: 0, rms: 0 }
  };
  let chart1History: number[] = [];
  let chart2History: number[] = [];

  // ── CAN Live Device States ──
  let hasCanData = false;
  let canConnected = false;
  let lastCanFrameTime = '---';
  let fccuState: FccuState | number = FccuState.Stopped; // 0=INIT, 1=STARTING, 2=RUNNING, 3=STOPPED
  let fccuMainValve = false;
  let fccuPurgeValve = false;
  let fccuFanState = false;
  let fccuFanDuty = 0;
  let fccuVersion = '—';

  // ── CAN controls state ──
  let fccuCmds: FccuCommandsState = {
    systemEnable: false,
    mainValve: false,
    manualPurge: false,
    fanOn: false
  };

  // FCCU Settings
  let fccuSettings: FccuSettings = {
    fanMode: FccuFanMode.Manual, // 0=Ręczny, 1=Liniowy temp., 2=Regulator PID
    purgeInterval: 60,          // 0-16383 sekund
    pGain: 100,                 // 0-1000
    iGain: 50,                  // 0-1000
    dGain: 10                   // 0-1000
  };

  $: isPidEnabled = Number(fccuSettings.fanMode) === FccuFanMode.PidController;

  // MCU Lighting
  let lighting: LightingState = {
    headlight: false,
    positionLight: false,
    brakeLight: false,
    leftIndicator: false,
    rightIndicator: false,
    hazard: false
  };

  // CAN send status
  let canSendStatus = '';

  async function toggleFccuCmd(cmdType: FccuCommandType) {
    if (cmdType === 'system_enable') fccuCmds.systemEnable = !fccuCmds.systemEnable;
    else if (cmdType === 'main_valve') fccuCmds.mainValve = !fccuCmds.mainValve;
    else if (cmdType === 'manual_purge') fccuCmds.manualPurge = !fccuCmds.manualPurge;
    else if (cmdType === 'fan_on') fccuCmds.fanOn = !fccuCmds.fanOn;

    const stateMap: Record<FccuCommandType, boolean> = {
      system_enable: fccuCmds.systemEnable,
      main_valve: fccuCmds.mainValve,
      manual_purge: fccuCmds.manualPurge,
      fan_on: fccuCmds.fanOn
    };

    try {
      const result = await api.sendFccuCommand(cmdType, stateMap[cmdType]);
      canSendStatus = result;
    } catch (e: unknown) {
      canSendStatus = `Błąd: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  async function sendFccuSettings() {
    try {
      const result = await api.sendFccuSettings(fccuSettings);
      canSendStatus = result;
    } catch (e: unknown) {
      canSendStatus = `Błąd: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  async function sendLightingCmd() {
    try {
      const result = await api.sendLighting(lighting);
      canSendStatus = result;
    } catch (e: unknown) {
      canSendStatus = `Błąd: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  // ── Init charts ──
  async function initGrafanaCharts() {
    if (grafanaChartsInitialized) return;
    await tick();
    if (!voltChart || !currChart || !tempChart || !rpmChart || !powerChart) return;
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
    if (!canChart1 || !canChart2) return;
    Plotly.newPlot(canChart1, canChartConfigs.chart1.data, canChartConfigs.chart1.layout, plotlyOptions);
    Plotly.newPlot(canChart2, canChartConfigs.chart2.data, canChartConfigs.chart2.layout, plotlyOptions);
    canChartsInitialized = true;
  }

  async function switchSource(source: DataSource) {
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
    if (selectedSource === 'Grafana') {
      initGrafanaCharts();
    } else {
      initCanCharts();
    }

    // PostgreSQL stream → Grafana tab
    const unlistenPgPromise = api.subscribeEngineData((data) => {
      if (selectedSource === 'Grafana') {
        currentReadings = data;
      }
      const now = new Date().toLocaleTimeString();

      if (
        selectedSource === 'Grafana' &&
        grafanaChartsInitialized &&
        voltChart &&
        currChart &&
        tempChart &&
        rpmChart &&
        powerChart
      ) {
        Plotly.extendTraces(voltChart, { x: [[now], [now], [now]], y: [[data.voltage1], [data.voltage2], [data.voltage3]] }, [0, 1, 2], MAX_POINTS);
        Plotly.extendTraces(currChart, { x: [[now], [now], [now]], y: [[data.current1], [data.current2], [data.current3]] }, [0, 1, 2], MAX_POINTS);
        Plotly.extendTraces(tempChart, { x: [[now]], y: [[data.temperature]] }, [0], MAX_POINTS);
        Plotly.extendTraces(rpmChart, { x: [[now]], y: [[data.rpm]] }, [0], MAX_POINTS);
        Plotly.extendTraces(powerChart, { x: [[now], [now], [now]], y: [[data.power1], [data.power2], [data.power3]] }, [0, 1, 2], MAX_POINTS);
      }
    });

    // CAN stream → CAN tab
    const unlistenCanPromise = api.subscribeCanData((d) => {
      const now = new Date().toLocaleTimeString();

      if (selectedSource === 'CAN') {
        hasCanData = true;
        canConnected = true;
        lastCanFrameTime = now;

        if (d.fccu_state !== undefined) fccuState = d.fccu_state;
        if (d.fccu_main_valve !== undefined) fccuMainValve = d.fccu_main_valve;
        if (d.fccu_purge_valve !== undefined) fccuPurgeValve = d.fccu_purge_valve;
        if (d.fccu_fan_state !== undefined) fccuFanState = d.fccu_fan_state;
        if (d.fccu_fan_duty !== undefined) fccuFanDuty = d.fccu_fan_duty;
        if (d.fccu_version && d.fccu_version.trim().length > 0) fccuVersion = d.fccu_version;

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
        if (canChartsInitialized && canChart1 && canChart2) {
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
      unlistenPgPromise.then(u => u());
      unlistenCanPromise.then(u => u());
    };
  });
</script>

<div class="dashboard">
  <!-- ═════════ SIDEBAR ═════════ -->
  <aside class="sidebar">
    <h1 class="logo">Monitor magistrali CAN</h1>

    <div class="section">
      <h2 class="section-title">Źródło danych</h2>
      <div class="source-buttons">
        {#each sources as source}
          <button class="source-btn {selectedSource === source ? 'active' : ''}" on:click={() => switchSource(source)}>
            {source}
          </button>
        {/each}
      </div>
    </div>

    <div class="section">
      <h2 class="section-title">Pomiary na żywo</h2>
      
      <!-- Napięcia 1, 2, 3 -->
      <div class="metric-group">
        <div class="metric-card status-{getStatus(currentReadings.voltage1, thresholds.voltage)}">
          <div class="metric-header"><span class="metric-label">Napięcie 1</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal(currentReadings.voltage1, 1)} <span class="unit">V</span></div>
        </div>
        <div class="metric-card status-{getStatus(currentReadings.voltage2, thresholds.voltage)}">
          <div class="metric-header"><span class="metric-label">Napięcie 2</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal(currentReadings.voltage2, 1)} <span class="unit">V</span></div>
        </div>
        <div class="metric-card status-{getStatus(currentReadings.voltage3, thresholds.voltage)}">
          <div class="metric-header"><span class="metric-label">Napięcie 3</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal(currentReadings.voltage3, 1)} <span class="unit">V</span></div>
        </div>
      </div>

      <!-- Prądy 1, 2, 3 -->
      <div class="metric-group">
        <div class="metric-card status-{getStatus(currentReadings.current1, thresholds.current)}">
          <div class="metric-header"><span class="metric-label">Prąd 1</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal(currentReadings.current1, 1)} <span class="unit">A</span></div>
        </div>
        <div class="metric-card status-{getStatus(currentReadings.current2, thresholds.current)}">
          <div class="metric-header"><span class="metric-label">Prąd 2</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal(currentReadings.current2, 1)} <span class="unit">A</span></div>
        </div>
        <div class="metric-card status-{getStatus(currentReadings.current3, thresholds.current)}">
          <div class="metric-header"><span class="metric-label">Prąd 3</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal(currentReadings.current3, 1)} <span class="unit">A</span></div>
        </div>
      </div>

      <!-- Moce 1, 2, 3 -->
      <div class="metric-group">
        <div class="metric-card status-{getStatus(currentReadings.power1, thresholds.power)}">
          <div class="metric-header"><span class="metric-label">Moc 1</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal(currentReadings.power1, 0)} <span class="unit">W</span></div>
        </div>
        <div class="metric-card status-{getStatus(currentReadings.power2, thresholds.power)}">
          <div class="metric-header"><span class="metric-label">Moc 2</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal(currentReadings.power2, 0)} <span class="unit">W</span></div>
        </div>
        <div class="metric-card status-{getStatus(currentReadings.power3, thresholds.power)}">
          <div class="metric-header"><span class="metric-label">Moc 3</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal(currentReadings.power3, 0)} <span class="unit">W</span></div>
        </div>
      </div>

      <!-- Temperatura -->
      <div class="metric-card status-{getStatus(currentReadings.temperature, thresholds.temperature)}">
        <div class="metric-header"><span class="metric-label">Temperatura</span><div class="indicator"></div></div>
        <div class="metric-value">{fmtVal(currentReadings.temperature, 1)} <span class="unit">°C</span></div>
      </div>

      <!-- Prędkość obrotowa -->
      <div class="metric-card status-{getStatus(currentReadings.rpm, thresholds.rpm)}">
        <div class="metric-header"><span class="metric-label">Prędkość obrotowa</span><div class="indicator"></div></div>
        <div class="metric-value">{fmtVal(currentReadings.rpm, 0)} <span class="unit">obr./min</span></div>
      </div>
    </div>

    <!-- Komunikacja CAN status -->
    <div class="section can-comm-section">
      <h2 class="section-title">Komunikacja CAN</h2>
      <div class="can-comm-box">
        <div class="can-comm-row">
          <span class="can-comm-label">Połączenie</span>
          <span class="can-comm-value">{canConnected ? 'Połączono' : 'Brak informacji'}</span>
        </div>
        <div class="can-comm-row">
          <span class="can-comm-label">Ostatnia ramka</span>
          <span class="can-comm-value">{lastCanFrameTime}</span>
        </div>
      </div>
      <p class="can-comm-note">Po utracie danych pokaż „Brak odczytu”, zamiast sugerować poprawny stan urządzeń.</p>
    </div>
  </aside>

  <!-- ═════════ MAIN CHARTS / PANELS AREA ═════════ -->
  <main class="charts-area">
    {#if selectedSource === 'Grafana'}
      <!-- ═══ GRAFANA VIEW (NIETKNIĘTA ZGODNIE Z POLECENIEM) ═══ -->
      <div class="chart-container"><h3 class="chart-title">Voltages</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={voltChart}></div></div></div>
      <div class="chart-container"><h3 class="chart-title">Currents</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={currChart}></div></div></div>
      <div class="chart-container"><h3 class="chart-title">Temperature</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={tempChart}></div></div></div>
      <div class="chart-container"><h3 class="chart-title">RPM</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={rpmChart}></div></div></div>
      <div class="chart-container"><h3 class="chart-title">Power</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={powerChart}></div></div></div>

    {:else if selectedSource === 'CAN'}
      <!-- ═══ NOWY WIDOK ZAKŁADKI CAN ZGODNY ZE ZDJĘCIEM ═══ -->
      <div class="can-view-wrapper">
        
        <!-- 1. GÓRNY PASEK STANU URZĄDZEŃ (5 KOLUMN) -->
        <div class="top-status-bar">
          <div class="top-status-item">
            <span class="status-lbl">Stan systemu</span>
            <span class="status-val bold" style="color: {getFccuStateColor(fccuState)}">
              {getFccuStateLabel(fccuState)}
            </span>
            <span class="status-sub">Sterownik FCCU</span>
          </div>

          <div class="top-status-item">
            <span class="status-lbl">Zawór główny</span>
            <span class="status-val bold">{fccuMainValve ? 'Otwarty' : 'Zamknięty'}</span>
            <span class="status-sub">MAIN VALVE</span>
          </div>

          <div class="top-status-item">
            <span class="status-lbl">Zawór przedmuchu</span>
            <span class="status-val bold">{fccuPurgeValve ? 'Otwarty' : 'Zamknięty'}</span>
            <span class="status-sub">PURGE VALVE</span>
          </div>

          <div class="top-status-item">
            <span class="status-lbl">Wentylator</span>
            <span class="status-val bold">{fccuFanState ? 'Włączony' : 'Wyłączony'}</span>
            <span class="status-sub">
              {fccuSettings.fanMode === 0 ? 'Tryb ręczny' : fccuSettings.fanMode === 1 ? 'Tryb liniowy' : 'Tryb PID'} · {fccuFanDuty ? fccuFanDuty * 30 : 0} obr./min
            </span>
          </div>

          <div class="top-status-item">
            <span class="status-lbl">Wersja oprogramowania</span>
            <span class="status-val bold">{fccuVersion}</span>
            <span class="status-sub">{fccuVersion === '—' ? 'Brak odczytu wersji' : 'Oprogramowanie FCCU'}</span>
          </div>
        </div>

        <!-- 2. WYKRES 1: NAPIĘCIE OGNIWA (FC) -->
        <div class="chart-section-card">
          <div class="chart-header-row">
            <h3 class="chart-main-title">Napięcie ogniwa (FC)</h3>
            <span class="chart-header-time">Ostatnie 60 s</span>
          </div>
          <div class="chart-card-content">
            <div class="chart-graph-box">
              {#if !hasCanData}
                <div class="chart-empty-hint">Oczekiwanie na próbki CAN</div>
              {/if}
              <div class="plotly-chart-inner" bind:this={canChart1}></div>
            </div>
            <div class="stats-column">
              <div class="stat-box">
                <span class="stat-box-lbl">Bieżąca</span>
                <span class="stat-box-val">{fmtVal(canStats.chart1.newest)} V</span>
              </div>
              <div class="stat-box">
                <span class="stat-box-lbl">Średnia</span>
                <span class="stat-box-val">{fmtVal(canStats.chart1.average)} V</span>
              </div>
              <div class="stat-box">
                <span class="stat-box-lbl">Wartość skuteczna</span>
                <span class="stat-box-val">{fmtVal(canStats.chart1.rms)} V</span>
              </div>
            </div>
          </div>
        </div>

        <!-- 3. WYKRES 2: PRĄD OGNIWA (FC) -->
        <div class="chart-section-card">
          <div class="chart-header-row">
            <h3 class="chart-main-title">Prąd ogniwa (FC)</h3>
            <span class="chart-header-time">Ostatnie 60 s</span>
          </div>
          <div class="chart-card-content">
            <div class="chart-graph-box">
              {#if !hasCanData}
                <div class="chart-empty-hint">Oczekiwanie na próbki CAN</div>
              {/if}
              <div class="plotly-chart-inner" bind:this={canChart2}></div>
            </div>
            <div class="stats-column">
              <div class="stat-box">
                <span class="stat-box-lbl">Bieżąca</span>
                <span class="stat-box-val">{fmtVal(canStats.chart2.newest)} A</span>
              </div>
              <div class="stat-box">
                <span class="stat-box-lbl">Średnia</span>
                <span class="stat-box-val">{fmtVal(canStats.chart2.average)} A</span>
              </div>
              <div class="stat-box">
                <span class="stat-box-lbl">Wartość skuteczna</span>
                <span class="stat-box-val">{fmtVal(canStats.chart2.rms)} A</span>
              </div>
            </div>
          </div>
        </div>

        <!-- 4. KOMENDY STEROWNIKA FCCU -->
        <div class="section-card">
          <div class="section-card-header">
            <h3 class="section-card-title">KOMENDY STEROWNIKA FCCU</h3>
            <span class="section-card-desc">Stan urządzenia po lewej · komenda do wysłania po prawej</span>
          </div>
          <div class="cmd-grid-2x2">
            <!-- System -->
            <div class="cmd-tile">
              <div class="cmd-tile-info">
                <span class="cmd-tile-name">System</span>
                <span class="cmd-tile-state">Stan: {fccuState === 2 ? 'włączony' : 'wyłączony'}</span>
              </div>
              <button class="action-btn btn-blue" on:click={() => toggleFccuCmd('system_enable')}>
                {fccuCmds.systemEnable ? 'Wyłącz system' : 'Włącz system'}
              </button>
            </div>

            <!-- Zawór główny -->
            <div class="cmd-tile">
              <div class="cmd-tile-info">
                <span class="cmd-tile-name">Zawór główny</span>
                <span class="cmd-tile-state">Stan: {fccuMainValve ? 'otwarty' : 'zamknięty'}</span>
              </div>
              <button class="action-btn btn-grey" on:click={() => toggleFccuCmd('main_valve')}>
                {fccuCmds.mainValve ? 'Zamknij zawór główny' : 'Otwórz zawór główny'}
              </button>
            </div>

            <!-- Przedmuch (purge) -->
            <div class="cmd-tile">
              <div class="cmd-tile-info">
                <span class="cmd-tile-name">Przedmuch (purge)</span>
                <span class="cmd-tile-state">Stan: {fccuPurgeValve ? 'aktywny' : 'nieaktywny'}</span>
              </div>
              <button class="action-btn btn-grey" on:click={() => toggleFccuCmd('manual_purge')}>
                Uruchom przedmuch
              </button>
            </div>

            <!-- Wentylator -->
            <div class="cmd-tile">
              <div class="cmd-tile-info">
                <span class="cmd-tile-name">Wentylator</span>
                <span class="cmd-tile-state">Stan: {fccuFanState ? 'włączony' : 'wyłączony'}</span>
              </div>
              <button class="action-btn btn-grey" on:click={() => toggleFccuCmd('fan_on')}>
                {fccuCmds.fanOn ? 'Wyłącz wentylator' : 'Włącz wentylator'}
              </button>
            </div>
          </div>
          <span class="section-footer-hint">Wysłanie komendy nie oznacza wykonania — aktualny stan pokaż dopiero po potwierdzeniu przez sterownik.</span>
        </div>

        <!-- 5. USTAWIENIA STEROWNIKA FCCU -->
        <div class="section-card">
          <div class="section-card-header">
            <h3 class="section-card-title">USTAWIENIA STEROWNIKA FCCU</h3>
            <span class="section-card-desc">Wartości do wysłania</span>
          </div>

          <div class="settings-content-layout">
            <!-- Lewa kolumna: formularz nastaw -->
            <div class="settings-form-column">
              <div class="setting-form-row">
                <label for="fan-mode-select" class="setting-form-label">Tryb wentylatora</label>
                <select id="fan-mode-select" class="setting-form-select" bind:value={fccuSettings.fanMode}>
                  <option value={0}>Ręczny</option>
                  <option value={1}>Liniowy temp.</option>
                  <option value={2}>Regulator PID</option>
                </select>
                <span class="setting-form-helper">Wybór sposobu sterowania</span>
              </div>

              <div class="setting-form-row">
                <label for="purge-interval-input" class="setting-form-label">Odstęp między przedmuchami</label>
                <div class="input-unit-wrapper">
                  <input id="purge-interval-input" type="number" class="setting-form-input" bind:value={fccuSettings.purgeInterval} min="0" max="16383" />
                  <span class="input-unit-badge">s</span>
                </div>
                <span class="setting-form-helper">Czas między cyklami purge</span>
              </div>

              <div class="setting-form-row" class:disabled={!isPidEnabled}>
                <label for="p-gain-input" class="setting-form-label">Wzmocnienie P</label>
                <input
                  id="p-gain-input"
                  type="number"
                  class="setting-form-input"
                  bind:value={fccuSettings.pGain}
                  min="0"
                  max="1000"
                  disabled={!isPidEnabled}
                />
                <span class="setting-form-helper">Część proporcjonalna</span>
              </div>

              <div class="setting-form-row" class:disabled={!isPidEnabled}>
                <label for="i-gain-input" class="setting-form-label">Wzmocnienie I</label>
                <input
                  id="i-gain-input"
                  type="number"
                  class="setting-form-input"
                  bind:value={fccuSettings.iGain}
                  min="0"
                  max="1000"
                  disabled={!isPidEnabled}
                />
                <span class="setting-form-helper">Część całkująca</span>
              </div>

              <div class="setting-form-row" class:disabled={!isPidEnabled}>
                <label for="d-gain-input" class="setting-form-label">Wzmocnienie D</label>
                <input
                  id="d-gain-input"
                  type="number"
                  class="setting-form-input"
                  bind:value={fccuSettings.dGain}
                  min="0"
                  max="1000"
                  disabled={!isPidEnabled}
                />
                <span class="setting-form-helper">Część różniczkująca</span>
              </div>

              <span class="setting-form-footer-note">
                Nowe wartości wymagają potwierdzenia przez sterownik.
                {#if !isPidEnabled}
                  <span class="pid-disabled-hint">Nastawy PID są nieaktywne w tym trybie.</span>
                {/if}
              </span>
            </div>

            <!-- Prawa kolumna: odczyt wentylatora i przycisk wysłania -->
            <div class="settings-readout-column">
              <div class="fan-readout-panel">
                <h4 class="fan-readout-panel-title">BIEŻĄCY ODCZYT WENTYLATORA</h4>
                <div class="fan-readout-item">
                  <span class="fan-readout-key">Stan</span>
                  <span class="fan-readout-val bold">{fccuFanState ? 'Włączony' : 'Wyłączony'}</span>
                </div>
                <div class="fan-readout-item">
                  <span class="fan-readout-key">Tryb</span>
                  <span class="fan-readout-val">
                    {fccuSettings.fanMode === 0 ? 'Ręczny' : fccuSettings.fanMode === 1 ? 'Liniowy' : 'Regulator PID'}
                  </span>
                </div>
                <div class="fan-readout-item">
                  <span class="fan-readout-key">Prędkość</span>
                  <span class="fan-readout-val">{fccuFanDuty ? fccuFanDuty * 30 : 0} obr./min</span>
                </div>
                <span class="fan-readout-subnote">Włączanie i wyłączanie: sekcja komend powyżej.</span>
              </div>
              <span class="settings-separation-note">Ustawienia i stan pracy są pokazane osobno.</span>
              <button class="blue-submit-btn" on:click={sendFccuSettings}>Wyślij ustawienia</button>
            </div>
          </div>
        </div>

        <!-- 6. STEROWANIE OŚWIETLENIEM -->
        <div class="section-card">
          <div class="section-card-header">
            <h3 class="section-card-title">STEROWANIE OŚWIETLENIEM</h3>
            <span class="section-card-desc">Ustaw żądany stan, a następnie wyślij do sterownika.</span>
          </div>

          <div class="lighting-grid-layout">
            <!-- Kolumna 1 -->
            <div class="lighting-column">
              <div class="light-switch-row">
                <label class="switch-container">
                  <input type="checkbox" bind:checked={lighting.headlight} />
                  <span class="switch-track"></span>
                  <span class="switch-label-text">Światła główne</span>
                </label>
                <span class="switch-status-text">{lighting.headlight ? 'Włączone' : 'Wyłączone'}</span>
              </div>

              <div class="light-switch-row">
                <label class="switch-container">
                  <input type="checkbox" bind:checked={lighting.brakeLight} />
                  <span class="switch-track"></span>
                  <span class="switch-label-text">Światło STOP</span>
                </label>
                <span class="switch-status-text">{lighting.brakeLight ? 'Włączone' : 'Wyłączone'}</span>
              </div>

              <div class="light-switch-row">
                <label class="switch-container">
                  <input type="checkbox" bind:checked={lighting.rightIndicator} />
                  <span class="switch-track"></span>
                  <span class="switch-label-text">Prawy kierunkowskaz</span>
                </label>
                <span class="switch-status-text">{lighting.rightIndicator ? 'Włączone' : 'Wyłączone'}</span>
              </div>
            </div>

            <!-- Kolumna 2 -->
            <div class="lighting-column">
              <div class="light-switch-row">
                <label class="switch-container">
                  <input type="checkbox" bind:checked={lighting.positionLight} />
                  <span class="switch-track"></span>
                  <span class="switch-label-text">Światła pozycyjne</span>
                </label>
                <span class="switch-status-text">{lighting.positionLight ? 'Włączone' : 'Wyłączone'}</span>
              </div>

              <div class="light-switch-row">
                <label class="switch-container">
                  <input type="checkbox" bind:checked={lighting.leftIndicator} />
                  <span class="switch-track"></span>
                  <span class="switch-label-text">Lewy kierunkowskaz</span>
                </label>
                <span class="switch-status-text">{lighting.leftIndicator ? 'Włączone' : 'Wyłączone'}</span>
              </div>

              <div class="light-switch-row">
                <label class="switch-container">
                  <input type="checkbox" bind:checked={lighting.hazard} />
                  <span class="switch-track"></span>
                  <span class="switch-label-text">Światła awaryjne</span>
                </label>
                <span class="switch-status-text">{lighting.hazard ? 'Włączone' : 'Wyłączone'}</span>
              </div>
            </div>
          </div>

          <div class="lighting-action-row">
            <span class="lighting-action-hint">Wybrany stan zostanie wysłany po użyciu przycisku.</span>
            <button class="blue-submit-btn" on:click={sendLightingCmd}>Wyślij oświetlenie</button>
          </div>
        </div>

        <!-- 7. STATUS BAR WYSYŁANIA & DOLNY TEKST INFORMACYJNY -->
        {#if canSendStatus}
          <div class="can-status-bar-notification">{canSendStatus}</div>
        {/if}

        <div class="bottom-page-footer">
          <span class="footer-note-left">Propozycja zmian istniejącego interfejsu - przykładowe stany urządzeń - wartości pomiarów jak na zrzutach</span>
          <span class="footer-note-right">Stan odczytany i wysłana komenda to osobne informacje.</span>
        </div>

      </div>
    {/if}
  </main>
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    background-color: #09090b;
    color: #e4e4e7;
    font-family: 'Segoe UI', system-ui, sans-serif;
    height: 100vh;
    overflow: hidden;
  }

  .dashboard {
    display: flex;
    height: 100vh;
    width: 100vw;
  }

  /* ═════════ SIDEBAR ═════════ */
  .sidebar {
    width: 300px;
    background-color: #18181b;
    border-right: 1px solid #27272a;
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 24px;
    overflow-y: auto;
    flex-shrink: 0;
  }

  .logo {
    font-size: 1.15rem;
    font-weight: 600;
    margin: 0;
    color: #ffffff;
    letter-spacing: -0.01em;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .section-title {
    font-size: 0.8rem;
    color: #a1a1aa;
    margin: 0;
    font-weight: 500;
  }

  .source-buttons {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .source-btn {
    background-color: #27272a;
    color: #e4e4e7;
    border: none;
    border-radius: 8px;
    padding: 11px 16px;
    text-align: left;
    font-size: 0.95rem;
    cursor: pointer;
    transition: all 0.2s;
  }

  .source-btn.active {
    background-color: #3b82f6;
    color: #ffffff;
    font-weight: 500;
  }

  .metric-group {
    display: flex;
    gap: 6px;
  }

  .metric-card {
    background-color: #27272a;
    border-radius: 8px;
    padding: 9px 10px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    flex: 1;
    border: 1px solid transparent;
    transition: all 0.3s ease;
    min-width: 0;
  }

  .metric-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .indicator {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background-color: #71717a;
    transition: all 0.3s ease;
  }

  .status-normal .indicator { background-color: #10b981; }
  .status-warning .indicator { background-color: #f59e0b; box-shadow: 0 0 8px #f59e0b; }
  .status-alarm { border-color: #ef444433; background-color: #450a0a33; }
  .status-alarm .indicator { background-color: #ef4444; box-shadow: 0 0 12px #ef4444; animation: blink 1s infinite; }
  @keyframes blink { 0% { opacity: 1; } 50% { opacity: 0.3; } 100% { opacity: 1; } }

  .metric-label {
    font-size: 0.68rem;
    color: #a1a1aa;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .metric-value {
    font-size: 1.05rem;
    color: #ffffff;
    font-weight: 500;
  }

  .unit {
    font-size: 0.72rem;
    color: #71717a;
    font-weight: normal;
  }

  /* CAN Communication sidebar box */
  .can-comm-section {
    margin-top: 10px;
    border-top: 1px solid #27272a;
    padding-top: 18px;
  }

  .can-comm-box {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 0.8rem;
  }

  .can-comm-row {
    display: flex;
    justify-content: space-between;
    color: #a1a1aa;
  }

  .can-comm-value {
    color: #ffffff;
    font-weight: 500;
  }

  .can-comm-note {
    font-size: 0.72rem;
    color: #71717a;
    line-height: 1.35;
    margin: 6px 0 0 0;
  }

  /* ═════════ MAIN AREA ═════════ */
  .charts-area {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  /* ── Grafana container ── */
  .chart-container {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 12px;
    padding: 16px 20px;
  }
  .chart-title { font-size: 0.9rem; color: #a1a1aa; margin: 0 0 10px 0; }
  .canvas-wrapper { height: 220px; width: 100%; }
  .plotly-chart { width: 100%; height: 100%; }

  /* ═════════ CAN VIEW (DOKŁADNY UKŁAD ZE ZDJĘCIA) ═════════ */
  .can-view-wrapper {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  /* 1. Górny pasek stanu (5 kolumn) */
  .top-status-bar {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 10px;
    padding: 14px 20px;
    gap: 16px;
  }

  .top-status-item {
    display: flex;
    flex-direction: column;
    gap: 3px;
    border-right: 1px solid #27272a;
    padding-right: 12px;
  }

  .top-status-item:last-child {
    border-right: none;
    padding-right: 0;
  }

  .status-lbl {
    font-size: 0.72rem;
    color: #71717a;
  }

  .status-val {
    font-size: 1.15rem;
    color: #ffffff;
    letter-spacing: -0.01em;
  }

  .status-val.bold {
    font-weight: 700;
  }

  .status-sub {
    font-size: 0.68rem;
    color: #52525b;
  }

  /* 2. Karty wykresów z prawą kolumną statystyk */
  .chart-section-card {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 10px;
    padding: 14px 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .chart-header-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .chart-main-title {
    font-size: 0.95rem;
    font-weight: 600;
    color: #ffffff;
    margin: 0;
  }

  .chart-header-time {
    font-size: 0.75rem;
    color: #71717a;
  }

  .chart-card-content {
    display: flex;
    gap: 16px;
    align-items: stretch;
  }

  .chart-graph-box {
    flex: 1;
    height: 150px;
    position: relative;
    min-width: 0;
  }

  .chart-empty-hint {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #52525b;
    font-size: 0.85rem;
    pointer-events: none;
    z-index: 2;
  }

  .plotly-chart-inner {
    width: 100%;
    height: 100%;
  }

  .stats-column {
    width: 140px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    justify-content: center;
    flex-shrink: 0;
  }

  .stat-box {
    background-color: #27272a;
    border: 1px solid #3f3f46;
    border-radius: 6px;
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stat-box-lbl {
    font-size: 0.68rem;
    color: #71717a;
  }

  .stat-box-val {
    font-size: 1.05rem;
    font-weight: 600;
    color: #ffffff;
    font-variant-numeric: tabular-nums;
  }

  /* 3. Panele sekcji (Komendy, Ustawienia, Oświetlenie) */
  .section-card {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 10px;
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .section-card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .section-card-title {
    font-size: 0.85rem;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: #e4e4e7;
    margin: 0;
    text-transform: uppercase;
  }

  .section-card-desc {
    font-size: 0.75rem;
    color: #71717a;
  }

  /* Siatka komend 2x2 */
  .cmd-grid-2x2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .cmd-tile {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background-color: #27272a;
    border: 1px solid #3f3f46;
    border-radius: 8px;
    padding: 12px 16px;
  }

  .cmd-tile-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .cmd-tile-name {
    font-size: 0.95rem;
    font-weight: 600;
    color: #ffffff;
  }

  .cmd-tile-state {
    font-size: 0.8rem;
    color: #a1a1aa;
  }

  .action-btn {
    border: none;
    border-radius: 6px;
    padding: 10px 18px;
    font-size: 0.85rem;
    cursor: pointer;
    transition: all 0.2s ease;
    white-space: nowrap;
  }

  .btn-blue {
    background-color: #3b82f6;
    color: #ffffff;
    font-weight: 600;
  }
  .btn-blue:hover { background-color: #2563eb; }

  .btn-grey {
    background-color: #27272a;
    color: #e4e4e7;
    border: 1px solid #52525b;
    font-weight: 500;
  }
  .btn-grey:hover { background-color: #3f3f46; border-color: #71717a; }

  .section-footer-hint {
    font-size: 0.72rem;
    color: #71717a;
  }

  /* Układ ustawień FCCU */
  .settings-content-layout {
    display: flex;
    gap: 24px;
    align-items: stretch;
  }

  .settings-form-column {
    flex: 1.3;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .setting-form-row {
    display: flex;
    align-items: center;
    gap: 14px;
    transition: opacity 0.25s ease, filter 0.25s ease;
  }

  .setting-form-row.disabled {
    opacity: 0.38;
    filter: grayscale(0.85);
  }

  .setting-form-row.disabled .setting-form-label,
  .setting-form-row.disabled .setting-form-helper {
    color: #71717a;
    cursor: not-allowed;
    user-select: none;
  }

  .setting-form-label {
    width: 190px;
    font-size: 0.85rem;
    color: #a1a1aa;
    flex-shrink: 0;
  }

  .setting-form-select {
    width: 140px;
    background-color: #27272a;
    border: 1px solid #3f3f46;
    border-radius: 6px;
    padding: 7px 10px;
    color: #e4e4e7;
    font-size: 0.85rem;
    outline: none;
  }

  .setting-form-select:focus { border-color: #3b82f6; }

  .input-unit-wrapper {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 140px;
  }

  .setting-form-input {
    width: 100%;
    background-color: #27272a;
    border: 1px solid #3f3f46;
    border-radius: 6px;
    padding: 7px 10px;
    color: #e4e4e7;
    font-size: 0.85rem;
    outline: none;
    transition: all 0.2s ease;
  }

  .setting-form-input:focus { border-color: #3b82f6; }

  .setting-form-input:disabled {
    background-color: #1c1c1f;
    border-color: #27272a;
    color: #71717a;
    cursor: not-allowed;
    box-shadow: none;
  }

  .input-unit-badge {
    font-size: 0.8rem;
    color: #71717a;
  }

  .setting-form-helper {
    font-size: 0.75rem;
    color: #71717a;
  }

  .setting-form-footer-note {
    font-size: 0.72rem;
    color: #71717a;
    margin-top: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .pid-disabled-hint {
    color: #f59e0b;
    font-size: 0.72rem;
  }

  /* Prawa kolumna ustawień (Odczyt wentylatora) */
  .settings-readout-column {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
    justify-content: space-between;
  }

  .fan-readout-panel {
    background-color: #121215;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .fan-readout-panel-title {
    font-size: 0.72rem;
    font-weight: 700;
    color: #71717a;
    letter-spacing: 0.05em;
    margin: 0 0 4px 0;
  }

  .fan-readout-item {
    display: flex;
    justify-content: space-between;
    font-size: 0.85rem;
  }

  .fan-readout-key { color: #a1a1aa; }
  .fan-readout-val { color: #e4e4e7; }
  .fan-readout-val.bold { font-weight: 700; color: #ffffff; }

  .fan-readout-subnote {
    font-size: 0.7rem;
    color: #52525b;
    margin-top: 6px;
  }

  .settings-separation-note {
    font-size: 0.72rem;
    color: #71717a;
  }

  .blue-submit-btn {
    align-self: flex-end;
    background-color: #3b82f6;
    color: #ffffff;
    font-weight: 600;
    border: none;
    border-radius: 6px;
    padding: 10px 22px;
    font-size: 0.85rem;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .blue-submit-btn:hover { background-color: #2563eb; }

  /* Oświetlenie */
  .lighting-grid-layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 36px;
  }

  .lighting-column {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .light-switch-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .switch-container {
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
  }

  .switch-container input { display: none; }

  .switch-track {
    position: relative;
    width: 38px;
    height: 20px;
    background-color: #3f3f46;
    border-radius: 10px;
    transition: background-color 0.2s;
    flex-shrink: 0;
  }

  .switch-track::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background-color: #71717a;
    border-radius: 50%;
    transition: all 0.2s;
  }

  .switch-container input:checked + .switch-track {
    background-color: #3b82f6;
  }

  .switch-container input:checked + .switch-track::after {
    left: 20px;
    background-color: #ffffff;
  }

  .switch-label-text {
    font-size: 0.85rem;
    color: #e4e4e7;
  }

  .switch-status-text {
    font-size: 0.78rem;
    color: #71717a;
  }

  .lighting-action-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-top: 1px solid #27272a;
    padding-top: 14px;
    margin-top: 4px;
  }

  .lighting-action-hint {
    font-size: 0.72rem;
    color: #71717a;
  }

  /* Pasek powiadomień po wysłaniu */
  .can-status-bar-notification {
    background-color: #14532d;
    border: 1px solid #22c55e33;
    color: #86efac;
    border-radius: 6px;
    padding: 8px 14px;
    font-size: 0.85rem;
  }

  /* Dolna stopka strony */
  .bottom-page-footer {
    display: flex;
    justify-content: space-between;
    font-size: 0.7rem;
    color: #52525b;
    padding: 4px 0 16px 0;
  }
</style>