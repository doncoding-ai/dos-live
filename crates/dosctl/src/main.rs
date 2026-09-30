//! dosctl — the one command Hermes (or any script) needs to reach Dos Live.
//!
//!   dosctl say "Goat prices are up 8% in Machakos" --world farm --speak
//!   dosctl ask "Draft reply to the landlord is ready. Send it?" --timeout 300
//!   dosctl status
//!   dosctl ping
//!
//! `ask` blocks until you answer on the island (or by voice) and prints
//! do_it / hold / skip — exit code 0, 10, 11 respectively, 2 on timeout.
//! Anything filed under "work" is refused: Sportserve never rides this pipe.

use dos_core::relay::{Level, Request, Response, DEFAULT_ASK_SECS, MAX_ASK_SECS};

const USAGE: &str = "usage:
  dosctl say <text> [--title T] [--world ID] [--alert] [--speak] [--source NAME]
  dosctl ask <question> [--detail D] [--world ID] [--timeout SECS] [--source NAME]
  dosctl status
  dosctl ping

worlds: finance land farm creative donstech boom   (work is refused)";

fn parse(args: &[String]) -> Result<Request, String> {
    let mut it = args.iter();
    let op = it.next().ok_or(USAGE)?;
    let mut text: Vec<String> = Vec::new();
    let (mut title, mut world, mut detail, mut source) = (None, None, None, None);
    let (mut alert, mut speak) = (false, false);
    let mut timeout = None;
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--title" => title = Some(value()?),
            "--world" => world = Some(value()?),
            "--detail" => detail = Some(value()?),
            "--source" => source = Some(value()?),
            "--timeout" => timeout = Some(value()?.parse::<u64>().map_err(|_| "--timeout takes seconds")?),
            "--alert" => alert = true,
            "--speak" => speak = true,
            "-h" | "--help" => return Err(USAGE.into()),
            other if other.starts_with("--") => return Err(format!("unknown option {other}\n{USAGE}")),
            other => text.push(other.to_string()),
        }
    }
    let text = text.join(" ");
    let source = source.or_else(|| Some("hermes".into()));
    match op.as_str() {
        "say" => Ok(Request::Say { text, title, world, level: if alert { Level::Alert } else { Level::Info }, speak, source }),
        "ask" => Ok(Request::Ask {
            question: text,
            detail,
            world,
            timeout_secs: Some(timeout.unwrap_or(DEFAULT_ASK_SECS).min(MAX_ASK_SECS)),
            source,
        }),
        "status" => Ok(Request::Status),
        "ping" => Ok(Request::Ping),
        _ => Err(USAGE.into()),
    }
}

#[cfg(windows)]
fn send(line: &str, wait: std::time::Duration) -> Result<String, String> {
    let _ = wait;
    dos_win::pipe::request(line, std::time::Duration::from_secs(3)).map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn send(_line: &str, _wait: std::time::Duration) -> Result<String, String> {
    Err("dosctl talks to Dos Live over a Windows named pipe; it only runs on Windows".into())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let req = match parse(&args) {
        Ok(r) => r,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(64);
        }
    };
    if let Err(why) = dos_core::relay::check(&req) {
        eprintln!("refused: {why}");
        std::process::exit(65);
    }
    let wait = match &req {
        Request::Ask { timeout_secs, .. } => std::time::Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_ASK_SECS) + 5),
        _ => std::time::Duration::from_secs(5),
    };
    let line = serde_json::to_string(&req).expect("request serializes");
    let reply = match send(&line, wait) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(69);
        }
    };
    let resp: Response = match serde_json::from_str(&reply) {
        Ok(r) => r,
        Err(_) => {
            eprintln!("unexpected reply: {reply}");
            std::process::exit(70);
        }
    };
    if !resp.ok {
        eprintln!("{}", resp.error.unwrap_or_else(|| "refused".into()));
        std::process::exit(if matches!(req, Request::Ask { .. }) { 2 } else { 1 });
    }
    match req {
        Request::Ask { .. } => {
            let d = resp.decision.unwrap_or_default();
            println!("{d}");
            std::process::exit(match d.as_str() {
                "do_it" => 0,
                "hold" => 10,
                "skip" => 11,
                _ => 2,
            });
        }
        Request::Status => println!("{}", serde_json::to_string_pretty(&resp.status.unwrap_or_default()).unwrap_or_default()),
        _ => println!("ok"),
    }
}
