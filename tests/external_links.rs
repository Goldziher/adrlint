//! `MADR11` against a local HTTP server on 127.0.0.1, so the tests never reach the network.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;

use tempfile::TempDir;

/// Every request the server received, as `(method, path)`.
type Requests = Arc<Mutex<Vec<(String, String)>>>;

/// Serves `/ok` (200), `/missing` (404), `/no-head` (405 for HEAD, 200 for GET), `/moved` (301 to
/// `/ok`), `/busy` (429) and 404 for anything else. Each response closes the connection.
fn serve() -> (SocketAddr, Requests) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("binding a local port");
    let address = listener.local_addr().unwrap();
    let requests: Requests = Arc::default();
    let recorded = Arc::clone(&requests);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let recorded = Arc::clone(&recorded);
            thread::spawn(move || respond(stream, &recorded));
        }
    });
    (address, requests)
}

fn respond(mut stream: TcpStream, requests: &Mutex<Vec<(String, String)>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    // Drain the headers so closing the socket does not reset the client mid-request.
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) | Err(_) => break,
            Ok(_) if header == "\r\n" || header == "\n" => break,
            Ok(_) => {}
        }
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    requests.lock().unwrap().push((method.clone(), path.clone()));

    let (status, extra) = match (method.as_str(), path.as_str()) {
        (_, "/ok") => ("200 OK", String::new()),
        ("HEAD", "/no-head") => ("405 Method Not Allowed", String::new()),
        ("GET", "/no-head") => ("200 OK", String::new()),
        (_, "/moved") => ("301 Moved Permanently", "Location: /ok\r\n".to_owned()),
        (_, "/busy") => ("429 Too Many Requests", String::new()),
        _ => ("404 Not Found", String::new()),
    };
    let response = format!("HTTP/1.1 {status}\r\n{extra}Content-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(response.as_bytes());
}

/// A valid ADR whose `More Information` section cites `links`.
fn adr(title: &str, links: &[String]) -> String {
    let valid = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/adr/valid/0001-record-architecture-decisions.md");
    let source = fs::read_to_string(valid).unwrap();
    let (head, _) = source.split_once("## More Information").unwrap();
    let head = head.replacen("# Record architecture decisions", &format!("# {title}"), 1);
    let list = links
        .iter()
        .enumerate()
        .map(|(index, url)| format!("* [link {index}]({url})\n"))
        .collect::<String>();
    format!("{head}## More Information\n\n{list}")
}

/// A project with two ADRs that both cite `/ok` and `/missing`, plus the rest of the server's routes.
fn project(address: SocketAddr, lint_table: &str, rules_table: &str) -> (TempDir, PathBuf) {
    let base = format!("http://{address}");
    let directory = TempDir::new().unwrap();
    let adrs = directory.path().join("docs/adr");
    fs::create_dir_all(&adrs).unwrap();
    fs::write(
        adrs.join("0001-first.md"),
        adr(
            "First",
            &[
                format!("{base}/ok"),
                format!("{base}/missing"),
                format!("{base}/no-head"),
                format!("{base}/busy"),
                "mailto:team@example.com".to_owned(),
            ],
        ),
    )
    .unwrap();
    fs::write(
        adrs.join("0002-second.md"),
        adr(
            "Second",
            &[
                format!("{base}/ok#fragment"),
                format!("{base}/missing"),
                format!("{base}/moved"),
            ],
        ),
    )
    .unwrap();
    let config = directory.path().join(".adrlint.toml");
    fs::write(&config, format!("[adr]\ndir = \"docs/adr\"\n{lint_table}{rules_table}")).unwrap();
    (directory, config)
}

fn lint(config: &Path, flags: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_adrlint"));
    command
        .current_dir(config.parent().unwrap())
        .arg("--config")
        .arg(config)
        .arg("lint")
        .args(["--format", "json"])
        .args(flags);
    // A developer's proxy must not intercept requests to the local server.
    for variable in [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        command.env_remove(variable);
    }
    command.env("NO_PROXY", "127.0.0.1");
    command.output().expect("running adrlint")
}

