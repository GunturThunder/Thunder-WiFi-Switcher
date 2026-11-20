<script lang="ts">
  // First-run steps: Location permission, then adding networks. Hidden once
  // both are done.
  import { api } from './api'
  import { app } from './store.svelte'

  let asking = $state(false)
  // Set when the user asked but no answer came: offer System Settings instead.
  let noPrompt = $state(false)

  let permission = $derived(app.status?.permission ?? 'granted')
  let isMac = $derived(app.info?.platform === 'macos')
  // macOS asks each app; Windows 11 24H2+ has one switch for all desktop apps,
  // and the app only learns it's off when Wi-Fi details come back blocked.
  let needsPermission = $derived(
    permission !== 'granted' || app.status?.connection === 'permission_needed',
  )
  let needsNetworks = $derived((app.config?.networks.length ?? 0) === 0)

  async function allow() {
    asking = true
    noPrompt = false
    try {
      await api.requestPermission()
    } catch (err) {
      app.flash('error', `Couldn't ask for Location permission: ${err}`)
    } finally {
      // The answer arrives with a later status update (every 5 seconds).
      setTimeout(() => {
        asking = false
        noPrompt = app.status?.permission !== 'granted'
      }, 8000)
    }
  }
</script>

{#if needsPermission || needsNetworks}
  <section class="card" aria-label="Get started">
    {#if needsPermission && !isMac}
      <div class="step">
        <span class="number" aria-hidden="true">1</span>
        <div>
          <h2>Turn on location for desktop apps</h2>
          <p>
            Windows treats Wi-Fi details as location data. Turn on "Let desktop apps access your location" so the
            app can see your networks. It never reads or stores your location.
          </p>
          <button type="button" class="primary" onclick={() => api.openLink('location_settings')}>
            Open Location settings
          </button>
          <button type="button" class="link" onclick={() => api.openLink('privacy')}>Why?</button>
        </div>
      </div>
    {:else if needsPermission}
      <div class="step">
        <span class="number" aria-hidden="true">1</span>
        <div>
          <h2>Allow Location access</h2>
          <p>
            macOS only shows Wi-Fi network names to apps with Location permission. The app never
            reads or stores your location.
          </p>
          {#if permission === 'denied'}
            <p>Location is turned off for Thunder WiFi Switcher.</p>
            <button type="button" class="primary" onclick={() => api.openLink('location_settings')}>
              Open System Settings
            </button>
          {:else}
            <button type="button" class="primary" onclick={allow} disabled={asking}>
              {asking ? 'Answer the macOS prompt…' : 'Allow Location'}
            </button>
          {/if}
          <button type="button" class="link" onclick={() => api.openLink('privacy')}>Why?</button>
          {#if noPrompt && permission !== 'denied'}
            <p class="fallback">
              Didn't see a prompt? Turn on Thunder WiFi Switcher in
              <button type="button" class="link" onclick={() => api.openLink('location_settings')}>
                Location Services settings
              </button>.
            </p>
          {/if}
        </div>
      </div>
    {/if}
    {#if needsNetworks}
      <div class="step">
        <span class="number" aria-hidden="true">{needsPermission ? 2 : 1}</span>
        <div>
          <h2>Add the networks you use</h2>
          <p>Pick them from the list below. The app only ever switches between these.</p>
        </div>
      </div>
    {/if}
  </section>
{/if}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin-bottom: 18px;
    padding: 14px 16px;
    border: 1px solid var(--accent-border);
    border-radius: 10px;
    background: var(--accent-bg);
  }

  .step {
    display: flex;
    gap: 12px;
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

  h2 {
    margin: 1px 0 4px;
    font-size: 14px;
  }

  p {
    margin: 0 0 8px;
    color: var(--muted);
  }

  .fallback {
    margin: 8px 0 0;
  }
</style>
