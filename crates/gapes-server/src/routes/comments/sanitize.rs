//! Markdown → HTML pipeline for comment bodies.
//!
//! Two passes: pulldown-cmark renders a deliberately narrow subset of
//! markdown, then ammonia scrubs the output against a strict allowlist. The
//! sanitizer is the source of truth — pulldown is just a convenience renderer.
//! Anything that slips past pulldown (raw HTML events, exotic constructs) is
//! still removed by ammonia.
//!
//! Allowed tags (after both passes): `p, em, strong, code, pre, ul, ol, li,
//! a, br`. Anchors are forced to `rel="nofollow ugc"` and `target="_blank"`,
//! and URL schemes are restricted to http / https / mailto. Images, tables,
//! headings, blockquotes, and raw HTML are not emitted by the pipeline.

use std::collections::{HashMap, HashSet};

use pulldown_cmark::{Options, Parser, html};

pub fn markdown_to_safe_html(src: &str) -> String {
    // Default options ⇒ no tables, no footnotes, no task lists. We don't add
    // anything; the subset stays intentionally tiny.
    let opts = Options::empty();
    let parser = Parser::new_ext(src, opts);

    let mut rendered = String::with_capacity(src.len() + 32);
    html::push_html(&mut rendered, parser);

    let allowed_tags: HashSet<&str> = ["p", "em", "strong", "code", "pre", "ul", "ol", "li", "a", "br"]
        .into_iter()
        .collect();

    let mut tag_attrs: HashMap<&str, HashSet<&str>> = HashMap::new();
    tag_attrs.insert("a", ["href", "title"].into_iter().collect());

    let url_schemes: HashSet<&str> = ["http", "https", "mailto"].into_iter().collect();

    let mut force_anchor_attrs: HashMap<&str, HashMap<&str, &str>> = HashMap::new();
    let mut anchor_forced: HashMap<&str, &str> = HashMap::new();
    anchor_forced.insert("target", "_blank");
    force_anchor_attrs.insert("a", anchor_forced);

    let mut builder = ammonia::Builder::default();
    builder
        .tags(allowed_tags)
        .tag_attributes(tag_attrs)
        .url_schemes(url_schemes)
        .link_rel(Some("nofollow ugc"))
        .set_tag_attribute_values(force_anchor_attrs);

    builder.clean(&rendered).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_scripts_and_raw_html() {
        let html = markdown_to_safe_html("hello **world** <script>alert(1)</script>");
        assert!(!html.contains("<script"));
        assert!(html.contains("<strong>world</strong>"));
    }

    #[test]
    fn forces_link_rel_and_target() {
        let html = markdown_to_safe_html("see [docs](https://example.com)");
        assert!(html.contains("rel=\"nofollow ugc\""));
        assert!(html.contains("target=\"_blank\""));
    }

    #[test]
    fn drops_image_tags() {
        let html = markdown_to_safe_html("![alt](https://example.com/x.png)");
        assert!(!html.contains("<img"));
    }
}
