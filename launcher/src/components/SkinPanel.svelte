<script lang="ts">
  import Icon from "$components/Icon.svelte";
  import { inTauri, pickFolder, type ActionView, type VersionView, api } from "$lib/api";
  import { app, guardShown, jobs, runChain, toast } from "$lib/app.svelte";
  import { SKINS } from "$lib/modsText";
  import { hidePaths } from "$lib/paths";
  import type { Job } from "$lib/queue";
  import { toolFailure } from "$lib/toolErrors";
  import CommandBox from "./CommandBox.svelte";

  let { version: v, onimported }: { version: VersionView; onimported?: () => void } = $props();

  const developer = $derived(app.config?.developer ?? false);
  const scope = $derived({ kind: "version" as const, key: v.key });
  const action = (id: string): ActionView | null => v.actions.find((a) => a.id === id && a.thisPlatform) ?? null;
  const exporter = $derived(action("skin-export"));
  const editor = $derived(action("skin-editor"));
  const importer = $derived(action("skin-import"));
  const reader = $derived(action("port-levels"));
  let busy = $state<string | null>(null);
  let said = $state<{ text: string; failed: boolean } | null>(null);

  const levels = $derived(
    app.info?.standalone && app.config?.installDir
      ? `${app.config.installDir}/active/${v.game}/data/port/${v.name}`
      : `assets/port/${v.game}/${v.name}`,
  );

  function ending(id: string): { lines: Job["lines"]; ok: boolean; folder: string | null } | null {
    const job = jobs.list.filter((j) => j.scope.kind === "version" && j.actionId === id).at(-1);
    if (!job) return null;
    return { lines: job.lines, ok: job.state === "succeeded", folder: job.outputs[0] ?? null };
  }

  async function exportSkin() {
    if (!exporter) return;
    busy = "export";
    const ok = await runChain(scope, [exporter]);
    busy = null;
    const end = ending("skin-export");
    if (ok && end?.folder) {
      toast(SKINS.exported);
      await guardShown(api.openOutput(end.folder));
    } else if (!ok) {
      toast(SKINS.exportStopped, "error");
    }
  }

  async function readLevels() {
    if (!reader) return;
    busy = "read";
    const ok = await runChain(scope, [reader]);
    busy = null;
    if (!ok) toast(SKINS.readStopped, "error");
  }

  async function openEditor() {
    if (!editor) return;
    busy = "editor";
    const ok = await runChain(scope, [editor]);
    busy = null;
    if (!ok) toast(SKINS.editorStopped, "error");
  }

  async function importSkin() {
    if (!importer) return;
    const folder = inTauri ? await pickFolder(SKINS.pickImport) : "/home/you/exports/ratchet-skin";
    if (!folder) return;
    busy = "import";
    const ok = await runChain(scope, [importer], { path: folder });
    busy = null;
    const end = ending("skin-import");
    if (ok) {
      said = { text: SKINS.imported, failed: false };
      onimported?.();
    } else {
      const failure = toolFailure(end?.lines ?? []);
      said = { text: `${failure.title}. ${failure.line}`, failed: true };
    }
  }
</script>

{#if exporter}
  <div class="skin-workbench">
    <div class="head">
      <div>
        <p class="eyebrow">{SKINS.badge}</p>
        <h3>{SKINS.title}</h3>
        <p class="muted">{SKINS.lead}</p>
      </div>
      <button class="ghost" onclick={() => void guardShown(api.openPath("editor/SKINS.md"))}>
        <Icon name="book" size={15} />{SKINS.guide}
      </button>
    </div>

    <div class="steps">
      <div class="step">
        <strong>1. {SKINS.exportTitle}</strong>
        <span class="muted">{exporter.runnable ? SKINS.exportLine : SKINS.readFirst}</span>
        {#if !exporter.runnable && reader}
          <button disabled={!reader.runnable || busy !== null} onclick={() => void readLevels()}>
            {#if busy === "read"}<span class="spinner"></span>{:else}<Icon name="build" size={16} />{/if}{SKINS.read}
          </button>
        {:else}
          <button disabled={!exporter.runnable || busy !== null} onclick={() => void exportSkin()}>
            {#if busy === "export"}<span class="spinner"></span>{:else}<Icon name="external" size={16} />{/if}{SKINS.export}
          </button>
        {/if}
      </div>

      <div class="step">
        <strong>2. {SKINS.editTitle}</strong>
        <span class="muted">{SKINS.editLine}</span>
        <button disabled={!editor?.runnable || busy !== null} onclick={() => void openEditor()}>
          {#if busy === "editor"}<span class="spinner"></span>{:else}<Icon name="edit" size={16} />{/if}{SINS.edit}
        </button>
      </div>

      <div class="step">
        <strong>3. {SKINS.importTitle}</strong>
        <span class="muted" class:failed={said?.failed} class:done={said && !said.failed}>
          {said ? hidePaths(said.text).text : SKINS.importLine}
        </span>
        <button disabled={!importer?.runnable || busy !== null} onclick={() => void importSkin()}>
          {#if busy === "import"}<span class="spinner"></span>{:else}<Icon name="save" size={16} />{/if}{SKINS.import}
        </button>
      </div>
    </div>

    {#if developer}
      <CommandBox
        lines={[
          `python3 editor/skins.py export ${levels} <skin-workspace>`,
          `python3 editor/skins.py editor`,
          `python3 editor/skins.py pack <skin-workspace> --levels ${levels} --into <data>/overrides`,
        ]}
      />
    {/if}
  </div>
{/if}

<style>
  .skin-workbench {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    margin-bottom: 16px;
    border-radius: var(--radius-sm);
    background: var(--card-hi);
  }
  .head {
    display: flex;
    gap: 16px;
    align-items: flex-start;
    justify-content: space-between;
  }
  .head h3,
  .head p { margin: 0; }
  .eyebrow {
    color: var(--amber);
    font-size: 0.75rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .steps {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr)i);
    gap: 10px;
  }
  .step {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: var(--radius-sm);
    background: var(--card);
  }
  .step .muted { min-height: 3em; }
  .failed { color: var(--err); }
  .done { color: var(--ok); }
  @media (max-width: 760px) {
    .steps { grid-template-columns: 1fr; }
    .head { flex-direction: column; }
  }
</style>
