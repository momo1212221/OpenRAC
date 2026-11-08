(() => {
  const $ = (id) => document.getElementById(id);
  const canvas = $('canvas');
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  const file = $('file');
  const color = $('color');
  const size = $('size');
  const opacity = $('opacity');
  const tint = $('tint');
  const status = $('status');
  const undo = $('undo');
  const reset = $('reset');
  const save = $('save');
  let name = 'ratchet-skin.png';
  let original = null;
  let history = [];
  let mode = 'brush';
  let down = false;
  let last = null;

  const setText = () => {
    $('sizeOut').textContent = `${size.value} px`;
    $('opacityOut').textContent = `${opacity.value}%`;
    $('tintOut').textContent = `${tint.value}%`;
  };
  [size, opacity, tint].forEach((el) => el.addEventListener('input', setText));
  setText();

  function snapshot() {
    if (!canvas.width || !canvas.height) return;
    history.push(ctx.getImageData(0, 0, canvas.width, canvas.height));
    if (history.length > 30) history.shift();
    undo.disabled = history.length === 0;
  }

  function loaded() {
    return original !== null;
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
    ctx.globalCompositeOperation = mode === 'erase' ? 'destination-out' : 'source-over';
    ctx.strokeStyle = color.value;
    ctx.lineWidth = Number(size.value);
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.stroke();
    ctx.restore();
  }

  canvas.addEventListener('pointerdown', (e) => {
    if (!loaded()) return;
    snapshot();
    down = true;
    last = point(e);
    canvas.setPointerCapture(e.pointerId);
    stroke(last, { x: last.x + .01, y: last.y + .01 });
  });
  canvas.addEventListener('pointermove', (e) => {
    if (!down || !last) return;
    const next = point(e);
    stroke(last, next);
    last = next;
  });
  const end = () => { down = false; last = null; };
  canvas.addEventListener('pointerup', end);
  canvas.addEventListener('pointercancel', end);

  file.addEventListener('change', () => {
    const picked = file.files?.[0];
    if (!picked) return;
    name = picked.name;
    const image = new Image();
    image.onload = () => {
      canvas.width = image.naturalWidth;
      canvas.height = image.naturalHeight;
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.drawImage(image, 0, 0);
      original = ctx.getImageData(0, 0, canvas.width, canvas.height);
      history = [];
      undo.disabled = true;
      reset.disabled = false;
      save.disabled = false;
      status.textContent = `${name} — ${canvas.width} × ${canvas.height}`;
      URL.revokeObjectURL(image.src);
    };
    image.src = URL.createObjectURL(picked);
  });

  function selectMode(next) {
    mode = next;
    $('brush').classList.toggle('active', next === 'brush');
    $('erase').classList.toggle('active', next === 'erase');
  }
  $('brush').onclick = () => selectMode('brush');
  $('erase').onclick = () => selectMode('erase');

  undo.onclick = () => {
    const previous = history.pop();
    if (previous) ctx.putImageData(previous, 0, 0);
    undo.disabled = history.length === 0;
  };
  reset.onclick = () => {
    if (!original) return;
    snapshot();
    ctx.putImageData(original, 0, 0);
  };

  $('applyTint').onclick = () => {
    if (!loaded()) return;
    snapshot();
    const image = ctx.getImageData(0, 0, canvas.width, canvas.height);
    const hex = color.value;
    const tr = parseInt(hex.slice(1, 3), 16), tg = parseInt(hex.slice(3, 5), 16), tb = parseInt(hex.slice(5, 7), 16);
    const mix = Number(tint.value) / 100;
    for (let i = 0; i < image.data.length; i += 4) {
      image.data[i] = Math.round(image.data[i] * (1 - mix) + tr * mix);
      image.data[i + 1] = Math.round(image.data[i + 1] * (1 - mix) + tg * mix);
      image.data[i + 2] = Math.round(image.data[i + 2] * (1 - mix) + tb * mix);
    }
    ctx.putImageData(image, 0, 0);
  };

  $('gray').onclick = () => {
    if (!loaded()) return;
    snapshot();
    const image = ctx.getImageData(0, 0, canvas.width, canvas.height);
    for (let i = 0; i < image.data.length; i += 4) {
      const y = Math.round(image.data[i] * .2126 + image.data[i + 1] * .7152 + image.data[i + 2] * .0722);
      image.data[i] = image.data[i + 1] = image.data[i + 2] = y;
    }
    ctx.putImageData(image, 0, 0);
  };

  save.onclick = () => {
    canvas.toBlob((blob) => {
      if (!blob) return;
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = name;
      a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    }, 'image/png');
  };
})();
