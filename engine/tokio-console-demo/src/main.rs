use axum::{extract::Query, response::Html, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    name: String,
    age: u32,
}

// Shared state that will cause a deadlock
#[derive(Clone)]
struct AppState {
    counter: Arc<Mutex<u64>>,
    users: Arc<Mutex<Vec<User>>>,
}

#[tokio::main]
async fn main() {
    // Initialize console subscriber for tokio-console
    console_subscriber::init();

    info!("Starting tokio-console demo server");

    // Initialize shared state
    let state = AppState {
        counter: Arc::new(Mutex::new(0)),
        users: Arc::new(Mutex::new(Vec::new())),
    };

    // Build the application with routes
    let app = Router::new()
        .route("/", get(hello_world))
        .route("/counter", get(get_counter))
        .route("/counter/increment", axum::routing::post(increment_counter))
        .route("/users", get(get_users))
        .route("/users/add", axum::routing::post(add_user))
        .route("/deadlock", get(trigger_deadlock))
        .route("/slow", get(slow_endpoint))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    info!("Server running on http://127.0.0.1:3000");
    info!("Try these endpoints:");
    info!("  GET  /           - Hello world");
    info!("  GET  /counter    - Get current counter value");
    info!("  POST /counter    - Increment counter");
    info!("  GET  /users      - Get all users");
    info!("  POST /users      - Add a new user (JSON: {{\"name\":\"Alice\",\"age\":30}})");
    info!("  GET  /slow       - Slow endpoint (simulates work)");
    info!("  GET  /deadlock   - Trigger intentional deadlock for tokio-console demo");

    axum::serve(listener, app).await.unwrap();
}

async fn hello_world() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1><p>This is a tokio-console demo server with Axum.</p>")
}

async fn get_counter(axum::extract::State(state): axum::extract::State<AppState>) -> String {
    let counter = state.counter.lock().unwrap();
    format!("Counter: {}", *counter)
}

async fn increment_counter(axum::extract::State(state): axum::extract::State<AppState>) -> String {
    let mut counter = state.counter.lock().unwrap();
    *counter += 1;
    format!("Counter incremented to: {}", *counter)
}

async fn get_users(axum::extract::State(state): axum::extract::State<AppState>) -> Json<Vec<User>> {
    let users = state.users.lock().unwrap();
    Json(users.clone())
}

async fn add_user(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(user): Json<User>,
) -> String {
    let mut users = state.users.lock().unwrap();
    users.push(user.clone());
    format!("Added user: {} (age {})", user.name, user.age)
}

async fn slow_endpoint(Query(params): Query<HashMap<String, String>>) -> String {
    let delay_ms: u64 = params
        .get("delay")
        .and_then(|d| d.parse().ok())
        .unwrap_or(2000);

    info!("Starting slow work for {}ms", delay_ms);

    // Simulate some CPU-intensive work
    tokio::spawn(async move {
        for i in 0..10 {
            sleep(Duration::from_millis(delay_ms / 10)).await;
            info!("Slow work progress: {}/10", i + 1);
        }
    })
    .await
    .unwrap();

    format!("Slow work completed after {}ms", delay_ms)
}

async fn trigger_deadlock(axum::extract::State(state): axum::extract::State<AppState>) -> String {
    warn!("⚠️  Triggering intentional deadlock for tokio-console demo!");

    // Create two tasks that will deadlock by acquiring locks in different orders
    let state1 = state.clone();
    let state2 = state.clone();

    let task1 = tokio::spawn(async move {
        info!("Task 1: Acquiring counter lock...");
        let _counter_lock = state1.counter.lock().unwrap();

        info!("Task 1: Sleeping while holding counter lock...");
        // Use std::thread::sleep instead of tokio::time::sleep to avoid Send issues
        std::thread::sleep(Duration::from_millis(100));

        info!("Task 1: Now trying to acquire users lock...");
        let _users_lock = state1.users.lock().unwrap();

        info!("Task 1: Got both locks!");
    });

    let task2 = tokio::spawn(async move {
        info!("Task 2: Acquiring users lock...");
        let _users_lock = state2.users.lock().unwrap();

        info!("Task 2: Sleeping while holding users lock...");
        // Use std::thread::sleep instead of tokio::time::sleep to avoid Send issues
        std::thread::sleep(Duration::from_millis(100));

        info!("Task 2: Now trying to acquire counter lock...");
        let _counter_lock = state2.counter.lock().unwrap();

        info!("Task 2: Got both locks!");
    });

    // This will hang indefinitely due to the deadlock
    // In tokio-console, you'll be able to see these tasks blocked
    let _ = tokio::join!(task1, task2);

    "This message will never be reached due to deadlock!".to_string()
}
