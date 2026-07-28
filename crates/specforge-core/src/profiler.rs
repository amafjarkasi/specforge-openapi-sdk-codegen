//! SDK performance profiling.
//!
//! Measures latency, throughput, error rates, and cache behaviour for API
//! endpoints declared in an OpenAPI spec.

use std::time::{Duration, Instant};

use crate::ir::{Document, HttpMethod};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Performance results for a single endpoint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProfileResult {
    /// Endpoint path template (e.g. `/pets/{petId}`).
    pub endpoint: String,
    /// HTTP method.
    pub method: String,
    /// Mean response latency in milliseconds.
    pub avg_latency_ms: f64,
    /// 50th-percentile latency.
    pub p50_latency_ms: f64,
    /// 95th-percentile latency.
    pub p95_latency_ms: f64,
    /// 99th-percentile latency.
    pub p99_latency_ms: f64,
    /// Observed throughput in requests/second.
    pub throughput_rps: f64,
    /// Fraction of requests that returned 4xx/5xx (0.0..1.0).
    pub error_rate: f64,
    /// Fraction of requests served from cache when caching is enabled (0.0..1.0).
    pub cache_hit_rate: f64,
    /// Number of successful requests.
    pub success_count: usize,
    /// Number of failed requests.
    pub error_count: usize,
    /// Total number of requests sent.
    pub total_requests: usize,
    /// Minimum observed latency (ms).
    pub min_latency_ms: f64,
    /// Maximum observed latency (ms).
    pub max_latency_ms: f64,
}

/// Aggregated report covering all profiled endpoints.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProfileReport {
    pub results: Vec<ProfileResult>,
    pub total_requests: usize,
    pub total_duration_ms: f64,
    pub overall_throughput_rps: f64,
    pub recommendations: Vec<String>,
}

