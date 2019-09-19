#[macro_use]
extern crate log;
extern crate env_logger;

// md parser + formatter
extern crate comrak;
// simple http server
extern crate simple_server;

// cmdline parsing
extern crate structopt;

use comrak::ComrakOptions;
use simple_server::Server;
use std::fs::File;
use std::io::Read;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
/// grup - a offline github markdown previewer
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

const DEFAULT_CSS: &[u8] = include_bytes!("../resource/github-markdown.css");

fn main() {
    env_logger::Builder::from_default_env().init();
    let cfg = Cfg::from_args();
    let file = cfg.md_file;

    // these were parsed and checked by structopt but now we need to turn them back to strings
    let (host, port) = (format!("{}", cfg.host), format!("{}", cfg.port));

    if !file.exists() {
        eprintln!("Error: {:#?} does not exist!", file);
        process::exit(-1);
    }

    if !file.is_file() {
        eprintln!("Error: {:#?} is not a file!", file);
        process::exit(-1);
    }

    // TODO: setup notify hook on file
    // - calls of inotify (debounced is fine; take Write Events; reparse and re-render)

    let server = Server::new(move |request, mut response| {
        info!("Request received. {} {}", request.method(), request.uri());

        // if they want the stylesheet serve it
        // else give them the formatted MD file
        if request.uri().path() == "/style.css" {
            return Ok(response.body(DEFAULT_CSS.to_vec())?);
        }

        let parsed_and_formatted = File::open(&file)
            .and_then(|mut f| {
                let mut s = String::new();
                f.read_to_string(&mut s).map(|_| s)
            })
            .and_then(|md| {
                let mut options = ComrakOptions::default();
                options.hardbreaks = true;
                Ok(comrak::markdown_to_html(&md, &options))
            })
            .unwrap_or_else(|e| format!("Grup encountered an error: <br> {:#?}", e));

        let title = String::from(file.to_str().unwrap_or(&format!("{:?}", file)));

        // push it all into a container
        let doc = format!(
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
                    <title>{}</title>
                </head>
                <body>
                <article class="markdown-body">
                {}
                <article class="markdown-body">
                </body>
            </html>"#,
            title, parsed_and_formatted
        );

        Ok(response.body(doc.into_bytes())?)
    });

    println!("Server running at http://{}:{}", host, port);
    println!("Press Ctrl-C to exit");
    server.listen(&host, &port);
}
