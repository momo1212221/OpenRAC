// SPDX-License-Identifier: MIT
export const PROTOCOL = 1;
let requestId = 1;
const pending = new Map();

window.addEventListener("message", (event) => {
  if (event.source !== window.parent) return;
  const msg = event.data;
  if (!msg || msg.openrac !== PROTOCOL || typeof msg.id !== "number") return;
  const task = pending.get(msg.id);
  if (!task) return;
  pending.delete(msg.id);
  clearTimeout(task.timer);
  if (msg.ok) task.resolve(msg.value);
  else task.reject(new Error(String(msg.error || "OpenRAC tool call failed")));
});

export function api(call, args = {}, transfer = [], timeout = 120000) {
  return new Promise((resolve, reject) => {
    const id = requestId++;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error("The OpenRAC launcher did not answer the Skin Studio request."));
    }, timeout);
    pending.set(id, {resolve, reject, timer});
    window.parent.postMessage({openrac: PROTOCOL, id, call, args}, "*", transfer);
  });
}

export function isUnsupported(error, call) {
  const text = error instanceof Error ? error.message : String(error);
  return text.includes("no call") && text.includes(call);
}
