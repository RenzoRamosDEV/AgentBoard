//! El servidor MCP solo escribe JSON-RPC en stdout; las trazas y avisos van a stderr.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// HOME vacío: el servidor no ve ningún log real del usuario.
fn empty_home(name: &str) -> PathBuf {
    let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("HOME temporal");
    home
}

fn run_mcp(log: &str, home: &PathBuf) -> (String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_agentboard-mcp"))
        .env("AGENTBOARD_LOG", log)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("arranca agentboard-mcp");
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#;
    let mut stdin = child.stdin.take().expect("stdin");
    writeln!(stdin, "{init}").expect("escribe initialize");
    drop(stdin);
    let out = child.wait_with_output().expect("salida del MCP");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_only_jsonrpc(stdout: &str) {
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "una respuesta a initialize: {stdout}");
    for l in lines {
        let v: serde_json::Value = serde_json::from_str(l).expect("cada línea de stdout es JSON");
        assert_eq!(v["jsonrpc"], "2.0", "{l}");
    }
}

#[test]
fn aviso_de_variable_invalida_sale_por_stderr_y_no_por_stdout() {
    let home = empty_home("mcp-invalida");
    let (stdout, stderr) = run_mcp("agentboard_lib=nivel_raro", &home);
    assert_only_jsonrpc(&stdout);
    assert!(stderr.contains("AGENTBOARD_LOG"), "stderr: {stderr}");
}

#[test]
fn con_debug_los_spans_van_a_stderr_y_stdout_sigue_limpio() {
    let home = empty_home("mcp-debug");
    let (stdout, stderr) = run_mcp("debug", &home);
    assert_only_jsonrpc(&stdout);
    assert!(stderr.contains("scan_all"), "stderr: {stderr}");
    assert!(stderr.contains("time.busy"), "stderr: {stderr}");
}
