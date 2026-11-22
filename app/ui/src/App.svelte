<script lang="ts">
  import { onMount } from 'svelte'

  import StatusBar from './lib/StatusBar.svelte'
  import { app, type TabId } from './lib/store.svelte'
  import About from './tabs/About.svelte'
  import General from './tabs/General.svelte'
  import Networks from './tabs/Networks.svelte'
  import Rules from './tabs/Rules.svelte'

  const tabs: { id: TabId; label: string }[] = [
    { id: 'networks', label: 'Networks' },
    { id: 'rules', label: 'Rules' },
    { id: 'general', label: 'General' },
    { id: 'about', label: 'About' },
  ]

  onMount(() => {
    app.load()
  })

  // Something the app needs is missing (first run, or Location turned off):
  // open the setup guide. It stays until the user closes it.
  $effect(() => {
    if (app.setupNeeded) app.setupOpen = true
  })

  function onTabKeydown(event: KeyboardEvent, index: number) {
    let next: number
    if (event.key === 'ArrowRight') next = (index + 1) % tabs.length
    else if (event.key === 'ArrowLeft') next = (index - 1 + tabs.length) % tabs.length
    else if (event.key === 'Home') next = 0
    else if (event.key === 'End') next = tabs.length - 1
    else return
    event.preventDefault()
    app.tab = tabs[next].id
    document.getElementById(`tab-${app.tab}`)?.focus()
  }
</script>

<main>
  <StatusBar />

  <div class="tablist" role="tablist" aria-label="Settings sections">
    {#each tabs as tab, index (tab.id)}
      <button
        id="tab-{tab.id}"
        role="tab"
        type="button"
        aria-selected={app.tab === tab.id}
        aria-controls="panel-{tab.id}"
        tabindex={app.tab === tab.id ? 0 : -1}
        onclick={() => (app.tab = tab.id)}
        onkeydown={(event) => onTabKeydown(event, index)}
      >
        {tab.label}
      </button>
    {/each}
  </div>

  <div id="panel-{app.tab}" class="panel" role="tabpanel" aria-labelledby="tab-{app.tab}">
    {#if app.notice}
      <p class="notice">
        {app.notice}
        <button type="button" class="link" onclick={() => (app.notice = null)}>Dismiss</button>
      </p>
    {/if}
    {#if app.tab === 'networks'}
      <Networks />
    {:else if app.tab === 'rules'}
      <Rules />
    {:else if app.tab === 'general'}
      <General />
    {:else}
      <About />
    {/if}
  </div>

  {#if app.message}
    <div class="message" class:error={app.message.kind === 'error'} role="status">{app.message.text}</div>
  {/if}
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
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
  }

  [role='tab'] {
    border: none;
    border-radius: 6px;
    padding: 5px 14px;
    background: transparent;
  }

  [role='tab']:hover {
    background: var(--hover);
  }

  [role='tab'][aria-selected='true'] {
    background: var(--accent);
    color: var(--on-accent);
  }

  .panel {
    flex: 1;
    overflow-y: auto;
    padding: 18px 20px 24px;
  }

  .notice {
    margin: 0 0 16px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--warn-bg);
    color: var(--text);
  }

  .message {
    padding: 8px 16px;
    border-top: 1px solid var(--border);
    background: var(--surface);
    font-size: 12px;
  }

  .message.error {
    background: var(--danger-bg);
    color: var(--danger);
  }
</style>
