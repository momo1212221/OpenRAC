<script lang="ts">
  // One gameplay mod's own page, opened from its row in My mods (PluginsPanel.svelte, lib/modNav):
  // the way back (a back button and where you are), the mod's head (its name, version, author and
  // source, its switch, Update, and the rest in its menu), what stops it, a folded About with the
  // long description and its folder, then its presets and its settings in the sections its manifest
  // groups them in. Every change saves as it is made (lib/autosave.ts): the line by the presets says
  // Saving… and then Saved; a refused one says why. Escape or Back returns to the list. No folder
  // shows outside About, folded (players share screenshots of this page).
  // A built-in entry (Enhanced graphics, core/src/enhanced.rs: `builtIn`) has no folder, menu or
  // source; its page also says how far its lighting bake is, with the way to run it.
  import { onDestroy, onMount, tick } from "svelte";
  import {
    api,
    ask,
    type BakeStatus,
    type ModSources,
    type ModUpdate,
    type PluginPreset,
    type PluginView,
    type VersionView,
  } from "$lib/api";
  import { phaseLabel, readProgress } from "$lib/progress";
  import type { Job } from "$lib/queue";
  import { Saver, type SaveState } from "$lib/autosave";
  import { keybindConflicts, sharedBinds } from "$lib/keybinds";
  import { closeMod } from "$lib/modNav.svelte";
  import * as T from "$lib/modsText";
  import { modHint, modKind, settingGroups, sharedWith } from "$lib/plugins";
  import { isPreset, presetChanges } from "$lib/presets";
  import Icon from "./Icon.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";
  import ModSetting from "./ModSetting.svelte";
  import Reason from "./Reason.svelte";
  import TexturePacksBlock from "./TexturePacksBlock.svelte";

  let {
    mod,
    source = null,
    update = null,
    purist = false,
    developer = false,
    updating = false,
    onsave,
    onenable,
    onupdate,
    onpin,
    onfolder,
    onhomepage,
    onremove,
    builtIn = null,
  }: {
    mod: PluginView;
    source?: ModSources["mods"][string] | null;
    update?: ModUpdate | null;
    purist?: boolean;
    developer?: boolean;
    updating?: boolean;
    /** Saves one setting; rejects with the core's reason. */
    onsave: (key: string, value: unknown) => Promise<void>;
    onenable: (on: boolean) => void;
    onupdate: () => void;
    onpin: (pin: boolean) => void;
    onfolder: () => void;
    onhomepage: () => void;
    onremove: () => void;
    /** A built-in entry's bake: its status, the job running it, and the way to start it. */
    builtIn?: {
      bake: BakeStatus;
      job: Job | null;
      game: string;
      version: string;
      versionView: VersionView;
      onbake: () => void;
      ondelete: () => void;
      onpack: () => void;
    } | null;
  } = $props();

  const bakeProgress = $derived(builtIn?.job ? readProgress(builtIn.job.actionId, builtIn.job.lines) : null);

  type AiAssetMode = "faithful" | "balanced" | "enhanced";
  let textureMessage = $state("");
  let textureReload = $state(0);
  let aiBusy = $state(false);
  const aiPref = (key: string, fallback: string) => {
    if (typeof localStorage === "undefined") return fallback;
    return localStorage.getItem(`openrac.graphics.${key}`) ?? fallback;
  };
  const savedAiMode = aiPref("aiMode", "balanced");
  let aiMode = $state<AiAssetMode>(savedAiMode === "faithful" || savedAiMode === "enhanced" ? savedAiMode : "balanced");
  let aiPreserveMicrodetail = $state(aiPref("preserveMicrodetail", "1") !== "0");
  let aiPreserveStars = $state(aiPref("preserveStars", "1") !== "0");

  function rememberAi(key: string, value: string) {
    if (typeof localStorage !== "undefined") localStorage.setItem(`openrac.graphics.${key}`, value);
  }

  async function aiPreRender() {
    if (!builtIn || aiBusy) return;
    aiBusy = true;
    textureMessage = "AI asset pre-render is running…";
    try {
      const result = await api.aiTextureBake(builtIn.game, builtIn.version, {
        mode: aiMode,
        preserveMicrodetail: aiPreserveMicrodetail,
        preserveStars: aiPreserveStars,
      });
      await api.texturePacksSet(builtIn.game, builtIn.version, { enabled: true });
      textureReload++;

      let meshText = "";
      try {
        const meshes = await api.aiMeshBake(builtIn.game, builtIn.version, aiMode);
        meshText = meshes.available
          ? ` ${meshes.processed}/${meshes.discovered} render meshes were AI-refined; collision was left original.`
          : ` ${meshes.discovered} render meshes were found; this computer has no 3D AI backend yet, so geometry was left original.`;
      } catch (meshError) {
        meshText = ` Texture AI succeeded; 3D AI failed: ${String(meshError)}`;
      }

      textureMessage = `${result.processed} textures rendered with ${result.model}; preserved ${result.preservedStars} stars/tiny lights and ${result.preservedMicrodetails} microdetails.${meshText}`;
      builtIn.onpack();
    } catch (e) {
      textureMessage = String(e);
    } finally {
      aiBusy = false;
    }
  }

  // ---- Saving as the player goes ----

  /** What the saver holds and does, copied on each of its changes so the page draws them. */
  let held = $state<Record<string, unknown>>({});
  let saveState = $state<SaveState>("idle");
  let failure = $state<string | null>(null);
  /** Per setting, how many times it was saved: its row's "Saved" plays again on each. */
  let savedCount = $state<Record<string, number>>({});
  /** The last time the saver went quiet having saved: the head's "Saved" for a few seconds. */
  let lastKey = $state<string | null>(null);

  const saver = new Saver<unknown>({
    save: (key, value) => onsave(key, value),
    onchange: () => {
      held = saver.held();
      saveState = saver.state;
    },
    onsaved: (key) => {
      savedCount[key] = (savedCount[key] ?? 0) + 1;
      lastKey = key;
      failure = null;
    },
    onerror: (key, e) => {
      const name = mod.settings.find((s) => s.key === key)?.name ?? key;
      failure = T.withName(name, typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
    },
  });

  /** The settings as their controls show them: a value not saved yet, else the core's. */
  const shown = $derived(mod.settings.map((s) => (s.key in held ? { ...s, value: held[s.key] } : s)));
  const groups = $derived(settingGroups(shown));
  const clashes = $derived(keybindConflicts(shown));
  const presets = $derived(mod.presets ?? []);
  /** The buttons over the settings: the mod's presets, with Default first unless one is "default". */
  const presetButtons = $derived<(PluginPreset | null)[]>(
    presets.length ? (presets.some((p) => p.id === "default") ? presets : [null, ...presets]) : [],
  );
  const changed = $derived(shown.filter((s) => s.value !== s.default && String(s.value) !== String(s.default)));

  function enhancedParent(key: string): string | null {
    if (mod.id !== "enhanced-graphics") return null;
    if (key === "agx_exposure" || key === "agx_brightness") return "agx_preview";
    if (key === "bloom_strength" || key === "bloom_radius") return "bloom";
    if (
      [
        "gtao_quality",
        "gtao_radius",
        "gtao_strength",
        "gtao_slices",
        "gtao_steps",
        "gtao_half_resolution",
        "gtao_hi_z",
      ].includes(key)
    )
      return "gtao";
    if (["rim_light_strength", "rim_light_falloff", "rim_shadow_lift"].includes(key)) return "rim_light";
    if (
      [
        "motion_blur_strength",
        "motion_blur_radius",
        "motion_blur_samples",
        "motion_blur_min_motion",
        "motion_blur_spread",
        "motion_blur_depth_tolerance",
        "motion_blur_cutscene_multiplier",
      ].includes(key)
    )
      return "motion_blur";
    return null;
  }

  function enhancedChildOf(key: string): string | undefined {
    const parent = enhancedParent(key);
    return parent ? `${mod.id}-${parent}-name` : undefined;
  }

  function enhancedSettingDisabled(key: string): boolean {
    const parent = enhancedParent(key);
    return parent ? shown.find((setting) => setting.key === parent)?.value !== true : false;
  }

  function change(key: string, value: unknown, delay: number) {
    saver.set(key, value, delay);
  }

  function applyPreset(preset: PluginPreset | null) {
    for (const [key, value] of Object.entries(presetChanges(shown, presets, preset))) saver.set(key, value, 0);
  }

  async function resetAll() {
    if (!changed.length) return;
    if (!(await ask(T.MOD_PAGE.resetAllConfirm(mod.name), T.MOD_PAGE.resetAll))) return;
    for (const s of changed) saver.set(s.key, s.default, 0);
  }

  // ---- The page ----

  const kind = $derived(modKind(mod.kind));
  const shared = $derived(sharedWith(mod));
  const sourceLabel = (url: string) => url.replace(/^https?:\/\/(www\.)?github\.com\//, "");
  const cannotLoad = $derived(mod.enabled && !purist && (mod.state === "refused" || mod.state === "failed"));

  const menu = $derived<MenuItem[]>(
    builtIn
      ? []
      : [
          ...(source
            ? [
                {
                  label: source.pin ? T.PANEL.letUpdate : T.PANEL.keepVersion,
                  icon: "save" as const,
                  run: () => {
                    onpin(!source.pin);
                  },
                },
              ]
            : []),
          ...(mod.homepage ? [{ label: T.PANEL.homepage, icon: "external" as const, run: onhomepage }] : []),
          { label: T.PANEL.openFolder, icon: "folder" as const, run: onfolder },
          { label: T.PANEL.remove, icon: "trash" as const, danger: true, run: onremove },
        ],
  );

  let heading = $state<HTMLElement | null>(null);

  async function back() {
    await saver.flush();
    closeMod();
  }

  onMount(() => {
    void tick().then(() => heading?.focus({ preventScroll: true }));
    // Escape goes back, unless something on the page takes it first: an open menu (a popover), a
    // key cap waiting (it takes every key), a field being typed in (Escape leaves the field).
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (document.querySelector(":popover-open")) return;
      const at = e.target as HTMLElement | null;
      if (at && (at.tagName === "INPUT" || at.tagName === "SELECT" || at.tagName === "TEXTAREA")) {
        const input = at as HTMLInputElement;
        if (input.type !== "checkbox" && input.type !== "range") {
          input.blur();
          return;
        }
      }
      e.preventDefault();
      void back();
    };
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("keydown", key);
    };
  });

  onDestroy(() => {
    void saver.flush();
  });