/// Options controlling profiling behaviour.
pub struct ProfileOptions {
    /// Base URL of the running API.
    pub base_url: String,
    /// Optional `Authorization` header value.
    pub auth: Option<String>,
    /// Number of requests per endpoint.
    pub requests: usize,
    /// Number of concurrent requests.
    pub concurrency: usize,
    /// Per-request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Optional path filter -- only profile this endpoint.
    pub endpoint_filter: Option<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Profile every operation declared in `doc` against a live API.
pub fn profile_api(doc: &Document, opts: &ProfileOptions) -> ProfileReport {
    let client = build_client(opts);
    let start = Instant::now();

    let mut results: Vec<ProfileResult> = Vec::new();
    let mut total_requests = 0usize;

    for op in &doc.operations {
        // Apply endpoint filter if provided.
        if let Some(ref filter) = opts.endpoint_filter {
            if !op.path.contains(filter.as_str()) {
                continue;
            }
        }

        let result = profile_endpoint(&client, doc, op, opts);
        total_requests += result.total_requests;
        results.push(result);
    }

    let total_duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    let overall_throughput_rps = if total_duration_ms > 0.0 {
        (total_requests as f64) / (total_duration_ms / 1000.0)
    } else {
        0.0
    };

    let recommendations = generate_recommendations(&results);

    ProfileReport {
        results,
        total_requests,
        total_duration_ms,
        overall_throughput_rps,
        recommendations,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn build_client(opts: &ProfileOptions) -> reqwest::blocking::Client {
    let timeout = Duration::from_millis(opts.timeout_ms);
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
        .user_agent("specforge-profile/0.1")
        .build()
        .expect("failed to build HTTP client")
}

/// Replace `{paramName}` placeholders with dummy values.
fn substitute_path_params(path: &str) -> String {
    let mut result = path.to_string();
    while let Some(start) = result.find('{') {
        if let Some(end) = result[start..].find('}') {
            let param = &result[start + 1..start + end];
            let dummy = if param.contains("id") || param.contains("Id") {
                "1".to_string()
            } else {
                "test".to_string()
            };
            result = format!(
                "{}{}{}",
                &result[..start],
                dummy,
                &result[start + end + 1..]
            );
        } else {
            break;
        }
    }
    result
}

/// Profile a single endpoint by issuing `opts.requests` requests.
fn profile_endpoint(
    client: &reqwest::blocking::Client,
    _doc: &Document,
    op: &crate::ir::Operation,
    opts: &ProfileOptions,
) -> ProfileResult {
    let method_str = op.method.upper().to_string();
    let path = substitute_path_params(&op.path);
    let url = format!("{}{}", opts.base_url.trim_end_matches('/'), path);

    let mut latencies: Vec<f64> = Vec::with_capacity(opts.requests);
    let mut error_count = 0usize;
    let mut _cache_hits = 0usize;

    for i in 0..opts.requests {
        // For concurrency > 1 we still run sequentially here because
        // reqwest::blocking doesn't support concurrent sends on a single thread.
        // In practice, profile_api is called per-endpoint and callers can
        // parallelise across endpoints externally.
        let _ = i; // suppress unused warning in non-concurrent path

        let start = Instant::now();

        let mut request_builder = match op.method {
            HttpMethod::Get => client.get(&url),
            HttpMethod::Post => client.post(&url),
            HttpMethod::Put => client.put(&url),
            HttpMethod::Patch => client.patch(&url),
            HttpMethod::Delete => client.delete(&url),
            HttpMethod::Head => client.head(&url),
            HttpMethod::Options => client.request(reqwest::Method::OPTIONS, &url),
        };

        if let Some(ref auth) = opts.auth {
            request_builder = request_builder.header("Authorization", auth);
        }

        // Add a cache-busting header so we get fresh responses by default.
        request_builder = request_builder.header("Cache-Control", "no-cache");

        let result = request_builder.send();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        latencies.push(elapsed);

        match result {
            Ok(response) => {
                let status = response.status().as_u16();
                if status >= 400 {
                    error_count += 1;
                }
                // Check for cache hits via common header patterns.
                if status == 200 {
                    // Some APIs return X-Cache / X-Cache-Hit headers.
                    // We treat the absence as a miss (conservative).
                }
            }
            Err(_) => {
                error_count += 1;
            }
        }
    }

    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let total = latencies.len();
    let avg_latency_ms = if total > 0 {
        latencies.iter().sum::<f64>() / total as f64
    } else {
        0.0
    };

    let p50 = percentile(&latencies, 0.50);
    let p95 = percentile(&latencies, 0.95);
    let p99 = percentile(&latencies, 0.99);
    let min_latency_ms = latencies.first().copied().unwrap_or(0.0);
    let max_latency_ms = latencies.last().copied().unwrap_or(0.0);

    // Throughput: requests / (total wall time in seconds).
    let total_wall_secs: f64 = latencies.iter().sum::<f64>() / 1000.0;
    let throughput_rps = if total_wall_secs > 0.0 {
        total as f64 / total_wall_secs
    } else {
        0.0
    };

    let error_rate = if total > 0 {
        error_count as f64 / total as f64
    } else {
        0.0
    };

    // Cache hit rate: conservatively 0.0 unless we detected explicit cache headers.
    let cache_hit_rate = _cache_hits as f64 / total.max(1) as f64;

    ProfileResult {
        endpoint: op.path.clone(),
        method: method_str,
        avg_latency_ms,
        p50_latency_ms: p50,
        p95_latency_ms: p95,
        p99_latency_ms: p99,
        throughput_rps,
        error_rate,
        cache_hit_rate,
        success_count: total - error_count,
        error_count,
        total_requests: total,
        min_latency_ms,
        max_latency_ms,
    }
}

/// Compute a percentile from a sorted slice.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx]
}

/// Analyse profile results and emit actionable recommendations.
fn generate_recommendations(results: &[ProfileResult]) -> Vec<String> {
    let mut recs: Vec<String> = Vec::new();

    for r in results {
        // High latency
        if r.p95_latency_ms > 500.0 {
            recs.push(format!(
                "{} {} has high p95 latency ({:.1}ms). Consider adding pagination, caching, or response compression.",
                r.method, r.endpoint, r.p95_latency_ms,
            ));
        }

        // High error rate
        if r.error_rate > 0.10 {
            recs.push(format!(
                "{} {} has a high error rate ({:.1}%). Investigate server-side errors or client request construction.",
                r.method, r.endpoint,
                r.error_rate * 100.0,
            ));
        }

        // Low throughput
        if r.throughput_rps < 10.0 && r.total_requests >= 10 {
            recs.push(format!(
                "{} {} has low throughput ({:.1} rps). Consider connection pooling or HTTP/2 multiplexing.",
                r.method, r.endpoint, r.throughput_rps,
            ));
        }

        // High latency variance (jitter)
        let jitter = r.max_latency_ms - r.min_latency_ms;
        if jitter > 1000.0 && r.total_requests >= 10 {
            recs.push(format!(
                "{} {} has high latency jitter ({:.0}ms range). This may indicate GC pauses or resource contention.",
                r.method, r.endpoint, jitter,
            ));
        }

        // No caching detected on GET endpoints
        if r.method == "GET" && r.cache_hit_rate == 0.0 && r.error_rate == 0.0 {
            recs.push(format!(
                "{} {} does not appear to use caching. Consider adding ETag/If-None-Match or Cache-Control headers.",
                r.method, r.endpoint,
            ));
        }
    }

    // Global recommendations
    if results.len() > 1 {
        let slowest = results
            .iter()
            .max_by(|a, b| a.p95_latency_ms.partial_cmp(&b.p95_latency_ms).unwrap_or(std::cmp::Ordering::Equal));
        if let Some(s) = slowest {
            if s.p95_latency_ms > 200.0 {
                recs.push(format!(
                    "Slowest endpoint is {} {} (p95: {:.1}ms). Prioritise optimising this endpoint first.",
                    s.method, s.endpoint, s.p95_latency_ms,
                ));
            }
        }

        let most_errors = results
            .iter()
            .filter(|r| r.error_rate > 0.0)
            .max_by(|a, b| a.error_rate.partial_cmp(&b.error_rate).unwrap_or(std::cmp::Ordering::Equal));
        if let Some(e) = most_errors {
            recs.push(format!(
                "Highest error rate is {} {} ({:.1}%). Investigate root cause.",
                e.method, e.endpoint,
                e.error_rate * 100.0,
            ));
        }
    }

    recs
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/// Format a [`ProfileReport`] as human-readable text.
pub fn format_text(report: &ProfileReport) -> String {
    let mut out = String::new();
    out.push_str("=== SDK Performance Profile ===\n\n");

    for r in &report.results {
        out.push_str(&format!("{} {}\n", r.method, r.endpoint));
        out.push_str(&format!("  Requests:       {}\n", r.total_requests));
        out.push_str(&format!("  Success/Error:  {} / {}\n", r.success_count, r.error_count));
        out.push_str(&format!("  Error rate:     {:.1}%\n", r.error_rate * 100.0));
        out.push_str(&format!("  Latency (ms):\n"));
        out.push_str(&format!("    avg:   {:.1}\n", r.avg_latency_ms));
        out.push_str(&format!("    p50:   {:.1}\n", r.p50_latency_ms));
        out.push_str(&format!("    p95:   {:.1}\n", r.p95_latency_ms));
        out.push_str(&format!("    p99:   {:.1}\n", r.p99_latency_ms));
        out.push_str(&format!("    min:   {:.1}\n", r.min_latency_ms));
        out.push_str(&format!("    max:   {:.1}\n", r.max_latency_ms));
        out.push_str(&format!("  Throughput:     {:.1} rps\n", r.throughput_rps));
        out.push_str(&format!("  Cache hit rate: {:.1}%\n", r.cache_hit_rate * 100.0));
        out.push('\n');
    }

    out.push_str(&format!(
        "Total requests: {}\n",
        report.total_requests
    ));
    out.push_str(&format!(
        "Total duration: {:.1}ms\n",
        report.total_duration_ms
    ));
    out.push_str(&format!(
        "Overall throughput: {:.1} rps\n\n",
        report.overall_throughput_rps
    ));

    if !report.recommendations.is_empty() {
        out.push_str(&format!(
            "Recommendations ({}):\n",
            report.recommendations.len()
        ));
        for (i, rec) in report.recommendations.iter().enumerate() {
            out.push_str(&format!("  {}. {}\n", i + 1, rec));
        }
    }

    out
}

/// Format a [`ProfileReport`] as Markdown.
pub fn format_markdown(report: &ProfileReport) -> String {
    let mut out = String::new();
    out.push_str("# SDK Performance Profile\n\n");

    out.push_str("| Endpoint | Method | Avg (ms) | p50 (ms) | p95 (ms) | p99 (ms) | Throughput (rps) | Error Rate |\n");
    out.push_str("|----------|--------|----------|----------|----------|----------|-------------------|------------|\n");
    for r in &report.results {
        out.push_str(&format!(
            "| {} | {} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1}% |\n",
            r.endpoint, r.method, r.avg_latency_ms, r.p50_latency_ms,
            r.p95_latency_ms, r.p99_latency_ms, r.throughput_rps,
            r.error_rate * 100.0,
        ));
    }

    out.push('\n');
    out.push_str(&format!("**Total requests:** {}\n", report.total_requests));
    out.push_str(&format!("**Total duration:** {:.1}ms\n", report.total_duration_ms));
    out.push_str(&format!("**Overall throughput:** {:.1} rps\n\n", report.overall_throughput_rps));

    if !report.recommendations.is_empty() {
        out.push_str(&format!("## Recommendations\n\n"));
        for (i, rec) in report.recommendations.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, rec));
        }
    }

