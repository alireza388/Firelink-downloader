import { info as tauriInfo, warn as tauriWarn, error as tauriError, debug as tauriDebug, trace as tauriTrace, attachLogger as tauriAttachLogger } from '@tauri-apps/plugin-log';
import { invoke } from '@tauri-apps/api/core';

// Default to true to match backend default
let isPaused = true;

let initPromise: Promise<void> | null = null;
let logPauseTransition = Promise.resolve();
let logStreamTransition = Promise.resolve();

export const initLogger = () => {
    if (!initPromise) {
        initPromise = invoke<boolean>('is_log_paused')
            .then(paused => { isPaused = paused; })
            .catch(e => console.error("Failed to init logger state", e));
    }
    return initPromise;
};

export const setLogPaused = (pause: boolean): Promise<void> => {
    const transition = logPauseTransition.then(async () => {
        await invoke('toggle_log_pause', { pause });
        isPaused = pause;
    });
    logPauseTransition = transition.catch(() => undefined);
    return transition;
};

export const setLogStreamActive = (active: boolean): Promise<void> => {
    const transition = logStreamTransition.then(async () => {
        await invoke('set_log_stream_active', { active });
    });
    logStreamTransition = transition.catch(() => undefined);
    return transition;
};

export const getLogPaused = () => isPaused;

export const info = async (message: string) => {
    if (!isPaused) return tauriInfo(message);
};
export const warn = async (message: string) => {
    if (!isPaused) return tauriWarn(message);
};
export const error = async (message: string) => {
    if (!isPaused) return tauriError(message);
};
export const debug = async (message: string) => {
    if (!isPaused) return tauriDebug(message);
};
export const trace = async (message: string) => {
    if (!isPaused) return tauriTrace(message);
};
export const attachLogger = tauriAttachLogger;
