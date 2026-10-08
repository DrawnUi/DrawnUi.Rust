// Browser host for drawnui. Owns the canvas, its WebGL2 context (or a 2D one when the app draws on the
// CPU), requestAnimationFrame and DOM events, and calls the exports of host_web.rs.
//
//   const { snapshot } = await DrawnUi.start({ canvas, create: createDrawnUi, configure: (module) => {},
//     imageWorker: true, history: true, resolveAsset: (url) => undefined });
//   snapshot("image/png") -> Promise<Blob>: a picture of the canvas (thumbnails).
//   history: false keeps the app out of the browser's history and URL hash (a page in an iframe).
//   fonts: [{ alias, url, weight = 400 }]: fonts the page adds, as the app's Ui::font (the first
//     frame waits for them), but never the default font: labels that name the alias use it. The
//     url goes through resolveAsset like any file.
//   resolveAsset(url) -> string | undefined: the address to load an app file from, asked for every
//     file the engine loads (images, GIFs and sprites, fonts, SVG, Lottie, shaders) with the url the
//     app gave ("assets/x.png"). A string is used as is (a blob: url made by the page works for the
//     image worker too); undefined loads the url as usual.
//
// Frame timing is collected in window.duiStats.

// Loads and decodes one picture for the engine: premultiplied RGBA pixels no larger than it takes
// to cover width x height (a 0 side does not count, both 0 = full size), never enlarged. With
// `frames`, every frame of an animated file (ImageDecoder; without it, the first frame only) as one
// buffer of frames plus their durations in ms. The browser decodes and resizes off the thread that
// calls this (createImageBitmap); what runs on it is the copy out of the bitmap. It runs in a
// worker, so it must not use anything outside itself.
async function drawnUiDecodeImage(url, width, height, frames) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  const blob = await response.blob();
  const started = performance.now();
  if (frames && typeof ImageDecoder === "function") {
    // A server that names no type most likely sent a GIF: this is a request to play frames.
    const decoder = new ImageDecoder({ data: await blob.arrayBuffer(), type: blob.type || "image/gif" });
    await decoder.tracks.ready;
    const count = decoder.tracks.selectedTrack.frameCount;
    const durations = new Uint32Array(count);
    let all = null;
    let context = null;
    let size = 0;
    for (let i = 0; i < count; i++) {
      const { image: frame } = await decoder.decode({ frameIndex: i });
      if (!all) {
        size = frame.displayWidth * frame.displayHeight * 4;
        all = new Uint8ClampedArray(size * count);
        context = new OffscreenCanvas(frame.displayWidth, frame.displayHeight).getContext("2d", { willReadFrequently: true });
      }
      // Each decoded frame is whole (the decoder composites it): nothing of the last one may stay.
      context.clearRect(0, 0, frame.displayWidth, frame.displayHeight);
      context.drawImage(frame, 0, 0);
      all.set(context.getImageData(0, 0, frame.displayWidth, frame.displayHeight).data, i * size);
      // Microseconds; at least 1 ms, as React's GifAnimation.
      durations[i] = Math.max(1, Math.round((frame.duration || 0) / 1000));
      frame.close();
    }
    const width = context.canvas.width;
    const height = context.canvas.height;
    decoder.close();
    premultiply(all);
    return { pixels: all, width, height, sourceWidth: width, sourceHeight: height, count, durations, copyMs: performance.now() - started };
  }
  let bitmap = await createImageBitmap(blob);
  const sourceWidth = bitmap.width;
  const sourceHeight = bitmap.height;
  const scale = width || height ? Math.min(1, Math.max(width / sourceWidth, height / sourceHeight)) : 1;
  const side = (pixels) => Math.max(1, Math.ceil(pixels * scale - 1e-6));
  if (scale < 1) {
    const full = bitmap;
    const resize = { resizeWidth: side(sourceWidth), resizeHeight: side(sourceHeight), resizeQuality: "high" };
    bitmap = await createImageBitmap(full, resize);
    full.close();
  }
  const context = new OffscreenCanvas(bitmap.width, bitmap.height).getContext("2d", { willReadFrequently: true });
  context.drawImage(bitmap, 0, 0);
  const pixels = context.getImageData(0, 0, bitmap.width, bitmap.height).data;
  const result = { pixels, width: bitmap.width, height: bitmap.height, sourceWidth, sourceHeight };
  bitmap.close();
  // A JPEG has no alpha. Everything else is premultiplied here: a canvas hands out plain alpha,
  // the engine draws premultiplied pixels.
  const opaque = blob.type === "image/jpeg";
  if (!opaque) premultiply(pixels);
  return { ...result, opaque, copyMs: performance.now() - started };

  function premultiply(rgba) {
    // The clamped array rounds.
    for (let i = 0; i < rgba.length; i += 4) {
      const alpha = rgba[i + 3];
      if (alpha < 255) {
        rgba[i] = (rgba[i] * alpha) / 255;
        rgba[i + 1] = (rgba[i + 1] * alpha) / 255;
        rgba[i + 2] = (rgba[i + 2] * alpha) / 255;
      }
    }
  }
}

