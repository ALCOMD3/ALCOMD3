use alcomd3_mcp_protocol::{
    ENDPOINT_FILE_ENV, EndpointMetadata, IPC_PROTOCOL_VERSION, IpcRequest, IpcResponse,
    IpcTransport,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const CURRENT_MCP_PROTOCOL_VERSION: &str = "2025-11-25";

#[test]
fn bridge_negotiates_current_mcp_protocol_version() {
    let test_dir =
        std::env::temp_dir().join(format!("alcomd3-mcp-current-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&test_dir).unwrap();

    let endpoint_file = test_dir.join("endpoint.json");
    let mut bridge = ChildGuard::new(
        Command::new(bridge_exe())
            .env(ENDPOINT_FILE_ENV, &endpoint_file)
            .env(
                "ALCOMD3_GUI_EXECUTABLE",
                test_dir.join(gui_executable_name()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    let mut stdin = bridge.stdin.take().unwrap();
    let stdout = bridge_output(bridge.stdout.take().unwrap());

    write_message(
        &mut stdin,
        &format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{CURRENT_MCP_PROTOCOL_VERSION}","capabilities":{{}},"clientInfo":{{"name":"current-protocol-test","version":"0.0.0"}}}}}}"#
        ),
    );
    let response = read_response(&stdout);

    assert_eq!(response["id"], 1);
    assert_eq!(
        response["result"]["protocolVersion"],
        CURRENT_MCP_PROTOCOL_VERSION
    );

    drop(stdin);
    bridge.kill().ok();
    bridge.wait().ok();
    fs::remove_dir_all(test_dir).ok();
}

#[test]
fn bridge_lists_project_tools() {
    let test_dir =
        std::env::temp_dir().join(format!("alcomd3-mcp-tools-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&test_dir).unwrap();

    let endpoint_file = test_dir.join("endpoint.json");
    let mut bridge = ChildGuard::new(
        Command::new(bridge_exe())
            .env(ENDPOINT_FILE_ENV, &endpoint_file)
            .env(
                "ALCOMD3_GUI_EXECUTABLE",
                test_dir.join(gui_executable_name()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    let mut stdin = bridge.stdin.take().unwrap();
    let stdout = bridge_output(bridge.stdout.take().unwrap());

    write_message(
        &mut stdin,
        &format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{CURRENT_MCP_PROTOCOL_VERSION}","capabilities":{{}},"clientInfo":{{"name":"tools-list-test","version":"0.0.0"}}}}}}"#
        ),
    );
    assert_eq!(read_response(&stdout)["id"], 1);

    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
    );

    let response = read_response(&stdout);
    assert_eq!(response["id"], 2);
    let tools = response["result"]["tools"].as_array().unwrap();
    let tool_names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        tool_names,
        BTreeSet::from([
            "alcomd3_add_existing_project",
            "alcomd3_add_repository",
            "alcomd3_backup_project",
            "alcomd3_copy_project",
            "alcomd3_create_project",
            "alcomd3_get_activity_log_context",
            "alcomd3_get_activity_log_entry",
            "alcomd3_get_environment_settings",
            "alcomd3_get_package_details",
            "alcomd3_get_project_details",
            "alcomd3_get_technical_log_entry",
            "alcomd3_install_project_package",
            "alcomd3_list_packages",
            "alcomd3_list_projects",
            "alcomd3_list_repositories",
            "alcomd3_list_repository_packages",
            "alcomd3_reinstall_project_package",
            "alcomd3_restore_project_from_backup",
            "alcomd3_search_activity_logs",
            "alcomd3_search_technical_logs",
            "alcomd3_summarize_activity_logs",
            "alcomd3_summarize_technical_logs",
            "alcomd3_uninstall_project_package",
        ])
    );
    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_list_packages")
        .expect("alcomd3_list_packages should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"]["properties"]["offset"].is_object());
    assert!(tool["inputSchema"]["properties"]["limit"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_list_repository_packages")
        .expect("alcomd3_list_repository_packages should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"]["properties"]["repository_id"].is_object());
    assert!(tool["inputSchema"]["properties"]["repository_url"].is_object());
    assert!(tool["inputSchema"]["properties"]["offset"].is_object());
    assert!(tool["inputSchema"]["properties"]["limit"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_add_repository")
        .expect("alcomd3_add_repository should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert_eq!(tool["annotations"]["idempotentHint"], false);
    assert_eq!(tool["annotations"]["openWorldHint"], true);
    assert!(tool["inputSchema"]["properties"]["repository_url"].is_object());
    assert!(tool["inputSchema"]["properties"]["headers"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_get_package_details")
        .expect("alcomd3_get_package_details should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"]["properties"]["package_name"].is_object());
    assert!(tool["inputSchema"]["properties"]["version"].is_object());
    assert!(tool["inputSchema"]["properties"]["repository_id"].is_object());
    assert!(tool["inputSchema"]["properties"]["repository_url"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_get_environment_settings")
        .expect("alcomd3_get_environment_settings should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_search_activity_logs")
        .expect("alcomd3_search_activity_logs should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"]["properties"]["search"].is_object());
    assert!(tool["inputSchema"]["properties"]["sources"].is_object());
    assert!(tool["inputSchema"]["properties"]["visibility"].is_object());
    assert!(tool["inputSchema"]["properties"]["offset"].is_object());
    assert!(tool["inputSchema"]["properties"]["limit"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_get_activity_log_entry")
        .expect("alcomd3_get_activity_log_entry should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"]["properties"]["id"].is_object());
    assert!(tool["inputSchema"]["properties"]["include_details"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_summarize_activity_logs")
        .expect("alcomd3_summarize_activity_logs should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"]["properties"]["group_by"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_get_activity_log_context")
        .expect("alcomd3_get_activity_log_context should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"]["properties"]["id"].is_object());
    assert!(tool["inputSchema"]["properties"]["before"].is_object());
    assert!(tool["inputSchema"]["properties"]["after"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_search_technical_logs")
        .expect("alcomd3_search_technical_logs should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"]["properties"]["levels"].is_object());
    assert!(tool["inputSchema"]["properties"]["scope"].is_object());
    assert!(tool["inputSchema"]["properties"]["max_message_chars"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_get_technical_log_entry")
        .expect("alcomd3_get_technical_log_entry should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"]["properties"]["id"].is_object());
    assert!(tool["inputSchema"]["properties"]["max_message_chars"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_summarize_technical_logs")
        .expect("alcomd3_summarize_technical_logs should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert!(tool["inputSchema"]["properties"]["group_by"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_create_project")
        .expect("alcomd3_create_project should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert!(tool["inputSchema"]["properties"]["project_name"].is_object());
    assert!(tool["inputSchema"]["properties"]["base_path"].is_object());
    assert!(tool["inputSchema"]["properties"]["template_id"].is_object());
    assert!(tool["inputSchema"]["properties"]["unity_version"].is_object());
    assert_eq!(tool["execution"]["taskSupport"], "optional");

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_add_existing_project")
        .expect("alcomd3_add_existing_project should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert!(tool["inputSchema"]["properties"]["project_path"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_backup_project")
        .expect("alcomd3_backup_project should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert!(tool["inputSchema"]["properties"]["project_path"].is_object());
    assert!(tool["inputSchema"]["properties"]["backup_name"].is_object());
    assert!(tool["inputSchema"]["properties"]["exclude_vpm_packages"].is_object());
    let required = tool["inputSchema"]["required"]
        .as_array()
        .expect("backup tool required fields should be an array");
    assert!(required.contains(&json!("project_path")));
    assert!(!required.contains(&json!("backup_name")));
    assert!(!required.contains(&json!("exclude_vpm_packages")));

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_copy_project")
        .expect("alcomd3_copy_project should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert!(tool["inputSchema"]["properties"]["source_project_path"].is_object());
    assert!(tool["inputSchema"]["properties"]["new_project_path"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_restore_project_from_backup")
        .expect("alcomd3_restore_project_from_backup should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert!(tool["inputSchema"]["properties"]["backup_path"].is_object());
    assert!(tool["inputSchema"]["properties"]["project_name"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_install_project_package")
        .expect("alcomd3_install_project_package should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert!(tool["inputSchema"]["properties"]["project_path"].is_object());
    assert!(tool["inputSchema"]["properties"]["package_name"].is_object());
    assert!(tool["inputSchema"]["properties"]["version_selector"].is_object());
    assert!(tool["inputSchema"]["properties"]["source"].is_object());
    assert!(tool["inputSchema"]["properties"]["allow_conflicts"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_uninstall_project_package")
        .expect("alcomd3_uninstall_project_package should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], true);
    assert!(tool["inputSchema"]["properties"]["project_path"].is_object());
    assert!(tool["inputSchema"]["properties"]["package_name"].is_object());
    assert!(tool["inputSchema"]["properties"]["allow_conflicts"].is_object());

    let tool = tools
        .iter()
        .find(|tool| tool["name"] == "alcomd3_reinstall_project_package")
        .expect("alcomd3_reinstall_project_package should be exposed");

    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
    assert!(tool["inputSchema"]["properties"]["project_path"].is_object());
    assert!(tool["inputSchema"]["properties"]["package_name"].is_object());
    assert!(tool["inputSchema"]["properties"]["allow_conflicts"].is_object());

    drop(stdin);
    bridge.kill().ok();
    bridge.wait().ok();
    fs::remove_dir_all(test_dir).ok();
}

#[test]
fn bridge_stays_connected_when_gui_endpoint_disappears() {
    let test_dir =
        std::env::temp_dir().join(format!("alcomd3-mcp-lifecycle-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&test_dir).unwrap();

    let endpoint_file = test_dir.join("endpoint.json");
    let metadata = EndpointMetadata {
        protocol_version: IPC_PROTOCOL_VERSION,
        transport: IpcTransport::Tcp,
        host: "127.0.0.1".to_string(),
        port: 9,
        token: "test-token".to_string(),
        pid: std::process::id(),
    };
    fs::write(&endpoint_file, serde_json::to_vec(&metadata).unwrap()).unwrap();

    let mut bridge = ChildGuard::new(
        Command::new(bridge_exe())
            .env(ENDPOINT_FILE_ENV, &endpoint_file)
            .env(
                "ALCOMD3_GUI_EXECUTABLE",
                test_dir.join(gui_executable_name()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    let mut stdin = bridge.stdin.take().unwrap();
    let stdout = bridge_output(bridge.stdout.take().unwrap());

    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"lifecycle-test","version":"0.0.0"}}}"#,
    );
    assert_eq!(read_response(&stdout)["id"], 1);

    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );

    fs::remove_file(&endpoint_file).unwrap();

    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"alcomd3_list_projects","arguments":{}}}"#,
    );
    let response = read_response(&stdout);
    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        response["result"]["structuredContent"]["error"]["code"],
        "alcomd3_unavailable"
    );
    assert!(
        bridge.try_wait().unwrap().is_none(),
        "bridge should stay alive after the GUI endpoint disappears"
    );

    drop(stdin);
    bridge.kill().ok();
    bridge.wait().ok();
    fs::remove_dir_all(test_dir).ok();
}

#[test]
fn bridge_attempts_to_start_gui_when_endpoint_is_missing() {
    let test_dir =
        std::env::temp_dir().join(format!("alcomd3-mcp-autostart-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&test_dir).unwrap();

    let endpoint_file = test_dir.join("endpoint.json");
    let missing_gui = test_dir.join(gui_executable_name());

    let mut bridge = ChildGuard::new(
        Command::new(bridge_exe())
            .env(ENDPOINT_FILE_ENV, &endpoint_file)
            .env("ALCOMD3_GUI_EXECUTABLE", &missing_gui)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    let mut stdin = bridge.stdin.take().unwrap();
    let stdout = bridge_output(bridge.stdout.take().unwrap());

    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"autostart-test","version":"0.0.0"}}}"#,
    );
    assert_eq!(read_response(&stdout)["id"], 1);

    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"alcomd3_list_projects","arguments":{}}}"#,
    );

    let response = read_response(&stdout);
    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["isError"], true);
    let message = response["result"]["structuredContent"]["error"]["message"]
        .as_str()
        .unwrap();
    assert!(
        message.contains("starting ALCOMD3 GUI"),
        "expected auto-start failure context, got: {message}"
    );
    assert!(
        bridge.try_wait().unwrap().is_none(),
        "bridge should stay alive after GUI auto-start fails"
    );

    drop(stdin);
    bridge.kill().ok();
    bridge.wait().ok();
    fs::remove_dir_all(test_dir).ok();
}

#[test]
fn bridge_does_not_start_gui_for_protocol_mismatch() {
    let test_dir =
        std::env::temp_dir().join(format!("alcomd3-mcp-protocol-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&test_dir).unwrap();

    let endpoint_file = test_dir.join("endpoint.json");
    let metadata = EndpointMetadata {
        protocol_version: IPC_PROTOCOL_VERSION + 1,
        transport: IpcTransport::Tcp,
        host: "127.0.0.1".to_string(),
        port: 9,
        token: "test-token".to_string(),
        pid: std::process::id(),
    };
    fs::write(&endpoint_file, serde_json::to_vec(&metadata).unwrap()).unwrap();

    let mut bridge = ChildGuard::new(
        Command::new(bridge_exe())
            .env(ENDPOINT_FILE_ENV, &endpoint_file)
            .env(
                "ALCOMD3_GUI_EXECUTABLE",
                test_dir.join(gui_executable_name()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    let mut stdin = bridge.stdin.take().unwrap();
    let stdout = bridge_output(bridge.stdout.take().unwrap());

    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"protocol-test","version":"0.0.0"}}}"#,
    );
    assert_eq!(read_response(&stdout)["id"], 1);

    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"alcomd3_list_projects","arguments":{}}}"#,
    );

    let response = read_response(&stdout);
    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["isError"], true);
    let message = response["result"]["structuredContent"]["error"]["message"]
        .as_str()
        .unwrap();
    assert!(
        message.contains("protocol mismatch"),
        "expected protocol mismatch context, got: {message}"
    );
    assert!(
        !message.contains("starting ALCOMD3 GUI"),
        "protocol mismatch must not trigger GUI auto-start: {message}"
    );
    assert!(
        bridge.try_wait().unwrap().is_none(),
        "bridge should stay alive after a protocol mismatch"
    );

    drop(stdin);
    bridge.kill().ok();
    bridge.wait().ok();
    fs::remove_dir_all(test_dir).ok();
}

#[test]
fn bridge_forwards_stdio_calls_to_loopback_ipc_and_preserves_gui_errors() {
    let test_dir =
        std::env::temp_dir().join(format!("alcomd3-mcp-loopback-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&test_dir).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let endpoint_file = test_dir.join("endpoint.json");
    let endpoint_token = "loopback-test-token";
    let metadata = EndpointMetadata {
        protocol_version: IPC_PROTOCOL_VERSION,
        transport: IpcTransport::Tcp,
        host: "127.0.0.1".to_string(),
        port,
        token: endpoint_token.to_string(),
        pid: std::process::id(),
    };
    fs::write(&endpoint_file, serde_json::to_vec(&metadata).unwrap()).unwrap();

    let server = thread::spawn(move || {
        let mut session_id = None;
        for response in [
            Ok(json!({
                "projects": [{ "name": "Temporary Project", "path": "C:/tmp/project" }]
            })),
            Err((
                "project_not_found",
                "The requested project does not exist",
                json!({ "path": "C:/tmp/missing" }),
            )),
        ] {
            let stream = accept_with_timeout(&listener);
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: IpcRequest = serde_json::from_str(&line).unwrap();

            assert_eq!(request.protocol_version, IPC_PROTOCOL_VERSION);
            assert_eq!(request.token, endpoint_token);
            assert_eq!(request.method, "list_projects");
            assert_eq!(request.params, json!({}));
            assert_eq!(request.client.name, "loopback-test");
            assert_eq!(request.client.version.as_deref(), Some("1.2.3"));
            if let Some(expected_session_id) = session_id {
                assert_eq!(request.client.session_id, expected_session_id);
            } else {
                session_id = Some(request.client.session_id);
            }

            let response = match response {
                Ok(result) => IpcResponse::success(request.request_id, result),
                Err((code, message, data)) => {
                    IpcResponse::error(request.request_id, code, message, Some(data))
                }
            };
            let stream = reader.get_mut();
            serde_json::to_writer(&mut *stream, &response).unwrap();
            stream.write_all(b"\n").unwrap();
            stream.flush().unwrap();
        }
    });

    let mut bridge = ChildGuard::new(
        Command::new(bridge_exe())
            .env(ENDPOINT_FILE_ENV, &endpoint_file)
            .env(
                "ALCOMD3_GUI_EXECUTABLE",
                test_dir.join(gui_executable_name()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    let mut stdin = bridge.stdin.take().unwrap();
    let stdout = bridge_output(bridge.stdout.take().unwrap());

    write_message(
        &mut stdin,
        &format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{CURRENT_MCP_PROTOCOL_VERSION}","capabilities":{{}},"clientInfo":{{"name":"loopback-test","version":"1.2.3"}}}}}}"#
        ),
    );
    assert_eq!(read_response(&stdout)["id"], 1);
    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );

    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"alcomd3_list_projects","arguments":{}}}"#,
    );
    let response = read_response(&stdout);
    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(response["result"]["structuredContent"]["ok"], true);
    assert_eq!(
        response["result"]["structuredContent"]["projects"][0]["name"],
        "Temporary Project"
    );

    write_message(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"alcomd3_list_projects","arguments":{}}}"#,
    );
    let response = read_response(&stdout);
    assert_eq!(response["id"], 3);
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        response["result"]["structuredContent"]["error"]["code"],
        "project_not_found"
    );
    assert_eq!(
        response["result"]["structuredContent"]["error"]["data"]["path"],
        "C:/tmp/missing"
    );

    server.join().unwrap();
    assert!(
        bridge.try_wait().unwrap().is_none(),
        "bridge should stay alive after a GUI business error"
    );
    drop(stdin);
    bridge.kill().ok();
    bridge.wait().ok();
    fs::remove_dir_all(test_dir).ok();
}

fn write_message(stdin: &mut ChildStdin, message: &str) {
    writeln!(stdin, "{message}").unwrap();
    stdin.flush().unwrap();
}

struct BridgeOutput {
    responses: Receiver<Result<Value, String>>,
}

fn bridge_output(stdout: ChildStdout) -> BridgeOutput {
    let (sender, responses) = mpsc::channel();
    thread::spawn(move || {
        let mut stdout = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            let response = match stdout.read_line(&mut line) {
                Ok(0) => Err("MCP bridge stdout closed before the next response".to_string()),
                Ok(_) => serde_json::from_str(&line)
                    .map_err(|error| format!("invalid MCP bridge response: {error}: {line}")),
                Err(error) => Err(format!("failed to read MCP bridge response: {error}")),
            };
            let terminal = response.is_err();
            if sender.send(response).is_err() || terminal {
                break;
            }
        }
    });
    BridgeOutput { responses }
}

fn read_response(stdout: &BridgeOutput) -> Value {
    stdout
        .responses
        .recv_timeout(Duration::from_secs(10))
        .expect("timed out waiting for an MCP bridge response")
        .unwrap_or_else(|error| panic!("{error}"))
}

fn accept_with_timeout(listener: &TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("failed to restore blocking mode on accepted MCP stream");
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for MCP loopback connection"
                );
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("failed to accept MCP loopback connection: {error}"),
        }
    }
}

struct ChildGuard(Child);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(child)
    }
}

impl Deref for ChildGuard {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            self.0.kill().ok();
            self.0.wait().ok();
        }
    }
}

fn bridge_exe() -> &'static str {
    env!("CARGO_BIN_EXE_alcomd3-mcp")
}

fn gui_executable_name() -> &'static Path {
    #[cfg(windows)]
    {
        Path::new("ALCOMD3.exe")
    }
    #[cfg(not(windows))]
    {
        Path::new("ALCOMD3")
    }
}
