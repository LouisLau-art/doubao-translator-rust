const state = {
  languages: {},
  auto: true,
  debounce: null,
  historyKey: "translator_history",
  maxHistory: 50,
  outputText: "",
};

const els = {
  sourceLang: document.getElementById("sourceLang"),
  targetLang: document.getElementById("targetLang"),
  inputText: document.getElementById("inputText"),
  outputRender: document.getElementById("outputRender"),
  autoToggle: document.getElementById("autoToggle"),
  swapBtn: document.getElementById("swapBtn"),
  focusBtn: document.getElementById("focusBtn"),
  translateBtn: document.getElementById("translateBtn"),
  copyBtn: document.getElementById("copyBtn"),
  clearBtn: document.getElementById("clearBtn"),
  pasteBtn: document.getElementById("pasteBtn"),
  clearOutput: document.getElementById("clearOutput"),
  fontSize: document.getElementById("fontSize"),
  historyList: document.getElementById("historyList"),
  clearHistory: document.getElementById("clearHistory"),
  divider: document.getElementById("divider"),
  panels: document.getElementById("panels"),
  charCount: document.getElementById("charCount"),
  errorMsg: document.getElementById("errorMsg"),
  cachedBadge: document.getElementById("cachedBadge"),
  loadingBadge: document.getElementById("loadingBadge"),
};

async function loadLanguages() {
  const res = await fetch("/api/languages");
  const data = await res.json();
  state.languages = data.languages || {};

  const sourceOptions = [
    { value: "", label: "自动检测" },
    ...Object.entries(state.languages).map(([value, label]) => ({ value, label })),
  ];
  const targetOptions = Object.entries(state.languages).map(([value, label]) => ({ value, label }));

  renderSelect(els.sourceLang, sourceOptions);
  renderSelect(els.targetLang, targetOptions);
  els.targetLang.value = "en";
}

function renderSelect(select, options) {
  select.innerHTML = "";
  options.forEach((opt) => {
    const option = document.createElement("option");
    option.value = opt.value;
    option.textContent = opt.label;
    select.appendChild(option);
  });
}

function debounceTranslate() {
  updateCharCount();
  if (!state.auto) return;
  clearTimeout(state.debounce);
  state.debounce = setTimeout(translate, 500);
}

