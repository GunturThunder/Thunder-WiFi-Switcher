<script lang="ts">
  import { onDestroy, onMount } from 'svelte'

  import { api } from '../lib/api'
  import SetupGuide from '../lib/SetupGuide.svelte'
  import { app } from '../lib/store.svelte'
  import type { JoinError, Nearby } from '../lib/types'

  let networks = $derived(app.config?.networks ?? [])
  let current = $derived(app.status?.connection === 'connected' ? app.status.ssid : null)
  let editable = $derived(app.info?.passwords_editable ?? false)
  let isMac = $derived(app.info?.platform === 'macos')

  let saved = $state<Record<string, boolean>>({})
  let editing = $state<string | null>(null)
  let password = $state('')
  let newName = $state('')
  let nearby = $state<Nearby[] | null>(null)
  let known = $state<string[]>([])
  let scanning = $state(false)
  let scanError = $state<string | null>(null)
  // The last scan failed only because Location wasn't allowed yet.
  let scanNeedsLocation = $state(false)
  let joining = $state<string | null>(null)
  // Why the last "Join now" failed, per network. Each one hides itself after
  // a few seconds, but not while the pointer or keyboard focus is on it.
  const JOIN_ERROR_MS = 5000
  let joinErrors = $state<Record<string, JoinError>>({})
  const joinErrorTimers = new Map<string, ReturnType<typeof setTimeout>>()
  let dragFrom = $state<number | null>(null)
  let dropAt = $state<number | null>(null)

  let names = $derived(new Set(networks.map((network) => network.ssid)))
  let nearbyToAdd = $derived((nearby ?? []).filter((network) => !names.has(network.ssid)))
  let suggestions = $derived(
    [...new Set([...(nearby ?? []).map((n) => n.ssid), ...known])].filter((ssid) => !names.has(ssid)),
  )

  // Which networks have a saved password (never the password itself).
  $effect(() => {
    const ssids = networks.map((network) => network.ssid)
    if (!editable || ssids.length === 0) return
    api.passwordStatus(ssids).then((status) => (saved = status))
  })

  onMount(() => {
    api.knownNetworks().then((list) => (known = list)).catch(() => {})
    scan()
  })

  async function scan() {
    scanning = true
    scanError = null
    scanNeedsLocation = false
    try {
      nearby = await api.nearbyNetworks()
    } catch (err) {
      scanNeedsLocation = String(err).includes('Location')
      scanError = scanNeedsLocation ? 'Allow Location access to see nearby networks.' : `Couldn't scan: ${err}`
    } finally {
      scanning = false
    }
  }

  // Scan again as soon as Location is allowed, so Nearby fills in by itself.
  $effect(() => {
    if (scanNeedsLocation && app.status?.permission === 'granted') scan()
  })

  async function join(ssid: string) {
    joining = ssid
    clearJoinError(ssid)
    try {
      await api.switchTo(ssid)
      app.flash('info', `Joined ${ssid}.`)
    } catch (err) {
      joinErrors = { ...joinErrors, [ssid]: err as JoinError }
      hideJoinErrorLater(ssid)
    } finally {
      joining = null
    }
  }

  function hideJoinErrorLater(ssid: string) {
    keepJoinError(ssid)
    joinErrorTimers.set(ssid, setTimeout(() => clearJoinError(ssid), JOIN_ERROR_MS))
  }

  function keepJoinError(ssid: string) {
    clearTimeout(joinErrorTimers.get(ssid))
    joinErrorTimers.delete(ssid)
  }

  function clearJoinError(ssid: string) {
    keepJoinError(ssid)
    const { [ssid]: _, ...rest } = joinErrors
    joinErrors = rest
  }

  onDestroy(() => joinErrorTimers.forEach((timer) => clearTimeout(timer)))

  async function allowLocation() {
    try {
      // macOS shows its prompt only once; after a "Don't Allow", open settings.
      if (isMac && app.status?.permission !== 'denied') await api.requestPermission()
      else await api.openLink('location_settings')
    } catch (err) {
      app.flash('error', `Couldn't open Location settings: ${err}`)
    }
  }

  function move(from: number, to: number) {
    if (to < 0 || to >= networks.length || from === to) return
    app.save((config) => {
      const [network] = config.networks.splice(from, 1)
      config.networks.splice(to, 0, network)
    })
  }

  function toggle(index: number) {
    app.save((config) => {
      config.networks[index].enabled = !config.networks[index].enabled
    })
  }

  async function remove(ssid: string) {
    await app.save((config) => {
      config.networks = config.networks.filter((network) => network.ssid !== ssid)
    })
    if (editable) await api.deletePassword(ssid).catch(() => {})
    if (editing === ssid) editing = null
    clearJoinError(ssid)
  }

  async function add(name: string) {
    const ssid = name.trim()
    if (!ssid) return
    if (names.has(ssid)) {
      app.flash('info', `${ssid} is already in your list.`)
      return
    }
    await app.save((config) => {
      config.networks.push({ ssid, enabled: true })
    })
    newName = ''
    if (editable) openEditor(ssid)
  }

  function openEditor(ssid: string) {
    editing = ssid
    password = ''
  }

  // Puts the cursor in the password field when it opens.
  function focus(input: HTMLInputElement) {
    input.focus()
  }

  async function savePassword(event: SubmitEvent) {
    event.preventDefault()
    if (!editing) return
    const ssid = editing
    try {
      await api.setPassword(ssid, password)
      saved = { ...saved, [ssid]: password !== '' }
      app.flash('info', password ? `Saved the password for ${ssid}.` : `${ssid} saved as an open network.`)
      editing = null
      clearJoinError(ssid)
    } catch (err) {
      app.flash('error', String(err))
    } finally {
      password = ''
    }
  }

  async function openNetwork(ssid: string) {
    password = ''
    try {
      await api.deletePassword(ssid)
      saved = { ...saved, [ssid]: false }
      app.flash('info', `${ssid} saved as an open network.`)
      editing = null
      clearJoinError(ssid)
    } catch (err) {
      app.flash('error', String(err))
    }
  }

  function onDrop(index: number) {
    if (dragFrom !== null) move(dragFrom, index)
    dragFrom = null
    dropAt = null
  }
