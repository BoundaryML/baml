use std::{num::NonZeroUsize, thread};

use crossbeam_channel::{Receiver, Sender};
use log::LevelFilter;
use lsp_server::Message;
use serde_json::json;
use tokio::sync::broadcast;

use crate::server::{connection::ConnectionInitializer, Server, ServerArgs};

pub struct TestServer {
    pub thread_join_handle: thread::JoinHandle<()>,
    pub sender: Sender<Message>,
    pub receiver: Receiver<Message>,
}

impl TestServer {
    pub fn req_respond(
        &self,
        req: lsp_server::Message,
    ) -> Result<lsp_server::Message, anyhow::Error> {
        self.sender.send(req)?;
        let resp = self.receiver.recv()?;
        Ok(resp)
    }
}

struct TestCase {
    /// Files to load at initialization time.
    files: Vec<(String, String)>,
    /// A list of pairs of client message, and expected server response messages.
    interactions: Vec<(Message, Vec<Message>)>,
}

impl TestCase {
    pub fn run(self) -> anyhow::Result<()> {
        simple_logging::log_to_file("test.log", LevelFilter::Info).unwrap();
        eprintln!("new_test_server");
        let test_server = new_test_server(NonZeroUsize::new(1).unwrap())?;
        eprintln!("about to loop");
        for (file_name, file_content) in self.files {
            test_server.sender.send(lsp_server::Message::Notification(
                lsp_server::Notification {
                    method: "textDocument/didOpen".to_string(),
                    params: json!({
                      "textDocument": {
                        "uri": format!("file:///{}", file_name),
                        "languageId": "baml",
                        "version": 1,
                        "text": file_content
                      }
                    }),
                },
            ))?;

            // Consume the post-opening notification.
            // eprintln!("Awaiting didOpen notif");
            // let did_open_notification = test_server.receiver.recv();
            // eprintln!("Got didOpen notif");
            // dbg!(&did_open_notification);
            // eprintln!("Await next notification");
            let next_notification = test_server.receiver.recv();
            eprintln!("Got next notif");
            dbg!(&next_notification);
        }

        for (req, expected_responses) in self.interactions {
            test_server.sender.send(req)?;
            for expected_response in expected_responses {
                let response = test_server.receiver.recv()?;
                match (&response, expected_response) {
                    (lsp_server::Message::Response(r1), lsp_server::Message::Response(r2)) => {
                        assert_eq!(r1.result, r2.result);
                    }
                    (_, lsp_server::Message::Response(r2)) => {
                        panic!("Expected response {r2:?}, got {response:?}");
                    }
                    (
                        lsp_server::Message::Notification(n1),
                        lsp_server::Message::Notification(n2),
                    ) => {
                        assert_eq!(n1.method, n2.method);
                        assert_eq!(n1.params, n2.params);
                    }
                    (_, lsp_server::Message::Notification(n2)) => {
                        panic!("Expected notification {n2:?}, got {response:?}");
                    }
                    _ => panic!("Should only expect responses and notifications."),
                }
            }
        }
        test_server.thread_join_handle.join().unwrap();
        Ok(())
    }

    pub fn mk_simple() -> Self {
        TestCase {
            files: vec![("test.baml".to_string(), SINGLE_FILE.to_string())],
            interactions: vec![],
        }
    }
}

pub fn new_test_server(worker_threads: NonZeroUsize) -> anyhow::Result<TestServer> {
    let initialize = lsp_server::Message::Request(lsp_server::Request {
        id: lsp_server::RequestId::from(1),
        method: "initialize".to_string(),
        params: neovim_initialize_params(),
    });
    let (server_connection, client_connection) = lsp_server::Connection::memory();

    client_connection.sender.send(initialize).unwrap();
    let thread_join_handle = thread::spawn(move || {
        let connection = ConnectionInitializer::new(server_connection);
        let (id, init_params) = connection.initialize_start().unwrap();

        let client_capabilities = init_params.capabilities.clone();
        let position_encoding = Server::find_best_position_encoding(&client_capabilities);
        let server_capabilities = Server::server_capabilities(position_encoding);

        let connection = connection
            .initialize_finish(
                id,
                &server_capabilities,
                crate::SERVER_NAME,
                crate::version(),
            )
            .unwrap();

        let (to_webview_router_tx, to_webview_router_rx) = broadcast::channel(1);
        let (webview_router_to_websocket_tx, _) = broadcast::channel(1);

        let server = Server::new_with_connection(
            worker_threads,
            connection,
            init_params,
            ServerArgs {
                tokio_runtime: tokio::runtime::Runtime::new().unwrap(),
                webview_router_to_websocket_tx,
                to_webview_router_rx,
                to_webview_router_tx,
                playground_port: 0,
                proxy_port: 0,
            },
        )
        .unwrap();
        server.run().unwrap();
    });

    let _handshake = client_connection.receiver.recv()?;
    client_connection
        .sender
        .send(lsp_server::Message::Notification(
            lsp_server::Notification {
                method: "initialized".to_string(),
                params: json!({}),
            },
        ))?;

    Ok(TestServer {
        thread_join_handle,
        sender: client_connection.sender,
        receiver: client_connection.receiver,
    })
}

