#[macro_use]
extern crate log;
extern crate comrak;
extern crate env_logger;
extern crate simple_server;
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
struct Cfg {
    #[structopt(name = "markdown_file")]
    /// The markdown file to be served
    md_file: PathBuf,
    #[structopt(long = "port", default_value = "8000", help = "the port to use")]
    port: u16,
    #[structopt(
        long = "host",
        default_value = "127.0.0.1",
        help = "the ip to serve under"
    )]
    host: IpAddr,
}

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
    // OPT: use ws to force page refresh on update maybe?

    let server = Server::new(move |request, mut response| {
        info!("Request received. {} {}", request.method(), request.uri());
        let parsed_and_formatted = File::open(&file)
            .and_then(|mut f| {
                let mut s = String::new();
                f.read_to_string(&mut s).map(|_| s)
            })
            .and_then(|md| {
                let mut options = ComrakOptions::default();
                options.hardbreaks = true;
                Ok(comrak::markdown_to_html(&md, &options))
            });
        let doc = match parsed_and_formatted {
            Ok(s) => s,
            Err(e) => format!(
                "<html>\n
                        <body>\nGrup encountered an error: <br> {:#?}</body>
                    </html>",
                e
            ),
        };

        Ok(response.body(doc.into_bytes())?)
    });

    println!("Server running at http://{}:{}", host, port);
    println!("Press Ctrl-C to exit");
    server.listen(&host, &port);
}
