use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiMethod {
    Get,
    Post,
}

#[derive(Clone, Debug, Default)]
pub struct ApiRequest {
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl ApiRequest {
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query.get(key).map(|s| s.as_str())
    }

    pub fn query_u64(&self, key: &str, dv: u64) -> u64 {
        self.query(key)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(dv)
    }

    pub fn query_usize(&self, key: &str, dv: usize) -> usize {
        self.query(key)
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(dv)
    }
}

#[derive(Clone, Debug)]
pub struct ApiResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl ApiResponse {
    pub fn json(body: String) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
            body: body.into_bytes(),
        }
    }
}

#[derive(Clone)]
pub struct ApiExecCtx {
    pub engine: Arc<dyn Engine>,
    pub hnoder: Arc<dyn HNoder>,
    pub launch_time: u64,
    pub miner_worker_notice_count: Arc<Mutex<u64>>,
}

pub type ApiHandlerFn = fn(&ApiExecCtx, ApiRequest) -> ApiResponse;
pub type ApiHandlerAsyncFn = fn(
    ApiExecCtx,
    ApiRequest,
) -> Pin<Box<dyn Future<Output = ApiResponse> + Send + 'static>>;

#[derive(Clone)]
pub enum ApiHandler {
    Sync(ApiHandlerFn),
    Async(ApiHandlerAsyncFn),
}

#[derive(Clone)]
pub struct ApiRoute {
    pub method: ApiMethod,
    pub path: String,
    pub handler: ApiHandler,
    pub debug: bool,
}

impl ApiRoute {
    pub fn get(path: &str, handler: ApiHandlerFn) -> Self {
        Self {
            method: ApiMethod::Get,
            path: path.to_owned(),
            handler: ApiHandler::Sync(handler),
            debug: false,
        }
    }

    pub fn post(path: &str, handler: ApiHandlerFn) -> Self {
        Self {
            method: ApiMethod::Post,
            path: path.to_owned(),
            handler: ApiHandler::Sync(handler),
            debug: false,
        }
    }

    pub fn get_async(path: &str, handler: ApiHandlerAsyncFn) -> Self {
        Self {
            method: ApiMethod::Get,
            path: path.to_owned(),
            handler: ApiHandler::Async(handler),
            debug: false,
        }
    }

    pub fn debug_get(path: &str, handler: ApiHandlerFn) -> Self {
        Self {
            method: ApiMethod::Get,
            path: debug_path(path),
            handler: ApiHandler::Sync(handler),
            debug: true,
        }
    }

    pub fn debug_post(path: &str, handler: ApiHandlerFn) -> Self {
        Self {
            method: ApiMethod::Post,
            path: debug_path(path),
            handler: ApiHandler::Sync(handler),
            debug: true,
        }
    }
}

fn debug_path(path: &str) -> String {
    let suffix = path.trim_start_matches('/');
    format!("/debug/{}", suffix)
}

pub trait ApiService: Send + Sync {
    fn name(&self) -> &'static str {
        "api-service"
    }

    fn routes(&self) -> Vec<ApiRoute>;
}
