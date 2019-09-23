#[macro_use]
extern crate log;

use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use comrak::ComrakOptions;
use futures_channel::oneshot::{channel, Sender};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use inotify::{EventMask, Inotify, WatchMask};
use structopt::StructOpt;
use tokio::prelude::*;
use tokio_fs::File;

#[derive(Debug, StructOpt)]
/// grup - an offline github markdown previewer
struct Cfg {
    #[structopt(name = "markdown_file", parse(from_os_str))]
    /// The markdown file to be served
    md_file: PathBuf,
    #[structopt(
        long = "port",
        default_value = "8000",
        help = "the port to use for the server"
    )]
    port: u16,
    #[structopt(
        long = "host",
        default_value = "127.0.0.1",
        help = "the ip to use for the server"
    )]
    host: IpAddr,
    #[structopt(
        long = "interval",
        default_value = "60",
        help = "timeout interval for long polling http"
    )]
    interval: u32,
    #[structopt(
        long = "serve-static",
        help = "serve static files relative to markdown file"
    )]
    serve_static: bool,
}

type CfgPtr = Arc<Cfg>;
type SenderListPtr = Arc<Mutex<Vec<Sender<()>>>>;

const DEFAULT_CSS: &[u8] = include_bytes!("../resource/github-markdown.css");

fn not_found() -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::builder();
    response.status(StatusCode::NOT_FOUND);
    Ok(response
        .body(Body::from(""))
        .expect("invalid response builder"))
}

async fn update(updaters: SenderListPtr) -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::builder();
    response.header("Cache-Control", "no-cache, no-store, must-revalidate");
    response.header("Pragma", "no-cache");
    response.header("Expires", "0");

    let (tx, rx) = channel();
    if let Ok(mut updaters) = updaters.lock() {
        updaters.push(tx);
    } else {
        error!("Internal error: mutex poisoned");
    }

    let _ = rx.await;
    Ok(response
        .body(Body::from("yes"))
        .expect("invalid response builder"))
}

async fn md_file(cfg: CfgPtr) -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::builder();
    response.header("Content-type", "text/html");

    let content = if let Ok(mut file) = File::open(&cfg.md_file).await {
        let mut buf = String::new();
        if file.read_to_string(&mut buf).await.is_ok() {
            let mut options = ComrakOptions::default();
            options.hardbreaks = true;
            comrak::markdown_to_html(&buf, &options)
        } else {
            return not_found();
        }
    } else {
        return not_found();
    };
    let title = String::from(
        cfg.md_file
            .to_str()
            .unwrap_or(&format!("{:?}", cfg.md_file)),
    );

    // push it all into a container
    let document = format!(
        r#"<!DOCTYPE html>
         <html>
            <head>
                <meta http-equiv="Content-Type" content="text/html; charset=utf-8"/>
                <style>
                    body {{
                    box-sizing: border-box;
                    min-width: 200px;
                    max-width: 980px;
                    margin: 0 auto;
                    padding: 45px;
                    }}
                </style>
                <link rel="stylesheet" href="style.css">
                <title>{title}</title>
            </head>
            <body>
            <article class="markdown-body">
            {content}
            </article>
            <script type="text/javascript">
            function reload_check () {{
                var xhr = new XMLHttpRequest();
                xhr.overrideMimeType("text/plain");
                xhr.timeout = {interval};
                xhr.onreadystatechange = function () {{
                    if(this.readyState === 4) {{
                        if (this.status === 200) {{
                            if (this.responseText == "yes") {{
                                location.reload();
                            }} else {{
                                reload_check();
                            }}
                        }}
                    }}
                }}
                xhr.ontimeout = function () {{
                    reload_check();
                }}
                xhr.open("GET", "/update", true);
                xhr.send();
            }}
            reload_check();
            </script>
            </body>
        </html>"#,
        title = title,
        content = content,
        interval = cfg.interval * 1000
    );
    Ok(response
        .body(Body::from(document))
        .expect("invalid response builder"))
}

