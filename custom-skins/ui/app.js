(() => {
  "use strict";

  const PROTOCOL = 1;
  const enc = new TextEncoder();
  const dec = new TextDecoder();
  let requestId = 1;
  const pending = new Map();

  window.addEventListener("message", (event) => {
    if (event.source !== window.parent) return;
    const msg = event.data;
    if (!msg || msg.openrac !== PROTOCOL || typeof msg.id !== "number" || !pending.has(msg.id)) return;
    const task = pending.get(msg.id);
    pending.delete(msg.id);
    if (msg.ok) task.resolve(msg.value);
    else task.reject(new Error(String(msg.error || "OpenRAC tool call failed")));
  });

  function api(call, args = {}, transfer = []) {
    return new Promise((resolve, reject) => {
      const id = requestId++;
      pending.set(id, { resolve, reject });
      window.parent.postMessage({ openrac: PROTOCOL, id, call, args }, "*", transfer);
    });
  }

  const $ = (id) => document.getElementById(id);
  const canvas = $("canvas");
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  const textureSelect = $("texture");
  const status = $("status");
  const undo = $("undo");
  const reset = $("reset");
  const build = $("build");
  const exportSource = $("exportSource");
  const color = $("color");
  const size = $("size");
  const opacity = $("opacity");
  const tint = $("tint");

  const state = {
    info: null,
    levelsFormat: null,
    groups: [],
    models: [],
    current: -1,
    history: [],
    mode: "brush",
    down: false,
    last: null,
  };

  function say(text, bad = false) {
    status.textContent = text;
    status.classList.toggle("bad", bad);
  }

  function bytes(value) {
    return value instanceof Uint8Array ? value : new Uint8Array(value);
  }

  async function read(root, path) {
    return bytes(await api("read", { root, path }));
  }

  async function readJson(root, path) {
    return JSON.parse(dec.decode(await read(root, path)));
  }

  async function sha256(data) {
    const digest = await crypto.subtle.digest("SHA-256", data);
    return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
  }

  function pngSize(data) {
    const b = bytes(data);
    if (
      b.length < 24 ||
      b[0] !== 137 || b[1] !== 80 || b[2] !== 78 || b[3] !== 71 ||
      b[12] !== 73 || b[13] !== 72 || b[14] !== 68 || b[15] !== 82
    ) throw new Error("The texture is not a PNG.");
    const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
    return [v.getUint32(16, false), v.getUint32(20, false)];
  }

  function glbJson(data) {
    const b = bytes(data);
    if (b.length < 20) throw new Error("Ratchet model is too short to be a GLB.");
    const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
    if (v.getUint32(0, true) !== 0x46546c67 || v.getUint32(4, true) !== 2) {
      throw new Error("Ratchet model is not glTF 2.0 binary.");
    }
    let at = 12;
    while (at + 8 <= b.length) {
      const n = v.getUint32(at, true);
      const kind = v.getUint32(at + 4, true);
      if (at + 8 + n > b.length) break;
      if (kind === 0x4e4f534a) {
        return JSON.parse(dec.decode(b.subarray(at + 8, at + 8 + n)).trim());
      }
      at += 8 + n + ((4 - (n % 4)) % 4);
    }
    throw new Error("Ratchet model has no glTF JSON chunk.");
  }

  function textureNames(doc) {
    const out = [];
    for (const image of doc.images || []) {
      const uri = typeof image.uri === "string" ? image.uri : "";
      const found = uri.match(/(?:^|\/)textures\/([A-Za-z0-9_.-]+)\.png$/);
      if (found && !out.includes(found[1])) out.push(found[1]);
    }
    return out;
  }

  function levelDir(path) {
    const found = path.match(/^(level_\d{2})\/manifest\.json$/);
    return found ? found[1] : null;
  }

  async function scan() {
    $("scan").disabled = true;
    build.disabled = true;
    exportSource.disabled = true;
    say("Reading prepared OpenRAC levels…");
    try {
      const info = await api("info");
      const listed = await api("list", { root: "levels" });
      const manifestPaths = listed.map((f) => f.path).filter((p) => /^level_\d{2}\/manifest\.json$/.test(p));
      if (!manifestPaths.length) throw new Error("No prepared levels are available. Use the game's set-up/read-levels action first.");

      const byHash = new Map();
      const models = [];
      let levelsFormat = null;

      for (const manifestPath of manifestPaths) {
        const dir = levelDir(manifestPath);
        const manifest = await readJson("levels", manifestPath);
        levelsFormat ??= Number.isInteger(manifest.format) ? manifest.format : null;
        const ratchet = manifest?.classes?.moby?.["0"];
        if (!ratchet || typeof ratchet.mesh !== "string" || !ratchet.mesh) continue;

        const modelPath = `${dir}/${ratchet.mesh}`;
        const modelBytes = await read("levels", modelPath);
        const names = textureNames(glbJson(modelBytes));
        models.push({ level: dir, path: modelPath, name: ratchet.name || "ratchet" });

        for (let slot = 0; slot < names.length; slot++) {
          const name = names[slot];
          const path = `${dir}/textures/${name}.png`;
          const original = await read("levels", path);
          const hash = await sha256(original);
          const [width, height] = pngSize(original);
          let group = byHash.get(hash);
          if (!group) {
            group = {
              hash,
              original,
              width,
              height,
              edited: null,
              places: [],
            };
            byHash.set(hash, group);
          }
          group.places.push({ level: dir, name, slot, count: names.length });
        }
      }

      const groups = [...byHash.values()];
      if (!groups.length) {
        throw new Error("Ratchet class 0 was found, but its model referenced no readable PNG textures.");
      }

      state.info = info;
      state.levelsFormat = levelsFormat || 4;
      state.groups = groups;
      state.models = models;
      state.current = -1;
      state.history = [];

      textureSelect.innerHTML = "";
      groups.forEach((group, i) => {
        const option = document.createElement("option");
        option.value = String(i);
        const places = group.places.length;
        option.textContent = `Skin ${i + 1} — ${group.width}×${group.height} — ${places} level texture${places === 1 ? "" : "s"}`;
        textureSelect.append(option);
      });
      textureSelect.disabled = false;
      build.disabled = false;
      exportSource.disabled = false;
      await showGroup(0, false);
      say(`Loaded ${groups.length} unique Ratchet texture${groups.length === 1 ? "" : "s"} from ${models.length} level model${models.length === 1 ? "" : "s"} (${info.game}/${info.version}).`);
    } catch (err) {
      say(err instanceof Error ? err.message : String(err), true);
    } finally {
      $("scan").disabled = false;
    }
  }

  function snapshot() {
    if (!canvas.width || !canvas.height) return;
    state.history.push(ctx.getImageData(0, 0, canvas.width, canvas.height));
    if (state.history.length > 30) state.history.shift();
    undo.disabled = state.history.length === 0;
  }

  async function imageFrom(data) {
    const url = URL.createObjectURL(new Blob([data], { type: "image/png" }));
    try {
      const img = new Image();
      await new Promise((resolve, reject) => {
        img.onload = resolve;
        img.onerror = () => reject(new Error("Could not decode the PNG."));
        img.src = url;
      });
      return img;
    } finally {
      setTimeout(() => URL.revokeObjectURL(url), 0);
    }
  }

  async function canvasPng() {
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!blob) throw new Error("Could not encode the edited PNG.");
    return new Uint8Array(await blob.arrayBuffer());
  }

  async function saveCurrent() {
    if (state.current < 0) return;
    const group = state.groups[state.current];
    const data = await canvasPng();
    group.edited = (await sha256(data)) === group.hash ? null : data;
  }

  async function showGroup(index, save = true) {
    if (save) await saveCurrent();
    const group = state.groups[index];
    if (!group) return;
    state.current = index;
    state.history = [];
    undo.disabled = true;
    reset.disabled = false;
    const source = group.edited || group.original;
    const img = await imageFrom(source);
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(img, 0, 0);
    textureSelect.value = String(index);
  }

  function point(event) {
    const r = canvas.getBoundingClientRect();
    return {
      x: (event.clientX - r.left) * canvas.width / r.width,
      y: (event.clientY - r.top) * canvas.height / r.height,
    };
  }

  function stroke(a, b) {
    ctx.save();
    ctx.globalAlpha = Number(opacity.value) / 100;
    ctx.globalCompositeOperation = state.mode === "erase" ? "destination-out" : "source-over";
    ctx.strokeStyle = color.value;
    ctx.lineWidth = Number(size.value);
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.stroke();
    ctx.restore();
  }

  canvas.addEventListener("pointerdown", (event) => {
    if (state.current < 0) return;
    snapshot();
    state.down = true;
    state.last = point(event);
    canvas.setPointerCapture(event.pointerId);
    stroke(state.last, { x: state.last.x + 0.01, y: state.last.y + 0.01 });
  });
  canvas.addEventListener("pointermove", (event) => {
    if (!state.down || !state.last) return;
    const next = point(event);
    stroke(state.last, next);
    state.last = next;
  });
  const endStroke = () => { state.down = false; state.last = null; };
  canvas.addEventListener("pointerup", endStroke);
  canvas.addEventListener("pointercancel", endStroke);

  $("brush").onclick = () => {
    state.mode = "brush";
    $("brush").classList.add("active");
    $("erase").classList.remove("active");
  };
  $("erase").onclick = () => {
    state.mode = "erase";
    $("erase").classList.add("active");
    $("brush").classList.remove("active");
  };

  size.oninput = () => $("sizeOut").textContent = `${size.value} px`;
  opacity.oninput = () => $("opacityOut").textContent = `${opacity.value}%`;
  tint.oninput = () => $("tintOut").textContent = `${tint.value}%`;

  undo.onclick = () => {
    const previous = state.history.pop();
    if (previous) ctx.putImageData(previous, 0, 0);
    undo.disabled = state.history.length === 0;
  };

  reset.onclick = async () => {
    const group = state.groups[state.current];
    if (!group) return;
    snapshot();
    group.edited = null;
    const img = await imageFrom(group.original);
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    ctx.drawImage(img, 0, 0);
  };

  $("applyTint").onclick = () => {
    if (state.current < 0) return;
    snapshot();
    const image = ctx.getImageData(0, 0, canvas.width, canvas.height);
    const mix = Number(tint.value) / 100;
    const hex = color.value;
    const tr = parseInt(hex.slice(1, 3), 16);
    const tg = parseInt(hex.slice(3, 5), 16);
    const tb = parseInt(hex.slice(5, 7), 16);
    for (let i = 0; i < image.data.length; i += 4) {
      image.data[i] = Math.round(image.data[i] * (1 - mix) + tr * mix);
      image.data[i + 1] = Math.round(image.data[i + 1] * (1 - mix) + tg * mix);
      image.data[i + 2] = Math.round(image.data[i + 2] * (1 - mix) + tb * mix);
    }
    ctx.putImageData(image, 0, 0);
  };

  $("gray").onclick = () => {
    if (state.current < 0) return;
    snapshot();
    const image = ctx.getImageData(0, 0, canvas.width, canvas.height);
    for (let i = 0; i < image.data.length; i += 4) {
      const y = Math.round(image.data[i] * 0.2126 + image.data[i + 1] * 0.7152 + image.data[i + 2] * 0.0722);
      image.data[i] = image.data[i + 1] = image.data[i + 2] = y;
    }
    ctx.putImageData(image, 0, 0);
  };

  $("replace").onchange = async () => {
    const file = $("replace").files?.[0];
    const group = state.groups[state.current];
    if (!file || !group) return;
    try {
      const data = new Uint8Array(await file.arrayBuffer());
      const img = await imageFrom(data);
      if (Math.abs(img.naturalWidth / img.naturalHeight - group.width / group.height) > 1e-6) {
        throw new Error(`Keep the original aspect ratio ${group.width}:${group.height}.`);
      }
      snapshot();
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;
      ctx.drawImage(img, 0, 0);
      say(`Loaded ${file.name} for this Ratchet texture.`);
    } catch (err) {
      say(err instanceof Error ? err.message : String(err), true);
    } finally {
      $("replace").value = "";
    }
  };

  textureSelect.onchange = () => {
    void showGroup(Number(textureSelect.value)).catch((err) => say(String(err), true));
  };

  function crcTable() {
    const table = new Uint32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = (c & 1) ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      table[n] = c >>> 0;
    }
    return table;
  }
  const CRC = crcTable();

  function crc32(data) {
    let c = 0xffffffff;
    for (const b of data) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  }

  function concat(parts) {
    const size = parts.reduce((n, p) => n + p.length, 0);
    const out = new Uint8Array(size);
    let at = 0;
    for (const part of parts) { out.set(part, at); at += part.length; }
    return out;
  }

  function u16(n) {
    const a = new Uint8Array(2);
    new DataView(a.buffer).setUint16(0, n, true);
    return a;
  }
  function u32(n) {
    const a = new Uint8Array(4);
    new DataView(a.buffer).setUint32(0, n >>> 0, true);
    return a;
  }

  function dosNow() {
    const d = new Date();
    const time = (d.getHours() << 11) | (d.getMinutes() << 5) | Math.floor(d.getSeconds() / 2);
    const date = ((Math.max(1980, d.getFullYear()) - 1980) << 9) | ((d.getMonth() + 1) << 5) | d.getDate();
    return [time, date];
  }

  function zip(files) {
    const locals = [];
    const central = [];
    let offset = 0;
    const [time, date] = dosNow();
    for (const file of files) {
      const name = enc.encode(file.name);
      const data = bytes(file.data);
      const crc = crc32(data);
      const local = concat([
        u32(0x04034b50), u16(20), u16(0x0800), u16(0), u16(time), u16(date),
        u32(crc), u32(data.length), u32(data.length), u16(name.length), u16(0), name, data,
      ]);
      locals.push(local);
      const record = concat([
        u32(0x02014b50), u16(20), u16(20), u16(0x0800), u16(0), u16(time), u16(date),
        u32(crc), u32(data.length), u32(data.length), u16(name.length), u16(0), u16(0),
        u16(0), u16(0), u32(0), u32(offset), name,
      ]);
      central.push(record);
      offset += local.length;
    }
    const directory = concat(central);
    const end = concat([
      u32(0x06054b50), u16(0), u16(0), u16(files.length), u16(files.length),
      u32(directory.length), u32(offset), u16(0),
    ]);
    return concat([...locals, directory, end]);
  }

  function addReplace(replaces, level, name) {
    const entry = replaces[level] ||= {};
    const list = entry.textures ||= [];
    if (!list.includes(name)) list.push(name);
  }

  async function buildPack() {
    build.disabled = true;
    try {
      await saveCurrent();
      const changed = state.groups.filter((group) => group.edited);
      if (!changed.length) throw new Error("Nothing changed yet. Edit or replace at least one Ratchet texture.");

      const packName = $("packName").value.trim() || "My Ratchet skin";
      const author = $("author").value.trim();
      const manifest = {
        format: 1,
        game: state.info.game,
        version: state.info.version,
        name: packName,
        author,
        description: "Custom Ratchet skin made with Custom Skin Studio.",
        levels_format: state.levelsFormat,
        replaces: {},
        originals: {},
      };
      const files = [];

      for (const group of changed) {
        for (const place of group.places) {
          const path = `${place.level}/textures/${place.name}.png`;
          files.push({ name: path, data: group.edited });
          addReplace(manifest.replaces, place.level, place.name);
          manifest.originals[path] = {
            sha256: group.hash,
            size: [group.width, group.height],
            used_by: [["moby", 0, place.slot, place.count]],
          };
        }
      }

      for (const value of Object.values(manifest.replaces)) value.textures.sort();
      const payload = zip([
        { name: "openrac-mod.json", data: enc.encode(JSON.stringify(manifest, null, 2) + "\n") },
        ...files,
      ]);
      const name = packName.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "ratchet-skin";
      const buffer = payload.buffer.slice(payload.byteOffset, payload.byteOffset + payload.byteLength);
      const saved = await api("save", { name, extension: "zip", bytes: buffer }, [buffer]);
      say(saved ? `Saved ${saved}. Add it with Mods → My mods → File…` : "Save cancelled.");
    } catch (err) {
      say(err instanceof Error ? err.message : String(err), true);
    } finally {
      build.disabled = false;
    }
  }

  async function exportWorkspace() {
    exportSource.disabled = true;
    try {
      if (!state.info) throw new Error("Load Ratchet's skin first.");
      await saveCurrent();
      const files = [];
      const seen = new Set();
      for (const model of state.models) {
        if (!seen.has(model.path)) {
          seen.add(model.path);
          files.push({ name: model.path, data: await read("levels", model.path) });
        }
      }
      for (const group of state.groups) {
        for (const place of group.places) {
          const path = `${place.level}/textures/${place.name}.png`;
          if (!seen.has(path)) {
            seen.add(path);
            files.push({ name: path, data: group.original });
          }
        }
      }
      const meta = {
        format: 1,
        kind: "openrac-custom-skin-workspace",
        game: state.info.game,
        version: state.info.version,
        class: 0,
        models: state.models.map((m) => m.path),
        textures: state.groups.flatMap((g) => g.places.map((p) => `${p.level}/textures/${p.name}.png`)),
      };
      const payload = zip([{ name: "skin.json", data: enc.encode(JSON.stringify(meta, null, 2) + "\n") }, ...files]);
      const buffer = payload.buffer.slice(payload.byteOffset, payload.byteOffset + payload.byteLength);
      const saved = await api("save", { name: "ratchet-skin-source", extension: "zip", bytes: buffer }, [buffer]);
      say(saved ? `Exported ${saved}. It contains Ratchet's source GLBs and PNGs from your own game data; do not redistribute those originals.` : "Export cancelled.");
    } catch (err) {
      say(err instanceof Error ? err.message : String(err), true);
    } finally {
      exportSource.disabled = false;
    }
  }

  $("scan").onclick = () => void scan();
  build.onclick = () => void buildPack();
  exportSource.onclick = () => void exportWorkspace();

  void api("info").then((info) => {
    state.info = info;
    say(`Ready for ${info.game}/${info.version}. Click “Load Ratchet skin”.`);
  }).catch((err) => say(err instanceof Error ? err.message : String(err), true));
})();