/// `(code, severity, file name, line, message)` for every finding.
fn findings(output: &Output) -> Vec<(String, String, String, u64, String)> {
    let values: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "parsing JSON diagnostics: {error}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    values
        .iter()
        .map(|value| {
            let path = PathBuf::from(value["path"].as_str().unwrap());
            (
                value["code"].as_str().unwrap().to_owned(),
                value["severity"].as_str().unwrap().to_owned(),
                path.file_name().unwrap().to_string_lossy().into_owned(),
                value["line"].as_u64().unwrap(),
                value["message"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn sorted_requests(requests: &Requests) -> Vec<(String, String)> {
    let mut requests = requests.lock().unwrap().clone();
    requests.sort();
    requests
}

/// The `More Information` list starts on this line of [`adr`]'s output.
fn first_link_line() -> u64 {
    let source = adr("First", &["x".to_owned()]);
    let index = source.lines().position(|line| line.starts_with("* [link 0]")).unwrap();
    u64::try_from(index + 1).unwrap()
}

#[test]
fn should_report_broken_links_once_per_location_and_request_each_url_once() {
    let (address, requests) = serve();
    let (_directory, config) = project(address, "", "");

    let output = lint(&config, &["--check-external-links"]);
    assert!(
        output.status.success(),
        "warnings must not fail the run, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let line = first_link_line();
    let message = format!("external link 'http://{address}/missing' returned HTTP 404");
    assert_eq!(
        findings(&output),
        [
            (
                "MADR11".to_owned(),
                "warning".to_owned(),
                "0001-first.md".to_owned(),
                line + 1,
                message.clone()
            ),
            (
                "MADR11".to_owned(),
                "warning".to_owned(),
                "0002-second.md".to_owned(),
                line + 1,
                message
            ),
        ]
    );

    assert_eq!(
        sorted_requests(&requests),
        [
            ("GET".to_owned(), "/no-head".to_owned()),
            ("HEAD".to_owned(), "/busy".to_owned()),
            ("HEAD".to_owned(), "/missing".to_owned()),
            ("HEAD".to_owned(), "/moved".to_owned()),
            ("HEAD".to_owned(), "/no-head".to_owned()),
            // Once for `/ok` and `/ok#fragment`, once more following the `/moved` redirect.
            ("HEAD".to_owned(), "/ok".to_owned()),
            ("HEAD".to_owned(), "/ok".to_owned()),
        ]
    );
}

#[test]
fn should_make_no_request_without_opting_in() {
    let (address, requests) = serve();
    let (_directory, config) = project(address, "", "");

    let output = lint(&config, &[]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(findings(&output).is_empty());
    assert!(sorted_requests(&requests).is_empty());
}

#[test]
fn should_enable_from_the_configuration_and_let_the_flag_override_it() {
    let (address, requests) = serve();
    let (_directory, config) = project(address, "[lint]\ncheck_external_links = true\n", "");

    let disabled = lint(&config, &["--no-check-external-links"]);
    assert!(findings(&disabled).is_empty());
    assert!(sorted_requests(&requests).is_empty());

    let enabled = lint(&config, &[]);
    let codes = findings(&enabled)
        .into_iter()
        .map(|(code, ..)| code)
        .collect::<Vec<_>>();
    assert_eq!(codes, ["MADR11", "MADR11"]);
}

#[test]
fn should_fail_the_run_when_configured_as_an_error() {
    let (address, _requests) = serve();
    let (_directory, config) = project(address, "", "[rules]\nMADR11 = \"error\"\n");

    let output = lint(&config, &["--check-external-links"]);
    assert_eq!(output.status.code(), Some(1));
    let severities = findings(&output)
        .into_iter()
        .map(|(code, severity, ..)| (code, severity))
        .collect::<Vec<_>>();
    assert_eq!(
        severities,
        [
            ("MADR11".to_owned(), "error".to_owned()),
            ("MADR11".to_owned(), "error".to_owned())
        ]
    );
}

#[test]
fn should_make_no_request_when_turned_off() {
    let (address, requests) = serve();
    let (_directory, config) = project(address, "", "[rules]\nMADR11 = \"off\"\n");

    let output = lint(&config, &["--check-external-links"]);
    assert!(output.status.success());
    assert!(findings(&output).is_empty());
    assert!(sorted_requests(&requests).is_empty(), "an off rule makes no request");
}

#[test]
fn should_report_a_connection_failure() {
    // Bind and drop a listener so the port is very likely closed.
    let address = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let directory = TempDir::new().unwrap();
    let adrs = directory.path().join("docs/adr");
    fs::create_dir_all(&adrs).unwrap();
    fs::write(
        adrs.join("0001-first.md"),
        adr("First", &[format!("http://{address}/gone")]),
    )
    .unwrap();
    let config = directory.path().join(".adrlint.toml");
    fs::write(&config, "[adr]\ndir = \"docs/adr\"\n").unwrap();

    let output = lint(&config, &["--check-external-links"]);
    let found = findings(&output);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "MADR11");
    assert!(
        found[0]
            .4
            .starts_with(&format!("external link 'http://{address}/gone' could not be fetched: ")),
        "{found:?}"
    );
}

#[test]
fn should_underline_the_citing_line_with_a_hint_and_no_fix() {
    let (address, _requests) = serve();
    let (_directory, config) = project(address, "", "");

    let output = lint(&config, &["--check-external-links"]);
    let values: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let first = values
        .iter()
        .find(|value| value["path"].as_str().unwrap().ends_with("0001-first.md"))
        .expect("a MADR11 finding in the first ADR");
    let cited = format!("* [link 1](http://{address}/missing)");
    assert_eq!(first["line"], first_link_line() + 1);
    assert_eq!(first["column"], 1);
    assert_eq!(first["end_line"], first_link_line() + 1);
    assert_eq!(first["end_column"], cited.chars().count() + 1);
    assert_eq!(first["fixable"], false);
    assert!(first["help"].as_str().is_some_and(|help| !help.is_empty()), "{first}");
}
