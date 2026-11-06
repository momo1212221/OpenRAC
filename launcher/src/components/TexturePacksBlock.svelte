<script lang="ts">
  // A game's texture pack (core/src/texture_packs.rs, docs/port/TEXTURE_PACKS.md), as three steps a
  // player follows: save the game's pictures while playing, make their own in any editor, and use
  // them as their pack, which they can also add from a .zip or share as one. Counts per folder
  // (Interface, Shared, each planet) open that folder. Hashes, paths and the environment the
  // game is started with stay out of sight: they are in the docs.
  import Icon from "./Icon.svelte";
  import {
    api,
    pickZip,
    type TextureFolder,
    type TexturePackImportMode,
    type TexturePackPreview,
    type TexturePackView,
    type VersionView,
  } from "$lib/api";
  import { toast } from "$lib/app.svelte";
  import { grouped } from "$lib/format";
  import { fold } from "$lib/motion";
  import { isPlaying } from "$lib/player.svelte";
  import { packSummary, previewSummary, textureCount } from "$lib/texturePacks";
  import { slide } from "svelte/transition";

  let { version }: { version: VersionView } = $props();

  let view = $state<TexturePackView | null>(null);
  let failed = $state<string | null>(null);
  let busy = $state<string | null>(null);
  /** A pack picked to add, waiting for the player's choice. */
  let adding = $state<{ source: string; preview: TexturePackPreview } | null>(null);

  /** The newest read wins: switching games while one is read drops the older answer. */
  let reading = 0;
  async function load() {
    const mine = ++reading;
    try {
      const read = await api.texturePacks(version.game, version.name);
      if (mine === reading) {
        view = read;
        failed = null;
      }
    } catch (e) {
      if (mine === reading) failed = String(e);
    }
  }

  // Read again when the page shows another game or disc, and when the game stops: it may have
  // saved textures.
  const playing = $derived(isPlaying(version));
  $effect(() => {
    if (version.key && !playing) {
      adding = null;
      void load();
    }
  });

  async function work(name: string, run: () => Promise<void>) {
    busy = name;
    try {
      await run();
    } catch (e) {
      toast(String(e), "error");
    } finally {
      busy = null;
    }
  }

  const set = (change: { enabled?: boolean; dumping?: boolean; ultra?: boolean }) =>
    work("set", async () => {
      view = await api.texturePacksSet(version.game, version.name, change);
      if (change.ultra !== undefined) {
        toast(
          change.ultra
            ? "Ultra textures will be prepared before Play and reused afterwards."
            : "Offline Ultra textures are off; the generated Ultra4K pack was removed.",
        );
      } else if (change.dumping !== undefined) {
        toast(
          change.dumping
            ? "The game saves its textures from your next Play."
            : "The game no longer saves its textures.",
        );
      } else {
        toast(
          change.enabled
            ? "Your texture pack is on from the next start."
            : "Your texture pack is off from the next start.",
        );
      }
    });

  const open = (dump: boolean, folder: TextureFolder | null = null) =>
    work("open", async () => {
      await api.texturePacksOpen(version.game, version.name, dump, folder?.path ?? "");
    });

  const pick = () =>
    work("pick", async () => {
      const source = await pickZip("Add a texture pack");
      if (!source) return;
      adding = { source, preview: await api.texturePacksPreview(version.game, version.name, source) };
    });

  const add = (mode: TexturePackImportMode) =>
    work(mode, async () => {
      if (!adding) return;
      const done = await api.texturePacksImport(version.game, version.name, adding.source, mode);
      adding = null;
      toast(
        done.kept
          ? `Added ${textureCount(done.textures)}. Your old pack is kept beside it, as “${done.kept}”.`
          : `Added ${textureCount(done.textures)} to your pack.`,
      );
      await load();
    });

  const prepareUltra = () =>
    work("ultra", async () => {
      const done = await api.texturePacksPrepareUltra(version.game, version.name);
      await load();
      if (!done.enabled) {
        toast("Turn Ultra textures before Play on first.", "error");
        return;
      }
      const mib = Math.round(done.bytes / (1024 * 1024));
      toast(
        done.cached
          ? `Ultra textures are already ready: ${grouped(done.textures)} textures (${grouped(mib)} MiB).`
          : `Prepared ${grouped(done.textures)} Ultra textures (${grouped(mib)} MiB).`,
      );
    });

  const share = () =>
    work("share", async () => {
      const done = await api.texturePacksExport(version.game, version.name);
      if (!done.saved) return;
      const left = done.leftOut ? ` ${grouped(done.leftOut)} unchanged picture(s) from the game were left out.` : "";
      toast(`Saved ${textureCount(done.textures)} as ${done.saved}.${left}`);
    });
