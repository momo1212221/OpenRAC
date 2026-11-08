// SPDX-License-Identifier: MIT
const enc = new TextEncoder();

export const bytes = (value) => value instanceof Uint8Array ? value : new Uint8Array(value);

export async function sha256(data) {
  const value = bytes(data);
  const input = value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength);
  const digest = await crypto.subtle.digest("SHA-256", input);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

export function pngSize(data) {
  const b = bytes(data);
  if (b.length < 24 || b[0] !== 137 || b[1] !== 80 || b[2] !== 78 || b[3] !== 71 ||
      b[12] !== 73 || b[13] !== 72 || b[14] !== 68 || b[15] !== 82) {
    throw new Error("The texture is not a PNG.");
  }
  const view = new DataView(b.buffer, b.byteOffset, b.byteLength);
  return [view.getUint32(16, false), view.getUint32(20, false)];
}

export function glbJson(data) {
  const b = bytes(data);
  if (b.length < 20) throw new Error("Hero model is too short to be a GLB.");
  const view = new DataView(b.buffer, b.byteOffset, b.byteLength);
  if (view.getUint32(0, true) !== 0x46546c67 || view.getUint32(4, true) !== 2) {
    throw new Error("Hero model is not glTF 2.0 binary.");
  }
  let at = 12;
  while (at + 8 <= b.length) {
    const size = view.getUint32(at, true);
    const type = view.getUint32(at + 4, true);
    if (at + 8 + size > b.length) break;
    if (type === 0x4e4f534a) {
      return JSON.parse(new TextDecoder().decode(b.subarray(at + 8, at + 8 + size)).trim());
    }
    at += 8 + size + ((4 - (size % 4)) % 4);
  }
  throw new Error("Hero model has no glTF JSON chunk.");
}

export function textureNames(doc) {
  const out = [];
  for (const image of doc.images || []) {
    const uri = typeof image.uri === "string" ? image.uri : "";
    const found = uri.match(/(?:^|\/)textures\/([A-Za-z0-9_.-]+)\.png$/);
    if (found && !out.includes(found[1])) out.push(found[1]);
  }
  return out;
}

const CRC = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = (c & 1) ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(data) {
  let c = 0xffffffff;
  for (const value of data) c = CRC[(c ^ value) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function concat(parts) {
  const size = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(size);
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

function u16(value) {
  const out = new Uint8Array(2);
  new DataView(out.buffer).setUint16(0, value, true);
  return out;
}

function u32(value) {
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, value >>> 0, true);
  return out;
}

export function zip(files) {
  const locals = [];
  const central = [];
  let offset = 0;
  const dosTime = 0;
  const dosDate = ((2026 - 1980) << 9) | (1 << 5) | 1;

  for (const file of files) {
    const name = enc.encode(file.name);
    const data = bytes(file.data);
    const crc = crc32(data);
    const local = concat([
      u32(0x04034b50), u16(20), u16(0x0800), u16(0), u16(dosTime), u16(dosDate),
      u32(crc), u32(data.length), u32(data.length), u16(name.length), u16(0), name, data,
    ]);
    locals.push(local);
    central.push(concat([
      u32(0x02014b50), u16(20), u16(20), u16(0x0800), u16(0), u16(dosTime), u16(dosDate),
      u32(crc), u32(data.length), u32(data.length), u16(name.length), u16(0), u16(0),
      u16(0), u16(0), u32(0), u32(offset), name,
    ]));
    offset += local.length;
  }

  const directory = concat(central);
  const end = concat([
    u32(0x06054b50), u16(0), u16(0), u16(files.length), u16(files.length),
    u32(directory.length), u32(offset), u16(0),
  ]);
  return concat([...locals, directory, end]);
}

export const slug = (value) =>
  String(value || "").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 64) || "ratchet-skin";