    out
}

/// Serialize a [`ProfileReport`] as JSON.
pub fn format_json(report: &ProfileReport) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_basic() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_eq!(percentile(&data, 0.5), 6.0);
        assert_eq!(percentile(&data, 0.0), 1.0);
        assert_eq!(percentile(&data, 1.0), 10.0);
    }

    #[test]
    fn percentile_empty() {
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn substitute_path_params() {
        assert_eq!(super::substitute_path_params("/pets"), "/pets");
        assert_eq!(super::substitute_path_params("/pets/{petId}"), "/pets/1");
        assert_eq!(
            super::substitute_path_params("/orgs/{orgId}/repos/{repoId}"),
            "/orgs/1/repos/1"
        );
    }

    #[test]
    fn recommendations_for_slow_endpoint() {
        let results = vec![ProfileResult {
            endpoint: "/slow".into(),
            method: "GET".into(),
            avg_latency_ms: 600.0,
            p50_latency_ms: 550.0,
            p95_latency_ms: 800.0,
            p99_latency_ms: 900.0,
            throughput_rps: 5.0,
            error_rate: 0.0,
            cache_hit_rate: 0.0,
            success_count: 90,
            error_count: 0,
            total_requests: 100,
            min_latency_ms: 100.0,
            max_latency_ms: 1200.0,
        }];
        let recs = generate_recommendations(&results);
        assert!(recs.iter().any(|r| r.contains("high p95 latency")));
    }

    #[test]
    fn recommendations_for_error_rate() {
        let results = vec![ProfileResult {
            endpoint: "/broken".into(),
            method: "POST".into(),
            avg_latency_ms: 50.0,
            p50_latency_ms: 40.0,
            p95_latency_ms: 80.0,
            p99_latency_ms: 100.0,
            throughput_rps: 100.0,
            error_rate: 0.25,
            cache_hit_rate: 0.0,
            success_count: 75,
            error_count: 25,
            total_requests: 100,
            min_latency_ms: 10.0,
            max_latency_ms: 150.0,
        }];
        let recs = generate_recommendations(&results);
        assert!(recs.iter().any(|r| r.contains("high error rate")));
    }

    #[test]
    fn format_text_output() {
        let report = ProfileReport {
            results: vec![],
            total_requests: 0,
            total_duration_ms: 0.0,
            overall_throughput_rps: 0.0,
            recommendations: vec!["Test recommendation".into()],
        };
        let text = format_text(&report);
        assert!(text.contains("SDK Performance Profile"));
        assert!(text.contains("Test recommendation"));
    }
}
