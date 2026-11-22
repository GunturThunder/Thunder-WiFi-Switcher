<script lang="ts">
  import { onMount } from 'svelte'

  import { api } from '../lib/api'
  import { app } from '../lib/store.svelte'
  import type { NotificationPermission } from '../lib/types'

  let autostart = $state<boolean | null>(null)
  let config = $derived(app.config)
  // Turning notifications on sends a test one; say so just then.
  let testSent = $state(false)
  // What macOS says about the app's notifications ('unknown' on Windows).
  let permission = $state<NotificationPermission>('unknown')
  let asking = $state(false)

  async function checkPermission() {
    permission = await api.notificationPermission().catch(() => 'unknown' as const)
  }

  async function setNotify(on: boolean) {
    // Ask when the user turns them on, not on the first switch. macOS asks
    // each app only once, so this does nothing after the first answer.
    if (on && permission === 'not_asked') {
      asking = true
      permission = await api.askNotificationPermission().catch(() => 'unknown' as const)
      asking = false
    }
    testSent = on
    await app.save((c) => (c.notify_on_switch = on))
  }

  async function openNotificationSettings(event: MouseEvent) {
    event.preventDefault() // don't toggle the checkbox this sits in
    try {
      await api.openLink('notification_settings')
    } catch (err) {
      app.flash('error', `Couldn't open notification settings: ${err}`)
    }
  }

  onMount(() => {
    api
      .getAutostart()
      .then((enabled) => (autostart = enabled))
      .catch(() => (autostart = false))
    checkPermission()
    // Check again when the user comes back, e.g. from System Settings.
    window.addEventListener('focus', checkPermission)
    return () => window.removeEventListener('focus', checkPermission)
  })

  async function setAutostart(enabled: boolean) {
    try {
      await api.setAutostart(enabled)
      autostart = enabled
    } catch (err) {
      app.flash('error', `Couldn't change "Open at login": ${err}`)
    }
  }
</script>

{#if config}
  <section class="options">
    <label class="option">
      <input
        type="checkbox"
        checked={config.auto_switch}
        onchange={(event) => app.save((c) => (c.auto_switch = event.currentTarget.checked))}
      />
      <span>
        <strong>Auto-switch</strong>
        <small>Switch networks by itself. Also in the menu bar menu.</small>
      </span>
    </label>

    <label class="option">
      <input
        type="checkbox"
        checked={autostart ?? false}
        disabled={autostart === null}
        onchange={(event) => setAutostart(event.currentTarget.checked)}
      />
      <span>
        <strong>Open at login</strong>
        <small>Start with your computer.</small>
      </span>
    </label>

    <label class="option">
      <input
        type="checkbox"
        checked={config.show_signal_label}
        onchange={(event) => app.save((c) => (c.show_signal_label = event.currentTarget.checked))}
      />
      <span>
        <strong>Show signal next to the menu bar icon</strong>
        <small>For example <code>-58 · 24ms</code> (signal and ping).</small>
      </span>
    </label>

    <label class="option">
      <input
        type="checkbox"
        checked={config.notify_on_switch}
        disabled={asking}
        onchange={(event) => setNotify(event.currentTarget.checked)}
      />
      <span>
        <strong>Notify me when it switches</strong>
        <small>Says which network it moved to and why. Not for switches you make yourself.</small>
        {#if asking}
          <small class="note">macOS is asking whether to allow notifications: choose Allow.</small>
        {:else if config.notify_on_switch && permission === 'blocked'}
          <small class="warning">
            macOS isn't showing notifications from this app.
            <button type="button" class="link" onclick={openNotificationSettings}>Open notification settings</button>,
            turn on Allow notifications and choose Banners.
          </small>
        {:else if config.notify_on_switch && permission === 'unknown'}
          <small class:note={testSent}>
            {#if testSent}A test notification should appear now.{/if}
            Not seeing them?
            <button type="button" class="link" onclick={openNotificationSettings}>Open notification settings</button>
          </small>
        {:else if config.notify_on_switch && testSent}
          <small class="note">You should see a test notification now.</small>
        {/if}
      </span>
    </label>

    <div class="option disabled">
      <span>
        <strong>Language</strong>
        <small>English. Bahasa Indonesia is coming in a later version.</small>
      </span>
    </div>
  </section>

  {#if app.info}
    <p class="hint path">Settings file: <code>{app.info.config_path}</code></p>
  {/if}
{/if}

<style>
  .options {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
  }

  .option {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 10px 12px;
    cursor: pointer;
  }

  .option + .option {
    border-top: 1px solid var(--border);
  }

  .option input {
    margin-top: 2px;
  }

  .option span {
    display: flex;
    flex-direction: column;
  }

  .option small {
    color: var(--muted);
    font-size: 12px;
  }

  .option small.note {
    color: var(--text);
  }

  .option small.warning {
    color: var(--danger);
  }

  .option.disabled {
    cursor: default;
    opacity: 0.6;
  }

  .path {
    margin-top: 14px;
    word-break: break-all;
  }
</style>
