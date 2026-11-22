//! Notes as Markdown: the text of one note, as copied or exported.

use serde::Serialize;

/// What a note quotes and where it belongs, resolved from its anchor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Anchor {
    pub reference: Option<String>,
    pub quote: Option<String>,
}

/// `[label](pergament://…)` becomes `label`; other links and text stay.
pub fn strip_app_links(text: &str) -> String {
    const SCHEME: &str = "](pergament://";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match app_link(after, SCHEME) {
            Some((label, len)) => {
                out.push_str(label);
                rest = &after[len..];
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The label and length of `label](pergament://target)` at the start of `s`.
fn app_link<'a>(s: &'a str, scheme: &str) -> Option<(&'a str, usize)> {
    let end = s.find([']', '\n'])?;
    let label = &s[..end];
    if label.is_empty() || !s[end..].starts_with(scheme) {
        return None;
    }
    let target_start = end + scheme.len();
    let close = s[target_start..].find(|c: char| c == ')' || c.is_whitespace())?;
    if close == 0 || !s[target_start + close..].starts_with(')') {
        return None;
    }
    Some((label, target_start + close + 1))
}

/// A tag as `#tag` would take it: whitespace runs become `-`.
pub fn tag_slug(tag: &str) -> String {
    tag.split_whitespace().collect::<Vec<_>>().join("-")
}

/// A JW Library timestamp as UTC `YYYY-MM-DDTHH:MM:SSZ`; `None` if it can't be read.
pub fn utc_timestamp(ts: &str) -> Option<String> {
    let b = ts.as_bytes();
    if b.len() < 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let year = digits(&b[0..4])?;
    let month = digits(&b[5..7])?;
    let day = digits(&b[8..10])?;
    let hour = digits(&b[11..13])?;
    let minute = digits(&b[14..16])?;
    let second = digits(&b[17..19])?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour >= 24
        || minute >= 60
        || second >= 60
    {
        return None;
    }
    let mut zone = &b[19..];
    if let Some(frac) = zone.strip_prefix(b".") {
        let n = frac.iter().take_while(|c| c.is_ascii_digit()).count();
        if n == 0 {
            return None;
        }
        zone = &frac[n..];
    }
    let offset = match zone {
        [] | [b'Z'] => 0,
        [sign @ (b'+' | b'-'), tail @ ..] => {
            let (h, m) = match tail {
                [h1, h2, b':', m1, m2] | [h1, h2, m1, m2] => {
                    (digits(&[*h1, *h2])?, digits(&[*m1, *m2])?)
                }
                _ => return None,
            };
            if h >= 24 || m >= 60 {
                return None;
            }
            let minutes = h * 60 + m;
            if *sign == b'-' { -minutes } else { minutes }
        }
        _ => return None,
    };
    let secs = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second
        - offset * 60;
    let (days, tod) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    Some(format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        tod % 3600 / 60,
        tod % 60
    ))
}

/// ASCII digits as a number.
fn digits(b: &[u8]) -> Option<i64> {
    b.iter().try_fold(0, |n, c| {
        c.is_ascii_digit().then(|| n * 10 + i64::from(c - b'0'))
    })
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The proleptic Gregorian date of a day count since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The text of `s`, unless it is missing or blank.
fn present(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|s| !s.trim().is_empty())
}

/// The Markdown of a note, as copied.
pub fn body(title: &str, content: &str, tags: &[String], anchor: &Anchor) -> String {
    let mut blocks = Vec::new();
    let title = title.trim();
    if !title.is_empty() {
        blocks.push(format!("**{title}**"));
    }
    let reference = present(&anchor.reference);
    match (present(&anchor.quote), reference) {
        (Some(quote), _) => {
            let mut lines: Vec<String> = quote.lines().map(|l| format!("> {l}")).collect();
            if let Some(r) = reference {
                lines.push(format!("> — {r}"));
            }
            blocks.push(lines.join("\n"));
        }
        (None, Some(r)) => blocks.push(format!("— {r}")),
        (None, None) => {}
    }
    let text = strip_app_links(content);
    let text = text.trim();
    if !text.is_empty() {
        blocks.push(text.to_string());
    }
    let tags: Vec<String> = tags
        .iter()
        .map(|t| tag_slug(t))
        .filter(|s| !s.is_empty())
        .map(|s| format!("#{s}"))
        .collect();
    if !tags.is_empty() {
        blocks.push(tags.join(" "));
    }
    if blocks.is_empty() {
        return String::new();
    }
    blocks.join("\n\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_links_keep_their_label() {
        assert_eq!(
            strip_app_links("a [Mt 24:14](pergament://bible/40:24:14-40:24:14) b"),
            "a Mt 24:14 b"
        );
        assert_eq!(
            strip_app_links("[Röm 8:38](pergament://verse/X:1001070143/8:38)"),
            "Röm 8:38"
        );
        assert_eq!(
            strip_app_links("see [p. 3](pergament://pub/X:1102016001/3)."),
            "see p. 3."
        );
    }

    #[test]
    fn other_links_and_brackets_stay() {
        assert_eq!(strip_app_links("[x](https://a.b)"), "[x](https://a.b)");
        assert_eq!(strip_app_links("[unclosed"), "[unclosed");
        assert_eq!(
            strip_app_links("[](pergament://bible/1)"),
            "[](pergament://bible/1)"
        );
        assert_eq!(
            strip_app_links("[a b](pergament://x y)"),
            "[a b](pergament://x y)"
        );
        assert_eq!(strip_app_links("[[x](pergament://bible/1)"), "[x");
    }

    #[test]
    fn tag_slug_joins_words() {
        assert_eq!(tag_slug("Gute Botschaft  Mt"), "Gute-Botschaft-Mt");
        assert_eq!(tag_slug("  two "), "two");
        assert_eq!(tag_slug("   "), "");
    }

    #[test]
    fn timestamps_become_utc() {
        assert_eq!(
            utc_timestamp("2025-03-21T19:29:43Z").as_deref(),
            Some("2025-03-21T19:29:43Z")
        );
        assert_eq!(
            utc_timestamp("2025-03-21T19:29:43+0100").as_deref(),
            Some("2025-03-21T18:29:43Z")
        );
        assert_eq!(
            utc_timestamp("2025-01-01T00:30:00+01:00").as_deref(),
            Some("2024-12-31T23:30:00Z")
        );
        assert_eq!(
            utc_timestamp("2025-03-21T23:00:00-0230").as_deref(),
            Some("2025-03-22T01:30:00Z")
        );
        assert_eq!(
            utc_timestamp("2025-03-21T19:29:43.123").as_deref(),
            Some("2025-03-21T19:29:43Z")
        );
        assert_eq!(
            utc_timestamp("2024-02-29T23:59:59.5-01:00").as_deref(),
            Some("2024-03-01T00:59:59Z")
        );
        assert_eq!(utc_timestamp("garbage"), None);
        assert_eq!(utc_timestamp("2025-13-01T00:00:00Z"), None);
        assert_eq!(utc_timestamp("2025-03-21T24:00:00Z"), None);
        assert_eq!(utc_timestamp("2025-03-21T19:29:43+01"), None);
        assert_eq!(utc_timestamp("2025-03-21T19:29:43Zx"), None);
        assert_eq!(utc_timestamp("2025-03-21T19:29:43."), None);
    }

    fn anchor(reference: Option<&str>, quote: Option<&str>) -> Anchor {
        Anchor {
            reference: reference.map(str::to_string),
            quote: quote.map(str::to_string),
        }
    }

    #[test]
    fn body_with_everything() {
        let tags = vec!["tag one".to_string(), "two".to_string()];
        assert_eq!(
            body(
                "Title",
                "See [Gen 1:1](pergament://bible/1:1:1-1:1:1) here.",
                &tags,
                &anchor(Some("Genesis 1:1"), Some("In the beginning")),
            ),
            "**Title**\n\n> In the beginning\n> — Genesis 1:1\n\nSee Gen 1:1 here.\n\n#tag-one #two\n"
        );
    }

    #[test]
    fn body_with_only_content() {
        assert_eq!(
            body("  ", "  just text \n", &[], &Anchor::default()),
            "just text\n"
        );
    }

    #[test]
    fn body_with_reference_but_no_quote() {
        assert_eq!(
            body("", "x", &[], &anchor(Some("Genesis 1:1"), None)),
            "— Genesis 1:1\n\nx\n"
        );
    }

    #[test]
    fn multi_line_quote_is_prefixed() {
        assert_eq!(body("", "", &[], &anchor(None, Some("a\nb"))), "> a\n> b\n");
    }

    #[test]
    fn body_with_everything_empty() {
        let tags = vec![" ".to_string()];
        assert_eq!(body(" ", " ", &tags, &Anchor::default()), "");
        assert_eq!(body("", "", &[], &anchor(Some(""), Some(" "))), "");
    }
}
