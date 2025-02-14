use anyhow::{Context, Result};
use axum::{routing::get, Router};
use std::rc::Rc;

mod btrace;
use btrace::WithTraceContext;
mod web_server;

fn main() {
    let msg = Rc::new("my name is cabbage".to_string());

    println!("starting server");
    // let localset = tokio::task::LocalSet::new();
    // localset.spawn_local(async move {
    //     if let Err(e) = run().await {
    //         tracing::error!("server error: {}", e);
    //     }
    // });

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let tctx = btrace::TraceContext {
        scope: btrace::InstrumentationScope::Root,
        tx,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.spawn(async move {
        println!("starting span receiver");
        while let Some(span) = rx.recv().await {
            // println!("received span: {:?}", span);
        }
    });
    rt.block_on(btrace::BAML_TRACE_CTX.scope(tctx, async {
        println!("starting parallel tasks");
        parallel_tasks().await;
        tokio::time::sleep(std::time::Duration::from_millis(3000)).await;
    }));
    println!("shutting down server");
}

// New async functions to create interesting tracing patterns
async fn parallel_tasks() -> Result<()> {
    let task1 = task_a().baml_traced("task_a");
    let task2 = task_b().baml_traced("task_b");
    let task3 = task_c().baml_traced("task_c");

    tokio::join!(task1, task2, task3);
    Ok(())
}

async fn task_a() {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let _ = nested_task_a().baml_traced("nested_task_a").await;
}

async fn nested_task_a() -> Result<()> {
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    task_d().baml_traced("task_d").await
}

async fn task_b() {
    tokio::time::sleep(std::time::Duration::from_millis(75)).await;
    let _ = nested_task_b().baml_traced("nested_task_b").await;
}

async fn nested_task_b() -> Result<()> {
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    Ok(())
}

async fn task_c() {
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
}

async fn task_d() -> Result<()> {
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    btrace::baml_trace_sync_scope("inside-task-d", || {
        println!("inside task d");
        Ok::<(), anyhow::Error>(())
    });
    Ok(())
}
