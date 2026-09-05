use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use std::{
    collections::BTreeMap,
    fmt::Write,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

const MAX_HTTP_LABEL_SETS: usize = 512;
const BUCKETS: [f64; 10] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0, 30.0];
type HttpKey = (String, &'static str, &'static str);
type JobKey = (&'static str, &'static str);
static HTTP: LazyLock<Mutex<BTreeMap<HttpKey, Histogram>>> = LazyLock::new(Mutex::default);
static JOBS: LazyLock<Mutex<BTreeMap<JobKey, Histogram>>> = LazyLock::new(Mutex::default);
static HTTP_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
static JOBS_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
static HTTP_DROPPED: AtomicU64 = AtomicU64::new(0);
static JOB_RETRIES: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct Histogram {
    count: u64,
    sum: f64,
    buckets: [u64; BUCKETS.len()],
}

impl Histogram {
    fn observe(&mut self, seconds: f64) {
        self.count = self.count.saturating_add(1);
        self.sum += seconds;
        for (index, bound) in BUCKETS.iter().enumerate() {
            if seconds <= *bound {
                self.buckets[index] = self.buckets[index].saturating_add(1);
            }
        }
    }

    fn render(&self, output: &mut String, name: &str, labels: &str) {
        for (index, bound) in BUCKETS.iter().enumerate() {
            let count = self.buckets[index];
            let _ = writeln!(output, "{name}_bucket{{{labels},le=\"{bound}\"}} {count}");
        }
        let _ = writeln!(
            output,
            "{name}_bucket{{{labels},le=\"+Inf\"}} {}",
            self.count
        );
        let _ = writeln!(output, "{name}_sum{{{labels}}} {}", self.sum);
        let _ = writeln!(output, "{name}_count{{{labels}}} {}", self.count);
    }
}

struct InFlight(&'static AtomicU64);

impl InFlight {
    fn new(counter: &'static AtomicU64) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self(counter)
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn method_label(method: &str) -> &'static str {
    match method {
        "GET" => "GET",
        "HEAD" => "HEAD",
        "POST" => "POST",
        "PUT" => "PUT",
        "PATCH" => "PATCH",
        "DELETE" => "DELETE",
        "OPTIONS" => "OPTIONS",
        _ => "OTHER",
    }
}

pub(crate) async fn observe_http(request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", MatchedPath::as_str)
        .to_owned();
    if matches!(
        route.as_str(),
        "/metrics" | "/health/live" | "/health/ready"
    ) {
        return next.run(request).await;
    }
    let method = method_label(request.method().as_str());
    let _active = InFlight::new(&HTTP_IN_FLIGHT);
    let start = Instant::now();
    let response = next.run(request).await;
    let status = match response.status().as_u16() / 100 {
        1 => "1xx",
        2 => "2xx",
        3 => "3xx",
        4 => "4xx",
        _ => "5xx",
    };
    if let Ok(mut series) = HTTP.lock() {
        let key = (route, method, status);
        if key.0.len() <= 256 && (series.contains_key(&key) || series.len() < MAX_HTTP_LABEL_SETS) {
            series
                .entry(key)
                .or_default()
                .observe(start.elapsed().as_secs_f64());
        } else {
            HTTP_DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
    response
}

// Job instrumentation is unused when the generated Jobs module is disabled.
#[allow(dead_code)]
pub(crate) struct JobAttempt {
    kind: &'static str,
    started: Instant,
    outcome: &'static str,
    _active: InFlight,
}

#[allow(dead_code)]
impl JobAttempt {
    pub(crate) fn new(kind: &str, attempts: i32) -> Self {
        let kind = match kind {
            "mail.send" => "mail",
            "report.render" => "report",
            "report.cleanup" => "report_cleanup",
            _ => "custom",
        };
        if attempts > 1 {
            JOB_RETRIES.fetch_add(1, Ordering::Relaxed);
        }
        Self {
            kind,
            started: Instant::now(),
            outcome: "interrupted",
            _active: InFlight::new(&JOBS_IN_FLIGHT),
        }
    }

    pub(crate) fn finish(&mut self, outcome: &'static str) {
        self.outcome = match outcome {
            "succeeded" => "succeeded",
            "failed" => "failed",
            "cancelled" => "cancelled",
            "lease_lost" => "lease_lost",
            "database_error" => "database_error",
            _ => "interrupted",
        };
    }
}

impl Drop for JobAttempt {
    fn drop(&mut self) {
        if let Ok(mut series) = JOBS.lock() {
            series
                .entry((self.kind, self.outcome))
                .or_default()
                .observe(self.started.elapsed().as_secs_f64());
        }
    }
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

pub(crate) fn render(ready: u8) -> String {
    let mut output = format!(
        "# HELP appstruct_health_ready Whether the application lifecycle is ready.\n# TYPE appstruct_health_ready gauge\nappstruct_health_ready {ready}\n"
    );
    for (name, counter, kind, help) in [
        (
            "appstruct_http_in_flight",
            &HTTP_IN_FLIGHT,
            "gauge",
            "HTTP requests awaiting response headers.",
        ),
        (
            "appstruct_jobs_in_flight",
            &JOBS_IN_FLIGHT,
            "gauge",
            "Active job attempts.",
        ),
        (
            "appstruct_http_dropped_observations_total",
            &HTTP_DROPPED,
            "counter",
            "HTTP observations omitted due to label limits.",
        ),
        (
            "appstruct_job_retries_total",
            &JOB_RETRIES,
            "counter",
            "Claimed job attempts after the first attempt.",
        ),
    ] {
        let value = counter.load(Ordering::Relaxed);
        let _ = writeln!(
            output,
            "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}"
        );
    }
    output.push_str("# HELP appstruct_http_request_duration_seconds Time to response headers, excluding health and metrics.\n# TYPE appstruct_http_request_duration_seconds histogram\n");
    if let Ok(series) = HTTP.lock() {
        for ((route, method, status), histogram) in series.iter() {
            let route = escape_label(route);
            let labels = format!("route=\"{route}\",method=\"{method}\",status_class=\"{status}\"");
            histogram.render(
                &mut output,
                "appstruct_http_request_duration_seconds",
                &labels,
            );
        }
    }
    output.push_str("# HELP appstruct_job_duration_seconds Claimed job attempt duration, including persistence.\n# TYPE appstruct_job_duration_seconds histogram\n");
    if let Ok(series) = JOBS.lock() {
        for ((kind, outcome), histogram) in series.iter() {
            let labels = format!("kind=\"{kind}\",outcome=\"{outcome}\"");
            histogram.render(&mut output, "appstruct_job_duration_seconds", &labels);
        }
    }
    output
}
