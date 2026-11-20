<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import Plotly from 'plotly.js-dist-min';
  import { baseLayout, plotlyOptions, MAX_POINTS, fmtVal, calcAvg, calcRms } from '$lib/chartUtils';
  import type { CanReadData, FccuSettings, LightingState } from '$lib/types';

  let canChart1: HTMLElement;
  let canChart2: HTMLElement;
  let canChartsInitialized = false;

  const canChartLayout = {
    ...baseLayout,
    margin: { t: 8, r: 8, l: 38, b: 24 },
    showlegend: false,
    xaxis: {
      gridcolor: '#27272a',
      zeroline: false,
      tickfont: { color: '#71717a' },
      autorange: true
    },
    yaxis: {
      gridcolor: '#27272a',
      zeroline: false,
      tickfont: { color: '#71717a' },
      autorange: true,
      rangemode: 'tozero'
    }
  };

  const canChartConfigs = {
    chart1: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Napięcie FC (V)', line: { color: '#3b82f6', width: 1.5 } }],
      layout: { ...canChartLayout, yaxis: { ...canChartLayout.yaxis, title: '' } }
    },
    chart2: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Prąd FC (A)', line: { color: '#3b82f6', width: 1.5 } }],
      layout: { ...canChartLayout, yaxis: { ...canChartLayout.yaxis, title: '' } }
    }
  };

  let canStats = {
    chart1: { newest: 0, average: 0, rms: 0 },
    chart2: { newest: 0, average: 0, rms: 0 }
  };
  let chart1History: number[] = [];
  let chart2History: number[] = [];

  let hasCanData = false;
  let fccuState = 3;
  let fccuMainValve = false;
  let fccuPurgeValve = false;
  let fccuFanState = false;
  let fccuFanDuty = 0;
  let fccuVersion = '—';

  function getFccuStateLabel(state: number): string {
    switch (state) {
      case 0: return 'Inicjalizacja';
      case 1: return 'Uruchamianie';
      case 2: return 'Włączony';
      case 3: return 'Wyłączony';
      default: return 'Brak odczytu';
    }
  }

  function getFccuStateColor(state: number): string {
    switch (state) {
      case 0: return '#3b82f6';
      case 1: return '#f59e0b';
      case 2: return '#22c55e';
      case 3: return '#f59e0b';
      default: return '#71717a';
    }
  }

  let fccuCmds = {
    systemEnable: false,
    mainValve: false,
    manualPurge: false,
    fanOn: false
  };

  let fccuSettings: FccuSettings = {
    fanMode: 0,
    purgeInterval: 60,
    pGain: 100,
    iGain: 50,
    dGain: 10
  };

  let lighting: LightingState = {
    headlight: false,
    positionLight: false,
    brakeLight: false,
    leftIndicator: false,
    rightIndicator: false,
    hazard: false
  };

  let toastMessage = '';
  let toastType: 'success' | 'error' = 'success';
  let toastVisible = false;
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  function showToast(message: string, isError = false) {
    if (toastTimer) clearTimeout(toastTimer);
    toastMessage = message;
    toastType = isError ? 'error' : 'success';
    toastVisible = true;
    toastTimer = setTimeout(() => {
      toastVisible = false;
    }, 2800);
  }

  async function toggleFccuCmd(cmdType: string) {
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
      showToast(result as string, false);
    } catch (e) {
      showToast(`Błąd: ${e}`, true);
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
      showToast(result as string, false);
    } catch (e) {
      showToast(`Błąd: ${e}`, true);
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
      showToast(result as string, false);
    } catch (e) {
      showToast(`Błąd: ${e}`, true);
    }
  }

  onMount(() => {
    if (canChart1 && canChart2) {
      Plotly.newPlot(canChart1, canChartConfigs.chart1.data, canChartConfigs.chart1.layout, plotlyOptions);
      Plotly.newPlot(canChart2, canChartConfigs.chart2.data, canChartConfigs.chart2.layout, plotlyOptions);
      canChartsInitialized = true;
    }

    const unlistenCan = listen<CanReadData>('can-data-update', (event) => {
      const d = event.payload;
      const now = new Date().toLocaleTimeString();

      hasCanData = true;

      if (d.fccu_state !== undefined) fccuState = d.fccu_state;
      if (d.fccu_main_valve !== undefined) fccuMainValve = d.fccu_main_valve;
      if (d.fccu_purge_valve !== undefined) fccuPurgeValve = d.fccu_purge_valve;
      if (d.fccu_fan_state !== undefined) fccuFanState = d.fccu_fan_state;
      if (d.fccu_fan_duty !== undefined) fccuFanDuty = d.fccu_fan_duty;
      if (d.fccu_version && d.fccu_version.trim().length > 0) fccuVersion = d.fccu_version;

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
    });

    return () => {
      unlistenCan.then((u) => u());
    };
  });
