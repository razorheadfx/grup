#[macro_use]
extern crate log;

use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use comrak::ComrakOptions;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use structopt::StructOpt;

use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot::{self, Sender};

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
    let response = Response::builder().status(StatusCode::NOT_FOUND);
    Ok(response
        .body(Body::from(""))
        .expect("invalid response builder"))
}

async fn update(updaters: SenderListPtr) -> Result<Response<Body>, hyper::Error> {
    let response = Response::builder()
        .header("Cache-Control", "no-cache, no-store, must-revalidate")
        .header("Pragma", "no-cache")
        .header("Expires", "0");

    let (tx, rx) = oneshot::channel();
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
    let response = Response::builder().header("Content-type", "text/html");

    let content = if let Ok(mut file) = File::open(&cfg.md_file).await {
        let mut buf = String::new();
        if file.read_to_string(&mut buf).await.is_ok() {
            let mut options = ComrakOptions::default();
            options.render.hardbreaks = true;
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

// serve the standard CSS
async fn css() -> Result<Response<Body>, hyper::Error> {
    let response = Response::builder().header("Content-type", "text/css");
    Ok(response
        .body(Body::from(DEFAULT_CSS))
        .expect("invalid response builder"))
}

// Will only serve files relative to the md file
async fn static_file(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let response = Response::builder();
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

// route the different URIs
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

// watch the provided file for changes
fn spawn_watcher(cfg: CfgPtr, updaters: SenderListPtr) -> notify::Result<RecommendedWatcher> {
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
        .unwrap_or_else(|| PathBuf::from("/"));

    // this uses os specific file watching where possible (i.e. inotify on linux)
    // it forks of a mio event loop in the background and then calls the provided closure
    // with the yielded events
    let md_file_name = cfg.md_file.file_name().expect("path was `..`").to_owned();
    let mut file_event_watcher: RecommendedWatcher =
        notify::recommended_watcher(move |event: notify::Result<Event>| {
            let event = match event {
                Ok(ev) => ev,
                Err(e) => {
                    error!("received file notifier error {:?}. Ignoring file event.", e);
                    return;
                }
            };

            match event.kind {
                EventKind::Create(_) => debug!("files created {:?}", &event.paths),
                EventKind::Modify(_) => debug!("files modified {:?}", &event.paths),
                _ => return,
            };

            if event.paths.iter().any(|path| path.eq(&md_file_name)) {
                info!("md file updated {:?}", cfg.md_file);
                if let Ok(mut updaters) = updaters.lock() {
                    for tx in updaters.drain(..) {
                        // ignore errors
                        let _ = tx.send(());
                    }
                } else {
                    error!("Internal error: mutex poisoned");
                }
            }
        })?;

    file_event_watcher.watch(&parent, RecursiveMode::NonRecursive)?;
    Ok(file_event_watcher)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::Builder::from_default_env().init();
    let cfg = Arc::new(Cfg::from_args());
    let file = &cfg.md_file;

    debug!("Configuration {:#?}", &cfg);

    if !file.exists() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("No (markdown) file at: {:?}", file),
        )
        .into());
    }

    if !file.is_file() {
        return Err(
            io::Error::new(io::ErrorKind::Other, format!("{:?} is not a file.", file)).into(),
        );
    }

    // move the workdir relative to the MD file
    // that way resources in directories relative to the MD file can be serveed as static files
    // also catch the case where the path has no parent directory
    //(i.e. file is in working directory & given filename is relative)
    // file.parent() will return an empty string in this case (i would expected it to return None in that case)
    if let Some(parent) = file
        .parent()
        .map(|s| s.to_str())
        .flatten()
        .filter(|s| !s.is_empty())
    {
        debug!("File is not in current WD. Changing WD to {}", &parent);
        eprintln!("* Switching working directory to {}", parent);
        std::env::set_current_dir(parent)?;
    }

    let updaters = Arc::new(Mutex::new(Vec::new()));
    // we just hold on to this, so the file watcher is killed when this function exits
    let _watcher = spawn_watcher(cfg.clone(), Arc::clone(&updaters));

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
    eprintln!("* Webserver running at http://{}", addr);
    eprintln!("Press Ctrl-C to stop it & exit");
    server.await?;
    Ok(())
}
