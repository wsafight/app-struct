use appstruct_generated_backend::AppExtensions;
mod approvals;
mod assistant;

#[cfg(test)]
mod tests;

pub fn extensions() -> AppExtensions {
    AppExtensions::builder()
        .routes(approvals::router().merge(assistant::router()))
        .job_handler(assistant::AiJobs::default())
        .build()
}
