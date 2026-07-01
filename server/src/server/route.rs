

// paths
pub fn query(p: &str) -> String {
    "/query/".to_owned() + p
}

pub fn create(p: &str) -> String {
    "/create/".to_owned() + p
}

pub fn submit(p: &str) -> String {
    "/submit/".to_owned() + p
}

pub fn operate(p: &str) -> String {
    "/operate/".to_owned() + p
}

pub fn util(p: &str) -> String {
    "/util/".to_owned() + p
}


/*
    routers
*/
pub fn router(
    hnoder: Arc<dyn HNoder>,
    mut rts: Vec<Router<ApiCtx>>,
    services: Vec<Arc<dyn ApiService>>,
    debug_open: bool,
) -> Router {

    let ctx = ApiCtx::new(hnoder.engine(), hnoder.clone());
    let nrt = crate::api::routes();
    let mut rtr = Router::new()
        .route("/_server_", get("Hacash Api Server"))
        .merge(nrt);
    rtr = merge_registered_services(rtr, services, debug_open);
    while let Some(r) = rts.pop() {
        rtr = rtr.merge(r);
    }
    rtr.with_state(ctx)
}
    
