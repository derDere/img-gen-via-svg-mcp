//! Protocol-level test.
//!
//! Starts the server as a subprocess and talks to it over real stdio, so that
//! the transport, the generated tool schemas and the result shape are exercised
//! the way a client exercises them.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Server {
    process: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: u64,
}

impl Server {
    fn start() -> Self {
        let mut process = Command::new(env!("CARGO_BIN_EXE_img-gen-via-svg-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server binary should start");
        let input = process.stdin.take().unwrap();
        let output = BufReader::new(process.stdout.take().unwrap());
        let mut server = Self { process, input, output, next_id: 0 };

        server.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "protocol-test", "version": "1" }
            }),
        );
        server.notify("notifications/initialized");
        server
    }

    fn send(&mut self, payload: Value) {
        writeln!(self.input, "{payload}").unwrap();
        self.input.flush().unwrap();
    }

    fn notify(&mut self, method: &str) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": {} }));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            let read = self.output.read_line(&mut line).expect("the server should answer");
            assert!(read > 0, "the server closed its output stream");
            let message: Value =
                serde_json::from_str(&line).expect("each line is one JSON message");
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                assert!(message.get("error").is_none(), "{method} failed: {message}");
                return message["result"].clone();
            }
        }
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({ "name": tool, "arguments": arguments }))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

#[test]
fn the_server_speaks_the_protocol_and_renders_over_it() {
    let mut server = Server::start();

    let listing = server.request("tools/list", json!({}));
    let names: Vec<&str> =
        listing["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "render_svg",
        "render_svg_batch",
        "render_icon",
        "probe_svg",
        "optimize_svg",
        "convert_image",
        "get_capabilities",
    ] {
        assert!(names.contains(&expected), "{expected} is missing from tools/list");
    }

    // Every tool has to describe itself, or a client cannot choose between them.
    for tool in listing["tools"].as_array().unwrap() {
        let description = tool["description"].as_str().unwrap_or_default();
        assert!(description.len() > 40, "{} has no useful description", tool["name"]);
        assert!(tool["inputSchema"]["type"] == "object");
    }

    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("protocol.png");
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dev/demos/svg/clip-path.svg");

    let result = server.call(
        "render_svg",
        json!({
            "svg_path": source.to_str().unwrap(),
            "output_path": out.to_str().unwrap(),
            "width": 128,
            "height": 128,
        }),
    );
    assert_ne!(result["isError"], json!(true), "{result}");
    assert_eq!(result["structuredContent"]["width"], 128);
    assert_eq!(result["structuredContent"]["format"], "png");
    assert!(out.exists());
    assert_eq!(image::open(&out).unwrap().width(), 128);

    // A failure comes back as a tool-level error carrying the stable code.
    let failure = server.call(
        "render_svg",
        json!({
            "svg_path": dir.path().join("missing.svg").to_str().unwrap(),
            "output_path": dir.path().join("x.png").to_str().unwrap(),
        }),
    );
    assert_eq!(failure["isError"], json!(true));
    assert_eq!(failure["structuredContent"]["error"]["code"], "input_not_found");

    // The server is still answering afterwards.
    let capabilities = server.call("get_capabilities", json!({}));
    assert!(capabilities["structuredContent"]["formats"]["encode"].as_array().unwrap().len() >= 13);
}

#[test]
fn an_image_comes_back_inline_when_asked_for() {
    let mut server = Server::start();
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dev/demos/svg/clip-path.svg");

    let result = server.call(
        "render_svg",
        json!({
            "svg_path": source.to_str().unwrap(),
            "return_mode": "image",
            "width": 32,
            "height": 32,
        }),
    );
    let content = result["content"].as_array().unwrap();
    let image = content.iter().find(|c| c["type"] == "image").expect("an image block");
    assert_eq!(image["mimeType"], "image/png");
    assert!(image["data"].as_str().unwrap().len() > 100);
}
