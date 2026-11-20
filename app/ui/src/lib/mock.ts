// A pretend backend for working on the settings window in a plain browser
// (`npm run dev`). Nothing here talks to real Wi-Fi.

import type { Api } from './api'
import type { Config, Status } from './types'

export function createMockApi(): Api {
  // `?platform=windows` previews the Windows version of the window.
  const platform = new URLSearchParams(window.location.search).get('platform') ?? 'macos'
  const windows = platform === 'windows'
  let config: Config = {
    version: 1,
    auto_switch: true,
    weak_signal_dbm: -75,
    bad_ping_ms: 300,
    bad_loss_pct: 10,
    notify_on_switch: false,
    show_signal_label: true,
    language: 'en',
    ping_targets: ['1.1.1.1', '8.8.8.8'],
    networks: [
      { ssid: 'Dorm-5G', enabled: true },
      { ssid: 'Dorm-2.4', enabled: true },
    ],
  }
  let status: Status = {
    connection: 'connected',
    ssid: 'Dorm-5G',
    rssi: -58,
    ping_ms: 24,
    loss_pct: 0,
    internet: 'good',
    managed: true,
    activity: 'watching',
    permission: windows ? 'denied' : 'not_determined',
    last_event: { time: '15:42', text: 'Switched from Dorm-2.4 to Dorm-5G (signal was -81 dBm)' },
    error: null,
    simulated: false,
  }
  const passwords = new Set(['Dorm-5G'])
  let autostart = false
  const statusListeners = new Set<(status: Status) => void>()
  const configListeners = new Set<(config: Config) => void>()

  const clone = <T>(value: T): T => structuredClone(value)
  const emitStatus = () => statusListeners.forEach((listener) => listener(clone(status)))
  const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

  return {
    appInfo: async () => ({
      version: 'dev (browser preview)',
      platform,
      simulated: false,
      passwords_editable: !windows,
      config_path: windows
        ? '%APPDATA%\\ThunderWiFi\\config.toml'
        : '~/Library/Application Support/ThunderWiFi/config.toml',
    }),
    getStatus: async () => clone(status),
    getConfig: async () => clone(config),
    saveConfig: async (next) => {
      const notes: string[] = []
      const clamped = Math.min(-65, Math.max(-80, next.weak_signal_dbm))
      if (clamped !== next.weak_signal_dbm) notes.push(`Weak signal set to ${clamped} dBm.`)
      config = { ...clone(next), weak_signal_dbm: clamped }
      status = { ...status, activity: config.auto_switch ? 'watching' : 'paused' }
      configListeners.forEach((listener) => listener(clone(config)))
      emitStatus()
      return { config: clone(config), notes }
    },
    switchTo: async (ssid) => {
      status = { ...status, activity: 'switching' }
      emitStatus()
      await delay(800)
      status = {
        ...status,
        ssid,
        rssi: -63,
        activity: 'settling',
        last_event: { time: 'now', text: `Switched to ${ssid}` },
      }
      emitStatus()
    },
    nearbyNetworks: async () => {
      await delay(600)
      return [
        { ssid: 'Dorm-5G', rssi: -58 },
        { ssid: 'Dorm-2.4', rssi: -66 },
        { ssid: 'Library', rssi: -79 },
        { ssid: 'Kos Melati', rssi: -84 },
      ]
    },
    knownNetworks: async () => ['Dorm-5G', 'Dorm-2.4', 'Library', 'Home'],
    passwordStatus: async (ssids) =>
      Object.fromEntries(ssids.map((ssid) => [ssid, passwords.has(ssid)])),
    setPassword: async (ssid, password) => {
      if (password) passwords.add(ssid)
      else passwords.delete(ssid)
    },
    deletePassword: async (ssid) => {
      passwords.delete(ssid)
    },
    requestPermission: async () => {
      await delay(1000)
      status = { ...status, permission: 'granted' }
      emitStatus()
    },
    openLink: async (link) => {
      console.info('would open', link)
    },
    getAutostart: async () => autostart,
    setAutostart: async (enabled) => {
      autostart = enabled
    },
    startupNotice: async () => null,
    onStatus: async (callback) => {
      statusListeners.add(callback)
      return () => statusListeners.delete(callback)
    },
    onConfig: async (callback) => {
      configListeners.add(callback)
      return () => configListeners.delete(callback)
    },
  }
}
