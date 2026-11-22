<script lang="ts">
  import appIcon from '../../../../assets/icons/thunder-wifi-macos.svg'
  import { api } from '../lib/api'
  import { app } from '../lib/store.svelte'

  let update = $derived(app.update)
  let isWindows = $derived(app.info?.platform === 'windows')
  let busy = $derived(update.state === 'checking' || update.state === 'downloading')
</script>

<div class="about">
  <img src={appIcon} width="88" height="88" alt="" />
  <h2>Thunder WiFi Switcher</h2>
  <p class="version">
    Version {app.info?.version ?? '…'}{app.info?.simulated ? ' · simulated networks' : ''}
  </p>
  <p>Keeps you on the network with the best internet, not just the strongest signal.</p>
  <div class="links">
    <button type="button" onclick={() => api.openLink('repo')}>GitHub</button>
    <button type="button" onclick={() => api.openLink('privacy')}>Privacy</button>
    <button type="button" onclick={() => api.openLink('issues')}>Report a problem</button>
  </div>
  <p>
    <button type="button" class="link" onclick={() => app.openSetup()}>Setup guide</button>
    · what the app asks for, and why
  </p>
  <section class="updates" aria-live="polite">
    {#if update.state === 'checking'}
      <p>Checking for updates…</p>
    {:else if update.state === 'downloading'}
      <p>Downloading version {update.version}…{update.percent !== null ? ` ${update.percent}%` : ''}</p>
    {:else if update.state === 'ready'}
      <p><strong>Version {update.version} is ready.</strong> It installs when the app restarts.</p>
      {#if !isWindows}
        <p class="hint">The app isn't signed by Apple, so macOS asks for Location again afterwards. The setup guide opens to help.</p>
      {/if}
      <button type="button" class="primary" onclick={() => app.installUpdate()}>Restart to update</button>
    {:else}
      {#if update.state === 'up_to_date'}
        <p>You have the newest version.</p>
      {:else if update.state === 'failed'}
        <p class="error">{update.message}</p>
      {/if}
      <button type="button" onclick={() => app.checkForUpdates()} disabled={busy}>Check for updates</button>
    {/if}
  </section>

  <p class="hint">Free and open source under the MIT license.</p>
</div>

<style>
  .about {
    text-align: center;
    padding-top: 12px;
  }

  h2 {
    margin: 10px 0 2px;
    font-size: 18px;
  }

  .version {
    margin-top: 0;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .updates {
    margin: 16px auto;
    padding: 10px 12px;
    max-width: 360px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  .updates p {
    margin: 0 0 8px;
  }

  .updates .error {
    color: var(--danger);
  }

  .links {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin: 16px 0;
  }
</style>
