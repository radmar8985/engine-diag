import { check, type DownloadEvent, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export type { Update };

export interface UpdateProgress {
  downloaded: number;
  total: number | null;
  percent: number | null;
  stage: 'started' | 'progress' | 'finished';
}

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function checkForAppUpdate(): Promise<Update | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  try {
    const update = await check();
    return update ?? null;
  } catch {
    return null;
  }
}

export async function installAppUpdate(
  update: Update,
  onProgress?: (progress: UpdateProgress) => void
): Promise<void> {
  let downloaded = 0;
  let total: number | null = null;

  await update.downloadAndInstall((event: DownloadEvent) => {
    if (event.event === 'Started') {
      total = event.data.contentLength ?? null;
      downloaded = 0;
      onProgress?.({
        downloaded,
        total,
        percent: total && total > 0 ? 0 : null,
        stage: 'started',
      });
    } else if (event.event === 'Progress') {
      downloaded += event.data.chunkLength;
      const percent = total && total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null;
      onProgress?.({
        downloaded,
        total,
        percent,
        stage: 'progress',
      });
    } else if (event.event === 'Finished') {
      onProgress?.({
        downloaded,
        total,
        percent: 100,
        stage: 'finished',
      });
    }
  });

  await relaunch();
}
