use actix_web::web;
pub mod shared;
pub(crate) use shared::*;

#[cfg(test)]
mod tests;

pub mod generate_process_draft_from_file;
pub mod get_task;
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/workflows")
            .route(
                "/definitions/drafts/from-file",
                web::post().to(generate_process_draft_from_file::generate_process_draft_from_file),
            )
            .route(
                "/definitions/drafts/assistant-chat",
                web::post().to(generate_process_draft_from_file::chat_process_draft_assistant_stream),
            )
            .route(
                "/definitions/drafts/assistant-chat/stream",
                web::post().to(generate_process_draft_from_file::chat_process_draft_assistant_stream),
            )
            .route(
                "/definitions",
                web::get().to(generate_process_draft_from_file::list_process_definitions),
            )
            .route(
                "/definitions/{process_definition_id}",
                web::get().to(generate_process_draft_from_file::get_process_definition),
            )
            .route(
                "/definitions/{process_definition_id}/xml",
                web::get().to(generate_process_draft_from_file::get_process_definition_xml),
            )
            .route(
                "/deployments",
                web::get().to(generate_process_draft_from_file::list_deployments),
            )
            .route(
                "/deployments",
                web::post().to(generate_process_draft_from_file::create_deployment),
            )
            .route(
                "/deployments/{deployment_id}",
                web::delete().to(generate_process_draft_from_file::delete_deployment),
            )
            .route(
                "/instances",
                web::post().to(generate_process_draft_from_file::start_process_instance),
            )
            .route(
                "/instances",
                web::get().to(generate_process_draft_from_file::list_process_instances),
            )
            .route(
                "/instances/{process_instance_id}",
                web::get().to(generate_process_draft_from_file::get_process_instance),
            )
            .route(
                "/instances/{process_instance_id}",
                web::delete().to(generate_process_draft_from_file::delete_process_instance),
            )
            .route("/tasks", web::get().to(generate_process_draft_from_file::list_tasks))
            .route("/tasks/{task_id}", web::get().to(get_task::get_task))
            .route("/tasks/{task_id}/claim", web::post().to(get_task::claim_task))
            .route("/tasks/{task_id}/unclaim", web::post().to(get_task::unclaim_task))
            .route("/tasks/{task_id}/complete", web::post().to(get_task::complete_task))
            .route(
                "/instances/with-subprocess",
                web::post().to(get_task::start_process_with_subprocess),
            )
            .route(
                "/instances/{process_instance_id}/executions",
                web::get().to(get_task::get_executions),
            )
            .route(
                "/instances/{process_instance_id}/subprocess-result",
                web::get().to(get_task::get_subprocess_result),
            )
            .route(
                "/instances/{process_instance_id}/variables",
                web::get().to(get_task::get_variables),
            )
            .route(
                "/instances/{process_instance_id}/variables",
                web::post().to(get_task::set_variables),
            )
            .route("/history/instances", web::get().to(get_task::history_process_instances))
            .route("/history/tasks", web::get().to(get_task::history_tasks))
            .route("/health", web::get().to(get_task::flowable_health)),
    );
}
