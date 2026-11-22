<script lang="ts">
  // The setup guide: what the app needs, what each permission is for, and
  // exactly what the system will ask, so nothing comes as a surprise. It
  // opens by itself while something required is missing (first run, or
  // Location turned off), and from About → Setup guide.
  import { api } from './api'
  import { app } from './store.svelte'

  type StepId = 'location' | 'networks' | 'notifications' | 'login'
  type Badge = { text: string; tone: 'done' | 'todo' | 'off' | 'warn' }

  let isWindows = $derived(app.info?.platform === 'windows')
  let permission = $derived(app.status?.permission ?? 'not_determined')
  let locationOk = $derived(permission === 'granted' && app.status?.connection !== 'permission_needed')
  let networkCount = $derived(app.config?.networks.length ?? 0)
  let ready = $derived(locationOk && networkCount > 0)
  let notifyOn = $derived(app.config?.notify_on_switch ?? false)

  // The first required step that isn't done is open. A click opens or closes
  // any step; `null` means "follow the current step".
  let current = $derived<StepId | null>(!locationOk ? 'location' : networkCount === 0 ? 'networks' : null)
  let opened = $state<StepId | 'none' | null>(null)
  const isOpen = (id: StepId) => opened === id || (opened === null && current === id)
  const toggle = (id: StepId) => (opened = isOpen(id) ? 'none' : id)
  // When a step gets done, open the next one.
  $effect(() => {
    void current
    opened = null
  })

  let badges = $derived<Record<StepId, Badge>>({
    location: locationOk
      ? { text: 'Done', tone: 'done' }
      : permission === 'denied'
        ? { text: 'Turned off', tone: 'warn' }
        : { text: 'Needed', tone: 'todo' },
    networks: networkCount > 0 ? { text: `${networkCount} added`, tone: 'done' } : { text: 'Needed', tone: 'todo' },
    notifications: !notifyOn
      ? { text: 'Off', tone: 'off' }
      : app.notifications === 'blocked'
        ? { text: 'Blocked', tone: 'warn' }
        : { text: 'On', tone: 'done' },
    login: app.autostart ? { text: 'On', tone: 'done' } : { text: 'Off', tone: 'off' },
  })
  let requiredDone = $derived((locationOk ? 1 : 0) + (networkCount > 0 ? 1 : 0))

  // Location: macOS answers through a later status update (every 5 seconds).
  let asking = $state(false)
  // Set when the user asked but no answer came: offer System Settings instead.
  let noPrompt = $state(false)

  async function allowLocation() {
    asking = true
    noPrompt = false
    try {
      await api.requestPermission()
    } catch (err) {
      app.flash('error', `Couldn't ask for Location permission: ${err}`)
    } finally {
      setTimeout(() => {
        asking = false
        noPrompt = app.status?.permission !== 'granted'
      }, 8000)
    }
  }
</script>

