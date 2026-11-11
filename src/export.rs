use pinboard_core::{Canvas, markdown};

pub fn render(c: &Canvas, format: &str) -> Option<(&'static str, String)> {
    match format {
        "markdown" => Some(("text/markdown; charset=utf-8", markdown(c))),
        "json" => Some(("application/json", serde_json::to_string_pretty(c).ok()?)),
        "mermaid" => {
            let mut out = String::from("flowchart LR\n");
            // Encode user text as Mermaid numeric entities; never interpolate directives.
            let label = |s: &str| {
                s.chars()
                    .map(|ch| format!("#{};", ch as u32))
                    .collect::<String>()
            };
            for (i, n) in c.nodes.iter().enumerate() {
                out.push_str(&format!("  n{i}[\"{}\"]\n", label(&n.title)));
            }
            for e in &c.edges {
                let a = c.nodes.iter().position(|n| n.id == e.from)?;
                let b = c.nodes.iter().position(|n| n.id == e.to)?;
                out.push_str(&format!("  n{a} -->|\"{}\"| n{b}\n", label(&e.label)));
            }
            Some(("text/plain; charset=utf-8", out))
        }
        "slides" => {
            let mut html = format!(
                "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{}</title><style>body{{margin:0;background:#f5f3ee;color:#292d29;font:24px system-ui}}section{{box-sizing:border-box;min-height:100vh;padding:10vh 12vw;scroll-snap-align:start;page-break-after:always}}h1{{font-size:clamp(40px,7vw,92px);letter-spacing:-.05em;max-width:1100px}}p{{white-space:pre-wrap;line-height:1.6;max-width:900px}}small{{color:#666}}@media print{{section{{min-height:95vh}}}}</style><section><small>PINBOARD · {}</small><h1>{}</h1><p>Scroll to present · Print to save as PDF</p></section>",
                escape(&c.title),
                escape(&c.updated_at[..10]),
                escape(&c.title)
            );
            let mut nodes = c.nodes.iter().collect::<Vec<_>>();
            nodes.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
            for n in nodes {
                html.push_str(&format!(
                    "<section><small>{}</small><h1>{}</h1><p>{}</p><small>{} {}</small></section>",
                    escape(&n.group),
                    escape(&n.title),
                    escape(&n.body),
                    escape(&n.owner),
                    escape(&n.due)
                ));
            }
            html.push_str("</html>");
            Some(("text/html; charset=utf-8", html))
        }
        _ => None,
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slides_escape_untrusted_canvas_content() {
        let mut c = Canvas::new("<script>alert(1)</script>");
        c.nodes.push(pinboard_core::Node::note(
            "<img src=x onerror=alert(1)>",
            0.,
            0.,
        ));
        let (_, html) = render(&c, "slides").unwrap();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img"));
        assert!(html.contains("&lt;script&gt;"));
    }
    #[test]
    fn mermaid_encodes_directives_as_label_entities() {
        let mut c = Canvas::new("Diagram");
        c.nodes
            .push(pinboard_core::Node::note("\"]\n%%{init: evil}%%", 0., 0.));
        let (_, text) = render(&c, "mermaid").unwrap();
        assert!(!text.contains("%%{init"));
        assert_eq!(text.lines().count(), 2);
    }
}