async function translate() {
  const text = els.inputText.value.trim();
  if (!text) {
    setOutput("");
    return;
  }

  setError("");
  setLoading(true);

  const payload = {
    text,
    source: els.sourceLang.value || "",
    target: els.targetLang.value,
  };

  try {
    const res = await fetch("/api/translate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });

    const data = await res.json();
    if (!data.success) {
      setError(data.error || "翻译失败");
      setOutput("");
      return;
    }

    setCached(!!data.cached);
    setOutput(data.text || "");
    addHistory(payload, data.text || "");
    renderHistory();
  } catch (err) {
    setError("网络错误");
  } finally {
    setLoading(false);
  }
}

function setOutput(text) {
  state.outputText = text;
  if (!text) {
    els.outputRender.innerHTML = "";
    return;
  }
  const html = window.marked ? window.marked.parse(text) : text;
  els.outputRender.innerHTML = html;
  if (window.MathJax && window.MathJax.typesetPromise) {
    window.MathJax.typesetPromise([els.outputRender]).catch(() => {});
  }
}

function setError(message) {
  if (!message) {
    els.errorMsg.textContent = "";
    els.errorMsg.classList.add("hidden");
    return;
  }
  els.errorMsg.textContent = message;
  els.errorMsg.classList.remove("hidden");
}

function setCached(isCached) {
  if (isCached) {
    els.cachedBadge.classList.remove("hidden");
  } else {
    els.cachedBadge.classList.add("hidden");
  }
}

function setLoading(isLoading) {
  if (isLoading) {
    els.loadingBadge.classList.remove("hidden");
  } else {
    els.loadingBadge.classList.add("hidden");
  }
}

function updateCharCount() {
  els.charCount.textContent = `${els.inputText.value.length} / 5000`;
}

function swapLanguages() {
  const source = els.sourceLang.value;
  els.sourceLang.value = els.targetLang.value;
  els.targetLang.value = source || "en";
  const input = els.inputText.value;
  els.inputText.value = state.outputText;
  setOutput(input);
  updateCharCount();
  if (state.auto) translate();
}

function copyOutput() {
  const text = state.outputText;
  if (!text) return;
  navigator.clipboard.writeText(text).catch(() => {});
}

async function pasteText() {
  try {
    const text = await navigator.clipboard.readText();
    if (!text) return;
    els.inputText.value = text;
    updateCharCount();
    if (state.auto) translate();
  } catch {
    setError("无法读取剪贴板");
  }
}

function clearInput() {
  els.inputText.value = "";
  updateCharCount();
}

function clearOutput() {
  setOutput("");
}

function addHistory(payload, result) {
  const history = getHistory();
  history.unshift({
    text: payload.text,
    result,
    source: payload.source,
    target: payload.target,
    ts: Date.now(),
  });
  while (history.length > state.maxHistory) history.pop();
  localStorage.setItem(state.historyKey, JSON.stringify(history));
}

function getHistory() {
  try {
    const raw = localStorage.getItem(state.historyKey);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function renderHistory() {
  const history = getHistory();
  els.historyList.innerHTML = "";
  if (!history.length) {
    els.historyList.innerHTML = "<div class=\"history-item\">暂无记录</div>";
    return;
  }
  history.forEach((item) => {
    const el = document.createElement("div");
    el.className = "history-item";
    const preview = item.text.length > 40 ? `${item.text.slice(0, 40)}...` : item.text;
    el.innerHTML = `<strong>${escapeHtml(preview)}</strong><small>${item.source || "auto"} → ${item.target}</small>`;
    el.addEventListener("click", () => {
      els.inputText.value = item.text;
      updateCharCount();
      setOutput(item.result);
    });
    els.historyList.appendChild(el);
  });
}

function escapeHtml(text) {
  const map = { "&": "&amp;", "<": "&lt;", ">": "&gt;" };
  return text.replace(/[&<>]/g, (ch) => map[ch]);
}


function toggleFocus() {
  document.body.classList.toggle("focus-output");
}

function initDivider() {
  const divider = els.divider;
  const panels = els.panels;
  if (!divider || !panels) return;

  let dragging = false;
  const onMove = (e) => {
    if (!dragging) return;
    const rect = panels.getBoundingClientRect();
    const min = rect.width * 0.2;
    const max = rect.width * 0.8;
    let x = e.clientX - rect.left;
    x = Math.max(min, Math.min(max, x));
    const pct = (x / rect.width) * 100;
    panels.style.setProperty("--input-width", `${pct}%`);
  };

  const stop = () => {
    if (!dragging) return;
    dragging = false;
    panels.classList.remove("dragging");
    document.removeEventListener("mousemove", onMove);
    document.removeEventListener("mouseup", stop);
  };

  divider.addEventListener("mousedown", () => {
    dragging = true;
    panels.classList.add("dragging");
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", stop);
  });
}

function updateFontSize() {
  const size = `${els.fontSize.value}px`;
  document.documentElement.style.setProperty("--font-size", size);
}

els.inputText.addEventListener("input", debounceTranslate);
els.sourceLang.addEventListener("change", () => state.auto && translate());
els.targetLang.addEventListener("change", () => state.auto && translate());
els.autoToggle.addEventListener("change", (e) => {
  state.auto = e.target.checked;
  if (state.auto) translate();
});
els.swapBtn.addEventListener("click", swapLanguages);
if (els.focusBtn) {
  els.focusBtn.addEventListener("click", toggleFocus);
}
els.translateBtn.addEventListener("click", translate);
els.copyBtn.addEventListener("click", copyOutput);
els.clearBtn.addEventListener("click", clearInput);
els.pasteBtn.addEventListener("click", pasteText);
els.clearOutput.addEventListener("click", clearOutput);
els.fontSize.addEventListener("input", updateFontSize);
els.clearHistory.addEventListener("click", () => {
  localStorage.removeItem(state.historyKey);
  renderHistory();
});

updateFontSize();
updateCharCount();
initDivider();
loadLanguages().then(renderHistory);
