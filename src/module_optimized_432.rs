use actix_web::{web, App, HttpResponse, HttpServer, Responder, middleware};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: String,
    username: String,
    email: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateUserRequest {
    username: String,
    email: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateUserRequest {
    username: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
    meta: Option<Meta>,
}

#[derive(Debug, Serialize)]
struct Meta {
    page: usize,
    per_page: usize,
    total: usize,
    total_pages: usize,
}

struct AppState {
    users: Mutex<HashMap<String, User>>,
    metrics: Mutex<Metrics>,
}

#[derive(Debug, Default)]
struct Metrics {
    total_requests: u64,
    total_errors: u64,
    active_connections: u64,
}

impl AppState {
    fn new() -> Self {
        Self {
            users: Mutex::new(HashMap::new()),
            metrics: Mutex::new(Metrics::default()),
        }
    }
}

// Handler functions

async fn health_check(data: web::Data<AppState>) -> impl Responder {
    let metrics = data.metrics.lock().unwrap();
    
    HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(serde_json::json!({
            "status": "healthy",
            "timestamp": Utc::now(),
            "metrics": {
                "total_requests": metrics.total_requests,
                "total_errors": metrics.total_errors,
                "active_connections": metrics.active_connections,
            }
        })),
        error: None,
        meta: None,
    })
}

async fn get_users(data: web::Data<AppState>) -> impl Responder {
    let users = data.users.lock().unwrap();
    let user_list: Vec<User> = users.values().cloned().collect();
    let total = user_list.len();

    HttpResponse::Ok().json(ApiResponse {
        success: true,
        data: Some(user_list),
        error: None,
        meta: Some(Meta {
            page: 1,
            per_page: total,
            total,
            total_pages: 1,
        }),
    })
}

async fn get_user(
    data: web::Data<AppState>,
    user_id: web::Path<String>,
) -> impl Responder {
    let users = data.users.lock().unwrap();
    
    match users.get(user_id.as_str()) {
        Some(user) => HttpResponse::Ok().json(ApiResponse {
            success: true,
            data: Some(user.clone()),
            error: None,
            meta: None,
        }),
        None => HttpResponse::NotFound().json(ApiResponse::<User> {
            success: false,
            data: None,
            error: Some("User not found".to_string()),
            meta: None,
        }),
    }
}

async fn create_user(
    data: web::Data<AppState>,
    req: web::Json<CreateUserRequest>,
) -> impl Responder {
    if req.username.is_empty() || req.email.is_empty() {
        return HttpResponse::BadRequest().json(ApiResponse::<User> {
            success: false,
            data: None,
            error: Some("Username and email are required".to_string()),
            meta: None,
        });
    }

    let user = User {
        id: Uuid::new_v4().to_string(),
        username: req.username.clone(),
        email: req.email.clone(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let mut users = data.users.lock().unwrap();
    users.insert(user.id.clone(), user.clone());

    HttpResponse::Created().json(ApiResponse {
        success: true,
        data: Some(user),
        error: None,
        meta: None,
    })
}

async fn update_user(
    data: web::Data<AppState>,
    user_id: web::Path<String>,
    req: web::Json<UpdateUserRequest>,
) -> impl Responder {
    let mut users = data.users.lock().unwrap();
    
    match users.get_mut(user_id.as_str()) {
        Some(user) => {
            if let Some(username) = &req.username {
                user.username = username.clone();
            }
            if let Some(email) = &req.email {
                user.email = email.clone();
            }
            user.updated_at = Utc::now();

            HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(user.clone()),
                error: None,
                meta: None,
            })
        }
        None => HttpResponse::NotFound().json(ApiResponse::<User> {
            success: false,
            data: None,
            error: Some("User not found".to_string()),
            meta: None,
        }),
    }
}

async fn delete_user(
    data: web::Data<AppState>,
    user_id: web::Path<String>,
) -> impl Responder {
    let mut users = data.users.lock().unwrap();
    
    match users.remove(user_id.as_str()) {
        Some(_) => HttpResponse::NoContent().finish(),
        None => HttpResponse::NotFound().json(ApiResponse::<User> {
            success: false,
            data: None,
            error: Some("User not found".to_string()),
            meta: None,
        }),
    }
}

// Middleware for metrics
async fn metrics_middleware(
    data: web::Data<AppState>,
    req: actix_web::HttpRequest,
    srv: actix_web::dev::Service,
) -> Result<actix_web::dev::ServiceResponse, actix_web::Error> {
    let mut metrics = data.metrics.lock().unwrap();
    metrics.total_requests += 1;
    metrics.active_connections += 1;
    drop(metrics);

    let res = srv.call(req).await?;

    let mut metrics = data.metrics.lock().unwrap();
    metrics.active_connections -= 1;
    
    if res.status().is_server_error() || res.status().is_client_error() {
        metrics.total_errors += 1;
    }

    Ok(res)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let app_state = web::Data::new(AppState::new());

    println!("🚀 Server starting on http://127.0.0.1:8080");

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .wrap(middleware::Logger::default())
            .wrap(middleware::Compress::default())
            .route("/health", web::get().to(health_check))
            .service(
                web::scope("/api/v1")
                    .route("/users", web::get().to(get_users))
                    .route("/users", web::post().to(create_user))
                    .route("/users/{id}", web::get().to(get_user))
                    .route("/users/{id}", web::put().to(update_user))
                    .route("/users/{id}", web::delete().to(delete_user))
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}