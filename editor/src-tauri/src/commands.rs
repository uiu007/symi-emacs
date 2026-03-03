use symi::compiler::types::EventBody;
use symi::Compiler;
use tauri::Emitter;
use std::sync::Arc;
use std::thread;

fn build_midi_bytes(
    file_id: String,
    source: String,
    pitch_bend_range_semitones: u16,
    ticks_per_quarter: u32,
    time_tolerance_seconds: f64,
    pitch_tolerance_cents: f64,
) -> Result<Vec<u8>, String> {
    crate::manager::MANAGER
        .write()
        .update_file(file_id.clone(), source);

    let manager = crate::manager::MANAGER.read();
    let Some(lang_manager) = manager.files.get(&file_id) else {
        return Err("file not found".to_string());
    };

    if let Some(parse_err) = lang_manager.parse.errors().first() {
        return Err(format!("parse error: {}", parse_err.message));
    }

    if let Some(diag) = lang_manager
        .compiler
        .diagnostics
        .iter()
        .find(|d| matches!(d.level, symi::compiler::types::DiagnosticLevel::Error))
    {
        return Err(format!("compile error: {}", diag.message));
    }

    let config = symi::midi::writer::MidiWriterConfig {
        pitch_bend_range_semitones,
        ticks_per_quarter,
        time_tolerance_seconds,
        pitch_tolerance_cents,
    };

    symi::midi::writer::export_smf_format1(&lang_manager.compiler.events, config)
        .map_err(|e| format!("midi export failed: {e}"))
}

#[tauri::command]
pub fn file_update(app: tauri::AppHandle, file_id: String, source: String) {
    crate::manager::MANAGER
        .write()
        .update_file(file_id, source.clone());
    app.emit("file_updated", ()).unwrap();
}

#[tauri::command]
pub fn file_close(app: tauri::AppHandle, file_id: String) {
    crate::manager::MANAGER.write().close_file(&file_id);
    app.emit("file_closed", ()).unwrap();
}

