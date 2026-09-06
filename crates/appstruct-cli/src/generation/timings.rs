use super::web_format;
use std::time::Duration;

#[derive(Default)]
pub(super) struct GenerationTimings {
    pub cache_lookup: Duration,
    pub compiler: Option<Duration>,
    pub codegen: Option<appstruct_codegen::PlanTimings>,
    pub web_format: Option<web_format::FormatTimings>,
    pub output: Option<Duration>,
    pub total: Duration,
}

pub(super) fn render_success(
    check: bool,
    app_name: &str,
    artifact_count: usize,
    changed: usize,
    cache_hit: bool,
    timings: Option<&GenerationTimings>,
) {
    if crate::report::is_json() {
        let mut result = serde_json::json!({
            "command": "generate",
            "mode": if check { "check" } else { "write" },
            "app": app_name,
            "artifact_count": artifact_count,
            "changed": changed,
            "cache_hit": cache_hit,
            "current": check,
        });
        if let Some(timings) = timings {
            result["timings"] = timings_json(timings);
        }
        crate::report::success(&result);
    } else if check {
        let cache = if cache_hit { "; cache hit" } else { "" };
        println!("Generated artifacts are current ({artifact_count} files{cache})");
    } else {
        let cache = if cache_hit { "; cache hit" } else { "" };
        println!("Generated {artifact_count} artifacts for {app_name} ({changed} changed{cache})");
    }
    if !crate::report::is_json()
        && let Some(timings) = timings
    {
        render_text_timings(timings);
    }
}

fn timings_json(timings: &GenerationTimings) -> serde_json::Value {
    let mut value = serde_json::json!({
        "total_ms": milliseconds(timings.total),
        "cache_lookup_ms": milliseconds(timings.cache_lookup),
        "compiler_ms": timings.compiler.map(milliseconds),
        "output_ms": timings.output.map(milliseconds),
    });
    if let Some(codegen) = &timings.codegen {
        value["codegen"] = serde_json::json!({
            "total_ms": milliseconds(codegen.total),
            "validation_ms": milliseconds(codegen.validation),
            "canonical_ir_ms": milliseconds(codegen.canonical_ir),
            "planners_ms": {
                "database": milliseconds(codegen.planners.database),
                "backend": milliseconds(codegen.planners.backend),
                "openapi": milliseconds(codegen.planners.openapi),
                "typescript": milliseconds(codegen.planners.typescript),
                "web": milliseconds(codegen.planners.web),
                "modules": milliseconds(codegen.planners.modules),
            },
            "rustfmt": {
                "total_ms": milliseconds(codegen.rustfmt.total),
                "process_ms": milliseconds(codegen.rustfmt.process),
                "memory_hits": codegen.rustfmt.memory_hits,
                "persistent_hits": codegen.rustfmt.persistent_hits,
                "misses": codegen.rustfmt.misses,
            },
        });
    }
    if let Some(web) = &timings.web_format {
        value["web_format"] = serde_json::json!({
            "total_ms": milliseconds(web.total),
            "formatter_setup_ms": milliseconds(web.formatter_setup),
            "prettier_ms": milliseconds(web.prettier),
            "persistent_hits": web.persistent_hits,
            "misses": web.misses,
        });
    }
    value
}

fn render_text_timings(timings: &GenerationTimings) {
    println!(
        "Timings: total {:.1} ms; cache lookup {:.1} ms; compiler {:.1} ms; output {:.1} ms",
        milliseconds(timings.total),
        milliseconds(timings.cache_lookup),
        timings.compiler.map_or(0.0, milliseconds),
        timings.output.map_or(0.0, milliseconds),
    );
    if let Some(codegen) = &timings.codegen {
        println!(
            "  planners: database {:.1}; backend {:.1}; OpenAPI {:.1}; TypeScript {:.1}; Web {:.1}; modules {:.1} ms",
            milliseconds(codegen.planners.database),
            milliseconds(codegen.planners.backend),
            milliseconds(codegen.planners.openapi),
            milliseconds(codegen.planners.typescript),
            milliseconds(codegen.planners.web),
            milliseconds(codegen.planners.modules),
        );
        println!(
            "  rustfmt: {:.1} ms ({:.1} ms process; {} memory hits; {} persistent hits; {} misses)",
            milliseconds(codegen.rustfmt.total),
            milliseconds(codegen.rustfmt.process),
            codegen.rustfmt.memory_hits,
            codegen.rustfmt.persistent_hits,
            codegen.rustfmt.misses,
        );
    }
    if let Some(web) = &timings.web_format {
        println!(
            "  Prettier: {:.1} ms ({:.1} ms setup; {} persistent hits; {} misses)",
            milliseconds(web.prettier),
            milliseconds(web.formatter_setup),
            web.persistent_hits,
            web.misses,
        );
    }
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
