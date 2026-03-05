use super::*;
// ---------------------------------------------------------------------------
// P2: Stream management endpoints
// ---------------------------------------------------------------------------

/// GET /v1/streams — list active stream subscriptions
pub(super) async fn handle_streams_list() -> impl IntoResponse {
    let data = blockcell_tools::stream_subscribe::list_streams().await;
    Json(data)
}

#[derive(Deserialize)]
pub(super) struct StreamDataQuery {
    #[serde(default = "default_stream_limit")]
    limit: usize,
}

fn default_stream_limit() -> usize {
    50
}

/// GET /v1/streams/:id/data — get buffered data for a stream
pub(super) async fn handle_stream_data(
    AxumPath(stream_id): AxumPath<String>,
    Query(params): Query<StreamDataQuery>,
) -> impl IntoResponse {
    match blockcell_tools::stream_subscribe::get_stream_data(&stream_id, params.limit).await {
        Ok(data) => Json(data),
        Err(e) => Json(serde_json::json!({ "error": format!("{}", e) })),
    }
}
