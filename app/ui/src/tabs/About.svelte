<script lang="ts">
  import appIcon from '../../../../assets/icons/thunder-wifi-macos.svg'
  import { api } from '../lib/api'
  import { app } from '../lib/store.svelte'
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
  <p class="hint">
    Free and open source under the MIT license. Automatic updates arrive with v1.0; until then, new versions
    are on GitHub.
  </p>
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

  .links {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin: 16px 0;
  }
</style>