</script>

<div class="mod-page">
  <nav class="crumbs" aria-label={T.MOD_PAGE.crumbsLabel}>
    <button class="back" aria-label={T.MOD_PAGE.back} title={T.MOD_PAGE.back} onclick={() => void back()}
      ><Icon name="arrow" size={18} /></button
    >
    <ol>
      <li><button class="crumb" onclick={() => void back()}>{T.MOD_PAGE.crumbRoot}</button></li>
      <li aria-current="page"><Icon name="next" size={14} /><span>{mod.name}</span></li>
    </ol>
  </nav>

  <section class="panel head" class:off={!mod.enabled}>
    <div class="title">
      <h2 bind:this={heading} tabindex="-1">{mod.name}</h2>
      <p class="meta">
        <span>{mod.version}</span>
        {#if mod.author}<span>{mod.author}</span>{/if}
        {#if source}<span
            >{T.fromSource(sourceLabel(source.url))}{#if source.pin}, {source.pinReason === "shared"
                ? T.pinnedShared
                : source.pinReason === "rollback"
                  ? T.pinnedRollback
                  : T.keptAt(source.version)}{/if}</span
          >{/if}
        <span class={`kind ${kind.tone}`} title={modHint(mod.kind)}><span class="dot"></span>{kind.text}</span>
      </p>
      {#if mod.description}<p class="desc">{mod.description}</p>{/if}
    </div>
    <div class="actions">
      {#if update && !source?.pin}
        <button disabled={updating} onclick={onupdate}
          >{#if updating}<span class="spinner"></span>{:else}<Icon name="refresh" size={16} />{/if}{T.MOD_PAGE.update(
            update.available,
          )}</button
        >
      {/if}
      <label class="power" class:on={mod.enabled}>
        <span>{mod.enabled ? T.MOD_PAGE.on : T.MOD_PAGE.off}</span>
        <input
          type="checkbox"
          role="switch"
          class="switch"
          aria-label={T.useAria(mod.name)}
          checked={mod.enabled}
          disabled={!mod.enabled && mod.blocked !== ""}
          onchange={(e) => {
            onenable(e.currentTarget.checked);
          }}
        />
      </label>
      {#if menu.length}<Menu label={T.MOD_PAGE.more(mod.name)} items={menu} />{/if}
    </div>

    {#if purist && mod.enabled}
      <p class="notice"><Icon name="info" size={16} /><span>{T.WITH_MODS.offNotice}</span></p>
    {/if}
    {#if cannotLoad}
      <div class="notice err"><Icon name="warn" size={16} /><Reason text={mod.reason} tone="err" /></div>
    {:else if !mod.enabled && mod.blocked}
      <div class="notice"><Icon name="info" size={16} /><Reason text={T.cannotSwitchOn(mod.blocked)} /></div>
    {:else if !mod.enabled && mod.settings.length}
      <p class="notice dim"><Icon name="info" size={16} /><span>{T.MOD_PAGE.offNote}</span></p>
    {/if}
    {#if builtIn}
      <p class="notice"><Icon name="info" size={16} /><span>{T.ENHANCED.vulkan}</span></p>
    {/if}
    {#if shared && mod.enabled}
      <p class="notice"><Icon name="info" size={16} /><span>{shared}</span></p>
    {/if}

    <details class="more about">
      <summary>{T.MOD_PAGE.about}</summary>
      <div class="about-body">
        {#if builtIn}<p class="long">{T.ENHANCED.about}</p>
        {:else if mod.description}<p class="long">{mod.description}</p>{/if}
        <dl>
          <dt>{T.MOD_PAGE.aboutKind}</dt>
          <dd>{kind.text}. {modHint(mod.kind)}</dd>
          {#if source}
            <dt>{T.PANEL.source}</dt>
            <dd>
              <span class="mono">{source.url}</span>{#if source.path}&nbsp;(<span class="mono">{source.path}/</span
                >){/if}{#if source.tag}, {source.tag}{/if}
            </dd>
          {/if}
          {#if mod.license}<dt>{T.PANEL.license}</dt>
            <dd>{mod.license}</dd>{/if}
          {#if !builtIn}
            <dt>{T.PANEL.folder}</dt>
            <dd class="folder">
              <span class="mono">{mod.path}</span>
              <button class="small" onclick={onfolder}><Icon name="folder" size={14} />{T.PANEL.openFolder}</button>
            </dd>
          {/if}
          {#if developer}
            <dt>{T.PANEL.id}</dt>
            <dd><code>{mod.id}</code></dd>
            {#if mod.hooks.length}<dt>{T.PANEL.hooks}</dt>
              <dd class="mono">{mod.hooks.join(", ")}</dd>{/if}
          {/if}
        </dl>
      </div>
    </details>
  </section>

  {#if builtIn}
    {@const bake = builtIn.bake}
    {@const job = builtIn.job}
    <section class="panel bake" aria-labelledby={`${mod.id}-bake`}>
      <div class="bake-text">
        <h3 id={`${mod.id}-bake`}>{T.ENHANCED.lightingTitle}</h3>
        <p class="muted" role="status">
          {#if job}
            {phaseLabel(job)}{bakeProgress?.percent != null
              ? `, ${Math.round(bakeProgress.percent)} %${bakeProgress.left ? `, ${bakeProgress.left}` : ""}`
              : "…"}
          {:else if bake.busy}
            {T.ENHANCED.busy(bake.ready, bake.levels)}
          {:else if !bake.available}
            {bake.noBakerHere ? T.ENHANCED.noBakerHere : bake.levels === 0 ? T.ENHANCED.noLevels : T.ENHANCED.noBaker}
          {:else if bake.ready >= bake.levels}
            {T.ENHANCED.ready(bake.levels)}
          {:else}
            {T.ENHANCED.some(bake.ready, bake.levels)}
            {#if bake.partial}{T.ENHANCED.partial(bake.partial)}{/if}
            {#if bake.failed}{T.ENHANCED.failed(bake.failed)}{/if}
          {/if}
        </p>
        {#if job !== null || bake.busy}
          <div
            class="track"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={job ? (bakeProgress?.percent ?? undefined) : undefined}
            aria-label={bake.busy && !job ? T.ENHANCED.busy(bake.ready, bake.levels) : undefined}
          >
            {#if job && bakeProgress?.percent != null}<i style:width={`${bakeProgress.percent}%`}></i>{:else}<i
                class="moving"
              ></i>{/if}
          </div>
        {:else if bake.ready < bake.levels && !bake.noBakerHere}
          <p class="dim small-print">{T.ENHANCED.fallback}</p>
        {/if}
      </div>
      {#if !job && !bake.busy && (bake.available || bake.ready + bake.partial + bake.failed > 0)}
        <div class="bake-actions">
          {#if bake.available}
            <button class:ghost={bake.ready >= bake.levels} onclick={builtIn.onbake}
              ><Icon name="refresh" size={16} />{bake.ready >= bake.levels
                ? T.ENHANCED.again
                : bake.partial
                  ? T.ENHANCED.goOn
                  : T.ENHANCED.prepare}</button
            >
          {/if}
          {#if bake.ready + bake.partial + bake.failed > 0}
            <!-- Under the main one, plainer: it takes away what took a while to make. -->
            <button class="ghost small danger" onclick={builtIn.ondelete}
              ><Icon name="trash" size={14} />{T.ENHANCED.remove}</button
            >
          {/if}
        </div>
      {/if}
    </section>
  {/if}

  {#if builtIn}
    <section class="panel texture-tools" aria-labelledby={`${mod.id}-textures`}>
      <div class="texture-head">
        <div>
          <h3 id={`${mod.id}-textures`}>AI asset enhancement</h3>
          <p class="muted">
            Neural enhancement for textures and render meshes, including terrain, scenery and models. Collision stays
            untouched. Everything is pre-rendered, so there is no runtime AI cost.
          </p>
        </div>
      </div>

      <div class="texture-ai-grid">
        <label class="ai-profile">
          <span>AI asset profile</span>
          <select
            value={aiMode}
            disabled={aiBusy}
            onchange={(e) => {
              aiMode = e.currentTarget.value as AiAssetMode;
              rememberAi("aiMode", aiMode);
            }}
          >
            <option value="faithful">Faithful · preserve original look</option>
            <option value="balanced">Balanced · add material detail</option>
            <option value="enhanced">Maximum detail · strongest reconstruction</option>
          </select>
        </label>
        <label>
          <span>Preserve original micro-detail</span>
          <input
            type="checkbox"
            role="switch"
            class="switch"
            checked={aiPreserveMicrodetail}
            disabled={aiBusy}
            onchange={(e) => {
              aiPreserveMicrodetail = e.currentTarget.checked;
              rememberAi("preserveMicrodetail", aiPreserveMicrodetail ? "1" : "0");
            }}
          />
        </label>
        <label>
          <span>Preserve stars & tiny lights</span>
          <input
            type="checkbox"
            role="switch"
            class="switch"
            checked={aiPreserveStars}
            disabled={aiBusy}
            onchange={(e) => {
              aiPreserveStars = e.currentTarget.checked;
              rememberAi("preserveStars", aiPreserveStars ? "1" : "0");
            }}
          />
        </label>
      </div>

      <div class="texture-actions">
        <button
          disabled={aiBusy || builtIn.game !== "rac1" || builtIn.version !== "pal"}
          onclick={() => void aiPreRender()}
        >
          {#if aiBusy}<span class="spinner"></span>{:else}<Icon name="refresh" size={16} />{/if}
          Build AI asset pack
        </button>
        <span class="muted small-print">
          Faithful stays closest to the original. Balanced is the recommended default. Maximum Detail allows the
          strongest texture and mesh reconstruction; rigged assets must retain their skinning.
        </span>
      </div>
      {#if textureMessage}<p class="texture-message" role="status">{textureMessage}</p>{/if}
    </section>
    <TexturePacksBlock version={builtIn.versionView} reload={textureReload} />
  {/if}

  {#if mod.settings.length}
    <div class="toolbar">
      {#if presetButtons.length}
        <div class="presets">
          <span class="label" id={`${mod.id}-presets`}>{T.MOD_PAGE.presets}</span>
          <div class="segmented big" role="radiogroup" aria-labelledby={`${mod.id}-presets`}>
            {#each presetButtons as p (p?.id ?? "")}
              <button
                role="radio"
                aria-checked={isPreset(shown, presets, p)}
                onclick={() => {
                  applyPreset(p);
                }}>{p?.name ?? T.KEYBIND.defaults}</button
              >
            {/each}
          </div>
        </div>
      {/if}
      <div class="status" class:failed={saveState === "failed"} role="status" aria-live="polite">
        {#if saveState === "waiting" || saveState === "saving"}
          <span class="spinner"></span>{T.SAVE.saving}
        {:else if saveState === "failed"}
          <Icon name="warn" size={15} />{T.SAVE.failed}
        {:else if saveState === "saved" && lastKey}
          <Icon name="check" size={15} />{T.SAVE.saved}
        {:else}
          <Icon name="check" size={15} />{T.SAVE.idle}
        {/if}
      </div>
      <button class="small reset-all" disabled={!changed.length} onclick={() => void resetAll()}
        ><Icon name="reset" size={14} />{T.MOD_PAGE.resetAll}</button
      >
    </div>
    {#if failure}<div class="failure"><Reason text={failure} tone="err" role="alert" /></div>{/if}

    {#each groups as g (g.id)}
      {#if g.id === "advanced"}
        <!-- Settings few need: folded, closed at first. -->
        <details class="panel group advanced more">
          <summary><h3 id={`${mod.id}-g-${g.id}`}>{T.ENHANCED.advanced}</h3></summary>
          <div class="rows">
            {#each g.settings as s (s.key)}
              <ModSetting
                setting={s}
                childOf={enhancedChildOf(s.key)}
                disabled={enhancedSettingDisabled(s.key)}
                id={`${mod.id}-${s.key}`}
                saved={savedCount[s.key] ?? 0}
                conflicts={clashes[s.key] ?? []}
                clashing={clashes[s.key] ? sharedBinds(shown, s.key) : []}
                onchange={(value, delay) => {
                  change(s.key, value, delay);
                }}
              />
            {/each}
          </div>
        </details>
      {:else}
        <section class="panel group" aria-labelledby={`${mod.id}-g-${g.id}`}>
          <h3 id={`${mod.id}-g-${g.id}`}>{g.title}</h3>
          <div class="rows">
            {#each g.settings as s (s.key)}
              <ModSetting
                setting={s}
                childOf={enhancedChildOf(s.key)}
                disabled={enhancedSettingDisabled(s.key)}
                id={`${mod.id}-${s.key}`}
                saved={savedCount[s.key] ?? 0}
                conflicts={clashes[s.key] ?? []}
                clashing={clashes[s.key] ? sharedBinds(shown, s.key) : []}
                onchange={(value, delay) => {
                  change(s.key, value, delay);
                }}
              />
            {/each}
          </div>
        </section>
      {/if}
    {/each}
  {:else}
    <p class="none">{T.MOD_PAGE.noSettings}</p>
  {/if}
</div>

<style>
  .mod-page {
    display: flex;
    flex-direction: column;
    gap: var(--page-gap, 16px);
  }

  /* Where you are: back, then My mods › the mod. */
  .crumbs {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 10px;
  }

  .crumbs ol {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 0.875rem;
    min-width: 0;
  }

  .crumbs li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .crumbs li[aria-current="page"] {
    color: var(--text);
    font-weight: 600;
  }

  .crumbs li[aria-current="page"] :global(svg) {
    color: var(--dim);
    flex: none;
  }

  .crumbs li span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .back {
    width: 40px;
    height: 40px;
    padding: 0;
    justify-content: center;
    flex: none;
  }

  .crumb {
    border: 0;
    background: none;
    padding: 6px 4px;
    border-radius: 6px;
    color: var(--amber);
    font-weight: 600;
  }

  @media (hover: hover) {
    :global(html:where(:not([data-input="pad"]))) .crumb:hover {
      text-decoration: underline;
      text-underline-offset: 3px;
    }
  }

  /* The head: the name and what it is on the left, its switch and actions on the right. */
  /* The name and what it is on the left, the switch and actions on the right, side by side; the
     notices and About under both, the panel's width. Plain flex, no grid or :has(). */
  .head {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 14px 24px;
  }

  .head > * {
    flex: 1 1 100%;
    min-width: 0;
  }

  .head > .title {
    flex: 1 1 0;
    min-width: 0;
  }

  .head > .actions {
    flex: 0 0 auto;
  }

  h2 {
    font-size: 1.375rem;
    line-height: 1.25;
    outline: none;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 14px;
    margin: 6px 0 0;
    font-size: 0.8125rem;
    color: var(--soft);
  }

  .desc {
    margin: 10px 0 0;
    max-width: 70ch;
    font-size: 0.875rem;
    color: var(--body);
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }

  .kind {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    color: var(--dim);
    cursor: help;
  }

  .kind .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
  }

  .kind.warn {
    color: var(--warn);
  }

  .kind.ok .dot {
    background: var(--ok);
  }

  .actions {
    display: flex;
    flex-direction: row;
    flex-wrap: nowrap;
    align-items: center;
    gap: 12px;
  }

  /* The switch with its word, a target as big as a button. */
  .power {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
    padding: 0 6px 0 14px;
    border: 1px solid var(--line-strong);
    border-radius: 999px;
    background: var(--ink-deep);
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--soft);
    cursor: pointer;
  }

  .power.on {
    color: var(--text);
    border-color: color-mix(in srgb, var(--brand) 60%, transparent);
  }

  .power input.switch {
    margin-right: 2px;
  }

  .power input:focus-visible {
    outline: 2px solid var(--amber);
    outline-offset: 2px;
  }

  .notice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin: 0;
    padding: 9px 12px;
    border-radius: var(--radius-sm);
    background: var(--panel-hi);
    font-size: 0.8125rem;
    color: var(--body);
  }

  .notice > :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--amber);
  }

  .notice.err > :global(svg) {
    color: var(--err);
  }

  .notice.dim {
    color: var(--soft);
  }

  .notice.dim > :global(svg) {
    color: var(--dim);
  }

  .notice > :global(.reason) {
    flex: 1;
  }

  .about {
    border-top: 1px solid var(--line);
    padding-top: 12px;
  }

  .about > summary {
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--soft);
    padding: 4px 0;
  }

  .about-body {
    padding-top: 10px;
  }

  .long {
    margin: 0 0 12px;
    max-width: 72ch;
    font-size: 0.875rem;
    color: var(--body);
  }

  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 8px 18px;
    margin: 0;
    font-size: 0.8125rem;
  }

  dt {
    color: var(--dim);
  }

  dd {
    margin: 0;
    color: var(--body);
    overflow-wrap: anywhere;
  }

  .folder {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
  }

  .mono {
    font-family: var(--font-mono);
    font-size: 0.75rem;
  }

  /* Presets on the left, the saving line and Reset to defaults on the right. */
  .toolbar {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 16px;
  }

  .presets {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin-right: auto;
  }

  .presets .label {
    font-size: 0.8125rem;
    font-weight: 600;
    color: var(--soft);
  }

  .segmented.big {
    flex-wrap: wrap;
  }

  .segmented.big button {
    min-height: 36px;
    padding: 6px 18px;
    font-size: 0.875rem;
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 0.8125rem;
    color: var(--dim);
    margin-left: auto;
  }

  .status :global(svg) {
    color: var(--ok);
  }

  .status.failed {
    color: var(--err);
  }

  .status.failed :global(svg) {
    color: var(--err);
  }

  .status .spinner {
    width: 13px;
    height: 13px;
  }

  .reset-all {
    min-height: 34px;
  }

  .failure {
    margin-top: -6px;
  }

  .group h3 {
    font-family: var(--font-sans);
    font-size: 0.8125rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--gold);
    margin-bottom: 2px;
  }

  .rows {
    display: flex;
    flex-direction: column;
  }

  .none {
    margin: 0;
    color: var(--soft);
    font-size: 0.875rem;
  }

  /* A built-in entry's lighting bake: what it is at, a slim bar while it runs, its button. */
  .bake {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 16px;
  }

  /* Prepare, and under it the plainer Delete. */
  .bake-actions {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
  }

  .bake-actions .danger {
    color: var(--err);
  }

  .bake-text {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .bake h3,
  .advanced h3 {
    font-family: var(--font-sans);
    font-size: 0.8125rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--gold);
  }

  .bake p {
    margin: 0;
    font-size: 0.875rem;
  }

  .small-print {
    font-size: 0.8125rem;
  }

  .texture-tools {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .texture-head,
  .texture-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
  }

  .texture-head h3 {
    margin: 0 0 4px;
    font-family: var(--font-sans);
    font-size: 0.8125rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--gold);
  }

  .texture-head p,
  .texture-message {
    margin: 0;
    font-size: 0.8125rem;
  }

  .texture-ai-grid > label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
  }

  .texture-ai-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.5fr) minmax(0, 1fr) minmax(0, 1fr);
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    background: var(--panel-hi);
  }

  .texture-ai-grid label {
    font-size: 0.78rem;
  }

  .texture-ai-grid .ai-profile {
    align-items: stretch;
    flex-direction: column;
    gap: 6px;
  }

  .texture-ai-grid select {
    width: 100%;
  }

  .texture-actions .small-print {
    max-width: 54ch;
    text-align: right;
  }

  .texture-message {
    color: var(--soft);
  }

  @media (max-width: 760px) {
    .texture-head,
    .texture-actions {
      align-items: stretch;
      flex-direction: column;
    }

    .texture-ai-grid {
      grid-template-columns: 1fr;
    }

    .texture-actions .small-print {
      text-align: left;
    }
  }

  /* Its heading where the other groups have theirs, the fold's chevron at the right (app.css). */
  .advanced > summary {
    min-height: 44px;
    margin: -10px 0;
  }

  .advanced[open] > summary {
    margin-bottom: 2px;
  }

  @media (max-width: 599px) {
    .head > .title {
      flex-basis: 100%;
    }
  }
</style>
