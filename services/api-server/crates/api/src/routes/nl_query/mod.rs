mod conversations;
mod query;
mod shared;
mod stream;
mod tools_stream;

#[cfg(test)]
mod tests;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/ai/nl-query")
            .route("", web::post().to(query::query_natural_language))
            .route("/stream", web::post().to(stream::query_natural_language_stream))
            .route(
                "/stream-with-tools",
                web::post().to(tools_stream::stream_with_tools_gate),
            )
            .route("/suggestions", web::get().to(conversations::get_query_suggestions))
            .route("/conversations", web::get().to(conversations::list_conversations))
            .route(
                "/conversations/{conversation_id}/messages",
                web::get().to(conversations::get_conversation_messages),
            )
            .route("/{conversation_id}", web::delete().to(conversations::end_conversation)),
    );
}
