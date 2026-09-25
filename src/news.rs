use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::config::{Feed, Source, TimeKind};
use crate::model::{Headline, Pick};
use crate::rng::Rng;

fn identifier(name: &str) -> Result<&str, String> {
    let valid = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid { Ok(name) } else { Err(format!("\"{name}\" is not a plain column or table name")) }
}

fn column(name: &str) -> Result<String, String> {
    if name.is_empty() { Ok("NULL".into()) } else { identifier(name).map(str::to_string) }
}

pub fn read(path: &Path, feed: &Feed, now: f64) -> Result<Vec<Headline>, String> {
    let table = identifier(&feed.table)?;
    let (title, url, source, time) = (identifier(&feed.title)?, column(&feed.url)?, column(&feed.source)?, identifier(&feed.time)?);
    let content = column(&feed.content)?;
    let seconds = match feed.time_kind {
        TimeKind::Unix => format!("CAST({time} AS REAL)"),
        TimeKind::Iso => format!("CAST(strftime('%s', {time}) AS REAL)"),
    };
    let sql = format!(
        "SELECT {title}, {url}, {source}, {seconds} AS published, substr({content}, 1, 8000) FROM {table} \
         WHERE published >= ?1 AND {title} IS NOT NULL ORDER BY published DESC LIMIT 1000"
    );
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let db = Connection::open_with_flags(path, flags).map_err(|e| e.to_string())?;
    let mut statement = db.prepare(&sql).map_err(|e| e.to_string())?;
    let since = now - feed.window_hours * 3600.0;
    let rows = statement
        .query_map([since], |row| {
            Ok(Headline {
                title: row.get(0)?,
                url: row.get::<_, Option<String>>(1)?.filter(|u| !u.is_empty()),
                source: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                published: row.get::<_, Option<f64>>(3)?.unwrap_or(now),
                body: row.get::<_, Option<String>>(4)?.as_deref().and_then(first_paragraph),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub struct Picker {
    shown: HashSet<String>,
    rng: Rng,
}

impl Picker {
    pub fn new() -> Self {
        Self { shown: HashSet::new(), rng: Rng::seeded() }
    }

    // Weighted by source weight and freshness; every headline is shown once before any repeats.
    pub fn next(&mut self, headlines: &[Headline], feed: &Feed, now: f64) -> Option<Pick> {
        let mut fresh: Vec<&Headline> = headlines.iter().filter(|h| !self.shown.contains(&h.title)).collect();
        if fresh.is_empty() {
            self.shown.clear();
            fresh = headlines.iter().collect();
        }
        let weight = |h: &Headline| {
            let age_hours = ((now - h.published) / 3600.0).max(0.0);
            source_of(&feed.sources, h).map_or(1.0, |(_, s)| s.weight) * 0.5f64.powf(age_hours / feed.half_life_hours.max(0.1))
        };
        let total: f64 = fresh.iter().map(|h| weight(h)).sum();
        let mut target = self.rng.unit() * total;
        let chosen = *fresh.iter().find(|h| {
            target -= weight(h);
            target <= 0.0
        }).or(fresh.last())?;
        self.shown.insert(chosen.title.clone());
        let color = source_of(&feed.sources, chosen).map_or(0, |(i, s)| if s.color.is_some() && i < 15 { i as u8 + 1 } else { 0 });
        Some(Pick { title: chosen.title.clone(), url: chosen.url.clone(), body: chosen.body.clone(), color })
    }

    pub fn fallback(&mut self, phrases: &[String]) -> Option<Pick> {
        let title = phrases.get(self.rng.below(phrases.len()))?.clone();
        Some(Pick { title, url: None, body: None, color: 0 })
    }
}

// Feeds such as hnrss put only links in the body; a paragraph needs real words.
fn first_paragraph(html: &str) -> Option<String> {
    plain_text(html)
        .split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|p| p.split(' ').filter(|w| !w.contains("://")).map(|w| w.chars().count() + 1).sum::<usize>() > 40)
}

fn plain_text(html: &str) -> String {
    let mut text = String::new();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        text.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else { return decode(&text) };
        let tag = rest[open + 1..open + close].trim_start_matches('/').to_ascii_lowercase();
        let name = tag.split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or_default();
        if matches!(name, "p" | "br" | "div" | "li" | "blockquote" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
            text.push_str("\n\n");
        }
        rest = &rest[open + close + 1..];
    }
    text.push_str(rest);
    decode(&text)
}

fn decode(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let found = rest.find(';').filter(|&end| end < 10).and_then(|end| Some((end, entity(&rest[1..end])?)));
        match found {
            Some((end, c)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    let c = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "laquo" => '«',
        "raquo" => '»',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            return char::from_u32(code);
        }
    };
    Some(c)
}

fn source_of<'a>(sources: &'a [Source], headline: &Headline) -> Option<(usize, &'a Source)> {
    sources.iter().enumerate().find(|(_, s)| headline.source.contains(&s.matches))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn newsboat_db(path: &Path, now: f64) {
        let db = Connection::open(path).unwrap();
        db.execute_batch(
            "CREATE TABLE rss_item (id INTEGER PRIMARY KEY, title VARCHAR(1024), url VARCHAR(1024), feedurl VARCHAR(1024), pubDate INTEGER, content TEXT);",
        ).unwrap();
        for (title, feed, age) in [("Fresh", "https://a.example/feed", 60.0), ("Old", "https://b.example/rss", 3600.0 * 100.0)] {
            db.execute(
                "INSERT INTO rss_item (title, url, feedurl, pubDate) VALUES (?1, ?2, ?3, ?4)",
                (title, format!("https://{title}.example"), feed, (now - age) as i64),
            ).unwrap();
        }
    }

    #[test]
    fn reads_recent_newsboat_items_only() {
        let dir = std::env::temp_dir().join(format!("mnv-news-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache.db");
        let now = 1_800_000_000.0;
        newsboat_db(&path, now);
        let items = read(&path, &Feed::default(), now).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Fresh");
        assert_eq!(items[0].url.as_deref(), Some("https://Fresh.example"));
    }

    #[test]
    fn first_paragraph_skips_link_stubs_and_decodes_entities() {
        let article = "<p><span>Власти Италии &laquo;ввели&raquo; новые <a href=\"https://x.example\">правила</a> работы школ &amp; вузов&#33;</span></p><p>Второй абзац.</p>";
        assert_eq!(first_paragraph(article).as_deref(), Some("Власти Италии «ввели» новые правила работы школ & вузов!"));
        let stub = "\n<p>Article URL: <a href=\"https://a.example/x\">https://a.example/x</a></p>\n<p>Points: 7</p>\n";
        assert_eq!(first_paragraph(stub), None);
    }

    #[test]
    fn rejects_injected_identifiers() {
        let feed = Feed { table: "rss_item; DROP TABLE x".into(), ..Feed::default() };
        assert!(read(Path::new("/nonexistent"), &feed, 0.0).is_err());
    }

    #[test]
    fn picker_shows_everything_once_and_colors_by_source() {
        let now = 1000.0;
        let headline = |title: &str, source: &str| Headline { title: title.into(), url: None, body: None, source: source.into(), published: now };
        let headlines = vec![headline("A", "https://habr.com/rss"), headline("B", "https://hnrss.org")];
        let feed = Feed {
            sources: vec![Source { matches: "habr".into(), weight: 5.0, color: Some("#ff0000".into()) }],
            ..Feed::default()
        };
        let mut picker = Picker::new();
        let first = picker.next(&headlines, &feed, now).unwrap();
        let second = picker.next(&headlines, &feed, now).unwrap();
        assert_ne!(first.title, second.title);
        let a = if first.title == "A" { &first } else { &second };
        assert_eq!(a.color, 1);
    }
}
