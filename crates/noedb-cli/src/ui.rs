//! Branded terminal UI — NoeDB Studio (cyber / infra tool aesthetic).

use std::io::{self, Write};

use comfy_table::presets::{ASCII_FULL, UTF8_FULL};
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};
use noedb_engine::QueryResult;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const ESC: &str = "\x1b";

pub(crate) fn studio_enabled(args: &[String]) -> bool {
    !plain_mode(args)
        && (args.iter().any(|a| a == "--studio")
            || std::env::var("NOEDB_STUDIO").is_ok_and(|v| v != "0" && !v.is_empty()))
}

pub(crate) fn plain_mode(args: &[String]) -> bool {
    args.iter().any(|a| a == "--plain")
}

pub(crate) fn skip_banner() -> bool {
    std::env::var("NOEDB_SKIP_BANNER").is_ok_and(|v| v != "0" && !v.is_empty())
}

pub(crate) fn maybe_clear_screen(studio: bool) {
    if studio || std::env::var("NOEDB_CLEAR").is_ok_and(|v| v != "0" && !v.is_empty()) {
        print!("{ESC}[2J{ESC}[H");
        let _ = io::stdout().flush();
    }
}

pub(crate) fn print_banner(distributed: bool, data_dir: &str) {
    let mode_tag = if distributed { "CLUSTER" } else { "LOCAL" };
    let data = truncate_path(data_dir, 52);
    let ok = format!("{ESC}[1;32m[OK]{ESC}[0m");
    let active = format!("{ESC}[1;32m[Active]{ESC}[0m");
    let dim = format!("{ESC}[2m");
    let reset = format!("{ESC}[0m");
    let cyan = format!("{ESC}[1;36m");
    let gray = format!("{ESC}[90m");

    println!(
        "
{cyan}  ███╗   ██╗  ██████╗   ███████╗  ██████╗   ██████╗ {reset}
{cyan}  ████╗  ██║ ██╔═══██╗ ██╔════╝  ██╔══██╗  ██╔══██╗{reset}
{cyan}  ██╔██╗ ██║ ██║   ██║ █████╗    ██║  ██║  ██████╔╝{reset}
{cyan}  ██║╚██╗██║ ██║   ██║ ██╔══╝    ██║  ██║  ██╔══██╗{reset}
{cyan}  ██║ ╚████║ ╚██████╔╝ ███████╗  ██████╔╝  ██████╔╝{reset}
{cyan}  ╚═╝  ╚═══╝  ╚═════╝  ╚══════╝  ╚═════╝   ╚═════╝ {reset}
{gray}  ▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀{reset}
{gray}  ┌────────────────────────────────────────────────────────────────┐{reset}
{gray}  │{reset} Distributed SQL Engine {gray}│{reset} Rust 🦀 • Raft ⎈ • LSM 🗄️ • Made in CM 🇨🇲
{gray}  │{reset} Core Modules: Lexer {ok} • Parser {ok} • Planner {ok} • Storage {active} {gray}│{reset} Engine v{VERSION} {ok}
{gray}  │{reset} Session {gray}│{reset} mode {cyan}{mode_tag}{reset} {gray}│{reset} data {dim}{data}{reset}
{gray}  └────────────────────────────────────────────────────────────────┘{reset}
{gray}  ┌─ [ QUICKSTART & COMMANDS ] ────────────────────────────────────┐{reset}
{gray}  │{reset} SELECT 1;                                    {gray}(basic query){reset}
{gray}  │{reset} CREATE TABLE t (id TEXT); INSERT INTO t VALUES ('42');
{gray}  │{reset} \\explain SELECT * FROM t;                  {gray}(query plan){reset}
{gray}  │{reset} \\help                                       {gray}(show panel again){reset}
{gray}  └────────────────────────────────────────────────────────────────┘{reset}
",
    );
}
pub(crate) fn studio_prompt() -> String {
    format!("{ESC}[1;36mnoedb{ESC}[0m> ")
}