</script>

{#if app.setupOpen || app.setupNeeded}
  <SetupGuide />
{/if}

<section>
  <h2>Your networks</h2>
  <p class="hint">Higher in the list wins when two networks are about equally strong. Drag to reorder.</p>
  {#if app.info && !editable}
    <p class="hint">
      Windows uses the passwords it has already saved: connect to each network once from the Windows Wi-Fi menu
      first.
    </p>
  {/if}

  {#if networks.length === 0}
    <p class="empty">No networks yet.</p>
  {:else}
    <ul class="networks">
      {#each networks as network, index (network.ssid)}
        <li
          draggable="true"
          class:dragging={dragFrom === index}
          class:drop={dropAt === index && dragFrom !== index}
          ondragstart={() => (dragFrom = index)}
          ondragover={(event) => {
            event.preventDefault()
            dropAt = index
          }}
          ondragleave={() => (dropAt = null)}
          ondrop={() => onDrop(index)}
          ondragend={() => {
            dragFrom = null
            dropAt = null
          }}
        >
          <div class="row">
            <span class="handle" aria-hidden="true">⋮⋮</span>
            <input
              type="checkbox"
              checked={network.enabled}
              onchange={() => toggle(index)}
              aria-label="Switch to {network.ssid} automatically"
              title="Switch to this network automatically"
            />
            <div class="name">
              <span class="ssid" class:off={!network.enabled}>{network.ssid}</span>
              <span class="meta">
                {#if current === network.ssid}
                  <span class="badge">Connected</span>
                {:else if joining === network.ssid}
                  <span class="joining">Joining…</span>
                {:else if network.enabled}
                  <button type="button" class="link" onclick={() => join(network.ssid)} disabled={joining !== null}>
                    Join now
                  </button>
                {/if}
                {#if editable}
                  <span class="sep">·</span>
                  {saved[network.ssid] ? 'Password saved' : 'No password'}
                  <button type="button" class="link" onclick={() => openEditor(network.ssid)}>
                    {saved[network.ssid] ? 'Change' : 'Add'}
                  </button>
                {/if}
              </span>
            </div>
            <div class="actions">
              <button type="button" class="icon" onclick={() => move(index, index - 1)} disabled={index === 0} aria-label="Move {network.ssid} up">↑</button>
              <button type="button" class="icon" onclick={() => move(index, index + 1)} disabled={index === networks.length - 1} aria-label="Move {network.ssid} down">↓</button>
              <button type="button" class="icon danger" onclick={() => remove(network.ssid)} aria-label="Remove {network.ssid}" title="Remove (and its saved password)">✕</button>
            </div>
          </div>
          {#if joinErrors[network.ssid] && current !== network.ssid}
            {@const problem = joinErrors[network.ssid]}
            <div
              class="join-error"
              role="alert"
              onmouseenter={() => keepJoinError(network.ssid)}
              onmouseleave={() => hideJoinErrorLater(network.ssid)}
              onfocusin={() => keepJoinError(network.ssid)}
              onfocusout={() => hideJoinErrorLater(network.ssid)}
            >
              <p>{problem.message}</p>
              <div class="fixes">
                {#if editable && problem.kind === 'no_password' && editing !== network.ssid}
                  <button type="button" class="link" onclick={() => openEditor(network.ssid)}>Add password</button>
                {:else if editable && problem.kind === 'wrong_password' && editing !== network.ssid}
                  <button type="button" class="link" onclick={() => openEditor(network.ssid)}>Change password</button>
                {:else if problem.kind === 'permission'}
                  <button type="button" class="link" onclick={allowLocation}>
                    {isMac ? 'Allow Location' : 'Open Location settings'}
                  </button>
                {/if}
                {#if problem.kind !== 'needs_user_name'}
                  <button type="button" class="link" onclick={() => join(network.ssid)} disabled={joining !== null}>
                    Try again
                  </button>
                {/if}
                <button type="button" class="link" onclick={() => clearJoinError(network.ssid)}>Dismiss</button>
              </div>
            </div>
          {/if}
          {#if editing === network.ssid}
            <form class="password" onsubmit={savePassword}>
              <input
                type="password"
                use:focus
                bind:value={password}
                placeholder="Wi-Fi password for {network.ssid}"
                autocomplete="off"
                aria-label="Wi-Fi password for {network.ssid}"
              />
              <button type="submit" class="primary" disabled={password === ''}>Save</button>
              <button type="button" onclick={() => (editing = null)}>Cancel</button>
              <button type="button" class="link" onclick={() => openNetwork(network.ssid)}>It has no password</button>
              <p class="hint">Stored in your Keychain. macOS doesn't share the passwords it already knows with apps.</p>
            </form>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<section>
  <h2>Add a network</h2>
  <form
    class="add"
    onsubmit={(event) => {
      event.preventDefault()
      add(newName)
    }}
  >
    <input list="network-suggestions" bind:value={newName} placeholder="Network name" aria-label="Network name" />
    <datalist id="network-suggestions">
      {#each suggestions as ssid (ssid)}
        <option value={ssid}></option>
      {/each}
    </datalist>
    <button type="submit" class="primary" disabled={newName.trim() === ''}>Add</button>
  </form>

  <div class="nearby-head">
    <h3>Nearby</h3>
    <button type="button" class="link" onclick={scan} disabled={scanning}>{scanning ? 'Scanning…' : 'Scan again'}</button>
  </div>
  {#if scanError}
    <p class="hint">{scanError}</p>
  {:else if nearby === null}
    <p class="hint">Scanning…</p>
  {:else if nearbyToAdd.length === 0}
    <p class="hint">Every network in range is already in your list.</p>
  {:else}
    <ul class="nearby">
      {#each nearbyToAdd as network (network.ssid)}
        <li>
          <button type="button" class="chip" onclick={() => add(network.ssid)}>
            <span>+ {network.ssid}</span>
            <span class="rssi">{network.rssi} dBm</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  section + section {
    margin-top: 22px;
  }

  .networks {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
  }

  .networks li {
    background: var(--bg);
  }

  .networks li + li {
    border-top: 1px solid var(--border);
  }

  .networks li.dragging {
    opacity: 0.5;
  }

  .networks li.drop {
    box-shadow: inset 0 2px 0 var(--accent);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 6px;
  }

  .handle {
    color: var(--muted);
    cursor: grab;
    letter-spacing: -3px;
    padding: 0 4px;
  }

  .name {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .ssid {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ssid.off {
    color: var(--muted);
    font-weight: 400;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    font-size: 12px;
  }

  .badge {
    padding: 0 6px;
    border-radius: 999px;
    background: var(--ok-bg);
    color: var(--ok);
    font-weight: 600;
  }

  .actions {
    display: flex;
    gap: 2px;
  }

  .joining {
    font-style: italic;
  }

  .join-error {
    margin: 0 10px 10px 38px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--danger-bg);
    color: var(--danger);
    font-size: 12px;
  }

  .join-error p {
    margin: 0 0 4px;
  }

  .fixes {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .password {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 0 10px 10px 38px;
  }

  .password input {
    flex: 1 1 100%;
  }

  .password .hint {
    flex-basis: 100%;
    margin: 0;
  }

  .add {
    display: flex;
    gap: 8px;
  }

  .add input {
    flex: 1;
  }

  .nearby-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-top: 14px;
  }

  h3 {
    margin: 0 0 6px;
    font-size: 12px;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .nearby {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .chip {
    display: inline-flex;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
  }

  .rssi {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .empty {
    color: var(--muted);
  }
</style>
