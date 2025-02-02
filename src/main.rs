use axum_server::Handle;
use clap::Parser;
use db::RocksDB;
use index::{validate_rpc_connection, Index, RpcClientProvider, Settings};
use options::Options;
use server::{Server, ServerConfig};
use std::{panic, sync::Arc};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info};

mod api;
mod db;
mod index;
mod models;
mod options;
mod server;
mod util;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up the global tracing subscriber for logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // 1. Parse command-line options
    let options = parse_options()?;

    // 2. Prepare and validate configurations
    let settings = setup_settings(&options)?;
    let server_config = setup_server_config(&options)?;
    validate_rpc(&settings)?;

    // 3. Open RocksDB
    let db_arc = open_rocks_db(&settings)?;
    set_panic_hook(db_arc.clone());

    // 4. Create the index and set a panic hook that closes the DB
    let index = Arc::new(Index::new(db_arc.clone(), settings.clone()));
    index.validate_index()?;

    // 5. Spawn background threads (indexer, ZMQ listener, etc.)
    let index_handle = spawn_background_threads(index.clone());

    // 6. Start the HTTP server
    let handle = Handle::new();
    let server = Server;
    let http_server_jh = server.start(
        index.clone(),
        Arc::new(server_config),
        handle.clone(),
    )?;

    // 7. Wait for SIGINT (Ctrl-C) or SIGTERM
    wait_for_signals().await;

    // 10. Graceful shutdown (async)
    graceful_shutdown(
        index,
        subscription_manager,
        db_arc,
        &handle,
        index_handle,
        http_server_jh,
    );

    Ok(())
}

/// Parse CLI options
fn parse_options() -> Result<Options, Box<dyn std::error::Error>> {
    let options = Options::parse();
    Ok(options)
}

/// Convert `Options` to your local `Settings` struct, etc.
fn setup_settings(options: &Options) -> Result<Settings, Box<dyn std::error::Error>> {
    let settings = Settings::from(options.clone());
    Ok(settings)
}

fn setup_server_config(options: &Options) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let config = ServerConfig::from(options.clone());
    Ok(config)
}

/// Validate the RPC connection using your `validate_rpc_connection`
fn validate_rpc(settings: &Settings) -> Result<(), Box<dyn std::error::Error>> {
    validate_rpc_connection(settings.get_new_rpc_client()?, settings.chain)?;
    Ok(())
}

/// Open RocksDB, returning an `Arc<RocksDB>`
fn open_rocks_db(settings: &Settings) -> Result<Arc<RocksDB>, Box<dyn std::error::Error>> {
    let file = settings.chain.to_string();
    let db_path = settings.data_dir.join(file);
    let db_instance = RocksDB::open(db_path.to_str().unwrap())?;
    let db_arc = Arc::new(db_instance);
    Ok(db_arc)
}

/// Set a panic hook that closes or flushes the DB on panic
fn set_panic_hook(db_arc: Arc<RocksDB>) {
    let db_for_panic = db_arc.clone();
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Default hook prints the backtrace
        original_hook(panic_info);
        error!("Panic occurred: {:?}, shutting down...", panic_info);

        // Doing best effort to flush the db and close it
        match db_for_panic.flush() {
            Ok(_) => (),
            Err(e) => error!("Failed to flush RocksDB in panic hook: {:?}", e),
        }

        // Exit process.
        std::process::exit(1);
    }));
}

/// Spawn background threads: indexer loop, ZMQ listener, etc. Return their JoinHandles.
fn spawn_background_threads(
    index: Arc<Index>,
) -> std::thread::JoinHandle<()> {
    // 1) Spawn the indexer loop
    let index_clone = index.clone();
    let index_handle = std::thread::spawn(move || {
        index_clone.index();
    });

    // 2) Spawn the ZMQ listener
    index.start_zmq_listener();

    info!("Spawned background threads");

    index_handle
}

/// Block until either SIGINT or SIGTERM is received
async fn wait_for_signals() {
    use tokio::select;
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to open signal stream");

    select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT (Ctrl-C), shutting down...");
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM, shutting down...");
        }
    }
}

/// Perform graceful shutdown, then close RocksDB if possible
fn graceful_shutdown(
    index: Arc<Index>,
    db_arc: Arc<RocksDB>,
    handle: &Handle,
    index_handle: std::thread::JoinHandle<()>,
    http_server_jh: task::JoinHandle<io::Result<()>>,
) {
    // 1) Signal the index and other threads to stop
    index.shutdown();

    // 2) Graceful HTTP shutdown
    handle.graceful_shutdown(Some(std::time::Duration::from_secs(2)));

    // 3) Reset the panic hook to drop the RocksDB reference
    panic::set_hook(Box::new(|panic_info| {
        // Restore the default hook
        eprintln!("Panic occurred: {:?}", panic_info);
    }));

    // 4) Join background threads
    if let Err(e) = index_handle.join() {
        error!("Failed to join indexer thread: {:?}", e);
    }

    // 5) Await the Axum server
    match http_server_jh.await {
        Ok(Ok(_)) => info!("Axum server finished cleanly."),
        Ok(Err(e)) => error!("Server error: {:?}", e),
        Err(e) => error!("Failed to join Axum server task: {:?}", e),
    };

    // Dropping the index, makes dropping the remaining references to the db
    drop(index);

    // 5) Once threads are done, attempt to close RocksDB
    match Arc::try_unwrap(db_arc) {
        Ok(db) => {
            if let Err(e) = db.close() {
                error!("Error while closing RocksDB: {:?}", e);
            } else {
                info!("RocksDB closed successfully (normal shutdown).");
            }
        }
        Err(db) => {
            error!(
                "Still references {} to RocksDB exist, flushing before exit...",
                Arc::strong_count(&db)
            );
            // fallback flush:
            match db.flush() {
                Ok(_) => (),
                Err(e) => error!("Failed to flush RocksDB in graceful shutdown: {:?}", e),
            }
        }
    }

    info!("All threads have shut down. Exiting.");
}
