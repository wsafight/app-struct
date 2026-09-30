use serde::Serialize;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RuntimeStatus {
    pub api_url: String,
    pub api_ready: bool,
    pub web_url: String,
    pub web_ready: bool,
    pub metrics: Option<RuntimeMetrics>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RuntimeMetrics {
    pub requests: u64,
    pub server_errors: u64,
    pub average_latency_ms: Option<f64>,
    pub jobs_in_flight: u64,
    pub job_retries: u64,
}

pub(crate) fn runtime_status(project: &Path) -> RuntimeStatus {
    let environment = crate::environment::ProjectEnvironment::load(project).unwrap_or_default();
    let api_port = environment
        .get("APPSTRUCT_API_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);
    let web_port = environment
        .get("APPSTRUCT_WEB_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(5173);
    let api_url = format!("http://127.0.0.1:{api_port}");
    let web_url = format!("http://127.0.0.1:{web_port}");
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
    else {
        return RuntimeStatus {
            api_url,
            api_ready: false,
            web_url,
            web_ready: false,
            metrics: None,
        };
    };
    let api_ready = client
        .get(format!("{api_url}/health/ready"))
        .send()
        .is_ok_and(|response| response.status().is_success());
    let web_ready = client
        .get(&web_url)
        .send()
        .is_ok_and(|response| response.status().is_success());
    let metrics = api_ready
        .then(|| client.get(format!("{api_url}/metrics")).send().ok())
        .flatten()
        .filter(|response| response.status().is_success())
        .and_then(|response| response.text().ok())
        .map(|source| parse_metrics(&source));
    RuntimeStatus {
        api_url,
        api_ready,
        web_url,
        web_ready,
        metrics,
    }
}

fn parse_metrics(source: &str) -> RuntimeMetrics {
    let request_metric = "appstruct_http_request_duration_seconds_count";
    let request_samples = metric_value_sum(source, request_metric, None);
    let requests = metric_sum(source, request_metric, None);
    let server_errors = metric_sum(source, request_metric, Some("status_class=\"5xx\""));
    let duration = metric_value_sum(source, "appstruct_http_request_duration_seconds_sum", None);
    RuntimeMetrics {
        requests,
        server_errors,
        average_latency_ms: (request_samples > 0.0).then_some(duration * 1_000.0 / request_samples),
        jobs_in_flight: metric_sum(source, "appstruct_jobs_in_flight", None),
        job_retries: metric_sum(source, "appstruct_job_retries_total", None),
    }
}

fn metric_sum(source: &str, name: &str, required_label: Option<&str>) -> u64 {
    source
        .lines()
        .filter(|line| line.starts_with(name))
        .filter(|line| required_label.is_none_or(|label| line.contains(label)))
        .filter_map(|line| line.split_whitespace().last()?.parse::<u64>().ok())
        .fold(0, u64::saturating_add)
}

fn metric_value_sum(source: &str, name: &str, required_label: Option<&str>) -> f64 {
    source
        .lines()
        .filter(|line| line.starts_with(name))
        .filter(|line| required_label.is_none_or(|label| line.contains(label)))
        .filter_map(|line| line.split_whitespace().last()?.parse::<f64>().ok())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prometheus_metrics_are_summarized_without_label_cardinality() {
        let metrics = parse_metrics(
            "appstruct_http_request_duration_seconds_count{route=\"/a\",status_class=\"2xx\"} 3\n\
             appstruct_http_request_duration_seconds_sum{route=\"/a\",status_class=\"2xx\"} 0.12\n\
             appstruct_http_request_duration_seconds_count{route=\"/b\",status_class=\"5xx\"} 1\n\
             appstruct_http_request_duration_seconds_sum{route=\"/b\",status_class=\"5xx\"} 0.08\n\
             appstruct_jobs_in_flight 2\n\
             appstruct_job_retries_total 4\n",
        );
        assert_eq!(metrics.requests, 4);
        assert_eq!(metrics.server_errors, 1);
        assert_eq!(metrics.average_latency_ms, Some(50.0));
        assert_eq!(metrics.jobs_in_flight, 2);
        assert_eq!(metrics.job_retries, 4);
    }
}
