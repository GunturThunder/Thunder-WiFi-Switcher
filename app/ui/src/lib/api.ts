// Talks to the Rust side (app/src-tauri/src/commands.rs). In a plain browser
// (`npm run dev`) there is no Rust side, so a pretend backend stands in.

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import { createMockApi } from './mock'
import type { AppInfo, Config, Link, Nearby, Saved, Status } from './types'

export interface Api {
  appInfo(): Promise<AppInfo>
  getStatus(): Promise<Status>
  getConfig(): Promise<Config>
  saveConfig(config: Config): Promise<Saved>
  switchTo(ssid: string): Promise<void>
  nearbyNetworks(): Promise<Nearby[]>
  knownNetworks(): Promise<string[]>
  passwordStatus(ssids: string[]): Promise<Record<string, boolean>>
  setPassword(ssid: string, password: string): Promise<void>
  deletePassword(ssid: string): Promise<void>
  requestPermission(): Promise<void>
  openLink(link: Link): Promise<void>
  getAutostart(): Promise<boolean>
  setAutostart(enabled: boolean): Promise<void>
  startupNotice(): Promise<string | null>
  onStatus(callback: (status: Status) => void): Promise<UnlistenFn>
  onConfig(callback: (config: Config) => void): Promise<UnlistenFn>
}

const tauriApi: Api = {
  appInfo: () => invoke('app_info'),
  getStatus: () => invoke('get_status'),
  getConfig: () => invoke('get_config'),
  saveConfig: (config) => invoke('save_config', { config }),
  switchTo: (ssid) => invoke('switch_to', { ssid }),
  nearbyNetworks: () => invoke('nearby_networks'),
  knownNetworks: () => invoke('known_networks'),
  passwordStatus: (ssids) => invoke('password_status', { ssids }),
  setPassword: (ssid, password) => invoke('set_password', { ssid, password }),
  deletePassword: (ssid) => invoke('delete_password', { ssid }),
  requestPermission: () => invoke('request_permission'),
  openLink: (link) => invoke('open_link', { link }),
  getAutostart: () => invoke('get_autostart'),
  setAutostart: (enabled) => invoke('set_autostart', { enabled }),
  startupNotice: () => invoke('startup_notice'),
  onStatus: (callback) => listen<Status>('status', (event) => callback(event.payload)),
  onConfig: (callback) => listen<Config>('config', (event) => callback(event.payload)),
}

/** True inside the app; false in a plain browser preview. */
export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export const api: Api = inTauri ? tauriApi : createMockApi()
