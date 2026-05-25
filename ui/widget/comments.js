// pipa annotation widget — pure vanilla JS, no build, no deps.
// Inline text-selection commenting system with FAB + sidebar.
// Owner embed: <script src="/api/comments/widget.js" data-page="<uuid>" async></script>
(function () {
  "use strict";

  var SCRIPT = currentScript();
  if (!SCRIPT) return;
  var PAGE = SCRIPT.getAttribute("data-page");
  if (!PAGE) {
    console.warn("[pipa] missing data-page attribute");
    return;
  }
  var ORIGIN = new URL(SCRIPT.src, location.href).origin;
  var API = function (path) { return ORIGIN + "/api" + path; };
  var ADMIN_TOKEN = SCRIPT.getAttribute("data-token") || "";

  var COMMENT_SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>';
  var TRASH_SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>';

  var comments = [];
  var sidebarOpen = false;
  var fab, badge, sidebar, sidebarList, tooltip, inlineForm, overlay, hoverBadge, modal;
  var pendingAnchor = null;
  var pendingDeleteId = null;

  function init() {
    injectStyles();
    buildUI();
    loadComments();
    bindSelectionEvents();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }

  // ── UI construction ──────────────────────────────────────────────

  function buildUI() {
    fab = el("button", "gpc-fab");
    fab.setAttribute("aria-label", "Open annotations");
    fab.innerHTML = COMMENT_SVG;
    badge = el("span", "gpc-fab-badge");
    badge.hidden = true;
    fab.appendChild(badge);
    fab.addEventListener("click", toggleSidebar);
    document.body.appendChild(fab);

    overlay = el("div", "gpc-overlay");
    overlay.hidden = true;
    overlay.addEventListener("click", closeSidebar);
    document.body.appendChild(overlay);

    sidebar = el("aside", "gpc-sidebar");
    sidebar.setAttribute("aria-label", "Annotations");
    var header = el("header", "gpc-sidebar-header");
    var title = el("h2", "gpc-sidebar-title");
    title.textContent = "Annotations";
    var closeBtn = el("button", "gpc-sidebar-close");
    closeBtn.setAttribute("aria-label", "Close");
    closeBtn.textContent = "×";
    closeBtn.addEventListener("click", closeSidebar);
    header.appendChild(title);
    header.appendChild(closeBtn);
    sidebarList = el("div", "gpc-sidebar-list");
    sidebarList.setAttribute("role", "feed");
    sidebar.appendChild(header);
    sidebar.appendChild(sidebarList);
    document.body.appendChild(sidebar);

    tooltip = el("div", "gpc-tooltip");
    tooltip.hidden = true;
    tooltip.textContent = "+ Comment";
    tooltip.addEventListener("mousedown", onTooltipClick);
    document.body.appendChild(tooltip);

    inlineForm = el("div", "gpc-inline-form");
    inlineForm.hidden = true;
    document.body.appendChild(inlineForm);

    hoverBadge = el("div", "gpc-hover-badge");
    hoverBadge.innerHTML = COMMENT_SVG + '<span class="gpc-hover-badge-count"></span>';
    document.body.appendChild(hoverBadge);

    modal = el("div", "gpc-modal");
    modal.innerHTML =
      '<div class="gpc-modal-backdrop"></div>' +
      '<div class="gpc-modal-box">' +
        '<p class="gpc-modal-text">Delete this comment?</p>' +
        '<div class="gpc-modal-actions">' +
          '<button class="gpc-modal-cancel" type="button">Cancel</button>' +
          '<button class="gpc-modal-confirm" type="button">Delete</button>' +
        '</div>' +
      '</div>';
    modal.querySelector(".gpc-modal-backdrop").addEventListener("click", closeModal);
    modal.querySelector(".gpc-modal-cancel").addEventListener("click", closeModal);
    modal.querySelector(".gpc-modal-confirm").addEventListener("click", confirmDelete);
    document.body.appendChild(modal);
  }

  // ── Load comments + highlight ────────────────────────────────────

  function loadComments() {
    fetch(API("/pages/" + encodeURIComponent(PAGE) + "/comments"), {
      headers: { accept: "application/json" },
    })
      .then(function (r) {
        if (r.status === 404) return { comments: [] };
        if (!r.ok) return { comments: [] };
        return r.json();
      })
      .then(function (data) {
        comments = data.comments || [];
        updateBadge();
        highlightAll();
        renderSidebarList();
      })
      .catch(function () {
        comments = [];
      });
  }

  function updateBadge() {
    if (comments.length > 0) {
      badge.textContent = String(comments.length);
      badge.hidden = false;
    } else {
      badge.hidden = true;
    }
  }

  // ── Anchor grouping ──────────────────────────────────────────────

  function anchorKey(a) {
    return a.selector + "\0" + a.offset + "\0" + a.text;
  }

  function groupByAnchor() {
    var groups = [];
    var map = {};
    for (var i = 0; i < comments.length; i++) {
      var c = comments[i];
      if (!c.anchor) continue;
      var key = anchorKey(c.anchor);
      if (map[key] === undefined) {
        map[key] = groups.length;
        groups.push({ anchor: c.anchor, comments: [c] });
      } else {
        groups[map[key]].comments.push(c);
      }
    }
    return groups;
  }

  // ── Sidebar ──────────────────────────────────────────────────────

  function toggleSidebar() {
    if (sidebarOpen) closeSidebar();
    else openSidebar();
  }

  function openSidebar() {
    sidebarOpen = true;
    sidebar.classList.add("gpc-sidebar--open");
    overlay.hidden = false;
  }

  function closeSidebar() {
    sidebarOpen = false;
    sidebar.classList.remove("gpc-sidebar--open");
    overlay.hidden = true;
  }

  function renderSidebarList() {
    sidebarList.innerHTML = "";
    var groups = groupByAnchor();
    if (groups.length === 0) {
      var empty = el("p", "gpc-sidebar-empty");
      empty.textContent = "No annotations yet. Select text on the page to add one.";
      sidebarList.appendChild(empty);
      return;
    }
    for (var i = 0; i < groups.length; i++) {
      sidebarList.appendChild(renderGroup(groups[i]));
    }
  }

  function renderGroup(group) {
    var key = anchorKey(group.anchor);
    var card = el("div", "gpc-group");
    card.dataset.anchorKey = key;

    var quoteRow = el("div", "gpc-group-quote-row");
    var quote = el("div", "gpc-card-quote");
    quote.textContent = "“" + truncate(group.anchor.text, 80) + "”";
    quoteRow.appendChild(quote);
    quoteRow.addEventListener("click", function () {
      scrollToAnchor(key);
    });
    card.appendChild(quoteRow);

    for (var i = 0; i < group.comments.length; i++) {
      card.appendChild(renderComment(group.comments[i]));
    }

    card.appendChild(renderReplyInput(group.anchor));

    return card;
  }

  function renderComment(c) {
    var item = el("div", "gpc-comment");
    item.dataset.commentId = c.id;
    item.style.cursor = "pointer";
    item.addEventListener("click", function () {
      var key = anchorKey(c.anchor);
      scrollToAnchor(key);
    });

    var meta = el("div", "gpc-card-meta");
    var author = el("span", "gpc-card-author");
    author.textContent = c.author;
    var ts = el("time", "");
    var d = new Date((c.ts || 0) * 1000);
    ts.dateTime = d.toISOString();
    ts.textContent = d.toLocaleString();
    meta.appendChild(author);
    meta.appendChild(ts);
    item.appendChild(meta);

    var body = el("div", "gpc-card-body");
    body.innerHTML = c.html || "";
    item.appendChild(body);

    if (ADMIN_TOKEN) {
      var delBtn = el("button", "gpc-card-delete");
      delBtn.innerHTML = TRASH_SVG;
      delBtn.setAttribute("aria-label", "Delete comment");
      delBtn.setAttribute("title", "Delete");
      delBtn.addEventListener("click", function (e) {
        e.stopPropagation();
        deleteComment(c.id);
      });
      item.appendChild(delBtn);
    }

    return item;
  }

  function renderReplyInput(anchor) {
    var row = el("div", "gpc-reply-row");

    var nameInput = el("input", "gpc-reply-name");
    nameInput.placeholder = "Name";
    nameInput.maxLength = 64;
    var savedName = getSavedName();
    if (savedName) nameInput.value = savedName;
    row.appendChild(nameInput);

    var wrap = el("div", "gpc-reply-wrap");
    var input = el("input", "gpc-reply-input");
    input.placeholder = "Reply…";
    input.maxLength = 2000;
    var sendBtn = el("button", "gpc-reply-send");
    sendBtn.textContent = "→";
    sendBtn.setAttribute("aria-label", "Send");
    wrap.appendChild(input);
    wrap.appendChild(sendBtn);
    row.appendChild(wrap);

    var flash = el("span", "gpc-reply-flash");
    row.appendChild(flash);

    function submit() {
      var author = nameInput.value.trim();
      var body = input.value.trim();
      if (!author || !body) {
        flash.textContent = "Name and comment required.";
        flash.className = "gpc-reply-flash gpc-reply-flash-error";
        return;
      }
      sendBtn.disabled = true;
      flash.textContent = "";

      var payload = {
        author: author,
        body: body,
        anchor: { selector: anchor.selector, text: anchor.text, offset: anchor.offset },
      };

      fetch(API("/pages/" + encodeURIComponent(PAGE) + "/comments"), {
        method: "POST",
        headers: { "content-type": "application/json", accept: "application/json" },
        body: JSON.stringify(payload),
      })
        .then(function (r) {
          if (r.status === 429) {
            var retry = r.headers.get("retry-after") || "60";
            throw new Error("Rate limited. Try in " + retry + "s.");
          }
          if (!r.ok) return r.json().then(function (e) { throw new Error(e.message || "Error"); });
          return r.json();
        })
        .then(function (c) {
          saveName(author);
          input.value = "";
          if (c.status === "pending") {
            flash.textContent = "Awaiting moderation.";
            flash.className = "gpc-reply-flash";
            return;
          }
          comments.push({ id: c.id, author: author, html: c.html, ts: c.ts, anchor: c.anchor });
          updateBadge();
          clearHighlights();
          highlightAll();
          renderSidebarList();
        })
        .catch(function (err) {
          flash.textContent = err.message;
          flash.className = "gpc-reply-flash gpc-reply-flash-error";
        })
        .finally(function () {
          sendBtn.disabled = false;
        });
    }

    sendBtn.addEventListener("click", submit);
    input.addEventListener("keydown", function (e) {
      if (e.key === "Enter") { e.preventDefault(); submit(); }
    });

    return row;
  }

  // ── Delete ───────────────────────────────────────────────────────

  function deleteComment(commentId) {
    pendingDeleteId = commentId;
    modal.classList.add("gpc-modal--open");
  }

  function closeModal() {
    modal.classList.remove("gpc-modal--open");
    pendingDeleteId = null;
  }

  function confirmDelete() {
    var commentId = pendingDeleteId;
    if (!commentId) return;
    var btn = modal.querySelector(".gpc-modal-confirm");
    btn.disabled = true;
    btn.textContent = "Deleting…";
    fetch(API("/comments/" + encodeURIComponent(commentId)), {
      method: "DELETE",
      headers: { authorization: "Bearer " + ADMIN_TOKEN },
    })
      .then(function (r) {
        if (!r.ok) throw new Error("Failed: " + r.status);
        comments = comments.filter(function (c) { return c.id !== commentId; });
        updateBadge();
        clearHighlights();
        highlightAll();
        renderSidebarList();
        closeModal();
      })
      .catch(function () {
        closeModal();
      })
      .finally(function () {
        btn.disabled = false;
        btn.textContent = "Delete";
      });
  }

  // ── Text selection → tooltip ─────────────────────────────────────

  function bindSelectionEvents() {
    document.addEventListener("mouseup", onSelectionChange);
    document.addEventListener("touchend", onSelectionChange);
    document.addEventListener("mousedown", function (e) {
      if (!isInsideWidget(e.target) && !inlineForm.hidden) {
        hideInlineForm();
      }
    });
  }

  function onSelectionChange() {
    setTimeout(function () {
      var sel = window.getSelection();
      if (!sel || sel.isCollapsed || !sel.toString().trim()) {
        hideTooltip();
        return;
      }
      if (isInsideWidget(sel.anchorNode)) {
        hideTooltip();
        return;
      }
      var range = sel.getRangeAt(0);
      var rect = range.getBoundingClientRect();
      positionElement(tooltip, rect, true);
      tooltip.hidden = false;
    }, 10);
  }

  function onTooltipClick(e) {
    e.preventDefault();
    e.stopPropagation();

    var sel = window.getSelection();
    if (!sel || sel.isCollapsed) {
      hideTooltip();
      return;
    }

    var range = sel.getRangeAt(0);
    var text = sel.toString().trim();
    if (!text) {
      hideTooltip();
      return;
    }

    var ancestor = range.commonAncestorContainer;
    var element = ancestor.nodeType === Node.TEXT_NODE ? ancestor.parentElement : ancestor;
    var selector = computeSelector(element);
    var startOff = computeOffset(element, range);
    var endOff = computeEndOffset(element, range);
    var anchorText = getTextAt(element, startOff, endOff - startOff);
    var rect = range.getBoundingClientRect();

    if (!anchorText) {
      hideTooltip();
      return;
    }

    pendingAnchor = {
      selector: selector,
      text: anchorText.substring(0, 500),
      offset: startOff,
      rect: rect,
    };

    sel.removeAllRanges();
    hideTooltip();
    showInlineForm(rect);
  }

  // ── Inline form (first comment on new selection) ─────────────────

  function showInlineForm(rect) {
    inlineForm.innerHTML = "";

    var quote = el("div", "gpc-inline-form-quote");
    quote.textContent = "“" + truncate(pendingAnchor.text, 100) + "”";
    inlineForm.appendChild(quote);

    var nameLabel = el("label", "gpc-inline-label");
    nameLabel.textContent = "Name";
    var nameInput = el("input", "gpc-inline-input");
    nameInput.name = "author";
    nameInput.required = true;
    nameInput.maxLength = 64;
    nameInput.autocomplete = "nickname";
    var saved = getSavedName();
    if (saved) nameInput.value = saved;
    nameLabel.appendChild(nameInput);
    inlineForm.appendChild(nameLabel);

    var bodyLabel = el("label", "gpc-inline-label");
    bodyLabel.textContent = "Comment";
    var bodyInput = el("textarea", "gpc-inline-textarea");
    bodyInput.name = "body";
    bodyInput.required = true;
    bodyInput.maxLength = 2000;
    bodyInput.rows = 3;
    bodyLabel.appendChild(bodyInput);
    inlineForm.appendChild(bodyLabel);

    var actions = el("div", "gpc-inline-actions");
    var submit = el("button", "gpc-inline-submit");
    submit.type = "button";
    submit.textContent = "Post";
    var cancel = el("button", "gpc-inline-cancel");
    cancel.type = "button";
    cancel.textContent = "Cancel";
    var flash = el("span", "gpc-inline-flash");
    actions.appendChild(submit);
    actions.appendChild(cancel);
    actions.appendChild(flash);
    inlineForm.appendChild(actions);

    positionElement(inlineForm, rect, false);
    inlineForm.hidden = false;

    setTimeout(function () {
      (saved ? bodyInput : nameInput).focus();
    }, 50);

    submit.addEventListener("click", function () {
      var author = nameInput.value.trim();
      var body = bodyInput.value.trim();
      if (!author || !body) {
        setFlash(flash, "Name and comment are required.", true);
        return;
      }
      submit.disabled = true;
      setFlash(flash, "Posting…", false);

      var payload = {
        author: author,
        body: body,
        anchor: {
          selector: pendingAnchor.selector,
          text: pendingAnchor.text,
          offset: pendingAnchor.offset,
        },
      };

      fetch(API("/pages/" + encodeURIComponent(PAGE) + "/comments"), {
        method: "POST",
        headers: { "content-type": "application/json", accept: "application/json" },
        body: JSON.stringify(payload),
      })
        .then(function (r) {
          if (r.status === 429) {
            var retry = r.headers.get("retry-after") || "60";
            throw new Error("Rate limited. Try again in " + retry + "s.");
          }
          if (r.status === 404) throw new Error("Comments aren't enabled.");
          if (!r.ok) return r.json().then(function (e) { throw new Error(e.message || "Error " + r.status); });
          return r.json();
        })
        .then(function (c) {
          saveName(author);
          hideInlineForm();

          if (c.status === "pending") {
            openSidebar();
            return;
          }

          comments.push({ id: c.id, author: author, html: c.html, ts: c.ts, anchor: c.anchor });
          updateBadge();
          clearHighlights();
          highlightAll();
          renderSidebarList();
          openSidebar();
        })
        .catch(function (err) {
          setFlash(flash, err.message, true);
          submit.disabled = false;
        });
    });

    cancel.addEventListener("click", hideInlineForm);
  }

  function hideInlineForm() {
    inlineForm.hidden = true;
    inlineForm.innerHTML = "";
    pendingAnchor = null;
  }

  function hideTooltip() {
    tooltip.hidden = true;
  }

  // ── Highlight logic ──────────────────────────────────────────────
  // Highlights are created ONCE per unique anchor, not per comment.
  // This avoids DOM corruption when multiple comments share the same
  // text passage.

  function clearHighlights() {
    var marks = document.querySelectorAll(".gpc-highlight");
    for (var i = 0; i < marks.length; i++) {
      var m = marks[i];
      var parent = m.parentNode;
      while (m.firstChild) parent.insertBefore(m.firstChild, m);
      parent.removeChild(m);
    }
    document.body.normalize();
  }

  function highlightAll() {
    var groups = groupByAnchor();
    for (var i = 0; i < groups.length; i++) {
      highlightAnchor(groups[i]);
    }
  }

  function highlightAnchor(group) {
    var a = group.anchor;
    var key = anchorKey(a);
    var ids = group.comments.map(function (c) { return c.id; }).join(",");
    try {
      var container = document.querySelector(a.selector);
      if (container) {
        var textMatch = getTextAt(container, a.offset, a.text.length);
        if (textMatch === a.text) {
          wrapTextRange(container, a.offset, a.text.length, key, ids);
          return;
        }
        if (searchAndWrap(container, a.text, key, ids)) return;
      }
    } catch (e) { /* selector invalid */ }
    fuzzyHighlight(a, key, ids);
  }

  function wrapTextRange(container, charOffset, length, key, ids) {
    var walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    var current = 0;
    var node;
    var segments = [];

    while ((node = walker.nextNode())) {
      var nodeLen = node.textContent.length;
      if (current + nodeLen > charOffset && current < charOffset + length) {
        var startInNode = Math.max(0, charOffset - current);
        var endInNode = Math.min(nodeLen, charOffset + length - current);
        segments.push({ node: node, start: startInNode, end: endInNode });
      }
      current += nodeLen;
      if (current >= charOffset + length) break;
    }

    for (var i = segments.length - 1; i >= 0; i--) {
      var seg = segments[i];
      var range = document.createRange();
      range.setStart(seg.node, seg.start);
      range.setEnd(seg.node, seg.end);
      var mark = document.createElement("mark");
      mark.className = "gpc-highlight";
      mark.dataset.anchorKey = key;
      mark.dataset.commentIds = ids;
      bindHighlightEvents(mark);
      try {
        range.surroundContents(mark);
      } catch (e) {
        var extracted = range.extractContents();
        mark.appendChild(extracted);
        range.insertNode(mark);
      }
    }
  }

  function searchAndWrap(container, text, key, ids) {
    var walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    var fullText = "";
    var node;
    while ((node = walker.nextNode())) {
      fullText += node.textContent;
    }
    var idx = fullText.indexOf(text);
    if (idx === -1) return false;
    wrapTextRange(container, idx, text.length, key, ids);
    return true;
  }

  function fuzzyHighlight(anchor, key, ids) {
    var walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    var node;
    while ((node = walker.nextNode())) {
      if (isInsideWidget(node)) continue;
      var idx = node.textContent.indexOf(anchor.text);
      if (idx !== -1) {
        var range = document.createRange();
        range.setStart(node, idx);
        range.setEnd(node, Math.min(idx + anchor.text.length, node.textContent.length));
        var mark = document.createElement("mark");
        mark.className = "gpc-highlight";
        mark.dataset.anchorKey = key;
        mark.dataset.commentIds = ids;
        bindHighlightEvents(mark);
        try { range.surroundContents(mark); } catch (e) { /* cross-boundary */ }
        return;
      }
    }
  }

  function getTextAt(container, offset, length) {
    var walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    var current = 0;
    var result = "";
    var node;
    while ((node = walker.nextNode())) {
      var nodeLen = node.textContent.length;
      if (current + nodeLen > offset) {
        var start = Math.max(0, offset - current);
        var end = Math.min(nodeLen, offset + length - current);
        result += node.textContent.substring(start, end);
        if (result.length >= length) break;
      }
      current += nodeLen;
    }
    return result;
  }

  // ── Highlight interactions ───────────────────────────────────────

  function bindHighlightEvents(mark) {
    mark.addEventListener("click", onHighlightClick);
    mark.addEventListener("mouseenter", onHighlightEnter);
    mark.addEventListener("mouseleave", onHighlightLeave);
  }

  function onHighlightClick(e) {
    var key = e.currentTarget.dataset.anchorKey;
    openSidebar();
    scrollGroupIntoView(key);
  }

  function onHighlightEnter(e) {
    var mark = e.currentTarget;
    var ids = (mark.dataset.commentIds || "").split(",");
    var countEl = hoverBadge.querySelector(".gpc-hover-badge-count");
    countEl.textContent = String(ids.length);
    var rect = mark.getBoundingClientRect();
    var scrollX = window.pageXOffset || document.documentElement.scrollLeft;
    var scrollY = window.pageYOffset || document.documentElement.scrollTop;
    hoverBadge.style.cssText =
      "position:absolute;left:" + (rect.right + scrollX + 4) + "px;top:" +
      (rect.top + scrollY + (rect.height / 2) - 10) + "px";
    hoverBadge.classList.add("gpc-hover-badge--visible");
  }

  function onHighlightLeave() {
    hoverBadge.classList.remove("gpc-hover-badge--visible");
  }

  function scrollToAnchor(key) {
    var mark = document.querySelector('.gpc-highlight[data-anchor-key="' + CSS.escape(key) + '"]');
    if (!mark) return;
    mark.scrollIntoView({ behavior: "smooth", block: "center" });
    var all = document.querySelectorAll('.gpc-highlight[data-anchor-key="' + CSS.escape(key) + '"]');
    for (var i = 0; i < all.length; i++) {
      all[i].classList.remove("gpc-highlight--pulse");
      void all[i].offsetWidth;
      all[i].classList.add("gpc-highlight--pulse");
    }
    setTimeout(function () {
      for (var i = 0; i < all.length; i++) all[i].classList.remove("gpc-highlight--pulse");
    }, 1800);
  }

  function scrollGroupIntoView(key) {
    var group = sidebarList.querySelector('.gpc-group[data-anchor-key="' + CSS.escape(key) + '"]');
    if (group) {
      group.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  }

  // ── CSS selector computation ─────────────────────────────────────

  function computeSelector(node) {
    var element = node.nodeType === Node.TEXT_NODE ? node.parentElement : node;
    var parts = [];
    while (element && element !== document.body && element !== document.documentElement) {
      var tag = element.tagName.toLowerCase();
      if (element.id && document.querySelectorAll("#" + cssEscape(element.id)).length === 1) {
        parts.unshift("#" + cssEscape(element.id));
        break;
      }
      var parent = element.parentElement;
      if (parent) {
        var siblings = [];
        for (var i = 0; i < parent.children.length; i++) {
          if (parent.children[i].tagName === element.tagName) siblings.push(parent.children[i]);
        }
        if (siblings.length > 1) {
          var idx = siblings.indexOf(element) + 1;
          tag += ":nth-of-type(" + idx + ")";
        }
      }
      parts.unshift(tag);
      element = element.parentElement;
    }
    return parts.join(" > ");
  }

  function computeOffset(container, range) {
    var element = container.nodeType === Node.TEXT_NODE ? container.parentElement : container;
    var walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    var offset = 0;
    var node;
    while ((node = walker.nextNode())) {
      if (node === range.startContainer) return offset + range.startOffset;
      offset += node.textContent.length;
    }
    return offset;
  }

  function computeEndOffset(container, range) {
    var element = container.nodeType === Node.TEXT_NODE ? container.parentElement : container;
    var walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    var offset = 0;
    var node;
    while ((node = walker.nextNode())) {
      if (node === range.endContainer) return offset + range.endOffset;
      offset += node.textContent.length;
    }
    return offset;
  }

  // ── Utilities ────────────────────────────────────────────────────

  function el(tag, className) {
    var e = document.createElement(tag);
    if (className) e.className = className;
    return e;
  }

  function positionElement(element, rect, above) {
    var scrollX = window.pageXOffset || document.documentElement.scrollLeft;
    var scrollY = window.pageYOffset || document.documentElement.scrollTop;
    var left = rect.left + scrollX + rect.width / 2;
    var top;
    if (above) {
      top = rect.top + scrollY - 36;
    } else {
      top = rect.bottom + scrollY + 8;
    }
    element.style.cssText =
      "position:absolute;left:" + left + "px;top:" + top + "px;transform:translateX(-50%)";
  }

  function isInsideWidget(node) {
    var e = node;
    while (e) {
      if (e.nodeType === Node.ELEMENT_NODE) {
        var cls = e.className || "";
        if (typeof cls === "string" && cls.indexOf("gpc-") !== -1) return true;
        if (e === fab || e === sidebar || e === tooltip || e === inlineForm) return true;
      }
      e = e.parentNode;
    }
    return false;
  }

  function setFlash(e, msg, isError) {
    e.textContent = msg;
    e.className = "gpc-inline-flash" + (isError ? " gpc-inline-flash-error" : "");
  }

  function truncate(s, n) {
    if (!s) return "";
    if (s.length <= n) return s;
    return s.substring(0, n - 1) + "…";
  }

  function cssEscape(s) {
    if (typeof CSS !== "undefined" && CSS.escape) return CSS.escape(s);
    return s.replace(/([^\w-])/g, "\\$1");
  }

  function currentScript() {
    return (
      document.currentScript ||
      Array.from(document.scripts).find(function (s) {
        return s.src && s.src.indexOf("widget.js") !== -1;
      })
    );
  }

  function getSavedName() {
    try { return localStorage.getItem("gpc-author") || ""; } catch (e) { return ""; }
  }
  function saveName(name) {
    try { localStorage.setItem("gpc-author", name); } catch (e) { /* noop */ }
  }

  function injectStyles() {
    if (document.getElementById("gpc-styles")) return;
    var link = document.createElement("link");
    link.id = "gpc-styles";
    link.rel = "stylesheet";
    link.href = ORIGIN + "/api/comments/widget.css";
    document.head.appendChild(link);
  }
})();
