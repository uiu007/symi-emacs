use lsp_server::{Connection, ErrorCode, Message, Request, Response};
use lsp_types::*;
use std::sync::Arc;
use std::time::Instant;
use symi::{parse_source, Parse, SyntaxKind};
use crate::manager::{MANAGER, LanguageManager};
use crate::byte_char_mapper::ByteCharMapper;

/// Symi Language Server implementation
pub struct SymiLanguageServer {
    connection: Connection,
    root_uri: Option<Url>,
    workspace_folders: Vec<WorkspaceFolder>,
}

impl SymiLanguageServer {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            root_uri: None,
            workspace_folders: Vec::new(),
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Read the capabilities of the client
        let (initialize_id, initialize_params) = self.connection.initialize_start()?;
        let initialize_params: InitializeParams = serde_json::from_value(initialize_params)?;
        
        self.root_uri = initialize_params.root_uri.clone();
        if let Some(workspaces) = initialize_params.workspace_folders {
            self.workspace_folders = workspaces;
        }

        // Send initialize response
        let server_capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec!["@".to_string(), ":".to_string(), ",".to_string()]),
                all_commit_characters: None,
                work_done_progress_options: Default::default(),
                completion_item: None,
            }),
            definition_provider: Some(DefinitionProviderCapability::Simple(true)),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(
                    SemanticTokensOptions {
                        legend: SemanticTokensLegend {
                            token_types: vec![
                                SemanticTokenType::KEYWORD,
                                SemanticTokenType::STRING,
                                SemanticTokenType::NUMBER,
                                SemanticTokenType::OPERATOR,
                                SemanticTokenType::FUNCTION,
                                SemanticTokenType::VARIABLE,
                                SemanticTokenType::COMMENT,
                                SemanticTokenType::PUNCTUATION,
                            ],
                            token_modifiers: vec![
                                SemanticTokenModifier::DOCUMENTATION,
                                SemanticTokenModifier::DEPRECATED,
                            ],
                        },
                        range: Some(false),
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        ..Default::default()
                    }
                )
            ),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                DiagnosticOptions {
                    identifier: Some("symi".to_string()),
                    inter_file_dependencies: false,
                    workspace_diagnostics: false,
                }
            )),
            ..Default::default()
        };

        let initialize_result = InitializeResult {
            capabilities: server_capabilities,
            server_info: Some(ServerInfo {
                name: "symi-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        };

        let initialize_response = Response::new_ok(initialize_id, serde_json::to_value(initialize_result)?);
        self.connection.sender.send(Message::Response(initialize_response))?;

        // Start the main loop
        self.main_loop()
    }

    fn main_loop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for msg in &self.connection.receiver {
            match msg {
                Message::Request(req) => {
                    if self.connection.handle_shutdown(&req)? {
                        return Ok(());
                    }
                    self.handle_request(req)?;
                }
                Message::Response(_) => {}
                Message::Notification(not) => {
                    self.handle_notification(not)?;
                }
            }
        }
        Ok(())
    }

    fn handle_request(&mut self, request: Request) -> Result<(), Box<dyn std::error::Error>> {
        match request.method.as_str() {
            "textDocument/didOpen" => self.handle_did_open(request)?,
            "textDocument/didChange" => self.handle_did_change(request)?,
            "textDocument/didClose" => self.handle_did_close(request)?,
            "textDocument/hover" => self.handle_hover(request)?,
            "textDocument/completion" => self.handle_completion(request)?,
            "textDocument/definition" => self.handle_definition(request)?,
            "textDocument/semanticTokens/full" => self.handle_semantic_tokens(request)?,
            _ => {
                let response = Response::new_err(
                    request.id,
                    ErrorCode::MethodNotFound as i32,
                    format!("Method not found: {}", request.method),
                );
                self.connection.sender.send(Message::Response(response))?;
            }
        }
        Ok(())
    }

    fn handle_notification(&mut self, _notification: lsp_server::Notification) -> Result<(), Box<dyn std::error::Error>> {
        // Handle other notifications if needed
        Ok(())
    }

    fn handle_did_open(&self, request: Request) -> Result<(), Box<dyn std::error::Error>> {
        let params: DidOpenTextDocumentParams = serde_json::from_value(request.params)?;
        let uri = params.text_document.uri;
        let source = params.text_document.text;
        
        // Update the language manager
        let file_id = uri.to_string();
        MANAGER.write().update_file(file_id, source);
        
        // Send diagnostics
        self.send_diagnostics(&uri)?;
        
        Ok(())
    }

    fn handle_did_change(&self, request: Request) -> Result<(), Box<dyn std::error::Error>> {
        let params: DidChangeTextDocumentParams = serde_json::from_value(request.params)?;
        let uri = params.text_document.uri;
        let source = params.content_changes.into_iter().next().unwrap().text;
        
        // Update the language manager
        let file_id = uri.to_string();
        MANAGER.write().update_file(file_id, source);
        
        // Send diagnostics
        self.send_diagnostics(&uri)?;
        
        Ok(())
    }

    fn handle_did_close(&self, request: Request) -> Result<(), Box<dyn std::error::Error>> {
        let params: DidCloseTextDocumentParams = serde_json::from_value(request.params)?;
        let uri = params.text_document.uri;
        
        // Close the file in the language manager
        let file_id = uri.to_string();
        MANAGER.write().close_file(&file_id);
        
        Ok(())
    }

    fn handle_hover(&self, request: Request) -> Result<(), Box<dyn std::error::Error>> {
        let params: TextDocumentPositionParams = serde_json::from_value(request.params)?;
        let uri = params.text_document.uri;
        let position = params.position;
        
        let file_id = uri.to_string();
        let manager = MANAGER.read();
        let lang_manager = manager.files.get(&file_id);
        
        if let Some(lang_manager) = lang_manager {
            // Find the token at the position
            let token_info = self.get_token_info_at_position(lang_manager, position);
            
            let hover = if let Some(info) = token_info {
                Some(Hover {
                    contents: HoverContents::Scalar(MarkedString::String(info)),
                    range: None,
                })
            } else {
                None
            };
            
            let response = Response::new_ok(request.id, serde_json::to_value(hover)?);
            self.connection.sender.send(Message::Response(response))?;
        } else {
            let response = Response::new_ok(request.id, serde_json::to_value(None::<Hover>)?);
            self.connection.sender.send(Message::Response(response))?;
        }
        
        Ok(())
    }

    fn handle_completion(&self, request: Request) -> Result<(), Box<dyn std::error::Error>> {
        let params: CompletionParams = serde_json::from_value(request.params)?;
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        
        let file_id = uri.to_string();
        let manager = MANAGER.read();
        let lang_manager = manager.files.get(&file_id);
        
        let completions = if let Some(lang_manager) = lang_manager {
            self.get_completions_at_position(lang_manager, position)
        } else {
            Vec::new()
        };
        
        let response = Response::new_ok(request.id, serde_json::to_value(completions)?);
        self.connection.sender.send(Message::Response(response))?;
        
        Ok(())
    }

    fn handle_definition(&self, request: Request) -> Result<(), Box<dyn std::error::Error>> {
        let params: TextDocumentPositionParams = serde_json::from_value(request.params)?;
        let uri = params.text_document.uri;
        let position = params.position;
        
        let file_id = uri.to_string();
        let manager = MANAGER.read();
        let lang_manager = manager.files.get(&file_id);
        
        let definition = if let Some(lang_manager) = lang_manager {
            self.get_definition_at_position(lang_manager, position)
        } else {
            None
        };
        
        let response = Response::new_ok(request.id, serde_json::to_value(definition)?);
        self.connection.sender.send(Message::Response(response))?;
        
        Ok(())
    }

    fn handle_semantic_tokens(&self, request: Request) -> Result<(), Box<dyn std::error::Error>> {
        let params: SemanticTokensParams = serde_json::from_value(request.params)?;
        let uri = params.text_document.uri;
        
        let file_id = uri.to_string();
        let manager = MANAGER.read();
        let lang_manager = manager.files.get(&file_id);
        
        let tokens = if let Some(lang_manager) = lang_manager {
            self.get_semantic_tokens(lang_manager)
        } else {
            Vec::new()
        };
        
        let result = SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        });
        
        let response = Response::new_ok(request.id, serde_json::to_value(result)?);
        self.connection.sender.send(Message::Response(response))?;
        
        Ok(())
    }

    fn send_diagnostics(&self, uri: &Url) -> Result<(), Box<dyn std::error::Error>> {
        let file_id = uri.to_string();
        let manager = MANAGER.read();
        let lang_manager = manager.files.get(&file_id);
        
        let diagnostics = if let Some(lang_manager) = lang_manager {
            self.get_diagnostics(lang_manager)
        } else {
            Vec::new()
        };
        
        let params = PublishDiagnosticsParams {
            uri: uri.clone(),
            version: None,
            diagnostics,
        };
        
        let notification = lsp_server::Notification::new("textDocument/publishDiagnostics", params);
        self.connection.sender.send(Message::Notification(notification))?;
        
        Ok(())
    }

    fn get_diagnostics(&self, lang_manager: &LanguageManager) -> Vec<Diagnostic> {
        let mapper = &lang_manager.byte_char_mapper;
        let mut diagnostics = Vec::new();

        // Parse errors
        for err in lang_manager.parse.errors() {
            let start = mapper.byte_to_char(err.range.start().into());
            let end = mapper.byte_to_char(err.range.end().into());
            let (line, character) = self.byte_to_position(&lang_manager.source, start as usize);
            
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line, character },
                    end: Position { line, character: character + (end - start) },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("symi".to_string()),
                message: err.message.clone(),
                related_information: None,
                tags: None,
                data: None,
            });
        }

        // Compiler diagnostics
        let mut compiler = symi::Compiler::new();
        compiler.compile(&lang_manager.parse.syntax_node());
        
        for diag in &compiler.diagnostics {
            let start = mapper.byte_to_char(diag.span.start().into());
            let end = mapper.byte_to_char(diag.span.end().into());
            let (line, character) = self.byte_to_position(&lang_manager.source, start as usize);
            
            let severity = match diag.level {
                symi::compiler::types::DiagnosticLevel::Warning => DiagnosticSeverity::WARNING,
                symi::compiler::types::DiagnosticLevel::Error => DiagnosticSeverity::ERROR,
            };
            
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line, character },
                    end: Position { line, character: character + (end - start) },
                },
                severity: Some(severity),
                code: None,
                code_description: None,
                source: Some("symi".to_string()),
                message: diag.message.clone(),
                related_information: None,
                tags: None,
                data: None,
            });
        }

        diagnostics
    }

    fn get_token_info_at_position(&self, lang_manager: &LanguageManager, position: Position) -> Option<String> {
        // Convert position to byte offset
        let byte_offset = self.position_to_byte(&lang_manager.source, position);
        
        // Find token at position
        for token in &lang_manager.parse.tokens {
            let token_start = token.range.start().into();
            let token_end = token.range.end().into();
            
            if byte_offset >= token_start && byte_offset < token_end {
                return Some(format!("Token: {:?}\nKind: {:?}", token.text, token.kind));
            }
        }
        
        None
    }

    fn get_completions_at_position(&self, _lang_manager: &LanguageManager, _position: Position) -> Vec<CompletionItem> {
        // TODO: Implement intelligent completions for notes, macros, etc.
        vec![
            CompletionItem {
                label: "C4".to_string(),
                kind: Some(CompletionItemKind::VALUE),
                detail: Some("Middle C".to_string()),
                ..Default::default()
            },
            CompletionItem {
                label: "D4".to_string(),
                kind: Some(CompletionItemKind::VALUE),
                detail: Some("D above middle C".to_string()),
                ..Default::default()
            },
        ]
    }

    fn get_definition_at_position(&self, _lang_manager: &LanguageManager, _position: Position) -> Option<Location> {
        // TODO: Implement go-to-definition for macros and base pitch definitions
        None
    }

    fn get_semantic_tokens(&self, lang_manager: &LanguageManager) -> Vec<SemanticToken> {
        let mut tokens = Vec::new();
        let mapper = &lang_manager.byte_char_mapper;
        
        for token in &lang_manager.parse.tokens {
            let start_byte = token.range.start().into();
            let end_byte = token.range.end().into();
            let start_char = mapper.byte_to_char(start_byte);
            let end_char = mapper.byte_to_char(end_byte);
            
            let (line, character) = self.byte_to_position(&lang_manager.source, start_char as usize);
            
            let token_type = match token.kind {
                SyntaxKind::Identifier => 0, // KEYWORD
                SyntaxKind::PitchSpellOctave | SyntaxKind::PitchSpellSimple => 1, // STRING
                SyntaxKind::PitchCents | SyntaxKind::PitchRatio | SyntaxKind::PitchFrequency | SyntaxKind::PitchEdo => 2, // NUMBER
                SyntaxKind::At | SyntaxKind::Plus | SyntaxKind::Equals | SyntaxKind::Colon | SyntaxKind::Semicolon => 3, // OPERATOR
                SyntaxKind::LAngle | SyntaxKind::RAngle | SyntaxKind::LParen | SyntaxKind::RParen => 7, // PUNCTUATION
                SyntaxKind::Comment => 6, // COMMENT
                _ => 0, // KEYWORD
            };
            
            tokens.push(SemanticToken {
                delta_line: line,
                delta_start: character,
                length: (end_char - start_char) as u32,
                token_type,
                token_modifiers_bitset: 0,
            });
        }
        
        tokens
    }

    fn byte_to_position(&self, source: &str, byte_offset: usize) -> (u32, u32) {
        let mut line = 0;
        let mut character = 0;
        let mut current_byte = 0;
        
        for ch in source.chars() {
            if current_byte == byte_offset {
                break;
            }
            
            if ch == '\n' {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
            
            current_byte += ch.len_utf8();
        }
        
        (line, character)
    }

    fn position_to_byte(&self, source: &str, position: Position) -> usize {
        let mut line = 0;
        let mut character = 0;
        let mut current_byte = 0;
        
        for ch in source.chars() {
            if line == position.line && character == position.character {
                break;
            }
            
            if ch == '\n' {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
            
            current_byte += ch.len_utf8();
        }
        
        current_byte
    }
}