</script>

<section class="panel textures" aria-labelledby="textures-title">
  <div class="row top">
    <h2 id="textures-title" class="grow">Texture packs</h2>
  </div>
  <p class="muted lead">Replace the game's pictures with your own: sharper, repainted or animated.</p>
  {#if failed}
    <p class="muted"><Icon name="warn" size={14} /> {failed}</p>
  {:else if view}
    <div class="ultra-card">
      <div class="ultra-copy">
        <h3>Ultra textures before Play</h3>
        <p class="muted">
          Pre-render known game textures once at 8×, up to 4K, save them as a permanent texture pack and reuse them.
          Runtime 2×/4×/8× upscaling stays off while you play.
        </p>
        {#if view.ultraReady}
          <p class="ultra-status">
            <Icon name="check" size={14} />
            {grouped(view.ultraTextures)} textures ready · {grouped(Math.round(view.ultraBytes / (1024 * 1024)))} MiB
          </p>
        {:else if view.ultraEnabled}
          <p class="ultra-status muted">Not prepared yet. It will be built before Play, or you can prepare it now.</p>
        {:else}
          <p class="ultra-status muted">Off. No generated Ultra4K pack is used.</p>
        {/if}
      </div>
      <div class="controls ultra-controls">
        <label class="toggle">
          <span>{view.ultraEnabled ? "On" : "Off"}</span>
          <input
            type="checkbox"
            class="switch"
            role="switch"
            aria-label="Pre-render Ultra textures before Play"
            checked={view.ultraEnabled}
            disabled={busy !== null}
            onchange={(e) => void set({ ultra: e.currentTarget.checked })}
          />
        </label>
        <button
          class="primary small"
          disabled={busy !== null || !view.ultraEnabled}
          onclick={() => void prepareUltra()}
        >
          {#if busy === "ultra"}<span class="spinner"></span>{/if}{view.ultraReady
            ? "Check / rebuild now"
            : "Prepare now"}
        </button>
      </div>
    </div>

    <ol class="steps">
      <li>
        <div>
          <h3>Save the game's textures while you play</h3>
          <p class="muted">
            Every picture the game shows is saved once, sorted into Interface, Shared and a folder for each planet. They
            come from your disc: keep them to yourself.
          </p>
        </div>
        <div class="controls">
          <label class="toggle">
            <span>{view.dumping ? "On" : "Off"}</span>
            <input
              type="checkbox"
              class="switch"
              role="switch"
              aria-label="Save the game's textures while you play"
              checked={view.dumping}
              disabled={busy !== null}
              onchange={(e) => void set({ dumping: e.currentTarget.checked })}
            />
          </label>
          <button class="ghost small" disabled={busy !== null} onclick={() => void open(true)}>
            <Icon name="folder" size={14} />Open saved textures
          </button>
        </div>
        {#if view.dumpCount}
          <div class="span counts" aria-label="Saved textures by folder">
            <span class="total">{textureCount(view.dumpCount)} saved</span>
            {#each view.dump as folder (folder.path)}
              <button class="count" title={`Open ${folder.label}`} onclick={() => void open(true, folder)}>
                {folder.label}<b>{grouped(folder.count)}</b>
              </button>
            {/each}
          </div>
        {/if}
      </li>
      <li>
        <div>
          <h3>Make your own pictures</h3>
          <p class="muted">
            Open a saved picture in any image editor, upscale or repaint it at any size, and keep its file name. Put it
            in your pack, in any folder.
          </p>
          <p class="muted">
            Animated: put numbered pictures (0001.png, 0002.png…) in a folder named like the saved picture, without
            .png.
          </p>
        </div>
        <div class="controls">
          <button class="ghost small" disabled={busy !== null} onclick={() => void open(false)}>
            <Icon name="folder" size={14} />Open your pack
          </button>
        </div>
      </li>
      <li>
        <div>
          <h3>Your texture pack</h3>
          <p class="muted">{packSummary(view)}</p>
        </div>
        <div class="controls">
          <label class="toggle">
            <span>{view.enabled ? "On" : "Off"}</span>
            <input
              type="checkbox"
              class="switch"
              role="switch"
              aria-label="Use your texture pack"
              checked={view.enabled}
              disabled={busy !== null}
              onchange={(e) => void set({ enabled: e.currentTarget.checked })}
            />
          </label>
          <button class="ghost small" disabled={busy !== null} onclick={() => void pick()}>
            {#if busy === "pick"}<span class="spinner"></span>{/if}Add a pack…
          </button>
          <button class="ghost small" disabled={busy !== null || !view.packCount} onclick={() => void share()}>
            {#if busy === "share"}<span class="spinner"></span>{/if}Share my pack…
          </button>
        </div>
        {#if view.packCount}
          <div class="span counts" aria-label="Your pack by folder">
            {#each view.pack as folder (folder.path)}
              <button class="count" title={`Open ${folder.label}`} onclick={() => void open(false, folder)}>
                {folder.label}<b>{grouped(folder.count)}</b>
              </button>
            {/each}
          </div>
        {/if}
        {#if adding}
          <div class="span adding" transition:slide={fold}>
            <p>{previewSummary(adding.preview)}</p>
            <div class="row choices">
              {#if adding.preview.current}
                <button class="primary small" disabled={busy !== null} onclick={() => void add("merge")}>
                  {#if busy === "merge"}<span class="spinner"></span>{/if}Add to my pack
                </button>
                <button class="ghost small" disabled={busy !== null} onclick={() => void add("replace")}>
                  {#if busy === "replace"}<span class="spinner"></span>{/if}Replace my pack
                </button>
              {:else}
                <button class="primary small" disabled={busy !== null} onclick={() => void add("replace")}>
                  {#if busy === "replace"}<span class="spinner"></span>{/if}Add it
                </button>
              {/if}
              <button class="ghost small" disabled={busy !== null} onclick={() => (adding = null)}>Cancel</button>
            </div>
          </div>
        {/if}
      </li>
    </ol>
  {/if}
</section>

<style>
  .textures {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .lead {
    margin: 0;
  }
  .ultra-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 14px 16px;
    margin: 8px 0 10px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--raised);
  }
  .ultra-copy {
    min-width: 0;
  }
  .ultra-copy h3 {
    margin: 0 0 4px;
  }
  .ultra-copy p {
    margin: 0;
  }
  .ultra-status {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 7px !important;
    font-size: 0.8125rem;
  }
  .ultra-controls {
    flex-shrink: 0;
  }
  @media (max-width: 760px) {
    .ultra-card {
      align-items: stretch;
      flex-direction: column;
    }
    .ultra-controls {
      justify-content: flex-start;
    }
  }
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 0.8125rem;
    color: var(--soft);
    cursor: pointer;
  }
  ol.steps .controls button {
    min-width: 0;
  }
  /* A folder's count: its name and number, opening it. */
  .counts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .total {
    font-size: 0.8125rem;
    color: var(--soft);
    margin-right: 4px;
  }
  .count {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    padding: 3px 10px;
    font-size: 0.8125rem;
    color: var(--lav);
    background: var(--panel-hi);
    border: 1px solid var(--line);
    border-radius: 999px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count b {
    color: var(--amber);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  @media (hover: hover) {
    :global(html:where(:not([data-input="pad"]))) .count:hover {
      border-color: var(--line-strong);
    }
  }
  .adding {
    padding: 12px 14px;
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    background: var(--panel-hi);
  }
  .adding p {
    margin: 0 0 10px;
  }
  .choices {
    flex-wrap: wrap;
    gap: 8px;
  }
</style>
