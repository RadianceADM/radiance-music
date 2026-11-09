//! `rms`: starts Radiance Music Server with what its environment says (see
//! Config in lib.rs, and the README for every setting).

#[tokio::main]
async fn main() {
    let cfg = rms::Config::from_env();
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("Radiance Music Server {}", rms::VERSION);
        return;
    }
    rms::log(&format!("Radiance Music Server {} (data in {}, listening on {})", rms::VERSION, cfg.data.display(), cfg.bind));
    let bind = cfg.bind.clone();
    if std::env::var_os("RMS_PYTHON").is_some() || std::env::var_os("RMS_SCRIPTS").is_some() {
        rms::log("RMS_PYTHON and RMS_SCRIPTS aren't used any more: the server splits and times words by itself (no Python)");
    }
    let engine = std::sync::Arc::new(rms::engine::Onnx::new(cfg.models(), cfg.threads));
    let app = match rms::open_with(cfg, engine) {
        Ok(app) => app,
        Err(e) => {
            rms::log(&format!("couldn't start: {e}"));
            std::process::exit(1);
        }
    };
    let listener = match tokio::net::TcpListener::bind(&bind).await {
        Ok(l) => l,
        Err(e) => {
            rms::log(&format!("can't listen on {bind}: {e}"));
            std::process::exit(1);
        }
    };
    if let Err(e) = rms::serve(app, listener).await {
        rms::log(&format!("stopped: {e}"));
        std::process::exit(1);
    }
}