async fn css() -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::builder();
    response.header("Content-type", "text/css");
    Ok(response
        .body(Body::from(DEFAULT_CSS))
        .expect("invalid response builder"))
}

// Will only serve files relative to the md file
async fn static_file(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::builder();
    let cwd = std::env::current_dir().expect("no working dir");
    if req.uri().path().len() > 1 {
        let mut fullpath = cwd.clone();
        // path() contains preceeding forward slash: /some/web/page
        fullpath.push(&req.uri().path()[1..]);
        // canonicalize returns Err if path does not exist.
        if let Ok(fullpath) = fullpath.canonicalize() {
            if fullpath.starts_with(&cwd) {
                if let Ok(mut file) = File::open(&fullpath).await {
                    let mut buf = String::new();
                    if file.read_to_string(&mut buf).await.is_ok() {
                        return Ok(response
                            .body(Body::from(buf))
                            .expect("invalid response builder"));
                    }
                }
            }
        }
    }
    info!("{} not found", req.uri());
    not_found()
}

async fn router(
    cfg: CfgPtr,
    updaters: SenderListPtr,
    req: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
    match req.uri().path() {
        "/update" => update(updaters).await,
        "/" => md_file(cfg).await,
        "/style.css" => css().await,
        _ => {
            if cfg.serve_static {
                static_file(req).await
            } else {
                not_found()
            }
        }
    }
}

fn spawn_watcher(cfg: CfgPtr, updaters: SenderListPtr) {
    let parent = cfg
        .md_file
        .parent()
        .map(|x| {
            if x == Path::new("") {
                PathBuf::from(".")
            } else {
                PathBuf::from(x)
            }
        })
        .unwrap_or(PathBuf::from("/"));
    std::thread::spawn(move || {
        let mut inotify = Inotify::init().expect("inotify init failed");
        inotify
            .add_watch(&parent, WatchMask::MODIFY | WatchMask::CREATE)
            .expect("failed to watch");
        let mut buf = [0u8; 1024];
        let md_file_name = cfg.md_file.file_name().expect("path was `..`");
        loop {
            let events = inotify
                .read_events_blocking(&mut buf)
                .expect("failed to read events");
            for event in events {
                if event.name.is_none() {
                    break;
                }
                let name = event.name.unwrap();
                if event.mask.contains(EventMask::CREATE) {
                    debug!("file created {:?}", name);
                } else if event.mask.contains(EventMask::MODIFY) {
                    debug!("file modified {:?}", name);
                }
                if Path::new(name) == md_file_name {
                    info!("file updated {:?}", name);
                    if let Ok(mut updaters) = updaters.lock() {
                        for tx in updaters.drain(..) {
                            // ignore errors
                            let _ = tx.send(());
                        }
                    } else {
                        error!("Internal error: mutex poisoned");
                    }
                }
            }
        }
    });
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::Builder::from_default_env().init();
    let cfg = Arc::new(Cfg::from_args());
    let file = &cfg.md_file;
    if let Some(parent) = file.parent() {
        std::env::set_current_dir(parent)?;
    } else {
        std::env::set_current_dir(std::path::Component::RootDir.as_os_str())?;
    }

    if !file.exists() {
        return Err(
            io::Error::new(io::ErrorKind::Other, format!("No such file: {:?}", file)).into(),
        );
    }

    if !file.is_file() {
        return Err(
            io::Error::new(io::ErrorKind::Other, format!("No such file: {:?}", file)).into(),
        );
    }

    let updaters = Arc::new(Mutex::new(Vec::new()));
    spawn_watcher(cfg.clone(), Arc::clone(&updaters));

    let service = make_service_fn(|_| {
        let cfg = Arc::clone(&cfg);
        let updaters = Arc::clone(&updaters);
        async {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                router(Arc::clone(&cfg), Arc::clone(&updaters), req)
            }))
        }
    });

    let addr = std::net::SocketAddr::new(cfg.host, cfg.port);
    let server = Server::bind(&addr).serve(service);
    println!("Server running at http://{}", addr);
    println!("Press Ctrl-C to exit");
    server.await?;
    Ok(())
}
