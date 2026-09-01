/**
 * File Tools 前端：拖放工作台。
 * 大段 Base64 由 Rust 读剪贴板落临时文件，前端只显示摘要，避免 textarea 卡顿。
 */

const { invoke } = window.__TAURI__.core;
const { open, save } = window.__TAURI__.dialog;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;

/** @type {"encode"|"decode"|"split"|"merge"} */
let mode = "encode";

/** @type {"drop"|"paste"} */
let decodeFace = "drop";

/** 当前选中路径 */
let paths = [];

/** @type {{ tempPath: string, name: string, md5: string, sizeLabel: string } | null} */
let pasteIngest = null;

const copy = {
  encode: {
    title: "Drop files here",
    sub: "Outputs are written next to the source.",
    browse: "Browse files",
    run: "Convert",
    hint: "Encode writes <b>base64-N.txt</b><br />line 1 name · line 2 MD5 · line 3 Base64",
    multiple: true,
    dialogTitle: "Select files to encode",
    filters: null,
  },
  decode: {
    title: "Drop base64 text here",
    sub: "Switch to Paste for clipboard text. Format: name / MD5 / Base64.",
    browse: "Browse text files",
    run: "Restore",
    hint: "Format: <b>line 1 name · line 2 MD5 · line 3+ Base64</b>",
    multiple: true,
    dialogTitle: "Select base64 text files",
    filters: [
      { name: "Text", extensions: ["txt"] },
      { name: "All", extensions: ["*"] },
    ],
  },
  split: {
    title: "Drop a file to split",
    sub: "Chunks are written as .0001 · .0002 · … beside the source.",
    browse: "Browse file",
    run: "Split",
    hint: "Output: <b>&lt;file&gt;.0001 · .0002 · …</b>",
    multiple: false,
    dialogTitle: "Select file to split",
    filters: null,
  },
  merge: {
    title: "Drop first part (.0001)",
    sub: "Other numbered parts are detected automatically.",
    browse: "Browse .0001",
    run: "Merge",
    hint: "Auto-detects <b>.0002 …</b> · parts are removed after merge",
    multiple: false,
    dialogTitle: "Select first part (.0001)",
    filters: null,
  },
};

const dropEl = document.getElementById("drop");
const pastePanel = document.getElementById("paste-panel");
const pasteIdle = document.getElementById("paste-idle");
const pasteSummary = document.getElementById("paste-summary");
const pasteBtn = document.getElementById("paste-btn");
const pasteClear = document.getElementById("paste-clear");
const pasteName = document.getElementById("paste-name");
const pasteMd5 = document.getElementById("paste-md5");
const pasteSize = document.getElementById("paste-size");
const dropTitle = document.getElementById("drop-title");
const dropSub = document.getElementById("drop-sub");
const browseBtn = document.getElementById("browse");
const fileList = document.getElementById("file-list");
const faceToggle = document.getElementById("face-toggle");
const chunkRow = document.getElementById("chunk-row");
const hintEl = document.getElementById("hint");
const runBtn = document.getElementById("run");
const statusEl = document.getElementById("status");
const statusText = statusEl.querySelector(".status-text");

function setStatus(msg, kind = "") {
  statusText.textContent = msg;
  statusEl.className = "status" + (kind ? ` ${kind}` : "");
}

async function runJob(fn) {
  runBtn.disabled = true;
  setStatus("Working...", "working");
  try {
    const msg = await fn();
    setStatus(msg, "ok");
  } catch (e) {
    const err = typeof e === "string" ? e : e?.message || String(e);
    setStatus(`Error: ${err}`, "error");
  } finally {
    runBtn.disabled = false;
  }
}

function renderFiles() {
  const list = paths;
  dropEl.classList.toggle("has-files", list.length > 0);

  if (!list.length) {
    fileList.hidden = true;
    fileList.innerHTML = "";
    return;
  }

  fileList.innerHTML = "";
  for (const p of list) {
    const li = document.createElement("li");
    li.textContent = p;
    li.title = p;
    fileList.appendChild(li);
  }
  fileList.hidden = false;
}

/** 刷新粘贴摘要面板。 */
function renderPasteSummary() {
  const has = !!pasteIngest;
  pastePanel.classList.toggle("has-paste", has);
  pasteIdle.hidden = has;
  pasteSummary.hidden = !has;
  if (!has) return;
  pasteName.textContent = pasteIngest.name;
  pasteMd5.textContent = pasteIngest.md5;
  pasteSize.textContent = pasteIngest.sizeLabel;
}

async function clearPasteIngest() {
  if (pasteIngest?.tempPath) {
    try {
      await invoke("clear_paste_temp", { path: pasteIngest.tempPath });
    } catch {
      // 临时文件清理失败可忽略
    }
  }
  pasteIngest = null;
  renderPasteSummary();
}

/**
 * 从系统剪贴板摄入（Rust 侧读写，WebView 不持有大字符串）。
 */
async function ingestFromClipboard() {
  setStatus("Reading clipboard...", "working");
  pasteBtn.disabled = true;
  try {
    await clearPasteIngest();
    const info = await invoke("ingest_clipboard_b64");
    pasteIngest = {
      tempPath: info.tempPath,
      name: info.name,
      md5: info.md5,
      sizeLabel: info.sizeLabel,
    };
    if (paths.length) {
      paths = [];
      renderFiles();
    }
    renderPasteSummary();
    setStatus(`Clipboard ready · ${info.sizeLabel}`, "ok");
  } catch (e) {
    const err = typeof e === "string" ? e : e?.message || String(e);
    setStatus(`Error: ${err}`, "error");
  } finally {
    pasteBtn.disabled = false;
  }
}