{#snippet head(id: StepId, n: number, title: string, summary: string)}
  {@const badge = badges[id]}
  <button type="button" class="head" aria-expanded={isOpen(id)} onclick={() => toggle(id)}>
    <span class="number" class:done={badge.tone === 'done'} aria-hidden="true">
      {badge.tone === 'done' ? '✓' : n}
    </span>
    <span class="title">
      <strong>{title}</strong>
      <small>{summary}</small>
    </span>
    <span class="badge {badge.tone}">{badge.text}</span>
    <span class="chevron" aria-hidden="true">{isOpen(id) ? '−' : '+'}</span>
  </button>
{/snippet}

<section class="guide" aria-label="Setup guide">
  <header>
    <h2>Set up Thunder WiFi</h2>
    <span class="progress">{requiredDone} of 2 needed steps done</span>
  </header>
  <p class="intro">Here's what the app asks for, why, and what your computer will show you.</p>

  <ol>
    <li>
      {@render head(
        'location',
        1,
        isWindows ? 'Location for desktop apps' : 'Location access',
        'Needed to see the names of Wi-Fi networks.',
      )}
      {#if isOpen('location')}
        <div class="body">
          {#if isWindows}
            <p>
              Windows treats Wi-Fi details as location data. Turn on "Let desktop apps access your location" so
              the app can see your networks.
            </p>
          {:else}
            <p>
              macOS only shows Wi-Fi network names to apps with Location access. Without it, the app can't tell
              which network you're on.
            </p>
            <p class="system">
              macOS asks: <q>‘Thunder WiFi Switcher’ would like to use your current location.</q> Choose
              <strong>Allow</strong>.
            </p>
          {/if}
          <p>The app never reads or stores where you are.</p>
          <div class="actions">
            {#if locationOk}
              <span class="ok">Location is allowed.</span>
            {:else if isWindows}
              <button type="button" class="primary" onclick={() => api.openLink('location_settings')}>
                Open Location settings
              </button>
            {:else if permission === 'denied'}
              <span class="warn">You chose Don't Allow before, so macOS won't ask again.</span>
              <button type="button" class="primary" onclick={() => api.openLink('location_settings')}>
                Open Location settings
              </button>
            {:else}
              <button type="button" class="primary" onclick={allowLocation} disabled={asking}>
                {asking ? 'Answer the macOS question…' : 'Allow Location'}
              </button>
            {/if}
            <button type="button" class="link" onclick={() => api.openLink('privacy')}>Privacy details</button>
          </div>
          {#if noPrompt && !locationOk && permission !== 'denied' && !isWindows}
            <p class="fallback">
              No question appeared? Turn on Thunder WiFi Switcher in
              <button type="button" class="link" onclick={() => api.openLink('location_settings')}>
                Location Services settings
              </button>.
            </p>
          {/if}
          {#if !isWindows}
            <p class="small">Until the app is signed by Apple, macOS may ask again after an update.</p>
          {/if}
        </div>
      {/if}
    </li>

    <li>
      {@render head('networks', 2, 'Your networks', 'The networks the app may switch between.')}
      {#if isOpen('networks')}
        <div class="body">
          <p>Add the networks you use from <strong>Nearby</strong> below, or type a name. The app only ever switches between these.</p>
          {#if isWindows}
            <p>
              Windows keeps the passwords: connect to each network once from the Windows Wi-Fi menu, and the app
              can use it from then on.
            </p>
          {:else}
            <p>
              Then type each network's password once. It's kept in your Mac's Keychain, never in a file, and only
              read when joining.
            </p>
            <p class="system">
              The first time the app joins a network, macOS asks: <q>‘Thunder WiFi Switcher’ wants to use your
                confidential information stored in ‘ThunderWiFi’ in your keychain.</q> Type your Mac password and
              choose <strong>Always Allow</strong>.
            </p>
            <p class="small">Until the app is signed by Apple, macOS may ask again after an update.</p>
          {/if}
        </div>
      {/if}
    </li>
  </ol>

  <h3>Optional</h3>
  <ol start="3">
    <li>
      {@render head('notifications', 3, 'Notifications', 'A note when the app switches by itself.')}
      <div class="actions inline">
        {#if !notifyOn}
          <button type="button" onclick={() => app.setNotify(true)} disabled={app.askingNotifications}>
            {app.askingNotifications ? 'Answer the macOS question…' : 'Turn on'}
          </button>
        {:else if app.notifications === 'blocked'}
          <button type="button" class="primary" onclick={() => app.openNotificationSettings()}>
            Open notification settings
          </button>
        {:else}
          <button type="button" class="link" onclick={() => app.setNotify(false)}>Turn off</button>
        {/if}
      </div>
      {#if isOpen('notifications')}
        <div class="body">
          <p>It says where the app moved and why, for example "Switched to Dorm-2.4 · Dorm-5G's signal was weak". Never for switches you make yourself.</p>
          {#if isWindows}
            <p>Notifications appear in the corner of the screen. Not seeing them? Check Settings → System → Notifications.</p>
          {:else}
            <p class="system">
              macOS asks once, in a notification: <q>‘Thunder WiFi Switcher’ would like to send you notifications.</q>
              Click <strong>Allow</strong> (it may be under <strong>Options</strong>). Closing it counts as no.
            </p>
            {#if notifyOn && app.notifications === 'blocked'}
              <p class="warn">
                macOS is blocking them now. In the settings page, turn on Allow notifications and choose Banners.
              </p>
            {/if}
          {/if}
        </div>
      {/if}
    </li>

    <li>
      {@render head('login', 4, 'Open at login', 'Start the app with your computer.')}
      <div class="actions inline">
        {#if app.autostart}
          <button type="button" class="link" onclick={() => app.setAutostart(false)}>Turn off</button>
        {:else}
          <button type="button" onclick={() => app.setAutostart(true)} disabled={app.autostart === null}>
            Turn on
          </button>
        {/if}
      </div>
      {#if isOpen('login')}
        <div class="body">
          <p>Recommended, so the app keeps working after a restart.</p>
          {#if isWindows}
            <p>Windows lists it in Settings → Apps → Startup, where you can turn it off too.</p>
          {:else}
            <p class="system">
              macOS then shows <q>Background Items Added</q>. That's this app: nothing to do. You can turn it off
              in System Settings → General → Login Items too.
            </p>
          {/if}
        </div>
      {/if}
    </li>
  </ol>

  {#if ready}
    <div class="finish">
      <p>
        <strong>You're set.</strong> The app works from the {isWindows
          ? 'system tray, near the clock'
          : 'menu bar at the top of your screen'}: click its Wi-Fi icon for the menu. You can close this window.
      </p>
      <button type="button" class="primary" onclick={() => (app.setupOpen = false)}>Done</button>
    </div>
  {/if}
</section>

<style>
  .guide {
    margin-bottom: 18px;
    padding: 14px 16px;
    border: 1px solid var(--accent-border);
    border-radius: 10px;
    background: var(--accent-bg);
  }

  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }

  h2 {
    margin: 0;
    font-size: 15px;
  }

  h3 {
    margin: 12px 0 4px;
    font-size: 12px;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .progress,
  .intro,
  .small {
    color: var(--muted);
    font-size: 12px;
  }

  .intro {
    margin: 2px 0 8px;
  }

  ol {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    overflow: hidden;
  }

  li + li {
    border-top: 1px solid var(--border);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 9px 10px;
    border: none;
    border-radius: 0;
    background: none;
    text-align: left;
  }

  .number {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 12px;
    font-weight: 700;
  }

  .number.done {
    background: var(--ok);
  }

  .title {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .title small {
    color: var(--muted);
    font-size: 12px;
  }

  .badge {
    flex: none;
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
  }

  .badge.done {
    background: var(--ok-bg);
    color: var(--ok);
  }

  .badge.todo {
    background: var(--accent-bg);
    color: var(--accent);
  }

  .badge.off {
    background: var(--surface);
    color: var(--muted);
  }

  .badge.warn {
    background: var(--danger-bg);
    color: var(--danger);
  }

  .chevron {
    flex: none;
    width: 12px;
    color: var(--muted);
    text-align: center;
  }

  .body {
    padding: 0 12px 10px 42px;
  }

  .body p {
    margin: 0 0 6px;
  }

  .system {
    padding: 6px 8px;
    border-left: 3px solid var(--accent);
    background: var(--surface);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }

  .actions.inline {
    margin: -4px 0 8px 42px;
  }

  .ok {
    color: var(--ok);
  }

  .warn {
    color: var(--danger);
  }

  .fallback {
    margin-top: 8px;
  }

  .finish {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
  }

  .finish p {
    flex: 1;
    margin: 0;
  }
</style>