pub(crate) fn print_prompt_plain() {
    print!("noedb> ");
    let _ = io::stdout().flush();
}

pub(crate) fn print_result(r: &QueryResult, studio: bool) {
    if r.columns.is_empty() && r.rows.is_empty() {
        if studio {
            println!("{ESC}[32m✓{ESC}[0m {ESC}[2mOK{ESC}[0m");
        } else {
            println!("OK");
        }
        return;
    }
    println!("{}", format_query_table(r, studio));
}

fn format_query_table(r: &QueryResult, studio: bool) -> String {
    let mut table = Table::new();
    table.load_preset(if studio { UTF8_FULL } else { ASCII_FULL });
    table.set_content_arrangement(ContentArrangement::Dynamic);

    if studio {
        let header: Vec<Cell> = r
            .columns
            .iter()
            .map(|c| Cell::new(c).add_attribute(Attribute::Bold).fg(Color::Cyan))
            .collect();
        table.set_header(header);
    } else {
        table.set_header(&r.columns);
    }

    for row in &r.rows {
        table.add_row(row);
    }

    let n = r.rows.len();
    let count = if n == 1 {
        "(1 row)".to_string()
    } else {
        format!("({n} rows)")
    };

    if studio {
        format!("{table}\n{ESC}[2m  {count}{ESC}[0m")
    } else {
        format!("{table}\n{count}")
    }
}

pub(crate) fn print_error(msg: &str, studio: bool) {
    if studio {
        eprintln!("{ESC}[1;31m✗ ERROR{ESC}[0m {ESC}[2m{msg}{ESC}[0m");
    } else {
        eprintln!("error: {msg}");
    }
}

pub(crate) fn print_explain(text: &str, studio: bool) {
    if studio {
        println!("{ESC}[1;33m┌─ QUERY PLAN ─────────────────────────────────{ESC}[0m");
        for line in text.lines() {
            println!("{ESC}[90m│{ESC}[0m  {ESC}[2m{line}{ESC}[0m");
        }
        println!("{ESC}[1;33m└──────────────────────────────────────────────{ESC}[0m");
    } else {
        println!("{text}");
    }
}

pub(crate) fn print_help(studio: bool) {
    if studio {
        println!(
            "
{ESC}[90m┌─ [ COMMAND REFERENCE ] ─────────────────────────────────────────{ESC}[0m
{ESC}[90m│{ESC}[0m  {ESC}[1;36m\\q{ESC}[0m                 {ESC}[2mExit the SQL shell{ESC}[0m
{ESC}[90m│{ESC}[0m  {ESC}[1;36m\\help{ESC}[0m              {ESC}[2mShow this reference panel{ESC}[0m
{ESC}[90m│{ESC}[0m  {ESC}[1;36m\\clear{ESC}[0m             {ESC}[2mClear screen and redraw dashboard{ESC}[0m
{ESC}[90m│{ESC}[0m  {ESC}[1;36m\\explain SELECT …{ESC}[0m   {ESC}[2mDisplay optimizer plan{ESC}[0m
{ESC}[90m│{ESC}[0m
{ESC}[90m│{ESC}[0m  {ESC}[1;33mSQL{ESC}[0m  One statement per line, or several separated by {ESC}[2m;{ESC}[0m
{ESC}[90m└──────────────────────────────────────────────────────────────────{ESC}[0m
"
        );
    } else {
        println!("\\q          quit");
        println!("\\help       this message");
        println!("\\explain    show plan for SELECT");
        println!("SELECT ...;  one query per line");
    }
}

pub(crate) fn print_goodbye(studio: bool) {
    if studio {
        println!(
            "\n  {ESC}[90m[session closed] NoeDB v{VERSION} · github.com/toriyama237/NoeDB{ESC}[0m\n"
        );
    }
}

fn truncate_path(path: &str, max: usize) -> String {
    if path.len() <= max {
        return path.to_string();
    }
    format!("…{}", &path[path.len().saturating_sub(max - 1)..])
}
