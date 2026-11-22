// Talks to the Rust side (app/src-tauri/src/commands.rs). In a plain browser
// (`npm run dev`) there is no Rust side, so a pretend backend stands in.

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import { createMockApi } from './mock'
import type {
  AppInfo,
  Config,
  JoinError,
  Link,
  Nearby,
  NotificationPermission,
  Saved,
  Status,
  UpdateStatus,
} from './types'

export interface Api {
  appInfo(): Promise<AppInfo>
  getStatus(): Promise<Status>
  getConfig(): Promise<Config>
  saveConfig(config: Config): Promise<Saved>
  /** Joins a network. Fails with a {@link JoinError}. */
  switchTo(ssid: string): Promise<void>
  nearbyNetworks(): Promise<Nearby[]>
  knownNetworks(): Promise<string[]>
  passwordStatus(ssids: string[]): Promise<Record<string, boolean>>
  setPassword(ssid: string, password: string): Promise<void>
  deletePassword(ssid: string): Promise<void>
  requestPermission(): Promise<void>
  notificationPermission(): Promise<NotificationPermission>
  /** Asks the user; the system asks each app only once. */
  askNotificationPermission(): Promise<NotificationPermission>
  openLink(link: Link): Promise<void>
  getAutostart(): Promise<boolean>
  setAutostart(enabled: boolean): Promise<void>
  startupNotice(): Promise<string | null>
  updateStatus(): Promise<UpdateStatus>
  /** Looks for a new version now and downloads it. */
  checkForUpdates(): Promise<UpdateStatus>
  /** Installs the downloaded update and restarts the app. */
  installUpdate(): Promise<void>
  onStatus(callback: (status: Status) => void): Promise<UnlistenFn>
  onUpdate(callback: (update: UpdateStatus) => void): Promise<UnlistenFn>
  onConfig(callback: (config: Config) => void): Promise<UnlistenFn>
}

const tauriApi: Api = {
  appInfo: () => invoke('app_info'),
  getStatus: () => invoke('get_status'),
  getConfig: () => invoke('get_config'),
  saveConfig: (config) => invoke('save_config', { config }),
  switchTo: (ssid) =>
    invoke<void>('switch_to', { ssid }).catch((err: unknown) => {
      throw asJoinError(ssid, err)
    }),
  nearbyNetworks: () => invoke('nearby_networks'),
  knownNetworks: () => invoke('known_networks'),
  passwordStatus: (ssids) => invoke('password_status', { ssids }),
  setPassword: (ssid, password) => invoke('set_password', { ssid, password }),
  deletePassword: (ssid) => invoke('delete_password', { ssid }),
  requestPermission: () => invoke('request_permission'),
  notificationPermission: () => invoke('notification_permission'),
  askNotificationPermission: () => invoke('ask_notification_permission'),
  openLink: (link) => invoke('open_link', { link }),
  getAutostart: () => invoke('get_autostart'),
  setAutostart: (enabled) => invoke('set_autostart', { enabled }),
  startupNotice: () => invoke('startup_notice'),
  updateStatus: () => invoke('update_status'),
  checkForUpdates: () => invoke('check_for_updates'),
  installUpdate: () => invoke('install_update'),
  onStatus: (callback) => listen<Status>('status', (event) => callback(event.payload)),
  onUpdate: (callback) => listen<UpdateStatus>('update', (event) => callback(event.payload)),
  onConfig: (callback) => listen<Config>('config', (event) => callback(event.payload)),
}

/** The app sends a JoinError; anything else (a broken call) becomes one. */
function asJoinError(ssid: string, err: unknown): JoinError {
  if (typeof err === 'object' && err !== null && 'kind' in err && 'message' in err) {
    return err as JoinError
  }
  return { kind: 'other', message: `Couldn't join ${ssid}: ${err}` }
}

/** True inside the app; false in a plain browser preview. */
export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export const api: Api = inTauri ? tauriApi : createMockApi()