#[tauri::command]
pub async fn get_tokens(file_id: String) -> Vec<(String, u32, u32)> {
    let manager = crate::manager::MANAGER.read();
    if let Some(lang_manager) = manager.files.get(&file_id) {
        let tokens: Vec<(String, u32, u32)> = lang_manager
            .parse
            .tokens
            .iter()
            .map(|t| {
                let kind_str: &'static str = t.kind.into();
                (
                    kind_str.to_string(),
                    lang_manager
                        .byte_char_mapper
                        .byte_to_char(t.range.start().into()),
                    lang_manager
                        .byte_char_mapper
                        .byte_to_char(t.range.end().into()),
                )
            })
            .collect();
        tokens
    } else {
        Vec::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnostic {
    pub message: String,
    // Warning, Error, Info, etc.
    pub severity: String,
    pub from: u32,
    pub to: u32,
}

#[tauri::command]
pub async fn get_diagnostics(file_id: String) -> Vec<Diagnostic> {
    let manager = crate::manager::MANAGER.read();
    let Some(lang_manager) = manager.files.get(&file_id) else {
        return Vec::new();
    };

    let mapper = &lang_manager.byte_char_mapper;
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // 解析错误（lexer/parser）
    for err in lang_manager.parse.errors() {
        let start = mapper.byte_to_char(err.range.start().into());
        let end = mapper.byte_to_char(err.range.end().into());
        diagnostics.push(Diagnostic {
            message: err.message.clone(),
            severity: "Error".to_string(),
            from: if start == end { start - 1 } else { start },
            to: if start == end { end } else { end },
        });
    }

    // 编译诊断
    let mut compiler = Compiler::new();
    let root = lang_manager.parse.syntax_node();
    compiler.compile(&root);
    for diag in &compiler.diagnostics {
        let start = mapper.byte_to_char(diag.span.start().into());
        let end = mapper.byte_to_char(diag.span.end().into());
        let severity = match diag.level {
            symi::compiler::types::DiagnosticLevel::Warning => "Warning",
            symi::compiler::types::DiagnosticLevel::Error => "Error",
        };
        diagnostics.push(Diagnostic {
            message: diag.message.clone(),
            severity: severity.to_string(),
            from: if start == end { start - 1 } else { start },
            to: if start == end { end } else { end },
        });
    }

    diagnostics
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NoteEvent {
    pub r#type: &'static str,
    pub freq: f32,
    pub start_sec: f64,
    pub start_bar: u32,
    pub start_tick: (i32, i32),
    pub duration_sec: f64,
    pub duration_tick: (i32, i32),
    pub span_from: u32,
    pub span_to: u32,
    pub span_invoked_from: Option<u32>,
    pub span_invoked_to: Option<u32>,
    pub pitch_ratio: f32,
}

#[tauri::command]
pub fn get_events(file_id: String) -> Vec<NoteEvent> {
    let manager = crate::manager::MANAGER.read();
    let Some(lang_manager) = manager.files.get(&file_id) else {
        return Vec::new();
    };

    let btc = |byte: usize| lang_manager.byte_char_mapper.byte_to_char(byte as u32);

    lang_manager
        .compiler
        .events
        .iter()
        .filter_map(|event| match &event.body {
            EventBody::Note(note) => Some(NoteEvent {
                r#type: "Note",
                freq: note.freq,
                start_sec: event.start_time.seconds,
                start_bar: event.start_time.bars,
                start_tick: (
                    *event.start_time.ticks.numer(),
                    *event.start_time.ticks.denom(),
                ),
                duration_sec: note.duration_seconds,
                duration_tick: (*note.duration.numer(), *note.duration.denom()),
                span_from: btc(event.range.start().into()),
                span_to: btc(event.range.end().into()),
                span_invoked_from: event.range_invoked.map(|r| btc(r.start().into())),
                span_invoked_to: event.range_invoked.map(|r| btc(r.end().into())),
                pitch_ratio: note.pitch_ratio,
            }),
            EventBody::NewMeasure(bar) => Some(NoteEvent {
                r#type: "NewMeasure",
                freq: 0.0,
                start_sec: event.start_time.seconds,
                start_bar: *bar,
                start_tick: (
                    *event.start_time.ticks.numer(),
                    *event.start_time.ticks.denom(),
                ),
                duration_sec: 0.0,
                duration_tick: (0, 1),
                span_from: btc(event.range.start().into()),
                span_to: btc(event.range.end().into()),
                span_invoked_from: event.range_invoked.map(|r| btc(r.start().into())),
                span_invoked_to: event.range_invoked.map(|r| btc(r.end().into())),
                pitch_ratio: 0.0,
            }),
            EventBody::BaseFequencyDef(f) => Some(NoteEvent {
                r#type: "BaseFrequencyDef",
                freq: *f,
                start_sec: event.start_time.seconds,
                start_bar: event.start_time.bars,
                start_tick: (
                    *event.start_time.ticks.numer(),
                    *event.start_time.ticks.denom(),
                ),
                duration_sec: 0.0,
                duration_tick: (0, 1),
                span_from: btc(event.range.start().into()),
                span_to: btc(event.range.end().into()),
                span_invoked_from: event.range_invoked.map(|r| btc(r.start().into())),
                span_invoked_to: event.range_invoked.map(|r| btc(r.end().into())),
                pitch_ratio: 0.0,
            }),
            _ => None,
        })
        .collect()
}

#[tauri::command]
pub async fn play_note(frequency: f32, duration_sec: f32) {
    crate::manager::AUDIO_MANAGER
        .play_note(frequency, duration_sec)
        .await;
}

#[tauri::command]
pub fn set_volume(volume: f32) -> f32 {
    crate::manager::AUDIO_MANAGER.set_volume(volume);
    crate::manager::AUDIO_MANAGER.volume()
}

#[tauri::command]
pub fn get_volume() -> f32 {
    crate::manager::AUDIO_MANAGER.volume()
}

#[tauri::command]
pub fn validate_midi_export(
    file_id: String,
    source: String,
    pitch_bend_range_semitones: u16,
    ticks_per_quarter: u32,
    time_tolerance_seconds: f64,
    pitch_tolerance_cents: f64,
) -> Result<(), String> {
    build_midi_bytes(
        file_id,
        source,
        pitch_bend_range_semitones,
        ticks_per_quarter,
        time_tolerance_seconds,
        pitch_tolerance_cents,
    )
    .map(|_| ())
}

#[tauri::command]
pub fn export_midi(
    file_id: String,
    source: String,
    target_path: String,
    pitch_bend_range_semitones: u16,
    ticks_per_quarter: u32,
    time_tolerance_seconds: f64,
    pitch_tolerance_cents: f64,
) -> Result<(), String> {
    let bytes = build_midi_bytes(
        file_id,
        source,
        pitch_bend_range_semitones,
        ticks_per_quarter,
        time_tolerance_seconds,
        pitch_tolerance_cents,
    )?;

    std::fs::write(&target_path, &bytes).map_err(|e| format!("write file failed: {e}"))?;

    Ok(())
}

// LSP Server Commands
static mut LSP_SERVER_THREAD: Option<thread::JoinHandle<()>> = None;

#[tauri::command]
pub fn start_lsp_server(port: u16) -> Result<(), String> {
    unsafe {
        if LSP_SERVER_THREAD.is_some() {
            return Err("LSP server is already running".to_string());
        }

        LSP_SERVER_THREAD = Some(thread::spawn(move || {
            if let Err(e) = start_lsp_server_impl(port) {
                eprintln!("LSP server error: {}", e);
            }
        }));
    }
    Ok(())
}

#[tauri::command]
pub fn stop_lsp_server() -> Result<(), String> {
    unsafe {
        if let Some(handle) = LSP_SERVER_THREAD.take() {
            handle.join().map_err(|e| format!("Failed to stop LSP server: {:?}", e))?;
        }
    }
    Ok(())
}

fn start_lsp_server_impl(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    use lsp_server::{Connection, IoThreads};
    use std::net::{TcpListener, TcpStream};
    use std::io::{BufReader, BufWriter};

    // Create TCP listener
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
    println!("LSP server listening on port {}", port);

    // Accept connection
    let (stream, _) = listener.accept()?;
    let (reader, writer) = (BufReader::new(stream.try_clone()?), BufWriter::new(stream));
    
    let (connection, io_threads) = Connection::new(reader, writer);
    
    // Create and run LSP server
    let mut server = crate::bin::lsp_server::SymiLanguageServer::new(connection);
    server.run()?;
    
    io_threads.join()?;
    
    Ok(())
}
