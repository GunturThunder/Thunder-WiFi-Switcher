<script lang="ts">
  import { onMount } from 'svelte'
  import { getVersion } from '@tauri-apps/api/app'
  import appIcon from '../../../assets/icons/thunder-wifi-macos.svg'

  type TabId = 'networks' | 'rules' | 'general' | 'about'

  const tabs: { id: TabId; label: string }[] = [
    { id: 'networks', label: 'Networks' },
    { id: 'rules', label: 'Rules' },
    { id: 'general', label: 'General' },
    { id: 'about', label: 'About' },
  ]

  // What each tab will hold, from docs/TECHNICAL-DESIGN.md. Filled in during Phase 3.
  const planned: Record<Exclude<TabId, 'about'>, string[]> = {
    networks: [
      'Your saved networks, in priority order (drag to reorder)',
      'Turn each network on or off',
      'Add and remove networks',
      'On macOS, a password field for each network',
    ],
    rules: [
      'Weak-signal level (-65 to -80 dBm, default -75)',
      'Bad ping and packet-loss limits',
      'Ping targets (default 1.1.1.1 and 8.8.8.8)',
    ],
    general: [
      'Open at login',
      'Notifications when it switches',
      'Language (English or Bahasa Indonesia)',
      'Show signal and ping next to the menu bar icon',
    ],
  }

  let active = $state<TabId>('networks')
  let version = $state('')

  onMount(async () => {
    try {
      version = await getVersion()
    } catch {
      // Running in a plain browser (`npm run dev`) rather than inside Tauri.
      version = 'dev (browser preview)'
    }
  })

  function onTabKeydown(event: KeyboardEvent, index: number) {
    let next: number
    if (event.key === 'ArrowRight') next = (index + 1) % tabs.length
    else if (event.key === 'ArrowLeft') next = (index - 1 + tabs.length) % tabs.length
    else if (event.key === 'Home') next = 0
    else if (event.key === 'End') next = tabs.length - 1
    else return
    event.preventDefault()
    active = tabs[next].id
    document.getElementById(`tab-${active}`)?.focus()
  }
</script>

<main>
  <div class="tablist" role="tablist" aria-label="Settings sections">
    {#each tabs as tab, index (tab.id)}
      <button
        id="tab-{tab.id}"
        role="tab"
        type="button"
        aria-selected={active === tab.id}
        aria-controls="panel-{tab.id}"
        tabindex={active === tab.id ? 0 : -1}
        onclick={() => (active = tab.id)}
        onkeydown={(event) => onTabKeydown(event, index)}
      >
        {tab.label}
      </button>
    {/each}
  </div>

  <div id="panel-{active}" role="tabpanel" aria-labelledby="tab-{active}">
    {#if active === 'about'}
      <div class="about">
        <img src={appIcon} width="96" height="96" alt="" />
        <h1>Thunder WiFi Switcher</h1>
        <p class="version">Version {version}</p>
        <p>Keeps you on the network with the best internet, not just the strongest signal.</p>
        <p class="muted">
          Free and open source under the MIT license.<br />
          github.com/GunturThunder/Thunder-WiFi-Switcher
        </p>
      </div>
    {:else}
      <div class="placeholder">
        <p class="badge">Coming in v0.1</p>
        <h2>{tabs.find((t) => t.id === active)?.label}</h2>
        <p class="muted">This tab will let you:</p>
        <ul>
          {#each planned[active] as item (item)}
            <li>{item}</li>
          {/each}
        </ul>
      </div>
    {/if}
  </div>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .tablist {
    display: flex;
    justify-content: center;
    gap: 4px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }

  [role='tab'] {
    border: none;
    border-radius: 6px;
    padding: 6px 14px;
    background: transparent;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  [role='tab']:hover {
    background: var(--hover);
  }

  [role='tab'][aria-selected='true'] {
    background: var(--accent);
    color: var(--on-accent);
  }

  [role='tab']:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  [role='tabpanel'] {
    flex: 1;
    overflow-y: auto;
    padding: 24px 28px;
  }

  h1 {
    margin: 12px 0 4px;
    font-size: 20px;
  }

  h2 {
    margin: 8px 0;
    font-size: 18px;
  }

  ul {
    margin: 8px 0 0;
    padding-left: 20px;
    line-height: 1.7;
  }

  .about {
    text-align: center;
    padding-top: 24px;
  }

  .version {
    margin-top: 0;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .muted {
    color: var(--muted);
  }

  .badge {
    display: inline-block;
    margin: 0;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--hover);
    color: var(--muted);
    font-size: 12px;
  }
</style>
