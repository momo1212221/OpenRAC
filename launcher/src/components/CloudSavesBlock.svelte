<script lang="ts">
  import { api, ask, onAndroid, pickFolder, type CloudSaveStatus, type CloudSyncStrategy } from "$lib/api";
  import { app, toast } from "$lib/app.svelte";

  let { serial, onrestored }: { serial: string; onrestored?: () => void } = $props();

  let loading = $state(true);
  let working = $state(false);
  let status = $state<CloudSaveStatus | null>(null);
  let loadedSerial = "";

  const stateText = $derived.by(() => {
    if (!status?.enabled) return "Off";
    switch (status.state) {
      case "empty":
        return "No saves yet";
      case "synced":
        return "Up to date";
      case "local-changes":
        return "This device has changes";
      case "cloud-changes":
        return "Cloud copy has changes";
      case "conflict":
        return status.heads.length > 1 ? "Several cloud branches conflict" : "Both copies changed";
      default:
        return status.state;
    }
  });

  function shortDevice(name: string): string {
    return name.length > 36 ? `${name.slice(0, 33)}…` : name;
  }

  function stamp(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  async function load() {
    const mine = serial;
    loading = true;
    try {
      const read = await api.cloudSavesStatus(serial);
      if (mine === serial) {
        status = read;
        loadedSerial = serial;
      }
    } catch (e) {
      toast(`Could not read cloud saves: ${String(e)}`, "error");
    } finally {
      if (mine === serial) loading = false;
    }
  }

  async function sync(strategy: CloudSyncStrategy = "auto", announce = true) {
    if (working) return;
    working = true;
    try {
      const result = await api.cloudSavesSync(serial, strategy);
      status = result.status;
      if (result.action === "downloaded") onrestored?.();
      if (announce || result.action === "conflict") {
        toast(result.message, result.action === "conflict" ? "error" : "info");
      }
    } catch (e) {
      toast(`Cloud save sync failed: ${String(e)}`, "error");
      await load();
    } finally {
      working = false;
    }
  }

  async function chooseFolder() {
    if (working) return;
    const folder = await pickFolder("Choose a folder that your cloud service already syncs").catch((e: unknown) => {
      toast(`Could not choose a folder: ${String(e)}`, "error");
      return null;
    });
    if (!folder) return;
    working = true;
    try {
      await api.cloudSavesSetFolder(folder);
    } catch (e) {
      toast(`Could not use that cloud folder: ${String(e)}`, "error");
      working = false;
      return;
    }
    working = false;
    await sync("auto", true);
  }

  async function disable() {
    if (working || !status?.enabled) return;
    if (
      !(await ask(
        "Turn Cloud Saves off on this device? The snapshots already in your cloud folder are kept.",
        "Turn off",
      ))
    )
      return;
    working = true;
    try {
      await api.cloudSavesSetFolder(null);
      await load();
    } catch (e) {
      toast(`Could not turn Cloud Saves off: ${String(e)}`, "error");
    } finally {
      working = false;
    }
  }

  async function keepThisDevice() {
    if (
      !(await ask(
        "Keep this device's saves? OpenRAC will add them as a new snapshot. Existing cloud snapshots are kept.",
        "Keep this device",
      ))
    )
      return;
    await sync("use-local");
  }

  async function useCloud() {
    if (status && status.heads.length > 1) {
      toast(
        "The cloud contains more than one conflicting branch. Keep this device to resolve them without deleting history.",
        "error",
      );
      return;
    }
    if (
      !(await ask(
        "Use the cloud copy? OpenRAC backs up this device's current memory card first, then restores the cloud snapshot.",
        "Use cloud copy",
      ))
    )
      return;
    await sync("use-cloud");
  }

  $effect(() => {
    // The status is cheap and read-only apart from a temporary verified export. Read again after
    // the game, an import or an edit changes the memory card, but do not auto-resolve here: the
    // global play flow syncs immediately before and after a game.
    if (app.savesChanged >= 0 || loadedSerial !== serial) void load();
  });
</script>

<section class="cloud-saves" aria-label="Cloud saves">
  <div class="cloud-title">
    <div>
      <strong>Cloud saves</strong>
      <p>
        Uses a folder you own. Put it in OneDrive, Dropbox, iCloud Drive, Google Drive, Syncthing or a NAS; OpenRAC runs
        no storage server.
      </p>
    </div>
    {#if status?.enabled}
      <span class:problem={status.conflict} class="state">{stateText}</span>
    {/if}
  </div>

  {#if onAndroid}
    <p class="muted">Folder-backed Cloud Saves are available on desktop in V1.</p>
  {:else if loading && !status}
    <p class="muted">Checking cloud saves…</p>
  {:else if !status?.enabled}
    <div class="actions">
      <button class="small" disabled={working} onclick={() => void chooseFolder()}>Choose synced folder</button>
    </div>
  {:else}
    <div class="cloud-details">
      <div>
        <span class="label">Folder</span>
        <span class="path" title={status.folder ?? ""}>{status.folder}</span>
      </div>
      <div>
        <span class="label">History</span>
        <span>{status.snapshots} snapshot{status.snapshots === 1 ? "" : "s"}</span>
      </div>
      {#if status.latest}
        <div>
          <span class="label">Newest</span>
          <span title={status.latest.device}>{stamp(status.latest.created)} · {shortDevice(status.latest.device)}</span>
        </div>
      {/if}
    </div>

    {#if status.conflict}
      <div class="conflict" role="alert">
        <strong>Nothing was overwritten.</strong>
        {#if status.heads.length > 1}
          <p>
            Several devices created different cloud branches. Keeping this device creates one new snapshot that
            preserves the existing branches as history.
          </p>
        {:else}
          <p>This device and the cloud copy both changed since the last sync. Choose which save should continue.</p>
        {/if}
        <div class="actions">
          <button class="small" disabled={working} onclick={() => void keepThisDevice()}>Keep this device</button>
          <button class="small" disabled={working || status.heads.length !== 1} onclick={() => void useCloud()}>
            Use cloud copy
          </button>
        </div>
      </div>
    {:else}
      <div class="actions">
        <button class="small" disabled={working} onclick={() => void sync()}>{working ? "Syncing…" : "Sync now"}</button
        >
        <button class="small ghost" disabled={working} onclick={() => void chooseFolder()}>Change folder</button>
        <button class="small ghost" disabled={working} onclick={() => void disable()}>Turn off</button>
      </div>
    {/if}
  {/if}
</section>

<style>
  .cloud-saves {
    margin-top: 1rem;
    padding: 0.9rem 1rem;
    border: 1px solid var(--line, color-mix(in srgb, currentColor 18%, transparent));
    border-radius: 0.7rem;
  }
  .cloud-title {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
  }
  .cloud-title p,
  .conflict p {
    margin: 0.25rem 0 0;
    max-width: 54rem;
    opacity: 0.72;
  }
  .state {
    flex: none;
    font-size: 0.82rem;
    opacity: 0.78;
  }
  .state.problem {
    opacity: 1;
    font-weight: 650;
  }
  .cloud-details {
    display: grid;
    gap: 0.35rem;
    margin-top: 0.8rem;
    font-size: 0.9rem;
  }
  .cloud-details > div {
    display: grid;
    grid-template-columns: 5.5rem minmax(0, 1fr);
    gap: 0.7rem;
  }
  .label {
    opacity: 0.62;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 0.8rem;
  }
  .conflict {
    margin-top: 0.8rem;
    padding: 0.75rem;
    border-radius: 0.55rem;
    background: color-mix(in srgb, currentColor 7%, transparent);
  }
  @media (max-width: 560px) {
    .cloud-title {
      display: block;
    }
    .state {
      display: inline-block;
      margin-top: 0.45rem;
    }
  }
</style>
