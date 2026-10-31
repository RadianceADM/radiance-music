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
    let app = match rms::open(cfg) {
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
