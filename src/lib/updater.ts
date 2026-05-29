import { getVersion } from '@tauri-apps/api/app';
import type { Update } from '@tauri-apps/plugin-updater';

export type UpdaterPhase =
    | 'idle'
    | 'checking'
    | 'available'
    | 'downloading'
    | 'installing'
    | 'restarting'
    | 'upToDate'
    | 'error';

export interface UpdateInfo {
    currentVersion: string;
    availableVersion: string;
    notes?: string;
    pubDate?: string;
}

export interface UpdateProgressEvent {
    event: 'Started' | 'Progress' | 'Finished';
    total?: number;
    downloaded?: number;
}

export interface UpdateHandle {
    version: string;
    notes?: string;
    date?: string;
    downloadAndInstall: (
        onProgress?: (event: UpdateProgressEvent) => void,
    ) => Promise<void>;
}

export interface CheckUpdateOptions {
    timeout?: number;
}

export type UpdateCheckResult =
    | { status: 'up-to-date' }
    | { status: 'available'; info: UpdateInfo; update: UpdateHandle };

function mapUpdateHandle(update: Update): UpdateHandle {
    const raw = update as unknown as {
        version?: string;
        notes?: string;
        body?: string;
        date?: string;
        downloadAndInstall: (onProgress?: (event: unknown) => void) => Promise<void>;
    };

    return {
        version: raw.version ?? '',
        notes: raw.notes ?? raw.body,
        date: raw.date,
        async downloadAndInstall(onProgress) {
            await raw.downloadAndInstall((event) => {
                if (!onProgress || !event || typeof event !== 'object') return;
                const payload = event as {
                    event?: UpdateProgressEvent['event'];
                    data?: { contentLength?: number; chunkLength?: number };
                };
                if (payload.event === 'Started') {
                    onProgress({
                        event: 'Started',
                        total: payload.data?.contentLength ?? 0,
                        downloaded: 0,
                    });
                } else if (payload.event === 'Progress') {
                    onProgress({
                        event: 'Progress',
                        downloaded: payload.data?.chunkLength ?? 0,
                    });
                } else if (payload.event === 'Finished') {
                    onProgress({ event: 'Finished' });
                }
            });
        },
    };
}

export async function getCurrentVersion(): Promise<string> {
    try {
        return await getVersion();
    } catch {
        return '';
    }
}

export async function checkForUpdate(
    options: CheckUpdateOptions = {},
): Promise<UpdateCheckResult> {
    const { check } = await import('@tauri-apps/plugin-updater');
    const currentVersion = await getCurrentVersion();
    const update = await check({ timeout: options.timeout ?? 30000 });

    if (!update) {
        return { status: 'up-to-date' };
    }

    const mapped = mapUpdateHandle(update);
    return {
        status: 'available',
        info: {
            currentVersion,
            availableVersion: mapped.version,
            notes: mapped.notes,
            pubDate: mapped.date,
        },
        update: mapped,
    };
}

export async function relaunchApp(): Promise<void> {
    const { relaunch } = await import('@tauri-apps/plugin-process');
    await relaunch();
}
