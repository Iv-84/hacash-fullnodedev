use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::OnceLock;
use std::str::FromStr;

static GLOBAL_API_SERVICES: OnceLock<Mutex<Vec<Arc<dyn ApiService>>>> = OnceLock::new();

fn global_services_store() -> &'static Mutex<Vec<Arc<dyn ApiService>>> {
    GLOBAL_API_SERVICES.get_or_init(|| Mutex::new(vec![]))
}

pub(crate) fn set_api_services(services: Vec<Arc<dyn ApiService>>) {
    let mut locker = global_services_store().lock().unwrap();
    *locker = services;
}

pub fn global_api_services() -> Vec<Arc<dyn ApiService>> {
    global_services_store().lock().unwrap().clone()
}

fn api_exec_ctx(ctx: &ApiCtx) -> ApiExecCtx {
    ApiExecCtx {
        engine: ctx.engine.clone(),
        hnoder: ctx.hcshnd.clone(),
        launch_time: ctx.launch_time,
        miner_worker_notice_count: ctx.miner_worker_notice_count.clone(),
    }
}

fn map_headers(headers: &HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|vv| (k.to_string(), vv.to_owned())))
        .collect()
}

fn api_resp_to_axum(resp: ApiResponse) -> Response {
    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK);
    let mut headers = HeaderMap::new();
    for (k, v) in resp.headers {
        let Ok(name) = HeaderName::from_str(&k) else {
            continue;
        };
        let Ok(value) = HeaderValue::from_str(&v) else {
            continue;
        };
        headers.insert(name, value);
    }
    (status, headers, resp.body).into_response()
}

fn route_request(headers: HeaderMap, query: HashMap<String, String>, body: Vec<u8>) -> ApiRequest {
    ApiRequest {
        query,
        headers: map_headers(&headers),
        body,
    }
}

fn route_handler_exec(
    handler: ApiHandlerFn,
    ctx: &ApiCtx,
    headers: HeaderMap,
    query: HashMap<String, String>,
    body: Vec<u8>,
) -> Response {
    let req = route_request(headers, query, body);
    let exec = api_exec_ctx(ctx);
    api_resp_to_axum(handler(&exec, req))
}

async fn route_handler_exec_async(
    handler: ApiHandlerAsyncFn,
    ctx: ApiCtx,
    headers: HeaderMap,
    query: HashMap<String, String>,
    body: Vec<u8>,
) -> Response {
    let req = route_request(headers, query, body);
    let exec = api_exec_ctx(&ctx);
    api_resp_to_axum(handler(exec, req).await)
}

fn build_method_router(route: ApiRoute) -> MethodRouter<ApiCtx> {
    match (route.method, route.handler) {
        (ApiMethod::Get, ApiHandler::Sync(handler)) => get(
            move |State(ctx): State<ApiCtx>,
                  headers: HeaderMap,
                  Query(query): Query<HashMap<String, String>>| async move {
                route_handler_exec(handler, &ctx, headers, query, vec![])
            },
        ),
        (ApiMethod::Get, ApiHandler::Async(handler)) => get(
            move |State(ctx): State<ApiCtx>,
                  headers: HeaderMap,
                  Query(query): Query<HashMap<String, String>>| async move {
                route_handler_exec_async(handler, ctx, headers, query, vec![]).await
            },
        ),
        (ApiMethod::Post, ApiHandler::Sync(handler)) => post(
            move |State(ctx): State<ApiCtx>,
                  headers: HeaderMap,
                  Query(query): Query<HashMap<String, String>>,
                  body: Bytes| async move {
                route_handler_exec(handler, &ctx, headers, query, body.to_vec())
            },
        ),
        (ApiMethod::Post, ApiHandler::Async(handler)) => post(
            move |State(ctx): State<ApiCtx>,
                  headers: HeaderMap,
                  Query(query): Query<HashMap<String, String>>,
                  body: Bytes| async move {
                route_handler_exec_async(handler, ctx, headers, query, body.to_vec()).await
            },
        ),
    }
}

fn route_is_enabled(route: &ApiRoute, debug_open: bool) -> bool {
    !route.debug || debug_open
}

pub fn merge_registered_services(
    mut rtr: Router<ApiCtx>,
    services: Vec<Arc<dyn ApiService>>,
    debug_open: bool,
) -> Router<ApiCtx> {
    let mut all_services = global_api_services();
    all_services.extend(services);
    for svc in all_services {
        for route in svc.routes() {
            if !route_is_enabled(&route, debug_open) {
                continue;
            }
            let mr = build_method_router(route.clone());
            rtr = rtr.route(route.path.as_str(), mr);
        }
    }
    rtr
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

    struct DummyEngine;
    impl EngineRead for DummyEngine {}
    impl Engine for DummyEngine {}

    struct DummyNode {
        engine: Arc<dyn Engine>,
    }

    impl HNoder for DummyNode {
        fn engine(&self) -> Arc<dyn Engine> {
            self.engine.clone()
        }
    }

    fn test_ctx() -> ApiCtx {
        let engine: Arc<dyn Engine> = Arc::new(DummyEngine);
        let hnoder: Arc<dyn HNoder> = Arc::new(DummyNode {
            engine: engine.clone(),
        });
        ApiCtx {
            engine,
            hcshnd: hnoder,
            blocks: Arc::default(),
            miner_worker_notice_count: Arc::default(),
            launch_time: 0,
            blocks_max: 4,
        }
    }

    fn sync_handler(_: &ApiExecCtx, _: ApiRequest) -> ApiResponse {
        ApiResponse::json("{\"ret\":0,\"kind\":\"sync\"}".to_owned())
    }

    fn async_handler(
        _: ApiExecCtx,
        _: ApiRequest,
    ) -> Pin<Box<dyn Future<Output = ApiResponse> + Send + 'static>> {
        Box::pin(async {
            ApiResponse::json("{\"ret\":0,\"kind\":\"async\"}".to_owned())
        })
    }

    #[test]
    fn debug_routes_are_prefixed_and_gated() {
        let normal = ApiRoute::get("/query/ok", sync_handler);
        let debug = ApiRoute::debug_get("trace/tx", sync_handler);
        let debug_slash = ApiRoute::debug_post("/submit/raw", sync_handler);

        assert_eq!(debug.path, "/debug/trace/tx");
        assert_eq!(debug_slash.path, "/debug/submit/raw");
        assert!(!normal.debug);
        assert!(debug.debug);

        assert!(route_is_enabled(&normal, false));
        assert!(route_is_enabled(&normal, true));
        assert!(!route_is_enabled(&debug, false));
        assert!(route_is_enabled(&debug, true));
    }

    #[test]
    fn sync_and_async_handlers_both_execute() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let ctx = test_ctx();
            let sync_resp = route_handler_exec(
                sync_handler,
                &ctx,
                HeaderMap::new(),
                HashMap::new(),
                vec![],
            );
            let async_resp = route_handler_exec_async(
                async_handler,
                ctx,
                HeaderMap::new(),
                HashMap::new(),
                vec![],
            )
            .await;

            assert_eq!(sync_resp.status(), StatusCode::OK);
            assert_eq!(async_resp.status(), StatusCode::OK);
        });
    }
}
