#[macro_use]
extern crate log;

use std::io;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use comrak::ComrakOptions;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
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
}

type CfgPtr = Arc<Cfg>;

const DEFAULT_CSS: &[u8] = include_bytes!("../resource/github-markdown.css");

fn not_found() -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::builder();
    response.status(StatusCode::NOT_FOUND);
    Ok(response
        .body(Body::from(""))
        .expect("invalid response builder"))
}

// TODO: setup notify hook on file
// - calls of inotify (debounced is fine; take Write Events; reparse and re-render)

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
            </body>
        </html>"#,
        title = title,
        content = content,
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

async fn router(cfg: CfgPtr, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match req.uri().path() {
        "/" => md_file(cfg).await,
        "/style.css" => css().await,
        _ => not_found(),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::Builder::from_default_env().init();
    let cfg = Arc::new(Cfg::from_args());
    let file = &cfg.md_file;

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

    let service = make_service_fn(|_| {
        let cfg = Arc::clone(&cfg);
        async { Ok::<_, hyper::Error>(service_fn(move |req| router(Arc::clone(&cfg), req))) }
    });

    let addr = std::net::SocketAddr::new(cfg.host, cfg.port);
    let server = Server::bind(&addr).serve(service);
    println!("Server running at http://{}", addr);
    println!("Press Ctrl-C to exit");
    server.await?;
    Ok(())
}
