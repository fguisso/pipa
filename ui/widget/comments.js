// pipa comments widget — pure vanilla, no build, no deps.
// Owner embed: <script src="/api/comments/widget.js" data-page="<uuid>" async></script>
(function () {
  "use strict";

  const SCRIPT = currentScript();
  if (!SCRIPT) return;
  const PAGE = SCRIPT.getAttribute("data-page");
  if (!PAGE) {
    console.warn("[pipa-comments] missing data-page attribute");
    return;
  }
  const ORIGIN = new URL(SCRIPT.src, location.href).origin;
  const API = (path) => `${ORIGIN}/api${path}`;

  injectStyles();
  const root = ensureContainer();
  renderShell(root);
  loadComments();

  function loadComments() {
    const list = root.querySelector(".gpc-list");
    list.innerHTML = `<p class="gpc-muted">Loading…</p>`;
    fetch(API(`/pages/${encodeURIComponent(PAGE)}/comments`), {
      headers: { accept: "application/json" },
    })
      .then((r) => {
        if (r.status === 404) {
          throw new Error("Comments aren't available for this page.");
        }
        if (!r.ok) throw new Error(`unexpected status ${r.status}`);
        return r.json();
      })
      .then((data) => renderList(list, data.comments || []))
      .catch((err) => {
        list.innerHTML = `<p class="gpc-error">${escapeText(err.message)}</p>`;
      });
  }

  function renderList(list, comments) {
    if (!comments.length) {
      list.innerHTML = `<p class="gpc-muted">No comments yet. Be the first!</p>`;
      return;
    }
    list.innerHTML = "";
    comments.forEach((c) => list.appendChild(renderComment(c)));
  }

  function renderComment(c) {
    const div = document.createElement("article");
    div.className = "gpc-comment";
    div.dataset.id = c.id;

    const head = document.createElement("header");
    head.className = "gpc-meta";
    const author = document.createElement("span");
    author.className = "gpc-author";
    author.textContent = c.author;
    const time = document.createElement("time");
    time.className = "gpc-ts";
    const d = new Date((c.ts || 0) * 1000);
    time.dateTime = d.toISOString();
    time.textContent = d.toLocaleString();
    head.appendChild(author);
    head.appendChild(time);

    const body = document.createElement("div");
    body.className = "gpc-body";
    body.innerHTML = c.html || "";

    div.appendChild(head);
    div.appendChild(body);
    return div;
  }

  function renderShell(root) {
    root.innerHTML = `
      <section class="gpc-wrap">
        <h2 class="gpc-title">Comments</h2>
        <div class="gpc-list" role="feed" aria-busy="false"></div>
        <form class="gpc-form" novalidate>
          <label class="gpc-label">
            Name
            <input class="gpc-input" name="author" required maxlength="64" autocomplete="nickname" />
          </label>
          <label class="gpc-label">
            Comment
            <textarea class="gpc-textarea" name="body" required maxlength="2000" rows="4"></textarea>
          </label>
          <label class="gpc-label">
            Contact (optional, not shown publicly)
            <input class="gpc-input" name="contact" type="email" maxlength="256" autocomplete="email" />
          </label>
          <div class="gpc-actions">
            <button class="gpc-button" type="submit">Post comment</button>
            <span class="gpc-flash" role="status"></span>
          </div>
        </form>
      </section>
    `;
    const form = root.querySelector(".gpc-form");
    form.addEventListener("submit", onSubmit);
  }

  function onSubmit(ev) {
    ev.preventDefault();
    const form = ev.currentTarget;
    const flash = form.querySelector(".gpc-flash");
    const button = form.querySelector(".gpc-button");
    const payload = {
      author: form.author.value.trim(),
      body: form.body.value.trim(),
      contact: form.contact.value.trim() || undefined,
    };
    if (!payload.author || !payload.body) {
      setFlash(flash, "Name and comment are required.", true);
      return;
    }
    button.disabled = true;
    setFlash(flash, "Posting…", false);

    fetch(API(`/pages/${encodeURIComponent(PAGE)}/comments`), {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify(payload),
    })
      .then(async (r) => {
        if (r.status === 429) {
          const retry = r.headers.get("retry-after") || "60";
          throw new Error(`Rate limited. Try again in ${retry}s.`);
        }
        if (r.status === 404) {
          throw new Error("Comments aren't enabled here.");
        }
        if (!r.ok) {
          const err = await safeJson(r);
          throw new Error(err?.message || `Error: ${r.status}`);
        }
        return r.json();
      })
      .then((c) => {
        form.body.value = "";
        if (c.status === "pending") {
          setFlash(flash, "Thanks — your comment is awaiting moderation.", false);
        } else {
          setFlash(flash, "Posted.", false);
          const list = root.querySelector(".gpc-list");
          const placeholder = list.querySelector(".gpc-muted");
          if (placeholder) list.innerHTML = "";
          const node = renderComment({
            id: c.id,
            author: payload.author,
            html: c.html,
            ts: c.ts,
          });
          list.appendChild(node);
        }
      })
      .catch((err) => setFlash(flash, err.message, true))
      .finally(() => {
        button.disabled = false;
      });
  }

  function safeJson(r) {
    return r.json().catch(() => null);
  }

  function setFlash(el, msg, isError) {
    el.textContent = msg;
    el.className = "gpc-flash" + (isError ? " gpc-flash-error" : "");
  }

  function ensureContainer() {
    let el = document.getElementById("pipa-comments");
    if (!el) {
      el = document.createElement("div");
      el.id = "pipa-comments";
      document.body.appendChild(el);
    }
    return el;
  }

  function currentScript() {
    return (
      document.currentScript ||
      Array.from(document.scripts).find((s) => s.src && s.src.includes("widget.js"))
    );
  }

  function escapeText(s) {
    const d = document.createElement("div");
    d.textContent = String(s == null ? "" : s);
    return d.innerHTML;
  }

  function injectStyles() {
    // External stylesheet — inline <style> tags are blocked by the strict
    // per-page CSP (`default-src 'self'` only allows external resources).
    if (document.getElementById("gpc-styles")) return;
    const link = document.createElement("link");
    link.id = "gpc-styles";
    link.rel = "stylesheet";
    link.href = `${ORIGIN}/api/comments/widget.css`;
    document.head.appendChild(link);
  }
})();
