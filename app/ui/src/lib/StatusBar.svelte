<script lang="ts">
  import { app, bars } from './store.svelte'
  import type { Activity, Status } from './types'

  const activityText: Record<Activity, string> = {
    watching: 'Watching',
    scanning: 'Looking for a better network…',
    switching: 'Switching…',
    settling: 'Just switched',
    waiting_to_retry: 'Nothing better nearby',
    paused: 'Auto-switch is off',
    asleep: 'Asleep',
  }

  function headline(status: Status): string {
    switch (status.connection) {
      case 'connected':
        return status.ssid ?? 'Connected'
      case 'starting':
        return 'Starting…'
      case 'off':
        return 'Wi-Fi is off'
      case 'disconnected':
        return 'Not connected'
      case 'permission_needed':
        return 'Needs Location permission'
      case 'unavailable':
        return status.error ?? 'Wi-Fi unavailable'
    }
  }

  /** Same wording as the menu (status.rs `internet_problem`). */
  function internetProblem(status: Status): string | null {
    if (status.internet === 'bad_link') return 'Router not responding'
    if (status.internet !== 'bad') return null
    const loss = status.loss_pct ?? 100
    if (status.ping_ms === null || loss >= 100) return 'No internet'
    if (loss >= 10) return 'Internet dropping out'
    return `Slow internet (${status.ping_ms} ms)`
  }

  let status = $derived(app.status)
  let level = $derived(status?.connection === 'connected' && status.rssi !== null ? bars(status.rssi) : -1)
</script>

{#if status}
  <header class="status" aria-live="polite">
    <span class="signal" data-level={level} aria-hidden="true">
      <span></span><span></span><span></span>
    </span>
    <div class="text">
      <strong>{headline(status)}</strong>
      <span class="detail">
        {#if status.connection === 'connected' && status.place}
          {status.place} ·
        {/if}
        {#if status.connection === 'connected' && status.rssi !== null}
          {status.rssi} dBm ·
          {#if internetProblem(status)}
            <span class="bad">{internetProblem(status)}</span> ·
          {:else if status.ping_ms !== null}
            {status.ping_ms} ms ·
          {/if}
        {/if}
        {activityText[status.activity]}
        {#if status.connection === 'connected' && !status.managed}
          · not one of your networks
        {/if}
      </span>
    </div>
    {#if status.simulated}
      <span class="chip">Simulated</span>
    {/if}
  </header>
{/if}

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 18px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }

  strong {
    font-size: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .detail {
    color: var(--muted);
    font-size: 12px;
  }

  .bad {
    color: var(--danger);
    font-weight: 600;
  }

  .chip {
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--warn-bg);
    color: var(--warn);
    font-size: 11px;
    font-weight: 600;
  }

  /* Three bars that fill up with signal, like the menu bar icon. */
  .signal {
    display: inline-flex;
    align-items: flex-end;
    gap: 2px;
    height: 18px;
  }

  .signal span {
    width: 4px;
    border-radius: 1px;
    background: var(--border);
  }

  .signal span:nth-child(1) {
    height: 6px;
  }
  .signal span:nth-child(2) {
    height: 11px;
  }
  .signal span:nth-child(3) {
    height: 16px;
  }

  .signal[data-level='1'] span:nth-child(-n + 1),
  .signal[data-level='2'] span:nth-child(-n + 2),
  .signal[data-level='3'] span:nth-child(-n + 3) {
    background: var(--accent);
  }

  .signal[data-level='0'] span:nth-child(1) {
    background: var(--warn);
  }
</style>