fn neovim_initialize_params() -> serde_json::Value {
    let pwd = env!("CARGO_MANIFEST_DIR");
    json!({
        "workspaceFolders": [{
            "name": pwd,
            "uri": format!("file://{}", pwd)
        }],
        "trace": "off",
        "capabilities": {
            "workspace": {
                "workspaceFolders": true,
                "applyEdit": true,
                "workspaceEdit": {
                    "resourceOperations": ["rename", "create", "delete"]
                },
                "semanticTokens": {
                    "refreshSupport": true
                },
                "symbol": {
                    "dynamicRegistration": false,
                    "symbolKind": {
                        "valueSet": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26]
                    }
                },
                "configuration": true,
                "didChangeConfiguration": {
                    "dynamicRegistration": false
                },
                "didChangeWatchedFiles": {
                    "dynamicRegistration": true,
                    "relativePatternSupport": true
                },
                "inlayHint": {
                    "refreshSupport": true
                }
            },
            "window": {
                "showDocument": {
                    "support": true
                },
                "workDoneProgress": true,
                "showMessage": {
                    "messageActionItem": {
                        "additionalPropertiesSupport": false
                    }
                }
            },
            "textDocument": {
                "diagnostic": {
                    "dynamicRegistration": false
                },
                "formatting": {
                    "dynamicRegistration": true
                },
                "rangeFormatting": {
                    "dynamicRegistration": true
                },
                "completion": {
                    "contextSupport": false,
                    "completionItemKind": {
                        "valueSet": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25]
                    },
                    "completionList": {
                        "itemDefaults": ["editRange", "insertTextFormat", "insertTextMode", "data"]
                    },
                    "completionItem": {
                        "preselectSupport": false,
                        "deprecatedSupport": false,
                        "documentationFormat": ["markdown", "plaintext"],
                        "snippetSupport": false,
                        "commitCharactersSupport": false
                    },
                    "dynamicRegistration": false
                },
                "declaration": {
                    "linkSupport": true
                },
                "definition": {
                    "linkSupport": true,
                    "dynamicRegistration": true
                },
                "implementation": {
                    "linkSupport": true
                },
                "typeDefinition": {
                    "linkSupport": true
                },
                "inlayHint": {
                    "dynamicRegistration": true,
                    "resolveSupport": {
                        "properties": ["textEdits", "tooltip", "location", "command"]
                    }
                },
                "signatureHelp": {
                    "dynamicRegistration": false,
                    "signatureInformation": {
                        "documentationFormat": ["markdown", "plaintext"],
                        "activeParameterSupport": true,
                        "parameterInformation": {
                            "labelOffsetSupport": true
                        }
                    }
                },
                "semanticTokens": {
                    "formats": ["relative"],
                    "requests": {
                        "full": {
                            "delta": true
                        },
                        "range": false
                    },
                    "overlappingTokenSupport": true,
                    "multilineTokenSupport": false,
                    "serverCancelSupport": false,
                    "augmentsSyntaxTokens": true,
                    "tokenModifiers": ["declaration", "definition", "readonly", "static", "deprecated", "abstract", "async", "modification", "documentation", "defaultLibrary"],
                    "dynamicRegistration": false,
                    "tokenTypes": ["namespace", "type", "class", "enum", "interface", "struct", "typeParameter", "parameter", "variable", "property", "enumMember", "event", "function", "method", "macro", "keyword", "modifier", "comment", "string", "number", "regexp", "operator", "decorator"]
                },
                "hover": {
                    "dynamicRegistration": true,
                    "contentFormat": ["markdown", "plaintext"]
                },
                "documentSymbol": {
                    "symbolKind": {
                        "valueSet": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26]
                    },
                    "hierarchicalDocumentSymbolSupport": true,
                    "dynamicRegistration": false
                },
                "callHierarchy": {
                    "dynamicRegistration": false
                },
                "synchronization": {
                    "didSave": true,
                    "willSaveWaitUntil": true,
                    "dynamicRegistration": false,
                    "willSave": true
                },
                "publishDiagnostics": {
                    "relatedInformation": true,
                    "tagSupport": {
                        "valueSet": [1,2]
                    },
                    "dataSupport": true
                },
                "codeAction": {
                    "isPreferredSupport": true,
                    "dataSupport": true,
                    "codeActionLiteralSupport": {
                        "codeActionKind": {
                            "valueSet": ["", "quickfix", "refactor", "refactor.extract", "refactor.inline", "refactor.rewrite", "source", "source.organizeImports"]
                        }
                    },
                    "dynamicRegistration": true,
                    "resolveSupport": {
                        "properties": ["edit"]
                    }
                },
                "references": {
                    "dynamicRegistration": false
                },
                "rename": {
                    "dynamicRegistration": true,
                    "prepareSupport": true
                },
                "documentHighlight": {
                    "dynamicRegistration": false
                }
            },
            "general": {
                "positionEncodings": ["utf-16"]
            }
        },
        "clientInfo": {
            "version": "0.10.4",
            "name": "Neovim"
        },
        "rootPath": pwd,
        "rootUri": format!("file://{}", pwd),
        "initializationOptions": {},
        "workDoneToken": "1",
        "processId": 81093
    })
}

static SINGLE_FILE: &str = r##"
client<llm> GPT4 {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

generator lang_python {
  output_type python/pydantic
  output_dir "../python"
  version 0.74.0
}

class Foo {
  bar int
}

function Succ(inp: int) -> Foo {
  client GPT3
  prompt #"
    The successor of {{ inp }}.
    {{ ctx.output_format }}
  "#
}

test TestSucc {
  functions [Succ]
  args { inp 1 }
}
"##;

// This test can be useful for local debugging. But it can't run in CI
// or as part of the ful test suite because it requires Ctrl-C to
// terminate.
// #[test]
fn test_initialization() {
    TestCase::mk_simple().run().unwrap()
}
