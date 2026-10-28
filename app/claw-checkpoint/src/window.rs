//! Window paging — the `[from, to)` fetch walk, identical to the shell
//! daemon's `--since FROM --before CURSOR-1 --limit 200` loop: pages come
//! newest-first; a short or empty page ends the walk; otherwise the next
//! page is strictly before the current page's minimum `created_at`
//! (`until = min_ts - 1`, so `until` is inclusive like Nostr).

use claw_core::Event;
use std::future::Future;
use std::pin::Pin;

use crate::Error;

/// Events per page — matches the shell daemon's `--limit 200`.
pub const PAGE_LIMIT: usize = 200;

/// Hard bound on pages per window: the script ran 10 pages (~2000
/// events); a 60000-byte size cap already bounds a real window, so 256
/// pages is a generous safety rail against a pathological relay.
pub const MAX_PAGES: usize = 256;

/// A source of stored-window pages. `fetch_page(channel, since, until,
/// limit)` must apply inclusive `since`/`until` bounds on `created_at`
/// and return at most `limit` events — the relay's REQ semantics.
/// (`Pin<Box<dyn Future>>` instead of `async fn` in the trait so mock
/// sources work on stable Rust without `async_trait`.)
pub trait PageSource {
    fn fetch_page<'a>(
        &'a mut self,
        channel: &'a str,
        since: u64,
        until: u64,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Event>, Error>> + Send + 'a>>;
}

/// Collect the whole `[from, to)` window by paging `src` backward until a
/// short/empty page. Returned events may contain duplicates across page
/// boundaries — `anchor_package` dedupes.
pub async fn fetch_window<P: PageSource + ?Sized>(
    src: &mut P,
    channel: &str,
    from: u64,
    to: u64,
    limit: usize,
) -> Result<Vec<Event>, Error> {
    let mut out = Vec::new();
    let mut cursor = to;
    for _ in 0..MAX_PAGES {
        if cursor == 0 || cursor - 1 < from {
            break; // nothing left inside [from, to)
        }
        let page = src.fetch_page(channel, from, cursor - 1, limit).await?;
        if page.is_empty() {
            break;
        }
        let short = page.len() < limit;
        let min_ts = page.iter().map(|e| e.created_at).min().unwrap_or(0);
        out.extend(page);
        if short || min_ts <= from {
            break; // short page, or the next page would start before `from`
        }
        cursor = min_ts;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory page source honoring the relay contract: filter by
    /// inclusive bounds, serve newest-first, cap at limit. Records every
    /// request so tests can assert the cursor walk.
    struct MockSource {
        events: Vec<Event>,
        calls: Vec<(u64, u64, usize)>,
    }

    impl MockSource {
        fn new(timestamps: &[u64]) -> Self {
            let events = timestamps
                .iter()
                .enumerate()
                .map(|(i, &ts)| Event {
                    id: hex::encode([(i + 1) as u8; 32]),
                    pubkey: "ab".repeat(32),
                    created_at: ts,
                    kind: 9,
                    tags: vec![],
                    content: String::new(),
                    sig: String::new(),
                })
                .collect();
            Self {
                events,
                calls: vec![],
            }
        }
    }

    impl PageSource for MockSource {
        fn fetch_page<'a>(
            &'a mut self,
            _channel: &'a str,
            since: u64,
            until: u64,
            limit: usize,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<Event>, Error>> + Send + 'a>> {
            self.calls.push((since, until, limit));
            let mut page: Vec<Event> = self
                .events
                .iter()
                .filter(|e| e.created_at >= since && e.created_at <= until)
                .cloned()
                .collect();
            page.sort_by(|a, b| b.created_at.cmp(&a.created_at)); // newest first
            page.truncate(limit);
            Box::pin(async move { Ok(page) })
        }
    }

    #[tokio::test]
    async fn single_short_page_completes_window() {
        let mut src = MockSource::new(&[10, 20, 30]);
        let got = fetch_window(&mut src, "c", 0, 100, PAGE_LIMIT)
            .await
            .unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(src.calls, vec![(0, 99, PAGE_LIMIT)]);
    }

    #[tokio::test]
    async fn full_pages_walk_backward_until_short() {
        // 5 events at ts 1..5, limit 2: newest-first pages of (5,4),
        // (3,2), then (1) — three requests, cursors stepping by min ts.
        let mut src = MockSource::new(&[1, 2, 3, 4, 5]);
        let got = fetch_window(&mut src, "c", 1, 6, 2).await.unwrap();
        let mut ts: Vec<u64> = got.iter().map(|e| e.created_at).collect();
        ts.sort_unstable();
        assert_eq!(ts, vec![1, 2, 3, 4, 5]);
        // Request bounds: [1,5], [1,3], [1,1] — until = prev min - 1.
        assert_eq!(
            src.calls,
            vec![(1, 5, 2), (1, 3, 2), (1, 1, 2)]
        );
    }

    #[tokio::test]
    async fn full_page_at_from_bound_stops() {
        // limit 2, events only at ts 10: page returns 2 events at ts 10,
        // min_ts == from → no further page (would start before `from`).
        let mut src = MockSource::new(&[10, 10]);
        let got = fetch_window(&mut src, "c", 10, 20, 2).await.unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(src.calls.len(), 1);
    }

    #[tokio::test]
    async fn empty_window_returns_immediately() {
        let mut src = MockSource::new(&[]);
        let got = fetch_window(&mut src, "c", 5, 10, PAGE_LIMIT)
            .await
            .unwrap();
        assert!(got.is_empty());
        assert_eq!(src.calls, vec![(5, 9, PAGE_LIMIT)]);
    }

    #[tokio::test]
    async fn zero_width_window_makes_no_calls() {
        let mut src = MockSource::new(&[1, 2]);
        let got = fetch_window(&mut src, "c", 10, 10, PAGE_LIMIT)
            .await
            .unwrap();
        assert!(got.is_empty());
        assert!(src.calls.is_empty());
    }
}
