// Mirrors the Rust types the commands send (app/src-tauri/src/status.rs and
// thunder_core::Config). Field names stay snake_case, as serde sends them.

export type Language = 'en' | 'id'

export interface Network {
  ssid: string
  enabled: boolean
  /** Where it's used ("Dorm"); missing = no place. Only groups the list and menu. */
  place?: string
}

export interface Config {
  version: number
  auto_switch: boolean
  weak_signal_dbm: number
  bad_ping_ms: number
  bad_loss_pct: number
  notify_on_switch: boolean
  check_for_updates: boolean
  show_signal_label: boolean
  language: Language
  ping_targets: string[]
  networks: Network[]
}

export type Connection =
  | 'starting'
  | 'off'
  | 'disconnected'
  | 'connected'
  | 'permission_needed'
  | 'unavailable'

export type Activity =
  | 'watching'
  | 'scanning'
  | 'switching'
  | 'settling'
  | 'waiting_to_retry'
  | 'paused'
  | 'asleep'

export type PermissionState = 'granted' | 'denied' | 'not_determined'

export type InternetState = 'unknown' | 'good' | 'bad' | 'bad_link'

export interface LastEvent {
  time: string
  text: string
}

export interface Status {
  connection: Connection
  ssid: string | null
  /** The place of the current network, if it has one. */
  place: string | null
  rssi: number | null
  ping_ms: number | null
  loss_pct: number | null
  internet: InternetState
  managed: boolean
  activity: Activity
  permission: PermissionState
  last_event: LastEvent | null
  error: string | null
  simulated: boolean
}

export interface AppInfo {
  version: string
  platform: string
  simulated: boolean
  passwords_editable: boolean
  config_path: string
}

export interface Nearby {
  ssid: string
  rssi: number
}

export interface Saved {
  config: Config
  notes: string[]
}

export type Link = 'repo' | 'privacy' | 'issues' | 'location_settings' | 'notification_settings'

/** Where updating stands (updates.rs `UpdateStatus`). */
export type UpdateStatus =
  | { state: 'idle' }
  | { state: 'checking' }
  | { state: 'up_to_date' }
  | { state: 'downloading'; version: string; percent: number | null }
  | { state: 'ready'; version: string }
  | { state: 'failed'; message: string }

/** Whether the system shows the app's notifications (commands.rs). Only macOS can tell. */
export type NotificationPermission = 'allowed' | 'blocked' | 'not_asked' | 'unknown'

/** Why "Join now" failed (status.rs `JoinError`). */
export type JoinProblem =
  | 'not_in_range'
  | 'no_password'
  | 'wrong_password'
  | 'needs_user_name'
  | 'permission'
  | 'wifi_off'
  | 'timeout'
  | 'other'

export interface JoinError {
  kind: JoinProblem
  message: string
}
