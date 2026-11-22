// Shared state for the settings window: settings, live status, permissions,
// and a short message line. Every change is saved straight away (no Save
// button).

import { api } from './api'
import type { AppInfo, Config, NotificationPermission, Status } from './types'

export type Message = { kind: 'info' | 'error'; text: string }
export type TabId = 'networks' | 'rules' | 'general' | 'about'

class AppStore {
  info = $state<AppInfo | null>(null)
  status = $state<Status | null>(null)
  config = $state<Config | null>(null)
  notice = $state<string | null>(null)
  message = $state<Message | null>(null)
  tab = $state<TabId>('networks')
  /** The setup guide stays open until the user closes it (see `setupNeeded`). */
  setupOpen = $state(false)
  /** What the system says about the app's notifications ('unknown' where it can't tell). */
  notifications = $state<NotificationPermission>('unknown')
  /** Waiting for the user to answer the system's notification question. */
  askingNotifications = $state(false)
  /** Turning notifications on sends a test one; the window says so just then. */
  testNotificationSent = $state(false)
  /** Open at login; null until known. */
  autostart = $state<boolean | null>(null)
  #messageTimer: ReturnType<typeof setTimeout> | undefined

  async load() {
    try {
      const [info, status, config, notice] = await Promise.all([
        api.appInfo(),
        api.getStatus(),
        api.getConfig(),
        api.startupNotice(),
      ])
      this.info = info
      this.status = status
      this.config = config
      this.notice = notice
      await api.onStatus((status) => (this.status = status))
      await api.onConfig((config) => (this.config = config))
    } catch (err) {
      this.flash('error', `Couldn't load settings: ${err}`)
    }
    this.checkNotifications()
    api
      .getAutostart()
      .then((enabled) => (this.autostart = enabled))
      .catch(() => (this.autostart = false))
    // Check again when the user comes back, e.g. from System Settings.
    window.addEventListener('focus', () => this.checkNotifications())
  }

  /** Location and at least one network: what the app can't work without. */
  get setupNeeded(): boolean {
    if (!this.status || !this.config) return false
    const location = this.status.permission === 'granted' && this.status.connection !== 'permission_needed'
    return !location || this.config.networks.length === 0
  }

  openSetup() {
    this.setupOpen = true
    this.tab = 'networks'
  }

  async checkNotifications() {
    this.notifications = await api.notificationPermission().catch(() => 'unknown' as const)
  }

  /** Turns switch notifications on or off. Turning them on asks the system
   * first (macOS asks each app only once), then sends a test one. */
  async setNotify(on: boolean) {
    if (on && this.notifications === 'not_asked') {
      this.askingNotifications = true
      this.notifications = await api.askNotificationPermission().catch(() => 'unknown' as const)
      this.askingNotifications = false
    }
    this.testNotificationSent = on
    await this.save((config) => (config.notify_on_switch = on))
  }

  async openNotificationSettings() {
    try {
      await api.openLink('notification_settings')
    } catch (err) {
      this.flash('error', `Couldn't open notification settings: ${err}`)
    }
  }

  async setAutostart(enabled: boolean) {
    try {
      await api.setAutostart(enabled)
      this.autostart = enabled
    } catch (err) {
      this.flash('error', `Couldn't change "Open at login": ${err}`)
    }
  }

  /** Changes a copy of the settings and saves it. */
  async save(change: (config: Config) => void) {
    if (!this.config) return
    const next = $state.snapshot(this.config) as Config
    change(next)
    try {
      const saved = await api.saveConfig(next)
      this.config = saved.config
      if (saved.notes.length > 0) this.flash('info', saved.notes.join(' '))
    } catch (err) {
      this.flash('error', String(err))
    }
  }

  flash(kind: Message['kind'], text: string) {
    this.message = { kind, text }
    clearTimeout(this.#messageTimer)
    this.#messageTimer = setTimeout(() => (this.message = null), kind === 'error' ? 8000 : 4000)
  }
}

export const app = new AppStore()

/** Arcs (0–3) for a signal level, matching the menu bar icon. */
export function bars(rssi: number): number {
  if (rssi >= -60) return 3
  if (rssi >= -70) return 2
  if (rssi >= -80) return 1
  return 0
}
