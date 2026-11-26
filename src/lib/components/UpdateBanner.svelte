<script lang="ts">
  import { installAppUpdate, type Update, type UpdateProgress } from '$lib/updater';

  let { update, onDismiss }: { update: Update; onDismiss: () => void } = $props();

  let installing = $state(false);
  let errorMsg = $state<string | null>(null);
  let progress = $state<UpdateProgress | null>(null);

  function formatMb(bytes: number): string {
    return (bytes / (1024 * 1024)).toFixed(1).replace('.', ',');
  }

  async function handleUpdateNow() {
    installing = true;
    errorMsg = null;
    progress = { downloaded: 0, total: null, percent: 0, stage: 'started' };
    try {
      await installAppUpdate(update, (p) => {
        progress = p;
      });
    } catch (err: unknown) {
      errorMsg = err instanceof Error ? err.message : String(err || 'Nie udało się zainstalować aktualizacji');
      installing = false;
    }
  }
</script>

<div class="update-banner" role="status" aria-live="polite">
  <div class="update-header">
    <div class="update-title-row">
      <span class="update-dot"></span>
      <span class="update-title">Dostępna nowa wersja v{update.version}</span>
    </div>
    {#if !installing}
      <div class="update-actions">
        <button type="button" class="btn-update-later" onclick={onDismiss}>Później</button>
        <button type="button" class="btn-update-now" onclick={handleUpdateNow}>Zaktualizuj teraz</button>
      </div>
    {/if}
  </div>

  {#if update.body && update.body.trim().length > 0}
    <div class="update-body">{update.body}</div>
  {/if}

  {#if installing && progress}
    <div class="update-progress-wrap">
      <div class="update-progress-meta">
        <span>
          {#if progress.stage === 'finished'}
            Instalowanie i ponowne uruchamianie...
          {:else}
            Pobieranie aktualizacji...
          {/if}
        </span>
        <span>
          {#if progress.total && progress.total > 0}
            {formatMb(progress.downloaded)} / {formatMb(progress.total)} MB ({progress.percent ?? 0}%)
          {:else if progress.downloaded > 0}
            {formatMb(progress.downloaded)} MB
          {/if}
        </span>
      </div>
      <div class="update-progress-track">
        <div
          class="update-progress-fill"
          class:indeterminate={progress.percent === null}
          style={progress.percent !== null ? `width: ${progress.percent}%` : ''}
        ></div>
      </div>
    </div>
  {/if}

  {#if errorMsg}
    <div class="update-error">Błąd aktualizacji: {errorMsg}</div>
  {/if}
</div>

<style>
  .update-banner {
    background-color: #18181b;
    border: 1px solid rgba(59, 130, 246, 0.45);
    border-radius: 10px;
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
  }

  .update-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }

  .update-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .update-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background-color: #3b82f6;
    flex-shrink: 0;
  }

  .update-title {
    font-size: 0.9rem;
    font-weight: 600;
    color: #f4f4f5;
  }

  .update-body {
    font-size: 0.8rem;
    color: #a1a1aa;
    white-space: pre-line;
    max-height: 110px;
    overflow-y: auto;
    line-height: 1.4;
  }

  .update-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .btn-update-later {
    background-color: #27272a;
    border: 1px solid #3f3f46;
    color: #d4d4d8;
    font-size: 0.8rem;
    font-weight: 500;
    padding: 6px 12px;
    border-radius: 6px;
    cursor: pointer;
    transition: background-color 0.15s ease, color 0.15s ease;
  }

  .btn-update-later:hover {
    background-color: #3f3f46;
    color: #ffffff;
  }

  .btn-update-now {
    background: linear-gradient(135deg, #2563eb, #1d4ed8);
    border: 1px solid #3b82f6;
    color: #ffffff;
    font-size: 0.8rem;
    font-weight: 600;
    padding: 6px 14px;
    border-radius: 6px;
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .btn-update-now:hover {
    background: linear-gradient(135deg, #3b82f6, #2563eb);
  }

  .update-progress-wrap {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .update-progress-meta {
    display: flex;
    justify-content: space-between;
    font-size: 0.76rem;
    color: #a1a1aa;
  }

  .update-progress-track {
    width: 100%;
    height: 6px;
    background-color: #27272a;
    border-radius: 9999px;
    overflow: hidden;
  }

  .update-progress-fill {
    height: 100%;
    background-color: #3b82f6;
    border-radius: 9999px;
    transition: width 0.15s ease;
  }

  .update-progress-fill.indeterminate {
    width: 40%;
    animation: update-slide 1.2s ease-in-out infinite;
  }

  @keyframes update-slide {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(250%);
    }
  }

  .update-error {
    font-size: 0.78rem;
    color: #fca5a5;
  }
</style>