// The address of an app file: with the content stamp the build wrote into the page
// (`duiAssetVersions`, dev/build.ps1), so a changed file is never served from an old cache.
function drawnUiVersioned(url) {
  const version = window.duiAssetVersions?.[url.split(/[?#]/)[0].replace(/^\.\//, "")];
  if (!version) return url;
  const at = url.indexOf("#");
  const [base, fragment] = at < 0 ? [url, ""] : [url.slice(0, at), url.slice(at)];
  return `${base}${base.includes("?") ? "&" : "?"}v=${version}${fragment}`;
}

window.DrawnUi = {
  async start({ canvas, create, configure, imageWorker = true, history: useHistory = true, resolveAsset, fonts = [] }) {
    const module = await create();
    if (configure) configure(module);
    const app = module._dui_create();
    // Fonts of the page: registered before anything is laid out (an engine without it skips them).
    const utf8 = new TextEncoder();
    for (const { alias, url, weight = 400 } of module._dui_font ? fonts : []) {
      const [a, u] = [utf8.encode(alias), utf8.encode(url)];
      const ptr = module._dui_alloc(a.length + u.length);
      module.HEAPU8.set(a, ptr);
      module.HEAPU8.set(u, ptr + a.length);
      module._dui_font(app, ptr, a.length, ptr + a.length, u.length, weight);
      module._dui_free(ptr, a.length + u.length);
    }
    // The app's RenderingMode: Accelerated draws with WebGL2; Default, or a browser that refuses
    // WebGL2, draws on the CPU and hands each frame to a 2D context.
    const accelerated = module._dui_accelerated(app);
    const gl = accelerated ? canvas.getContext("webgl2", { antialias: false, depth: false, stencil: true, alpha: true }) : null;
    if (gl) module.GL.makeContextCurrent(module.GL.registerContext(gl, { majorVersion: 2 }));
    else if (accelerated) console.warn("drawnui: WebGL2 is not available, drawing on the CPU");
    const ctx2d = gl ? null : canvas.getContext("2d");
    // GesturesMode Lock (DrawnUi.Web applyGestureStyle): the canvas owns every touch. touch-action
    // alone leaves the page's own gestures (Safari's rubber band and its pull-down) free to start on
    // it; a touchmove that is not passive and cancelled stops them. Taps stay untouched: they focus
    // the hidden textarea (soft keyboard) and the accessibility overlay.
    if (module._dui_gestures_lock?.(app)) {
      Object.assign(canvas.style, { touchAction: "none", userSelect: "none", webkitUserSelect: "none", webkitTouchCallout: "none" });
      document.documentElement.style.overscrollBehavior = "none";
      document.body.style.overscrollBehavior = "none";
      canvas.addEventListener("touchmove", (e) => e.preventDefault(), { passive: false });
    }

    const size = () => {
      const scale = window.devicePixelRatio || 1;
      canvas.width = Math.max(1, Math.round(canvas.clientWidth * scale));
      canvas.height = Math.max(1, Math.round(canvas.clientHeight * scale));
      return scale;
    };
    const scale = size();
    if (!module._dui_attach(app, canvas.width, canvas.height, scale, gl ? 1 : 0) && gl) throw new Error("drawnui: no GPU context on WebGL2");
    if (!useHistory) module._dui_history(app, 0);

    // imageMs: main-thread milliseconds each arriving picture cost, outside the frames.
    const stats = (window.duiStats = { cpuMs: [], intervalMs: [], imageMs: [], frames: 0, firstFrameMs: 0 });
    let pending = 0;
    let last = 0;
    let wakeTimer = 0;

    // Pictures are decoded and read back in a worker; the main thread only copies the pixels
    // into the engine's memory. Without workers the read back runs here too.
    let decodeImage = drawnUiDecodeImage;
    if (imageWorker && window.Worker && window.OffscreenCanvas) {
      const source = `${drawnUiDecodeImage}
        onmessage = ({ data: [key, url, width, height, frames] }) =>
          drawnUiDecodeImage(url, width, height, frames).then(
            (image) => postMessage([key, image], [image.pixels.buffer]),
            (error) => postMessage([key, null, String(error)]),
          );`;
      const worker = new Worker(URL.createObjectURL(new Blob([source], { type: "text/javascript" })));
      const waiting = new Map();
      let keys = 0;
      worker.onmessage = ({ data: [key, image, error] }) => {
        const [resolve, reject] = waiting.get(key);
        waiting.delete(key);
        if (image) resolve({ ...image, copyMs: 0 });
        else reject(new Error(error));
      };
      decodeImage = (url, width, height, frames) =>
        new Promise((resolve, reject) => {
          waiting.set(++keys, [resolve, reject]);
          worker.postMessage([keys, url, width, height, frames]);
        });
    }

    // Where a file the app named is loaded from: the page's own answer, else the url with its stamp.
    const address = (url) => resolveAsset?.(url) ?? drawnUiVersioned(url);
    const pumpAssets = () => {
      for (let p; (p = module._dui_poll_image(app)); ) {
        const [id, width, height, frames, ...url] = module.UTF8ToString(p).split(" ");
        // The worker has no page to resolve a relative url against.
        decodeImage(new URL(address(url.join(" ")), document.baseURI).href, Number(width), Number(height), frames === "1")
          .then((image) => {
            const started = performance.now();
            const ptr = module._dui_alloc_pixels(app, image.pixels.length);
            module.HEAPU8.set(image.pixels, ptr);
            if (image.count) {
              const durations = module._dui_alloc(image.count * 4);
              module.HEAPU8.set(new Uint8Array(image.durations.buffer), durations);
              module._dui_image_frames(app, Number(id), ptr, image.width, image.height, image.count, durations);
            } else {
              const { sourceWidth, sourceHeight, opaque } = image;
              module._dui_image(app, Number(id), ptr, image.width, image.height, sourceWidth, sourceHeight, opaque);
            }
            stats.imageMs.push(performance.now() - started + image.copyMs);
            requestFrame();
          })
          .catch((e) => {
            console.error("drawnui image", e);
            module._dui_image(app, Number(id), 0, 0, 0, 0, 0, 0); // no pixels = failed
            requestFrame();
          });
      }
      for (let p; (p = module._dui_poll_request(app)); ) {
        const [id, ...rest] = module.UTF8ToString(p).split(" ");
        const url = rest.join(" ");
        fetch(address(url))
          .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(`${url}: ${r.status}`))))
          .then((buffer) => {
            const bytes = new Uint8Array(buffer);
            const ptr = module._dui_alloc(bytes.length);
            module.HEAPU8.set(bytes, ptr);
            module._dui_asset(app, Number(id), ptr, bytes.length);
            requestFrame();
          })
          .catch((e) => {
            console.error("drawnui asset", e);
            module._dui_asset(app, Number(id), 0, 0); // empty = failed; the app goes on without it
            requestFrame();
          });
      }
    };

    // Drawn on the CPU: the frame's pixels (RGBA, unpremultiplied) into the 2D context.
    const present2d = () => {
      const [w, h] = [canvas.width, canvas.height];
      const pixels = new Uint8ClampedArray(module.HEAPU8.buffer, module._dui_pixels(app), w * h * 4);
      ctx2d.putImageData(new ImageData(pixels, w, h), 0, 0);
    };

    // A lost WebGL context (a GPU reset, a mobile tab in the background): no frames until the
    // browser gives it back.
    let lost = false;
    const draw = (time) => {
      if (lost) return;
      const start = performance.now();
      const more = module._dui_frame(app, time);
      if (ctx2d) present2d();
      const end = performance.now();
      if (stats.frames++ === 0) stats.firstFrameMs = end;
      stats.cpuMs.push(end - start);
      if (last) stats.intervalMs.push(time - last);
      last = time;
      pumpAssets();
      applyOutput();
      clearTimeout(wakeTimer);
      if (more) return requestFrame();
      last = 0;
      // A timer in the app: no frames until its time comes.
      const wake = module._dui_wake(app);
      if (wake >= 0) wakeTimer = setTimeout(requestFrame, Math.max(0, wake - performance.now()));
    };
    const requestFrame = () => {
      if (!pending && !lost)
        pending = requestAnimationFrame((time) => {
          pending = 0;
          draw(time);
        });
    };

    // Resize draws at once, so the canvas never shows a blank frame.
    new ResizeObserver(() => {
      if (lost) return; // the size is taken when the context is back
      const scale = size();
      module._dui_resize(app, canvas.width, canvas.height, scale);
      placeOverlay();
      if (pending) cancelAnimationFrame(pending);
      pending = 0;
      draw(performance.now());
    }).observe(canvas);

    const toCanvas = (e) => {
      const rect = canvas.getBoundingClientRect();
      const scale = canvas.width / rect.width;
      return [(e.clientX - rect.left) * scale, (e.clientY - rect.top) * scale];
    };

    // The engine recognizes one press: its pointer and button. Other pointers (a second finger)
    // are not forwarded while it lasts; a mouse with no button down hovers, a touch never does.
    let press = null;
    const pointer = (kind) => (e) => {
      const [x, y] = toCanvas(e);
      let button = Math.max(0, e.button);
      if (kind === 0) {
        if (press) return;
        press = { id: e.pointerId, button };
        canvas.setPointerCapture(e.pointerId);
      } else if (!press || press.id !== e.pointerId) {
        if (kind === 1 && !press && e.pointerType === "mouse") {
          module._dui_pointer(app, 4, 0, x, y, e.timeStamp);
          requestFrame();
        }
        return;
      } else if (kind === 1) {
        button = press.button;
      } else if (kind === 3 || button === press.button) {
        press = null;
      }
      module._dui_pointer(app, kind, button, x, y, e.timeStamp);
      requestFrame();
    };
    canvas.addEventListener("pointerdown", pointer(0));
    canvas.addEventListener("pointermove", pointer(1));
    canvas.addEventListener("pointerup", pointer(2));
    // The browser took the touch (to scroll the page, for a system gesture): not a release.
    canvas.addEventListener("pointercancel", pointer(3));
    canvas.addEventListener("pointerleave", (e) => {
      if (press || e.pointerType !== "mouse") return;
      const [x, y] = toCanvas(e);
      module._dui_pointer(app, 5, 0, x, y, e.timeStamp);
      requestFrame();
    });

    // A right click, a long press on touch, the Menu key: routed like a tap in the engine; the
    // browser's own menu stays away when a control or the app took it (React Canvas.ContextMenu).
    canvas.addEventListener("contextmenu", (e) => {
      const [x, y] = toCanvas(e);
      const type = e.pointerType;
      const source = type === "touch" || type === "pen" ? 1 : type === "mouse" || e.button === 2 ? 0 : 2;
      if (module._dui_context_menu(app, x, y, source, e.timeStamp)) e.preventDefault();
      requestFrame();
    });

    // Keys: window-level listeners, as React's KeyboardManager and DrawnUi.Blazor. The key name
    // is the DOM `code`; text comes as its own event (a printable key without Ctrl / Alt / Meta,
    // AltGr included). The engine answers whether it used the key: then its default action is
    // prevented (Space does not scroll the page under a game).
    const encoder = new TextEncoder();
    let scratch = 0;
    let scratchSize = 0;
    const sendKey = (kind, code, text, e) => {
      const codeBytes = encoder.encode(code);
      const textBytes = encoder.encode(text);
      const need = codeBytes.length + textBytes.length;
      if (need > scratchSize) {
        if (scratch) module._dui_free(scratch, scratchSize);
        scratchSize = Math.max(256, need * 2);
        scratch = module._dui_alloc(scratchSize);
      }
      module.HEAPU8.set(codeBytes, scratch);
      module.HEAPU8.set(textBytes, scratch + codeBytes.length);
      const modifiers = (e.shiftKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0);
      const used = module._dui_key(app, kind, scratch, codeBytes.length, scratch + codeBytes.length, textBytes.length, modifiers, e.repeat ? 1 : 0);
      requestFrame();
      return used !== 0;
    };
    // With the text input focused, these keys arrive as its input events instead (below).
    const viaTextInput = (e) =>
      e.target === textInput &&
      (e.code === "Backspace" || e.code === "Delete" || e.code === "Enter" || e.code === "NumpadEnter" || ((e.ctrlKey || e.metaKey) && e.code === "KeyV"));
    const caretKeys = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"]);
    // The engine's elements: the canvas, its overlay, its text input, or the page with nothing
    // focused. A key aimed at anything else (an input of the page) belongs to it: the engine only
    // hears it, never takes it, and gets no typed text from it.
    const ours = (t) => !t || t === canvas || t === textInput || t === document.body || t === document.documentElement || overlay.contains(t);
    // An input method is composing: Backspace and the arrows edit its text, the text arrives
    // through the text input's composition (soft keyboards send key 229 / "Unidentified" and
    // delete through beforeinput).
    const composing = (e) => e.isComposing || e.keyCode === 229 || e.key === "Process";
    window.addEventListener(
      "keydown",
      (e) => {
        if (composing(e)) return;
        if (!ours(e.target)) {
          sendKey(0, e.code || "", "", e);
          return;
        }
        // Tab / Shift+Tab in a text field leave it for the next node, also after a click into it
        // (React TextInputProxy): the page focus goes to the field's node without giving the caret
        // back, and the browser's Tab goes on from there.
        if (e.key === "Tab" && textOpen && e.target === textInput && a11yNodes.has(String(textNode))) {
          tabbingOut = String(textNode);
          a11yNodes.get(tabbingOut).focus({ preventScroll: true });
        }
        let used = false;
        if (!viaTextInput(e)) used = sendKey(0, e.code || "", "", e);
        const printable = e.key && [...e.key].length === 1;
        const plain = (!e.ctrlKey && !e.altKey && !e.metaKey) || (e.getModifierState && e.getModifierState("AltGraph"));
        if (e.target !== textInput && printable && plain) used = sendKey(2, "", e.key, e) || used;
        // The text input keeps its caret behind its one character, so that Backspace always has
        // something to delete (Android keyboards send nothing on an empty field).
        if (e.target === textInput && caretKeys.has(e.code)) e.preventDefault();
        if (used) e.preventDefault();
      },
      true,
    );
    window.addEventListener(
      "keyup",
      (e) => {
        if (composing(e)) return;
        if (!viaTextInput(e) && sendKey(1, e.code || "", "", e) && ours(e.target)) e.preventDefault();
      },
      true,
    );
    // The page focus went to a field outside the canvas: no drawn control keeps the keyboard.
    document.addEventListener(
      "focusin",
      (e) => {
        if (ours(e.target)) return;
        module._dui_focus_out(app);
        requestFrame();
      },
      true,
    );
    // Keys released while the page had no focus never come: nothing counts as held.
    window.addEventListener("blur", () => {
      module._dui_blur(app);
      requestFrame();
    });
    canvas.addEventListener("webglcontextlost", (e) => {
      e.preventDefault(); // without it the browser never gives the context back
      lost = true;
      if (pending) cancelAnimationFrame(pending);
      pending = 0;
      clearTimeout(wakeTimer);
    });
    // Back: made current again, a new engine context and surface; caches are made again.
    canvas.addEventListener("webglcontextrestored", () => {
      module.GL.makeContextCurrent(module.GL.registerContext(gl, { majorVersion: 2 }));
      module._dui_gpu_restored(app);
      lost = false;
      const scale = size();
      module._dui_resize(app, canvas.width, canvas.height, scale);
      placeOverlay();
      requestFrame();
    });
    // A background tab (React Pong pauses on it).
    document.addEventListener("visibilitychange", () => {
      module._dui_visibility(app, document.hidden ? 0 : 1);
      requestFrame();
    });
    // Inside a cross-origin frame the embedding page keeps the focus: a press takes it.
    window.addEventListener("pointerdown", () => window.focus(), true);

    // The text input (React TextInputProxy): a hidden textarea over the focused editor, so that
    // soft keyboards, IMEs, paste and dictation reach it. It holds one character; what is typed
    // is sent as text, Backspace / Delete / Enter as keys, and the character stays.
    let textInput = null;
    let textOpen = false;
    // The accessibility node of the text field, and the node the keyboard is leaving it through.
    let textNode = null;
    let tabbingOut = null;
    const resetTextInput = () => {
      textInput.value = " ";
      textInput.setSelectionRange(1, 1);
    };
    const tapKey = (code, e) => {
      sendKey(0, code, "", e);
      sendKey(1, code, "", e);
    };
    const makeTextInput = () => {
      const el = document.createElement("textarea");
      el.dataset.drawnuiTextInput = "1";
      el.setAttribute("aria-hidden", "true");
      el.tabIndex = -1;
      el.autocomplete = "off";
      el.setAttribute("autocorrect", "off");
      el.setAttribute("autocapitalize", "off");
      el.spellcheck = false;
      el.rows = 1;
      // 1 x 1 over the editor: invisible, still focusable; 16px keeps iOS from zooming the page.
      el.style.cssText =
        "position:fixed;left:0;top:0;width:1px;height:1px;padding:0;border:0;margin:0;opacity:0.01;font-size:16px;line-height:1;resize:none;overflow:hidden;outline:none;background:transparent;color:transparent;caret-color:transparent;pointer-events:none;z-index:-1;";
      el.addEventListener("beforeinput", (e) => {
        // An IME composes in the field; the text comes at compositionend.
        if (e.isComposing || e.inputType === "insertCompositionText") return;
        if (e.inputType === "insertText" || e.inputType === "insertReplacementText") {
          if (e.data) sendKey(2, "", e.data, e);
        } else if (e.inputType === "insertLineBreak" || e.inputType === "insertParagraph") tapKey("Enter", e);
        else if (e.inputType.startsWith("deleteContentBackward") || e.inputType.startsWith("deleteWordBackward")) tapKey("Backspace", e);
        else if (e.inputType.startsWith("deleteContentForward") || e.inputType.startsWith("deleteWordForward")) tapKey("Delete", e);
        else return;
        e.preventDefault();
      });
      el.addEventListener("compositionend", (e) => {
        if (e.data) sendKey(2, "", e.data, e);
        resetTextInput();
      });
      el.addEventListener("paste", (e) => {
        const text = e.clipboardData && e.clipboardData.getData("text/plain");
        if (text) sendKey(2, "", text, e);
        e.preventDefault();
      });
      document.body.appendChild(el);
      return el;
    };
    const openTextInput = (rect) => {
      textOpen = !!rect;
      if (!rect) {
        if (textInput && document.activeElement === textInput) textInput.blur();
        return;
      }
      textInput ??= makeTextInput();
      const b = canvas.getBoundingClientRect();
      textInput.style.left = `${Math.round(b.left + rect[0])}px`;
      textInput.style.top = `${Math.round(b.top + rect[1])}px`;
      resetTextInput();
      textInput.focus({ preventScroll: true });
    };
    // A tap on the canvas moves the page focus away from the text input; while an editor is
    // focused it goes back at once, inside the tap, which is what keeps a soft keyboard open.
    window.addEventListener(
      "pointerup",
      (e) => {
        if (textOpen && textInput && e.target === canvas) textInput.focus({ preventScroll: true });
      },
      true,
    );

    // The accessibility overlay (React AccessibilityOverlay, DrawnUi.Blazor): the canvas is hidden
    // from assistive technology; invisible ARIA elements over it mirror the engine's snapshot.
    // They take no pointer events: hover and gestures reach the canvas, Tab / Enter / Space and
    // screen readers reach the elements and are routed back as a tap.
    if (!document.getElementById("drawnui-a11y-css")) {
      const css = document.createElement("style");
      css.id = "drawnui-a11y-css";
      css.textContent = `
.drawnui-a11y-overlay{position:absolute;overflow:hidden;pointer-events:none;user-select:none;-webkit-user-select:none}
.drawnui-a11y-node{position:absolute;margin:0;padding:0;border:0;background:transparent;color:transparent;overflow:hidden;white-space:nowrap;pointer-events:none;font:inherit;user-select:none;-webkit-user-select:none}
.drawnui-a11y-node:focus{outline:none}
.drawnui-a11y-node:focus-visible{outline:3px solid rgba(13,110,253,.85);outline-offset:1px;border-radius:3px}
.drawnui-a11y-node::selection,.drawnui-a11y-node *::selection{background:transparent;color:transparent}
.drawnui-a11y-text{overflow:visible}
.drawnui-a11y-text>span{position:absolute;white-space:pre;color:transparent;user-select:text;-webkit-user-select:text;pointer-events:auto;cursor:text;line-height:1;font-kerning:none;font-feature-settings:"kern" 0,"liga" 0,"calt" 0;text-rendering:geometricPrecision}
.drawnui-a11y-text>span::selection{background:rgba(110,168,254,.55);color:transparent}`;
      document.head.appendChild(css);
    }
    canvas.setAttribute("aria-hidden", "true");
    const overlay = document.createElement("div");
    overlay.className = "drawnui-a11y-overlay";
    canvas.insertAdjacentElement("afterend", overlay);
    // The browser scrolls an overflow:hidden box to show a focused child: it stays on the canvas.
    overlay.addEventListener("scroll", () => {
      overlay.scrollTop = 0;
      overlay.scrollLeft = 0;
    });
    function placeOverlay() {
      overlay.style.left = `${canvas.offsetLeft}px`;
      overlay.style.top = `${canvas.offsetTop}px`;
      overlay.style.width = `${canvas.clientWidth}px`;
      overlay.style.height = `${canvas.clientHeight}px`;
    }
    placeOverlay();
    // Selectable text takes pointer events: the wheel over it goes on to the canvas, so drawn
    // scrolls keep working.
    overlay.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        const { deltaX, deltaY, deltaMode, clientX, clientY, ctrlKey, shiftKey } = e;
        canvas.dispatchEvent(new WheelEvent("wheel", { deltaX, deltaY, deltaMode, clientX, clientY, ctrlKey, shiftKey, cancelable: true }));
      },
      { passive: false },
    );
    const a11yNodes = new Map();
    const activate = (id) => {
      module._dui_a11y_activate(app, Number(id));
      requestFrame();
    };
    // Arrow-key groups are one Tab stop (roving tabindex, C# IsTabStop): the item the keyboard was
    // on last, else the group's first; the nodes inside that item are stops, the others are not.
    const groupItems = new Map();
    const rove = () => {
      const current = new Map();
      for (const el of overlay.children) {
        const group = el.dataset.group;
        if (!group || el.dataset.kind !== "i") continue;
        const [g, item] = group.split(":");
        if (!current.has(g)) current.set(g, item);
        if (groupItems.get(g) === item) current.set(g, item);
      }
      for (const el of overlay.children) {
        if (el.dataset.kind !== "i") continue;
        const group = el.dataset.group;
        const [g, item] = group ? group.split(":") : [];
        el.tabIndex = !group || current.get(g) === item ? 0 : -1;
      }
    };
    // kind: "i" interactive (a tab stop), "t" selectable text lines, "p" plain.
    const makeA11yNode = (id, kind) => {
      const el = document.createElement("div");
      el.className = kind === "t" ? "drawnui-a11y-node drawnui-a11y-text" : "drawnui-a11y-node";
      el.dataset.kind = kind;
      if (kind === "i") {
        el.tabIndex = 0;
        el.style.fontSize = "0";
        el.addEventListener("click", () => activate(id));
        el.addEventListener("keydown", (e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            activate(id);
          }
        });
        el.addEventListener("focus", () => {
          overlay.scrollTop = 0;
          overlay.scrollLeft = 0;
          const group = el.dataset.group;
          if (group) {
            const [g, item] = group.split(":");
            groupItems.set(g, item);
            rove();
          }
          // Leaving a text field through its node: it does not take the caret again.
          if (tabbingOut === id) tabbingOut = null;
          else module._dui_a11y_focus(app, Number(id), 1);
          requestFrame();
        });
        el.addEventListener("blur", (e) => {
          // A text field moved the page focus to its own input.
          if (textInput && e.relatedTarget === textInput) return;
          module._dui_a11y_focus(app, Number(id), 0);
          requestFrame();
        });
      }
      return el;
    };
    const setAttribute = (el, name, value) => (value == null || value === "" ? el.removeAttribute(name) : el.setAttribute(name, value));
    // Selectable text (React AccessibilityTextSelectable): one positioned span per drawn line, in
    // the drawn font. The browser shapes the invisible text its own way: each line is stretched
    // with letter-spacing to the drawn width, so a selection covers the glyphs end to end.
    const renderTextLines = (el, lines) => {
      el.textContent = "";
      const all = lines.split("\x1e");
      all.forEach((line, i) => {
        const [text, left, top, width, height, family, weight, size] = line.split("\x1f");
        const span = document.createElement("span");
        span.textContent = i < all.length - 1 ? `${text}\n` : text;
        span.dataset.width = width;
        Object.assign(span.style, {
          left: `${left}px`,
          top: `${top}px`,
          height: `${height}px`,
          lineHeight: `${height}px`,
          // Quoted: an alias like "Default" is a CSS keyword unquoted.
          fontFamily: family
            .split(",")
            .map((f) => `"${f.trim()}"`)
            .join(", "),
          fontWeight: weight,
          fontSize: `${size}px`,
        });
        el.appendChild(span);
      });
    };
    const stretchTextLines = (el) => {
      for (const span of el.children) {
        const want = Number(span.dataset.width);
        // Letter-spacing follows every character, the last one too (React divides by one less
        // and ends one spacing too wide).
        const n = [...span.textContent.replace(/\n$/, "")].length;
        span.style.letterSpacing = "0px";
        if (want <= 0 || n <= 0) continue;
        const have = span.getBoundingClientRect().width;
        if (Math.abs(have - want) > 0.5) span.style.letterSpacing = `${(want - have) / n}px`;
      }
    };
    const renderAccessibility = (message) => {
      const stretch = [];
      const nodes = message.split("\n").slice(1).map((line) => line.split("\t"));
      // Gone nodes first: what stays is then moved only when the order changed, since moving the
      // focused element would take the focus away.
      const seen = new Set(nodes.map((n) => n[0]));
      for (const [id, el] of a11yNodes) {
        if (!seen.has(id)) {
          el.remove();
          a11yNodes.delete(id);
        }
      }
      nodes.forEach(([id, role, label, hint, left, top, width, height, interactive, pressed, live, group, value, lines], i) => {
        const canInteract = interactive === "1";
        const kind = canInteract ? "i" : lines ? "t" : "p";
        let el = a11yNodes.get(id);
        if (el && el.dataset.kind !== kind) {
          el.remove();
          el = undefined;
        }
        if (!el) a11yNodes.set(id, (el = makeA11yNode(id, kind)));
        if (group) el.dataset.group = group;
        else delete el.dataset.group;
        el.setAttribute("role", role);
        // Static text is real (transparent) text; a name only where the role needs one.
        setAttribute(el, "aria-label", kind === "i" || (kind === "p" && role !== "text") ? label : null);
        setAttribute(el, "title", hint);
        const state = pressed === "-1" ? null : pressed === "1" ? "true" : "false";
        setAttribute(el, "aria-pressed", role === "button" ? state : null);
        setAttribute(el, "aria-checked", role === "switch" || role === "checkbox" || role === "radio" ? state : null);
        setAttribute(el, "aria-live", live);
        setAttribute(el, "aria-disabled", interactive === "-" ? "true" : null);
        const [now, min, max, valueText] = value ? value.split("\x1f") : [];
        setAttribute(el, "aria-valuenow", now);
        setAttribute(el, "aria-valuemin", min);
        setAttribute(el, "aria-valuemax", max);
        setAttribute(el, "aria-valuetext", valueText);
        if (kind === "t") {
          if (el.dataset.lines !== lines) {
            el.dataset.lines = lines;
            renderTextLines(el, lines);
            stretch.push(el);
          }
        } else if (el.textContent !== label) el.textContent = label;
        Object.assign(el.style, { left: `${left}px`, top: `${top}px`, width: `${width}px`, height: `${height}px` });
        if (overlay.children[i] !== el) overlay.insertBefore(el, overlay.children[i] ?? null);
      });
      stretch.forEach(stretchTextLines);
      rove();
    };

    // The browser's history and URL hash (React SkiaShell UseBrowserHistory): the hash at start,
    // then every back / forward with the depth its entry was pushed with.
    const sendLocation = (popped) => {
      const bytes = encoder.encode(window.location.hash);
      const ptr = bytes.length ? module._dui_alloc(bytes.length) : 0;
      if (ptr) module.HEAPU8.set(bytes, ptr);
      module._dui_location(app, ptr, bytes.length, popped);
      if (ptr) module._dui_free(ptr, bytes.length);
      requestFrame();
    };
    if (useHistory) {
      sendLocation(-1);
      window.addEventListener("popstate", (e) => sendLocation((e.state && e.state.drawnui) || 0));
    }

    // The safe area (React Super.Insets): env(safe-area-inset-*) read from a probe element, at
    // start and on every resize.
    const insetsProbe = document.createElement("div");
    insetsProbe.style.cssText =
      "position:fixed;left:0;top:0;width:0;height:0;visibility:hidden;pointer-events:none;padding:env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left);";
    document.body.appendChild(insetsProbe);
    const sendInsets = () => {
      const cs = getComputedStyle(insetsProbe);
      const px = (v) => parseFloat(v) || 0;
      module._dui_safe_insets(app, px(cs.paddingLeft), px(cs.paddingTop), px(cs.paddingRight), px(cs.paddingBottom));
      requestFrame();
    };
    sendInsets();
    window.addEventListener("resize", sendInsets);

    // What the engine asked of the page during the frame.
    function applyOutput() {
      for (let p; (p = module._dui_poll_output(app)); ) {
        const message = module.UTF8ToString(p);
        const rest = message.slice(2);
        switch (message[0]) {
          case "c":
            canvas.style.cursor = ["", "pointer", "text"][Number(rest)] ?? "";
            break;
          case "t": {
            const area = message.length > 1 ? rest.split(" ").map(Number) : null;
            textNode = area && area.length > 4 ? area[4] : null;
            openTextInput(area);
            break;
          }
          case "f":
            // An arrow in a group moved the keyboard: the page focus follows.
            a11yNodes.get(rest)?.focus({ preventScroll: true });
            break;
          case "p":
            if (navigator.clipboard) navigator.clipboard.writeText(rest).catch((e) => console.error("drawnui clipboard", e));
            break;
          case "u":
            window.open(rest, "_blank", "noopener");
            break;
          case "h": {
            if (!useHistory) break;
            // "push D HASH", "replace D HASH", "back"; no hash keeps the URL (React SkiaShell).
            const [op, depth, ...hash] = rest.split(" ");
            // "#" alone: the URL without a hash (React: pathname + search when the stack is empty).
            const joined = hash.join(" ");
            const url = joined === "#" ? window.location.pathname + window.location.search : joined || undefined;
            if (op === "back") history.back();
            else history[op === "push" ? "pushState" : "replaceState"]({ drawnui: Number(depth) }, "", url);
            break;
          }
          case "v":
            if (navigator.clipboard)
              navigator.clipboard
                .readText()
                .then((text) => text && sendKey(2, "", text, {}))
                .catch((e) => console.error("drawnui clipboard", e));
            break;
          case "a":
            renderAccessibility(message);
            break;
        }
      }
    }

    // The wheel is shared with the page: the app answers at once whether it used the event, and
    // only then the page is kept from scrolling. That needs a listener that is not passive.
    canvas.addEventListener(
      "wheel",
      (e) => {
        const rect = canvas.getBoundingClientRect();
        const scale = canvas.width / rect.width;
        // To pixels, then to notches: 100 pixels is one mouse notch in Chrome and Edge. The DOM
        // counts down as positive, the engine counts it as negative.
        const unit = e.deltaMode === 1 ? 40 : e.deltaMode === 2 ? 800 : 1;
        // The dominant axis and which one it is: a vertical scroll leaves horizontal events.
        const horizontal = Math.abs(e.deltaX) > Math.abs(e.deltaY);
        const dominant = horizontal ? e.deltaX : e.deltaY;
        const x = (e.clientX - rect.left) * scale;
        const y = (e.clientY - rect.top) * scale;
        if (module._dui_wheel_axis(app, x, y, (-dominant * unit) / 100, horizontal ? 1 : 0, e.timeStamp)) {
          e.preventDefault();
          requestFrame();
        }
      },
      { passive: false },
    );

    // A picture of what the canvas shows (share thumbnails): a frame is drawn and read in the same
    // task, while WebGL still holds it, so no preserveDrawingBuffer is needed and other frames pay
    // nothing. `type` and `quality` as canvas.toBlob.
    const snapshot = (type = "image/png", quality) =>
      new Promise((resolve, reject) =>
        requestAnimationFrame((time) => {
          if (lost) return reject(new Error("drawnui: the WebGL context is lost"));
          draw(time);
          canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error("drawnui: no picture"))), type, quality);
        }),
      );

    pumpAssets();
    requestFrame();
    return { module, app, requestFrame, snapshot };
  },
};
