# Comments on a page

Owner side, one CLI call:

```sh
pipa comments enable 01HXYZ                       # turn it on
pipa comments require-approval 01HXYZ --on        # optional: hold new comments for review
```

Reader side, one tag in your HTML:

```html
<script src="http://127.0.0.1:8080/api/comments/widget.js"
        data-page="01HXYZ" async></script>
```

The widget is ~5 KB of vanilla JS, no framework. It renders a form, posts to the API, handles rate limiting (`429 Retry-After`), and shows "awaiting moderation" when needed. Markdown is sanitized server-side through `pulldown-cmark + ammonia`: `<script>` is text, links get `rel="nofollow ugc" target="_blank"`, raw HTML is dropped.

Moderate from the terminal:

```sh
pipa comments ls 01HXYZ --status pending
pipa comments approve 01HCMT...A1B2
pipa comments hide    01HCMT...C3D4
```
