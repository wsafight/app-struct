use appstruct_generated_backend::{
    ApiError, AppExtensions, HookOperation, RequestContext,
    api::project,
    database::{DatabaseExecutor, Sql},
    entities,
    extensions::{ProjectHooks, ProjectPolicy},
};
use async_trait::async_trait;

struct ProjectRules;

#[async_trait]
impl ProjectPolicy for ProjectRules {
    async fn can_create(
        &self,
        _ctx: &RequestContext,
        input: &project::CreateInput,
    ) -> Result<bool, ApiError> {
        // Demonstration rule; real deployments should combine their own actor and record policy.
        Ok(input.code != "DENY")
    }
}

#[async_trait]
impl ProjectHooks for ProjectRules {
    async fn after_create(
        &self,
        ctx: &RequestContext,
        model: &entities::project::Model,
    ) -> Result<(), ApiError> {
        ctx.database().execute(Sql::bind(
            "INSERT INTO tasks (id, project_id, title, status, priority, created_at, revision) VALUES ($1, $2, 'Review project', 'todo', 0, CURRENT_TIMESTAMP, 1)",
            [uuid::Uuid::now_v7().into(), model.id.into()],
        )).await?;
        // Explicit fault injection demonstrates rollback of both the project and its task.
        if model.code == "FAIL" {
            tracing::warn!(hook = "Project.after_create", "demonstration failure");
            return Err(ApiError::Forbidden);
        }
        Ok(())
    }

    async fn after_commit(
        &self,
        _ctx: &RequestContext,
        _operation: HookOperation,
        model: &entities::project::Model,
    ) -> Result<(), ApiError> {
        // This error is observable but cannot undo the committed database transaction.
        if model.code == "COMMIT" {
            return Err(ApiError::Internal);
        }
        Ok(())
    }
}

pub fn extensions() -> AppExtensions {
    AppExtensions::builder()
        .project_hooks(ProjectRules)
        .project_policy(ProjectRules)
        .build()
}
