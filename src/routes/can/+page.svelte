<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import Plotly from 'plotly.js-dist-min';
  import {
    baseLayout,
    plotlyOptions,
    CAN_WINDOW_MS,
    CAN_MAX_POINTS,
    formatPlotlyTimestamp,
    pruneWindowSamples,
    fmtVal,
    calcAvg,
    calcRms,
    type TimedSample
  } from '$lib/chartUtils';
  import { canConnected } from '$lib/stores';
  import type { CanReadData, ChartSeriesStats, FccuSettings, LightingState } from '$lib/types';

  let canChart1: HTMLElement;
  let canChart2: HTMLElement;
  let canChartsInitialized = false;

  function buildCanXAxisRange(nowMs: number): [string, string] {
    return [
      formatPlotlyTimestamp(nowMs - CAN_WINDOW_MS),
      formatPlotlyTimestamp(nowMs)
    ];
  }

  const initialRange = buildCanXAxisRange(Date.now());

  const canChartLayout = {
    ...baseLayout,
    margin: { t: 8, r: 8, l: 38, b: 24 },
    showlegend: false,
    xaxis: {
      type: 'date',
      gridcolor: '#27272a',
      zeroline: false,
      tickfont: { color: '#71717a' },
      tickformat: '%H:%M:%S',
      autorange: false,
      range: initialRange
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

  let canStats = emptyCanStats();
  let chart1Samples: TimedSample[] = [];
  let chart2Samples: TimedSample[] = [];

  let hasCanData = false;
  let fccuState = 3;
  let fccuMainValve = false;
  let fccuPurgeValve = false;
  let fccuFanState = false;
  let fccuFanDuty = 0;
  let fccuVersion = '—';

  const FCCU_STATES: Record<number, { label: string; color: string }> = {
    0: { label: 'Inicjalizacja', color: '#3b82f6' },
    1: { label: 'Uruchamianie', color: '#f59e0b' },
    2: { label: 'Włączony', color: '#22c55e' },
    3: { label: 'Wyłączony', color: '#f59e0b' }
  };

  function getFccuStateLabel(state: number): string {
    return FCCU_STATES[state]?.label ?? 'Brak odczytu';
  }

  function getFccuStateColor(state: number): string {
    return FCCU_STATES[state]?.color ?? '#71717a';
  }

  type FccuCmdKey = 'system_enable' | 'main_valve' | 'manual_purge' | 'fan_on';

  function idlePendingCmds(): Record<FccuCmdKey, boolean> {
    return { system_enable: false, main_valve: false, manual_purge: false, fan_on: false };
  }

  function emptyCanStats(): Record<'chart1' | 'chart2', ChartSeriesStats> {
    return {
      chart1: { newest: 0, average: 0, rms: 0 },
      chart2: { newest: 0, average: 0, rms: 0 }
    };
  }

  let pendingCmds = idlePendingCmds();

  let pendingTimeouts: Record<FccuCmdKey, ReturnType<typeof setTimeout> | null> = {
    system_enable: null,
    main_valve: null,
    manual_purge: null,
    fan_on: null
  };

  let expectedStates: Partial<Record<FccuCmdKey, boolean>> = {};

  let isSendingSettings = false;
  let isSendingLighting = false;
  let isSendingFanDuty = false;
  let fanDutyCmd = 100;

  function clampFanDuty(val: number): number {
    const n = Number(val);
    if (!Number.isFinite(n)) return 0;
    return Math.max(0, Math.min(100, Math.round(n)));
  }

  let isReconnectingPcan = false;

  async function handleReconnectPcan() {
    isReconnectingPcan = true;
    try {
      const res = await invoke<string>('reconnect_pcan');
      showToast(res, false);
      canConnected.set(true);
    } catch (err: any) {
      showToast(typeof err === 'string' ? err : (err?.message || 'Nie wykryto adaptera PCAN-USB'), true);
    } finally {
      isReconnectingPcan = false;
    }
  }

  let fccuSettings: FccuSettings = {
    fanMode: 0,
    purgeInterval: 60,
    pGain: 100,
    iGain: 50,
    dGain: 10
  };

  $: isPidEnabled = Number(fccuSettings.fanMode) === 2;

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

  async function toggleFccuCmd(cmdType: FccuCmdKey) {
    if (pendingCmds[cmdType]) return;

    const currentStates: Record<FccuCmdKey, boolean> = {
      system_enable: fccuState === 2,
      main_valve: fccuMainValve,
      manual_purge: fccuPurgeValve,
      fan_on: fccuFanState
    };
    const targetState = !currentStates[cmdType];

    pendingCmds = { ...pendingCmds, [cmdType]: true };
    expectedStates[cmdType] = targetState;

    if (pendingTimeouts[cmdType]) {
      clearTimeout(pendingTimeouts[cmdType]!);
    }
    pendingTimeouts[cmdType] = setTimeout(() => {
      if (pendingCmds[cmdType]) {
        pendingCmds = { ...pendingCmds, [cmdType]: false };
        delete expectedStates[cmdType];
        showToast('Brak potwierdzenia z magistrali CAN (timeout)', true);
      }
    }, 4000);

    try {
      const result =
        cmdType === 'fan_on'
          ? await invoke('send_fccu_fan', {
              state: targetState,
              duty: targetState ? (clampFanDuty(fanDutyCmd) || 100) : 0
            })
          : await invoke('send_fccu_command', {
              commandType: cmdType,
              state: targetState
            });
      if (cmdType === 'fan_on' && targetState && clampFanDuty(fanDutyCmd) === 0) {
        fanDutyCmd = 100;
      }
      showToast(result as string, false);
    } catch (e) {
      if (pendingTimeouts[cmdType]) {
        clearTimeout(pendingTimeouts[cmdType]!);
      }
      pendingCmds = { ...pendingCmds, [cmdType]: false };
      delete expectedStates[cmdType];
      showToast(`Błąd: ${e}`, true);
    }
  }

  async function sendFanDuty() {
    const duty = clampFanDuty(fanDutyCmd);
    fanDutyCmd = duty;
    isSendingFanDuty = true;
    await invokeWithToast(
      'send_fccu_fan',
      { state: duty > 0, duty },
      () => (isSendingFanDuty = false)
    );
  }

  async function invokeWithToast(command: string, args: Record<string, unknown>, onSettled: () => void) {
    try {
      const result = await invoke(command, args);
      showToast(result as string, false);
    } catch (e) {
      showToast(`Błąd: ${e}`, true);
    } finally {
      setTimeout(onSettled, 500);
    }
  }

  async function sendFccuSettings() {
    isSendingSettings = true;
    await invokeWithToast(
      'send_fccu_settings',
      {
        fanMode: fccuSettings.fanMode,
        purgeInterval: fccuSettings.purgeInterval,
        pGain: fccuSettings.pGain,
        iGain: fccuSettings.iGain,
        dGain: fccuSettings.dGain
      },
      () => (isSendingSettings = false)
    );
  }

  async function sendLightingCmd() {
    isSendingLighting = true;
    await invokeWithToast(
      'send_lighting',
      {
        headlight: lighting.headlight,
        positionLight: lighting.positionLight,
        brakeLight: lighting.brakeLight,
        leftIndicator: lighting.leftIndicator,
        rightIndicator: lighting.rightIndicator,
        hazard: lighting.hazard
      },
      () => (isSendingLighting = false)
    );
  }

  function clearAllPendingTimeouts() {
    (Object.keys(pendingTimeouts) as FccuCmdKey[]).forEach((k) => {
      if (pendingTimeouts[k]) clearTimeout(pendingTimeouts[k]!);
    });
  }

  function resolvePendingCmd(key: FccuCmdKey, currentState: boolean) {
    if (!pendingCmds[key]) return;
    if (expectedStates[key] === undefined || currentState === expectedStates[key]) {
      if (pendingTimeouts[key]) clearTimeout(pendingTimeouts[key]!);
      pendingCmds = { ...pendingCmds, [key]: false };
      delete expectedStates[key];
    }
  }

  function updateChartSeries(
    chartEl: HTMLElement,
    samples: TimedSample[],
    val: number,
    nowMs: number,
    xRange: [string, string]
  ): { samples: TimedSample[]; stats: ChartSeriesStats } {
    const next = pruneWindowSamples(
      [...samples, { t: nowMs, v: val }],
      nowMs,
      CAN_WINDOW_MS,
      CAN_MAX_POINTS
    );
    Plotly.update(
      chartEl,
      { x: [next.map((s) => formatPlotlyTimestamp(s.t))], y: [next.map((s) => s.v)] },
      { 'xaxis.range': xRange },
      [0]
    );
    const values = next.map((s) => s.v);
    return {
      samples: next,
      stats: { newest: val, average: calcAvg(values), rms: calcRms(values) }
    };
  }

  const statRows: { label: string; key: keyof ChartSeriesStats }[] = [
    { label: 'Bieżąca', key: 'newest' },
    { label: 'Średnia', key: 'average' },
    { label: 'Wartość skuteczna', key: 'rms' }
  ];

  const pidFields: { id: string; key: 'pGain' | 'iGain' | 'dGain'; label: string; helper: string }[] = [
    { id: 'p-gain-input', key: 'pGain', label: 'Wzmocnienie P', helper: 'Część proporcjonalna' },
    { id: 'i-gain-input', key: 'iGain', label: 'Wzmocnienie I', helper: 'Część całkująca' },
    { id: 'd-gain-input', key: 'dGain', label: 'Wzmocnienie D', helper: 'Część różniczkująca' }
  ];

  const lightingColumns: { key: keyof LightingState; label: string }[][] = [
    [
      { key: 'headlight', label: 'Światła główne' },
      { key: 'brakeLight', label: 'Światło STOP' },
      { key: 'rightIndicator', label: 'Prawy kierunkowskaz' }
    ],
    [
      { key: 'positionLight', label: 'Światła pozycyjne' },
      { key: 'leftIndicator', label: 'Lewy kierunkowskaz' },
      { key: 'hazard', label: 'Światła awaryjne' }
    ]
  ];

  $: cmdTiles = [
    {
      key: 'system_enable' as FccuCmdKey,
      name: 'System',
      stateText: fccuState === 2 ? 'włączony' : 'wyłączony',
      btnClass: 'btn-blue',
      active: fccuState === 2,
      pendingOn: 'Włączanie...',
      pendingOff: 'Wyłączanie...',
      labelOn: 'Wyłącz system',
      labelOff: 'Włącz system'
    },
    {
      key: 'main_valve' as FccuCmdKey,
      name: 'Zawór główny',
      stateText: fccuMainValve ? 'otwarty' : 'zamknięty',
      btnClass: 'btn-grey',
      active: fccuMainValve,
      pendingOn: 'Otwieranie...',
      pendingOff: 'Zamykanie...',
      labelOn: 'Zamknij zawór główny',
      labelOff: 'Otwórz zawór główny'
    },
    {
      key: 'manual_purge' as FccuCmdKey,
      name: 'Przedmuch (purge)',
      stateText: fccuPurgeValve ? 'aktywny' : 'nieaktywny',
      btnClass: 'btn-grey',
      active: fccuPurgeValve,
      pendingOn: 'Uruchamianie...',
      pendingOff: 'Zatrzymywanie...',
      labelOn: 'Zatrzymaj przedmuch',
      labelOff: 'Uruchom przedmuch'
    },
    {
      key: 'fan_on' as FccuCmdKey,
      name: 'Wentylator',
      stateText: fccuFanState ? `włączony (${fccuFanDuty}%)` : 'wyłączony',
      btnClass: 'btn-grey',
      active: fccuFanState,
      pendingOn: 'Włączanie...',
      pendingOff: 'Wyłączanie...',
      labelOn: 'Wyłącz wentylator',
      labelOff: 'Włącz wentylator'
    }
  ];

  function resetCanData() {
    hasCanData = false;
    fccuState = 0;
    fccuMainValve = false;
    fccuPurgeValve = false;
    fccuFanState = false;
    fccuFanDuty = 0;
    fccuVersion = '—';
    chart1Samples = [];
    chart2Samples = [];
    canStats = emptyCanStats();
    clearAllPendingTimeouts();
    pendingCmds = idlePendingCmds();
    expectedStates = {};
  }

  function applyCanStatus(connected: boolean) {
    canConnected.set(connected);
    if (!connected) {
      resetCanData();
    }
  }

  onMount(() => {
    invoke<boolean>('get_can_status')
      .then(applyCanStatus)
      .catch(() => {});

    const unlistenCanStatus = listen<boolean>('can-connection-status', (event) => applyCanStatus(event.payload));

    if (canChart1 && canChart2) {
      Plotly.newPlot(canChart1, canChartConfigs.chart1.data, canChartConfigs.chart1.layout, plotlyOptions);
      Plotly.newPlot(canChart2, canChartConfigs.chart2.data, canChartConfigs.chart2.layout, plotlyOptions);
      canChartsInitialized = true;
    }

    const unlistenCan = listen<CanReadData>('can-data-update', (event) => {
      if (!$canConnected) return;
      const d = event.payload;
      const nowMs = Date.now();

      hasCanData = true;

      if (d.fccu_state !== undefined) {
        fccuState = d.fccu_state;
        resolvePendingCmd('system_enable', fccuState === 2);
      }
      if (d.fccu_main_valve !== undefined) {
        fccuMainValve = d.fccu_main_valve;
        resolvePendingCmd('main_valve', fccuMainValve);
      }
      if (d.fccu_purge_valve !== undefined) {
        fccuPurgeValve = d.fccu_purge_valve;
        resolvePendingCmd('manual_purge', fccuPurgeValve);
      }
      if (d.fccu_fan_state !== undefined) {
        fccuFanState = d.fccu_fan_state;
        resolvePendingCmd('fan_on', fccuFanState);
      }

      if (d.fccu_fan_duty !== undefined) fccuFanDuty = d.fccu_fan_duty;
      if (d.fccu_version && d.fccu_version.trim().length > 0) fccuVersion = d.fccu_version;

      if (canChartsInitialized && canChart1 && canChart2) {
        const xRange = buildCanXAxisRange(nowMs);
        const r1 = updateChartSeries(canChart1, chart1Samples, d.fc_output_voltage ?? 0, nowMs, xRange);
        const r2 = updateChartSeries(canChart2, chart2Samples, d.fc_output_current ?? 0, nowMs, xRange);
        chart1Samples = r1.samples;
        chart2Samples = r2.samples;
        canStats = { chart1: r1.stats, chart2: r2.stats };
      }
    });

    return () => {
      unlistenCan.then((u) => u());
      unlistenCanStatus.then((u) => u());
      clearAllPendingTimeouts();
    };
  });
</script>

<div class="can-view-wrapper" class:disabled={!$canConnected}>
  {#if !$canConnected}
    <div class="can-disconnected-banner">
      <div class="can-disconnected-content">
        <span class="can-disconnected-icon">🔌</span>
        <div class="can-disconnected-text">
          <div class="can-disconnected-title">Brak połączenia z adapterem PCAN-USB (magistrala CAN)</div>
          <div class="can-disconnected-desc">
            Zakładka CAN jest zablokowana. Trwa automatyczne nasłuchiwanie USB w tle — po podłączeniu adaptera połączenie zostanie wznowione samoczynnie.
          </div>
        </div>
        <div class="can-disconnected-actions">
          <div class="auto-listening-pill">
            <span class="pulsing-circle"></span>
            <span>Nasłuchiwanie USB w toku...</span>
          </div>
          <button
            type="button"
            class="btn-reconnect-pcan"
            disabled={isReconnectingPcan}
            on:click={handleReconnectPcan}
          >
            {isReconnectingPcan ? 'Wykrywanie...' : 'Wymuś wykrycie'}
          </button>
        </div>
      </div>
    </div>
  {/if}

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
      <span class="status-val bold">{fccuFanState ? `Włączony (${fccuFanDuty}%)` : 'Wyłączony'}</span>
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
        {#each statRows as row}
          <div class="stat-box">
            <span class="stat-box-lbl">{row.label}</span>
            <span class="stat-box-val">{fmtVal(canStats.chart1[row.key])} V</span>
          </div>
        {/each}
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
        {#each statRows as row}
          <div class="stat-box">
            <span class="stat-box-lbl">{row.label}</span>
            <span class="stat-box-val">{fmtVal(canStats.chart2[row.key])} A</span>
          </div>
        {/each}
      </div>
    </div>
  </div>

  <div class="section-card">
    <div class="section-card-header">
      <h3 class="section-card-title">KOMENDY STEROWNIKA FCCU</h3>
    </div>
    <div class="cmd-grid-2x2">
      {#each cmdTiles as tile}
        <div class="cmd-tile" class:cmd-tile-fan={tile.key === 'fan_on'}>
          <div class="cmd-tile-main">
            <div class="cmd-tile-info">
              <span class="cmd-tile-name">{tile.name}</span>
              <span class="cmd-tile-state">Stan: {tile.stateText}</span>
            </div>
            <button
              class="action-btn {tile.btnClass}"
              class:is-pending={pendingCmds[tile.key]}
              disabled={pendingCmds[tile.key]}
              on:click={() => toggleFccuCmd(tile.key)}
            >
              {#if pendingCmds[tile.key]}
                <span class="btn-spinner"></span>
                <span>{expectedStates[tile.key] ? tile.pendingOn : tile.pendingOff}</span>
              {:else}
                {tile.active ? tile.labelOn : tile.labelOff}
              {/if}
            </button>
          </div>

          {#if tile.key === 'fan_on'}
            <div class="fan-duty-row">
              <label for="fan-duty-number" class="fan-duty-lbl">Wysterowanie (%)</label>
              <input
                type="range"
                class="fan-duty-slider"
                min="0"
                max="100"
                step="1"
                bind:value={fanDutyCmd}
                aria-label="Wypełnienie wentylatora w procentach"
              />
              <div class="input-unit-wrapper fan-duty-number-wrap">
                <input
                  id="fan-duty-number"
                  type="number"
                  class="setting-form-input fan-duty-number"
                  min="0"
                  max="100"
                  bind:value={fanDutyCmd}
                  on:change={() => (fanDutyCmd = clampFanDuty(fanDutyCmd))}
                  on:keydown={(e) => e.key === 'Enter' && sendFanDuty()}
                />
                <span class="input-unit-badge">%</span>
              </div>
              <button
                type="button"
                class="action-btn btn-blue fan-duty-btn"
                disabled={isSendingFanDuty}
                on:click={sendFanDuty}
              >
                {isSendingFanDuty ? 'Wysyłanie...' : 'Ustaw'}
              </button>
            </div>
          {/if}
        </div>
      {/each}
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

        {#each pidFields as field}
          <div class="setting-form-row" class:disabled={!isPidEnabled}>
            <label for={field.id} class="setting-form-label">{field.label}</label>
            <input
              id={field.id}
              type="number"
              class="setting-form-input"
              bind:value={fccuSettings[field.key]}
              min="0"
              max="1000"
              disabled={!isPidEnabled}
            />
            <span class="setting-form-helper">{field.helper}</span>
          </div>
        {/each}
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
            <span class="fan-readout-key">Wypełnienie</span>
            <span class="fan-readout-val">{fccuFanDuty} %</span>
          </div>
          <div class="fan-readout-item">
            <span class="fan-readout-key">Prędkość</span>
            <span class="fan-readout-val">{fccuFanDuty ? fccuFanDuty * 30 : 0} obr./min</span>
          </div>
        </div>
        <button class="blue-submit-btn" disabled={isSendingSettings} on:click={sendFccuSettings}>
          {isSendingSettings ? 'Wysyłanie...' : 'Wyślij ustawienia'}
        </button>
      </div>
    </div>
  </div>

  <div class="section-card">
    <div class="section-card-header">
      <h3 class="section-card-title">STEROWANIE OŚWIETLENIEM</h3>
    </div>

    <div class="lighting-grid-layout">
      {#each lightingColumns as col}
        <div class="lighting-column">
          {#each col as item}
            <div class="light-switch-row">
              <label class="switch-container">
                <input type="checkbox" bind:checked={lighting[item.key]} />
                <span class="switch-track"></span>
                <span class="switch-label-text">{item.label}</span>
              </label>
              <span class="switch-status-text">{lighting[item.key] ? 'Włączone' : 'Wyłączone'}</span>
            </div>
          {/each}
        </div>
      {/each}
    </div>

    <div class="lighting-action-row">
      <button class="blue-submit-btn" disabled={isSendingLighting} on:click={sendLightingCmd}>
        {isSendingLighting ? 'Wysyłanie...' : 'Wyślij oświetlenie'}
      </button>
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
    position: relative;
    transition: opacity 0.3s ease, filter 0.3s ease;
  }

  .can-view-wrapper.disabled > *:not(.can-disconnected-banner):not(.toast-container) {
    opacity: 0.32;
    filter: grayscale(0.85);
    pointer-events: none;
    user-select: none;
  }

  .can-disconnected-banner {
    pointer-events: auto;
    background: rgba(24, 24, 27, 0.95);
    border: 1px solid rgba(239, 68, 68, 0.4);
    border-radius: 10px;
    padding: 14px 20px;
    backdrop-filter: blur(6px);
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.5);
  }

  .can-disconnected-content {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  .can-disconnected-icon {
    font-size: 1.5rem;
  }

  .can-disconnected-text {
    flex: 1;
    min-width: 200px;
  }

  .can-disconnected-title {
    font-size: 0.95rem;
    font-weight: 600;
    color: #f87171;
  }

  .can-disconnected-desc {
    font-size: 0.8rem;
    color: #a1a1aa;
    margin-top: 3px;
  }

  .btn-reconnect-pcan {
    background: linear-gradient(135deg, #2563eb, #1d4ed8);
    border: 1px solid #3b82f6;
    color: #ffffff;
    font-weight: 600;
    font-size: 0.82rem;
    padding: 8px 16px;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
  }

  .btn-reconnect-pcan:hover:not(:disabled) {
    background: linear-gradient(135deg, #3b82f6, #2563eb);
    box-shadow: 0 0 10px rgba(59, 130, 246, 0.4);
  }

  .btn-reconnect-pcan:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .can-disconnected-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .auto-listening-pill {
    display: flex;
    align-items: center;
    gap: 8px;
    background: rgba(39, 39, 42, 0.85);
    border: 1px solid #3f3f46;
    padding: 6px 12px;
    border-radius: 9999px;
    font-size: 0.78rem;
    color: #cbd5e1;
    white-space: nowrap;
  }

  .pulsing-circle {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background-color: #38bdf8;
    box-shadow: 0 0 0 rgba(56, 189, 248, 0.5);
    animation: pulse-glow 1.5s infinite;
  }

  @keyframes pulse-glow {
    0% {
      transform: scale(0.95);
      box-shadow: 0 0 0 0 rgba(56, 189, 248, 0.7);
    }
    70% {
      transform: scale(1);
      box-shadow: 0 0 0 6px rgba(56, 189, 248, 0);
    }
    100% {
      transform: scale(0.95);
      box-shadow: 0 0 0 0 rgba(56, 189, 248, 0);
    }
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
    flex-direction: column;
    justify-content: center;
    gap: 10px;
  }

  .cmd-tile-main {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }

  .fan-duty-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-top: 10px;
    border-top: 1px solid #27272a;
  }

  .fan-duty-lbl {
    font-size: 0.75rem;
    color: #a1a1aa;
    white-space: nowrap;
  }

  .fan-duty-slider {
    flex: 1;
    min-width: 60px;
    accent-color: #3b82f6;
    cursor: pointer;
  }

  .fan-duty-number-wrap {
    width: 78px;
    flex-shrink: 0;
  }

  .fan-duty-number {
    padding: 6px 24px 6px 8px;
    font-size: 0.82rem;
  }

  .fan-duty-btn {
    border: none;
    padding: 7px 14px;
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
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  .action-btn:disabled,
  .action-btn.is-pending {
    opacity: 0.45;
    cursor: not-allowed;
    filter: grayscale(0.6);
    pointer-events: none;
  }

  .btn-spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: #ffffff;
    border-radius: 50%;
    animation: spin-btn 0.7s linear infinite;
    display: inline-block;
    flex-shrink: 0;
  }

  @keyframes spin-btn {
    to {
      transform: rotate(360deg);
    }
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
    transition: opacity 0.25s ease;
  }

  .setting-form-row.disabled {
    opacity: 0.42;
    cursor: not-allowed;
  }

  .setting-form-row.disabled .setting-form-label {
    color: #71717a;
    cursor: not-allowed;
  }

  .setting-form-row.disabled .setting-form-helper {
    color: #52525b;
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
    transition: background-color 0.25s ease, border-color 0.25s ease, color 0.25s ease;
  }
  .setting-form-select:focus,
  .setting-form-input:focus {
    border-color: #3b82f6;
  }

  .setting-form-input:disabled {
    background-color: #18181b;
    border-color: #27272a;
    color: #52525b;
    cursor: not-allowed;
    box-shadow: none;
    user-select: none;
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
  .blue-submit-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
    filter: grayscale(0.5);
    pointer-events: none;
  }

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
