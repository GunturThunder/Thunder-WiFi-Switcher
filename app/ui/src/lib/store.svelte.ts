// Shared state for the settings window: settings, live status, and a short
// message line. Every change is saved straight away (no Save button).

import { api } from './api'
import type { AppInfo, Config, Status } from './types'

export type Message = { kind: 'info' | 'error'; text: string }

class AppStore {
  info = $state<AppInfo | null>(null)
  status = $state<Status | null>(null)
  config = $state<Config | null>(null)
  notice = $state<string | null>(null)
  message = $state<Message | null>(null)
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
