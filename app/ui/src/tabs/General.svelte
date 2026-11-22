<script lang="ts">
  import { app } from '../lib/store.svelte'

  let config = $derived(app.config)

  function openNotificationSettings(event: MouseEvent) {
    event.preventDefault() // don't toggle the checkbox this sits in
    app.openNotificationSettings()
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
        checked={app.autostart ?? false}
        disabled={app.autostart === null}
        onchange={(event) => app.setAutostart(event.currentTarget.checked)}
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
        disabled={app.askingNotifications}
        onchange={(event) => app.setNotify(event.currentTarget.checked)}
      />
      <span>
        <strong>Notify me when it switches</strong>
        <small>Says which network it moved to and why. Not for switches you make yourself.</small>
        {#if app.askingNotifications}
          <small class="note">macOS is asking whether to allow notifications: choose Allow.</small>
        {:else if config.notify_on_switch && app.notifications === 'blocked'}
          <small class="warning">
            macOS isn't showing notifications from this app.
            <button type="button" class="link" onclick={openNotificationSettings}>Open notification settings</button>,
            turn on Allow notifications and choose Banners.
          </small>
        {:else if config.notify_on_switch && app.notifications === 'unknown'}
          <small class:note={app.testNotificationSent}>
            {#if app.testNotificationSent}A test notification should appear now.{/if}
            Not seeing them?
            <button type="button" class="link" onclick={openNotificationSettings}>Open notification settings</button>
          </small>
        {:else if config.notify_on_switch && app.testNotificationSent}
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