/**
 * @param {"drop"|"paste"} face
 */
function setDecodeFace(face) {
  decodeFace = face;
  const showPaste = face === "paste";

  dropEl.classList.toggle("active", !showPaste);
  dropEl.hidden = showPaste;
  pastePanel.classList.toggle("active", showPaste);
  pastePanel.hidden = !showPaste;

  document.querySelectorAll(".face-btn").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.face === face);
  });

  if (showPaste) {
    if (paths.length) {
      paths = [];
      renderFiles();
    }
    requestAnimationFrame(() => pastePanel.focus());
  } else {
    clearPasteIngest();
  }
}

function setPaths(next) {
  const cfg = copy[mode];
  const list = (Array.isArray(next) ? next : next ? [next] : []).filter(Boolean);
  paths = cfg.multiple ? list : list.slice(0, 1);
  if (paths.length && mode === "decode" && decodeFace === "paste") {
    setDecodeFace("drop");
  }
  if (paths.length) {
    clearPasteIngest();
  }
  renderFiles();
}

function applyMode(next) {
  mode = next;
  const cfg = copy[mode];

  document.querySelectorAll(".pill").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.mode === mode);
  });

  dropTitle.textContent = cfg.title;
  dropSub.textContent = cfg.sub;
  browseBtn.textContent = cfg.browse;
  runBtn.textContent = cfg.run;
  hintEl.innerHTML = cfg.hint;
  chunkRow.hidden = mode !== "split";
  faceToggle.hidden = mode !== "decode";
  setDecodeFace("drop");

  paths = [];
  renderFiles();
  setStatus("Ready");
}

document.querySelectorAll(".pill").forEach((btn) => {
  btn.addEventListener("click", () => applyMode(btn.dataset.mode));
});

document.querySelectorAll(".face-btn").forEach((btn) => {
  btn.addEventListener("click", () => setDecodeFace(btn.dataset.face));
});

document.querySelectorAll(".unit").forEach((btn) => {
  btn.addEventListener("click", () => {
    document.querySelectorAll(".unit").forEach((u) => u.classList.toggle("active", u === btn));
    document.getElementById("spl-unit").value = btn.dataset.unit;
  });
});

pasteBtn.addEventListener("click", () => ingestFromClipboard());
pasteClear.addEventListener("click", () => {
  clearPasteIngest();
  setStatus("Ready");
});

// 在 Paste 面拦截 ⌘V，阻止浏览器把巨量文本塞进 DOM
window.addEventListener(
  "paste",
  (e) => {
    if (mode !== "decode" || decodeFace !== "paste") return;
    e.preventDefault();
    ingestFromClipboard();
  },
  true
);

browseBtn.addEventListener("click", async () => {
  const cfg = copy[mode];
  const opts = {
    multiple: cfg.multiple,
    title: cfg.dialogTitle,
  };
  if (cfg.filters) opts.filters = cfg.filters;
  const selected = await open(opts);
  if (!selected) return;
  setPaths(Array.isArray(selected) ? selected : [selected]);
});

const appWindow = getCurrentWebviewWindow();
appWindow.onDragDropEvent((event) => {
  const { type } = event.payload;
  if (mode === "decode" && decodeFace === "paste") return;

  if (type === "enter" || type === "over") {
    dropEl.classList.add("dragover");
  } else if (type === "leave") {
    dropEl.classList.remove("dragover");
  } else if (type === "drop") {
    dropEl.classList.remove("dragover");
    const dropped = event.payload.paths || [];
    if (dropped.length) setPaths(dropped);
  }
});

runBtn.addEventListener("click", () => {
  runJob(async () => {
    if (mode === "encode") {
      if (!paths.length) throw new Error("Select at least one file.");
      return invoke("encode_files", { paths });
    }

    if (mode === "decode") {
      if (decodeFace === "paste" || pasteIngest) {
        if (!pasteIngest) throw new Error("Paste Base64 text first.");
        const outPath = await save({
          title: "Save restored file",
          defaultPath: pasteIngest.name,
        });
        if (!outPath) throw new Error("Save cancelled.");
        const msg = await invoke("decode_paste_temp", {
          tempPath: pasteIngest.tempPath,
          outPath,
        });
        pasteIngest = null;
        renderPasteSummary();
        return msg;
      }
      if (!paths.length) throw new Error("Drop a file or switch to Paste.");
      return invoke("decode_files", { paths });
    }

    if (mode === "split") {
      if (!paths.length) throw new Error("Select a valid file.");
      const size = Number(document.getElementById("spl-size").value);
      if (!Number.isInteger(size) || size < 1) {
        throw new Error("Size must be a positive integer.");
      }
      const unit = document.getElementById("spl-unit").value;
      return invoke("split_file", { path: paths[0], size, unit });
    }

    if (!paths.length) throw new Error("Select a valid part file.");
    return invoke("merge_files", { firstPart: paths[0] });
  });
});

applyMode("encode");