</script>

<div class="can-view-wrapper">
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

  <div class="section-card">
    <div class="section-card-header">
      <h3 class="section-card-title">KOMENDY STEROWNIKA FCCU</h3>
    </div>
    <div class="cmd-grid-2x2">
      <div class="cmd-tile">
        <div class="cmd-tile-info">
          <span class="cmd-tile-name">System</span>
          <span class="cmd-tile-state">Stan: {fccuState === 2 ? 'włączony' : 'wyłączony'}</span>
        </div>
        <button class="action-btn btn-blue" on:click={() => toggleFccuCmd('system_enable')}>
          {fccuCmds.systemEnable ? 'Wyłącz system' : 'Włącz system'}
        </button>
      </div>

      <div class="cmd-tile">
        <div class="cmd-tile-info">
          <span class="cmd-tile-name">Zawór główny</span>
          <span class="cmd-tile-state">Stan: {fccuMainValve ? 'otwarty' : 'zamknięty'}</span>
        </div>
        <button class="action-btn btn-grey" on:click={() => toggleFccuCmd('main_valve')}>
          {fccuCmds.mainValve ? 'Zamknij zawór główny' : 'Otwórz zawór główny'}
        </button>
      </div>

      <div class="cmd-tile">
        <div class="cmd-tile-info">
          <span class="cmd-tile-name">Przedmuch (purge)</span>
          <span class="cmd-tile-state">Stan: {fccuPurgeValve ? 'aktywny' : 'nieaktywny'}</span>
        </div>
        <button class="action-btn btn-grey" on:click={() => toggleFccuCmd('manual_purge')}>
          Uruchom przedmuch
        </button>
      </div>

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
  </div>

  <div class="section-card">
    <div class="section-card-header">
      <h3 class="section-card-title">USTAWIENIA STEROWNIKA FCCU</h3>
    </div>

    <div class="settings-content-layout">
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

        <div class="setting-form-row">
          <label for="p-gain-input" class="setting-form-label">Wzmocnienie P</label>
          <input id="p-gain-input" type="number" class="setting-form-input" bind:value={fccuSettings.pGain} min="0" max="1000" />
          <span class="setting-form-helper">Część proporcjonalna</span>
        </div>

        <div class="setting-form-row">
          <label for="i-gain-input" class="setting-form-label">Wzmocnienie I</label>
          <input id="i-gain-input" type="number" class="setting-form-input" bind:value={fccuSettings.iGain} min="0" max="1000" />
          <span class="setting-form-helper">Część całkująca</span>
        </div>

        <div class="setting-form-row">
          <label for="d-gain-input" class="setting-form-label">Wzmocnienie D</label>
          <input id="d-gain-input" type="number" class="setting-form-input" bind:value={fccuSettings.dGain} min="0" max="1000" />
          <span class="setting-form-helper">Część różniczkująca</span>
        </div>
      </div>

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
        </div>
        <button class="blue-submit-btn" on:click={sendFccuSettings}>Wyślij ustawienia</button>
      </div>
    </div>
  </div>

  <div class="section-card">
    <div class="section-card-header">
      <h3 class="section-card-title">STEROWANIE OŚWIETLENIEM</h3>
    </div>

    <div class="lighting-grid-layout">
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
      <button class="blue-submit-btn" on:click={sendLightingCmd}>Wyślij oświetlenie</button>
    </div>
  </div>

  {#if toastVisible}
    <div class="toast-container" transition:fly={{ y: 24, duration: 250 }}>
      <div class="toast-card {toastType}">
        <span class="toast-icon">
          {#if toastType === 'error'}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10"></circle>
              <line x1="12" y1="8" x2="12" y2="12"></line>
              <line x1="12" y1="16" x2="12.01" y2="16"></line>
            </svg>
          {:else}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="20 6 9 17 4 12"></polyline>
            </svg>
          {/if}
        </span>
        <div class="toast-body">
          <span class="toast-title">{toastType === 'error' ? 'Błąd CAN' : 'Wysłano ramkę CAN'}</span>
          <span class="toast-message">{toastMessage}</span>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .can-view-wrapper {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .top-status-bar {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 12px;
  }

  .top-status-item {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .status-lbl {
    font-size: 0.72rem;
    color: #a1a1aa;
  }

  .status-val {
    font-size: 1.15rem;
    color: #ffffff;
    font-weight: 500;
  }

  .status-val.bold {
    font-weight: 700;
  }

  .status-sub {
    font-size: 0.68rem;
    color: #71717a;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .chart-section-card {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 12px;
    padding: 16px 20px;
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
    font-weight: 700;
    color: #ffffff;
  }

  .section-card {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 12px;
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .section-card-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .section-card-title {
    font-size: 0.85rem;
    font-weight: 700;
    color: #e4e4e7;
    letter-spacing: 0.05em;
    margin: 0;
  }

  .cmd-grid-2x2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 20px;
  }

  .cmd-tile {
    background-color: #121215;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .cmd-tile-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .cmd-tile-name {
    font-size: 0.85rem;
    font-weight: 600;
    color: #ffffff;
  }

  .cmd-tile-state {
    font-size: 0.75rem;
    color: #71717a;
  }

  .action-btn {
    border-radius: 6px;
    padding: 8px 16px;
    font-size: 0.82rem;
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
  }

  .setting-form-label {
    width: 190px;
    font-size: 0.82rem;
    color: #d4d4d8;
    flex-shrink: 0;
  }

  .setting-form-select,
  .setting-form-input {
    background-color: #27272a;
    border: 1px solid #3f3f46;
    color: #ffffff;
    border-radius: 6px;
    padding: 7px 12px;
    font-size: 0.85rem;
    width: 120px;
    outline: none;
  }
  .setting-form-select:focus,
  .setting-form-input:focus {
    border-color: #3b82f6;
  }

  .input-unit-wrapper {
    position: relative;
    display: inline-flex;
    align-items: center;
    width: 120px;
  }

  .input-unit-wrapper .setting-form-input {
    width: 100%;
    padding-right: 28px;
  }

  .input-unit-badge {
    position: absolute;
    right: 10px;
    font-size: 0.75rem;
    color: #71717a;
    pointer-events: none;
  }

  .setting-form-helper {
    font-size: 0.75rem;
    color: #71717a;
  }

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
    padding: 4px 0;
  }

  .switch-container {
    display: flex;
    align-items: center;
    gap: 12px;
    cursor: pointer;
    user-select: none;
  }

  .switch-container input {
    display: none;
  }

  .switch-track {
    width: 36px;
    height: 20px;
    background-color: #3f3f46;
    border-radius: 10px;
    position: relative;
    transition: background-color 0.2s ease;
  }

  .switch-track::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background-color: #ffffff;
    border-radius: 50%;
    transition: transform 0.2s ease;
  }

  .switch-container input:checked + .switch-track {
    background-color: #3b82f6;
  }

  .switch-container input:checked + .switch-track::after {
    transform: translateX(16px);
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
    justify-content: flex-end;
    align-items: center;
    border-top: 1px solid #27272a;
    padding-top: 14px;
    margin-top: 4px;
  }

  .toast-container {
    position: fixed;
    bottom: 24px;
    right: 24px;
    z-index: 9999;
    pointer-events: none;
  }

  .toast-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 18px;
    border-radius: 8px;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.6), 0 8px 10px -6px rgba(0, 0, 0, 0.4);
    min-width: 260px;
    max-width: 450px;
    backdrop-filter: blur(8px);
    pointer-events: auto;
  }

  .toast-card.success {
    background-color: #0f231c;
    border: 1px solid #10b981;
    color: #ecfdf5;
  }

  .toast-card.error {
    background-color: #271216;
    border: 1px solid #ef4444;
    color: #fef2f2;
  }

  .toast-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .toast-card.success .toast-icon {
    color: #10b981;
  }

  .toast-card.error .toast-icon {
    color: #ef4444;
  }

  .toast-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .toast-title {
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #a7f3d0;
  }

  .toast-card.error .toast-title {
    color: #fca5a5;
  }

  .toast-message {
    font-size: 0.88rem;
    font-weight: 500;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }
</style>
