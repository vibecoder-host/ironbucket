use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tower::Layer;

#[derive(Clone)]
struct RateLimitInfo {
    count: u32,
    window_start: Instant,
}

#[derive(Clone)]
pub struct RateLimitLayer {
    limits: Arc<RwLock<HashMap<IpAddr, RateLimitInfo>>>,
    max_requests: u32,
    window_duration: Duration,
}

impl RateLimitLayer {
    pub fn new() -> Self {
        Self {
            limits: Arc::new(RwLock::new(HashMap::new())),
            max_requests: 100, // 100 requests
            window_duration: Duration::from_secs(60), // per minute
        }
    }

    pub fn with_limits(max_requests: u32, window_seconds: u64) -> Self {
        Self {
            limits: Arc::new(RwLock::new(HashMap::new())),
            max_requests,
            window_duration: Duration::from_secs(window_seconds),
        }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitMiddleware {
            inner,
            limits: self.limits.clone(),
            max_requests: self.max_requests,
            window_duration: self.window_duration,
        }
    }
}

#[derive(Clone)]
pub struct RateLimitMiddleware<S> {
    inner: S,
    limits: Arc<RwLock<HashMap<IpAddr, RateLimitInfo>>>,
    max_requests: u32,
    window_duration: Duration,
}

impl<S> RateLimitMiddleware<S> {
    async fn check_rate_limit(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut limits = self.limits.write().await;

        let info = limits.entry(ip).or_insert_with(|| RateLimitInfo {
            count: 0,
            window_start: now,
        });

        // Reset window if expired
        if now.duration_since(info.window_start) > self.window_duration {
            info.count = 0;
            info.window_start = now;
        }

        // Check if limit exceeded
        if info.count >= self.max_requests {
            return false;
        }

        info.count += 1;
        true
    }
}

impl<S> tower::Service<Request> for RateLimitMiddleware<S>
where
    S: tower::Service<Request, Response = Response> + Clone + Send + Sync + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let middleware = self.clone();

        Box::pin(async move {
            // Extract client IP (simplified - in production, handle X-Forwarded-For)
            let ip = req
                .extensions()
                .get::<std::net::SocketAddr>()
                .map(|addr| addr.ip())
                .unwrap_or_else(|| IpAddr::from([127, 0, 0, 1]));

            // Check rate limit
            if !middleware.check_rate_limit(ip).await {
                return Ok(Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .header("Retry-After", "60")
                    .body("Too many requests".into())
                    .unwrap());
            }

            inner.call(req).await
        })
    }
}