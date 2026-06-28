# Use Cases

Real-world ways people use pipa to turn agent output into shareable, gated pages.

## Security review reports, shared with a friend

I use the pipa skill inside my agent as the last mile of a source-code
security review: once the agent has finished researching a codebase and writing
up its findings, I ask it to render the report as both an `index.html` (a
polished, beautiful, human-readable page) and a `report.md` (the same content as
plain Markdown), then deploy them together to my pipa server as a single
password-protected page. I get back one URL plus a password, which I share with a
friend. They open the HTML to read it, or drop the `.md` URL and password
straight into their own agent so it can pull the report over auth and implement
the fixes.

## Agent final reports, nicer to read on mobile

Agents are great at producing `.md` files for their final report, research, or
prompt output. Today I use the Hermes agent, and with the pipa skill it can
transform that Markdown into HTML and serve it on my internal pipa. It is far
more pleasant to read the final report as a mobile website than as a mobile
Telegram message, which is Hermes' default delivery.
