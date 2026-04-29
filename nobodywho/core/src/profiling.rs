//! Tracing-based profiling layer for collecting span timing data.
//!
//! Provides a [`SpanProfiler`] that implements [`tracing_subscriber::Layer`] and collects
//! timing information for all spans. After a profiling run, call [`SpanProfiler::summary()`]
//! to get a formatted table of timing data.
//!
//! # Example
//!
//! ```ignore
//! use tracing_subscriber::prelude::*;
//! use nobodywho::profiling::SpanProfiler;
//!
//! let profiler = SpanProfiler::new();
//! tracing_subscriber::registry()
//!     .with(profiler.layer())
//!     .init();
//!
//! // ... run some code with tracing spans ...
//!
//! println!("{}", profiler.summary());
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::span::{Attributes, Id};
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

#[derive(Debug, Clone)]
struct SpanTiming {
    name: String,
    parent_name: Option<String>,
    total_ns: u64,
    count: u64,
    min_ns: u64,
    max_ns: u64,
    /// All individual durations, for computing median/percentiles.
    durations_ns: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct SpanStats {
    pub name: String,
    pub parent_name: Option<String>,
    pub count: u64,
    pub total_ns: u64,
    pub mean_ns: u64,
    pub median_ns: u64,
    pub p95_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
}

struct SpanData {
    name: String,
    parent_name: Option<String>,
    entered_at: Option<Instant>,
}

#[derive(Clone)]
pub struct SpanProfiler {
    timings: Arc<Mutex<HashMap<String, SpanTiming>>>,
}

impl SpanProfiler {
    pub fn new() -> Self {
        Self {
            timings: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a tracing layer from this profiler.
    /// The profiler and the layer share state, so you can call `summary()` after the layer
    /// has collected data.
    pub fn layer<S>(&self) -> SpanProfilerLayer<S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        SpanProfilerLayer {
            timings: Arc::clone(&self.timings),
            _subscriber: std::marker::PhantomData,
        }
    }

    /// Reset all collected data.
    pub fn reset(&self) {
        self.timings.lock().unwrap().clear();
    }

    /// Get raw span statistics sorted by total time descending.
    pub fn stats(&self) -> Vec<SpanStats> {
        let timings = self.timings.lock().unwrap();
        let mut stats: Vec<SpanStats> = timings
            .values()
            .map(|t| {
                let mut sorted = t.durations_ns.clone();
                sorted.sort_unstable();
                let median_ns = if sorted.is_empty() {
                    0
                } else {
                    sorted[sorted.len() / 2]
                };
                let p95_ns = if sorted.is_empty() {
                    0
                } else {
                    sorted[(sorted.len() as f64 * 0.95) as usize]
                };
                SpanStats {
                    name: t.name.clone(),
                    parent_name: t.parent_name.clone(),
                    count: t.count,
                    total_ns: t.total_ns,
                    mean_ns: if t.count > 0 {
                        t.total_ns / t.count
                    } else {
                        0
                    },
                    median_ns,
                    p95_ns,
                    min_ns: t.min_ns,
                    max_ns: t.max_ns,
                }
            })
            .collect();
        stats.sort_by(|a, b| b.total_ns.cmp(&a.total_ns));
        stats
    }

    /// Format a summary table of all collected span timings.
    pub fn summary(&self) -> String {
        let stats = self.stats();
        if stats.is_empty() {
            return "No span data collected.".to_string();
        }

        // Find the top-level span total for percentage calculation
        let top_total_ns = stats.first().map(|s| s.total_ns).unwrap_or(1);

        let mut lines = Vec::new();
        lines.push(format!(
            "{:<45} {:>8} {:>12} {:>12} {:>12} {:>12} {:>12} {:>12} {:>7}",
            "Span", "Calls", "Total", "Mean", "Median", "p95", "Min", "Max", "% top"
        ));
        lines.push("─".repeat(133));

        for s in &stats {
            let display_name = match &s.parent_name {
                Some(parent) => format!("├─ {}/{}", parent, s.name),
                None => s.name.clone(),
            };
            let pct = s.total_ns as f64 / top_total_ns as f64 * 100.0;
            lines.push(format!(
                "{:<45} {:>8} {:>12} {:>12} {:>12} {:>12} {:>12} {:>12} {:>6.1}%",
                display_name,
                s.count,
                format_duration(s.total_ns),
                format_duration(s.mean_ns),
                format_duration(s.median_ns),
                format_duration(s.p95_ns),
                format_duration(s.min_ns),
                format_duration(s.max_ns),
                pct,
            ));
        }

        lines.join("\n")
    }
}

impl Default for SpanProfiler {
    fn default() -> Self {
        Self::new()
    }
}

fn format_duration(ns: u64) -> String {
    if ns < 1_000 {
        format!("{}ns", ns)
    } else if ns < 1_000_000 {
        format!("{:.1}us", ns as f64 / 1_000.0)
    } else if ns < 1_000_000_000 {
        format!("{:.2}ms", ns as f64 / 1_000_000.0)
    } else {
        format!("{:.3}s", ns as f64 / 1_000_000_000.0)
    }
}

pub struct SpanProfilerLayer<S> {
    timings: Arc<Mutex<HashMap<String, SpanTiming>>>,
    _subscriber: std::marker::PhantomData<S>,
}

impl<S> Layer<S> for SpanProfilerLayer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span not found");
        let parent_name = span.parent().map(|p| p.name().to_string());
        let data = SpanData {
            name: attrs.metadata().name().to_string(),
            parent_name,
            entered_at: None,
        };
        span.extensions_mut().insert(data);
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span not found");
        let mut extensions = span.extensions_mut();
        if let Some(data) = extensions.get_mut::<SpanData>() {
            data.entered_at = Some(Instant::now());
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span not found");
        let extensions = span.extensions();
        let Some(data) = extensions.get::<SpanData>() else {
            return;
        };
        let Some(entered_at) = data.entered_at else {
            return;
        };

        let elapsed_ns = entered_at.elapsed().as_nanos() as u64;
        let name = data.name.clone();
        let parent_name = data.parent_name.clone();

        // Use "parent/name" as key to distinguish same-named spans under different parents
        let key = match &parent_name {
            Some(p) => format!("{}/{}", p, name),
            None => name.clone(),
        };

        let mut timings = self.timings.lock().unwrap();
        let entry = timings.entry(key).or_insert_with(|| SpanTiming {
            name,
            parent_name,
            total_ns: 0,
            count: 0,
            min_ns: u64::MAX,
            max_ns: 0,
            durations_ns: Vec::new(),
        });
        entry.total_ns += elapsed_ns;
        entry.count += 1;
        entry.min_ns = entry.min_ns.min(elapsed_ns);
        entry.max_ns = entry.max_ns.max(elapsed_ns);
        entry.durations_ns.push(elapsed_ns);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::debug_span;
    use tracing_subscriber::prelude::*;

    #[test]
    fn test_profiler_collects_spans() {
        let profiler = SpanProfiler::new();
        let _guard = tracing_subscriber::registry()
            .with(profiler.layer())
            .set_default();

        {
            let _outer = debug_span!("outer").entered();
            std::thread::sleep(std::time::Duration::from_millis(10));
            {
                let _inner = debug_span!("inner").entered();
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }

        let stats = profiler.stats();
        assert!(!stats.is_empty(), "Should have collected span data");

        let outer = stats.iter().find(|s| s.name == "outer");
        assert!(outer.is_some(), "Should have 'outer' span");
        assert_eq!(outer.unwrap().count, 1);

        let inner = stats.iter().find(|s| s.name == "inner");
        assert!(inner.is_some(), "Should have 'inner' span");
        assert_eq!(inner.unwrap().count, 1);
        assert_eq!(
            inner.unwrap().parent_name.as_deref(),
            Some("outer"),
            "inner should be child of outer"
        );

        let summary = profiler.summary();
        assert!(summary.contains("outer"));
        assert!(summary.contains("inner"));
    }

    #[test]
    fn test_profiler_aggregates_multiple_calls() {
        let profiler = SpanProfiler::new();
        let _guard = tracing_subscriber::registry()
            .with(profiler.layer())
            .set_default();

        for _ in 0..10 {
            let _span = debug_span!("repeated").entered();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        let stats = profiler.stats();
        let repeated = stats.iter().find(|s| s.name == "repeated").unwrap();
        assert_eq!(repeated.count, 10);
        assert!(repeated.min_ns <= repeated.mean_ns);
        assert!(repeated.mean_ns <= repeated.max_ns);
    }

    #[test]
    fn test_profiler_reset() {
        let profiler = SpanProfiler::new();
        let _guard = tracing_subscriber::registry()
            .with(profiler.layer())
            .set_default();

        {
            let _span = debug_span!("before_reset").entered();
        }
        assert!(!profiler.stats().is_empty());

        profiler.reset();
        assert!(profiler.stats().is_empty());
    }
}
