//! What's New, from CHANGELOG.md: the GTK app's About dialog shows its newest sections as HTML
//! (moved out of ui.rs, issue #45), and the Windows beta its own sections ("## Windows beta …").

/// The first `versions` sections as the About dialog's markup: each heading a paragraph, its
/// "- " items a list ("  " lines carry on the item before), **x** emphasised, & < > escaped.
pub fn html(changelog: &str, versions: usize) -> String {
    let esc = |t: &str| t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let inline = |t: &str| {
        let mut out = String::new();
        for (i, part) in esc(t).split("**").enumerate() {
            if i % 2 == 1 {
                out += &format!("<em>{part}</em>");
            } else {
                out += part;
            }
        }
        out
    };
    let mut html = String::new();
    let mut items: Vec<String> = vec![];
    let mut shown = 0;
    let flush = |html: &mut String, items: &mut Vec<String>| {
        if !items.is_empty() {
            *html += "<ul>";
            for i in items.drain(..) {
                *html += &format!("<li>{}</li>", inline(&i));
            }
            *html += "</ul>";
        }
    };
    for line in changelog.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            flush(&mut html, &mut items);
            shown += 1;
            if shown > versions {
                break;
            }
            html += &format!("<p>{}</p>", esc(head.trim()));
        } else if shown == 0 {
            continue;
        } else if let Some(item) = line.strip_prefix("- ") {
            items.push(item.trim().to_string());
        } else if line.starts_with("  ") && !items.is_empty() {
            let last = items.last_mut().unwrap();
            last.push(' ');
            last.push_str(line.trim());
        }
    }
    flush(&mut html, &mut items);
    html
}

/// The first `n` sections whose heading starts with `prefix` ("Windows beta"): (heading, items),
/// the items as plain words (** taken out, carried-on lines joined).
pub fn sections(changelog: &str, prefix: &str, n: usize) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![];
    let mut taking = false;
    for line in changelog.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            if out.len() >= n && taking {
                break;
            }
            taking = head.trim().starts_with(prefix) && out.len() < n;
            if taking {
                out.push((head.trim().to_string(), vec![]));
            }
        } else if !taking {
            continue;
        } else if let Some(item) = line.strip_prefix("- ") {
            out.last_mut().unwrap().1.push(item.trim().replace("**", ""));
        } else if line.starts_with("  ") {
            if let Some(last) = out.last_mut().unwrap().1.last_mut() {
                last.push(' ');
                last.push_str(&line.trim().replace("**", ""));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whats_new_comes_from_the_changelog() {
        let log = "# Changelog\n\n## v0.2.0 — today\n\n- **Bold** & new: one\n  two\n- three\n\n## v0.1.0 — before\n\n- old\n";
        assert_eq!(html(log, 1), "<p>v0.2.0 — today</p><ul><li><em>Bold</em> &amp; new: one two</li><li>three</li></ul>");
        assert!(html(log, 2).contains("<p>v0.1.0 — before</p><ul><li>old</li></ul>"));
    }

    #[test]
    fn the_betas_whats_new_is_its_own_sections() {
        let log = "# Changelog\n\n## v0.7.20 — today\n\n- linux\n\n## Windows beta 0.8.0-beta.3 — not released yet\n\nThe parts:\n- **The Mixer**, a pop-up\n  from the bar\n- Splitting\n\n## v0.7.19\n\n- old\n\n## Windows beta 0.8.0-beta.2\n\n- library\n";
        let s = sections(log, "Windows beta", 5);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].0, "Windows beta 0.8.0-beta.3 — not released yet");
        assert_eq!(s[0].1, ["The Mixer, a pop-up from the bar", "Splitting"]);
        assert_eq!(s[1].1, ["library"]);
        assert_eq!(sections(log, "Windows beta", 1).len(), 1);
    }
}
