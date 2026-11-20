<script lang="ts">
  import { app } from '../lib/store.svelte'

  let config = $derived(app.config)
  // The slider shows its value while dragging; it's saved on release.
  let weak = $state(-75)
  $effect(() => {
    if (config) weak = config.weak_signal_dbm
  })
  let targets = $derived(config?.ping_targets.join(', ') ?? '')

  function saveTargets(text: string) {
    const list = text
      .split(/[\s,]+/)
      .map((target) => target.trim())
      .filter(Boolean)
    app.save((config) => {
      config.ping_targets = list
    })
  }
</script>

{#if config}
  <section>
    <h2>Weak signal</h2>
    <p class="hint">
      Switch when the signal stays below <strong>{weak} dBm</strong> for 15 seconds and another of your
      networks is at least 8 dB stronger.
    </p>
    <input
      class="slider"
      type="range"
      min="-80"
      max="-65"
      step="1"
      bind:value={weak}
      onchange={() => app.save((config) => (config.weak_signal_dbm = Number(weak)))}
      aria-label="Weak signal level in dBm"
    />
    <div class="scale" aria-hidden="true">
      <span>-80 · switch later</span>
      <!-- -75 sits a third of the way along the -80…-65 slider. -->
      <span class="default">-75 · default</span>
      <span>-65 · switch sooner</span>
    </div>
  </section>

  <section>
    <h2>Internet quality</h2>
    <p class="hint">
      Coming in v0.5: switch when the internet is bad even though the signal is strong. These limits are
      saved now and used then.
    </p>
    <div class="grid">
      <label for="ping">Bad ping above</label>
      <span class="field">
        <input
          id="ping"
          type="number"
          min="50"
          max="2000"
          step="10"
          value={config.bad_ping_ms}
          onchange={(event) => app.save((c) => (c.bad_ping_ms = Number(event.currentTarget.value)))}
        />
        ms
      </span>
      <label for="loss">Bad packet loss above</label>
      <span class="field">
        <input
          id="loss"
          type="number"
          min="1"
          max="100"
          value={config.bad_loss_pct}
          onchange={(event) => app.save((c) => (c.bad_loss_pct = Number(event.currentTarget.value)))}
        />
        %
      </span>
      <label for="targets">Ping targets</label>
      <input
        id="targets"
        type="text"
        value={targets}
        onchange={(event) => saveTargets(event.currentTarget.value)}
        spellcheck="false"
      />
    </div>
    <p class="hint">Internet counts as bad only when both targets are slow, so one server's outage never triggers a switch.</p>
  </section>
{/if}

<style>
  section + section {
    margin-top: 24px;
  }

  .slider {
    width: 100%;
    accent-color: var(--accent);
  }

  .scale {
    position: relative;
    display: flex;
    justify-content: space-between;
    color: var(--muted);
    font-size: 11px;
  }

  .scale .default {
    position: absolute;
    left: 33.33%;
    transform: translateX(-50%);
  }

  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 8px 12px;
    margin-bottom: 8px;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }

  .field input {
    width: 80px;
  }
</style>
