<script lang="ts">
  // First-run steps: Location permission, then adding networks. Hidden once
  // both are done.
  import { api } from './api'
  import { app } from './store.svelte'

  let asking = $state(false)

  let permission = $derived(app.status?.permission ?? 'granted')
  let needsPermission = $derived(
    app.info?.platform === 'macos' &&
      (permission !== 'granted' || app.status?.connection === 'permission_needed'),
  )
  let needsNetworks = $derived((app.config?.networks.length ?? 0) === 0)

  async function allow() {
    asking = true
    try {
      await api.requestPermission()
    } finally {
      // The answer arrives with the next status update.
      setTimeout(() => (asking = false), 4000)
    }
  }
</script>

{#if needsPermission || needsNetworks}
  <section class="card" aria-label="Get started">
    {#if needsPermission}
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
</style>
