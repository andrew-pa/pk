use super::*;
use crate::buffer::Buffer;
 
pub trait CommandFn {
    fn process(&self, es: PEditorState, a: &regex::Captures) -> mode::ModeEventResult;
}

pub struct TestCommand;

impl CommandFn for TestCommand {
    fn process(&self, editor_state: PEditorState, args: &regex::Captures) -> mode::ModeEventResult {
        println!("args = {:?}", args.get(1));
        EditorState::process_usr_msgp(editor_state,
            UserMessage::info("This is a test".into(),
                              Some((vec!["option 1".into(), "looong option 2".into(), "option 3".into()],
                              Box::new(move |index, _state| {
                                  println!("selected {}", index);
                              })))));
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct EditFileCommand;

impl CommandFn for EditFileCommand {
    fn process(&self, es: PEditorState, a: &regex::Captures) -> mode::ModeEventResult {
        use std::path::PathBuf;
        let server_name: String = a.name("server_name").map(|m| m.as_str()).unwrap_or("local").to_owned();
        let path = a.name("path").map(|m| PathBuf::from(m.as_str()))
            .ok_or(Error::InvalidCommand("missing path for editing a file".into()))?;
        EditorState::make_request_async(es, server_name.clone(), protocol::Request::OpenFile { path: path.clone() }, |ess, resp| {
            match resp {
                protocol::Response::FileInfo { id, contents, version, format } => {
                    let mut state = ess.write().unwrap();
                    state.current_buffer = state.buffers.len();
                    state.buffers.push(Buffer::from_server(String::from(server_name),
                        path, id, contents, version, format));
                    state.force_redraw = true;
                },
                _ => panic!() 
            }
        });
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct BufferCommand;

impl CommandFn for BufferCommand {
    fn process(&self, es: PEditorState, a: &regex::Captures) -> mode::ModeEventResult {
        let name_query = a.name("name_query")
            .ok_or_else(|| Error::InvalidCommand("expected buffer name".into()))?.as_str();

        let mut bufs: Vec<(usize, i64)> = {
            use fuzzy_matcher::skim::fuzzy_match;
            es.read().unwrap().buffers.iter().enumerate()
                .flat_map(|(i, b)| b.path.to_str().and_then(|p| fuzzy_match(p, name_query)).map(|m| (i, m)))
                .collect()
        };

        bufs.sort_by(|a, b| a.1.cmp(&b.1));

        match a.name("subcmd").map(|m| m.as_str()) {
            None => {
                if let Some((index, _score)) = bufs.get(0) {
                    es.write().unwrap().current_buffer = *index;
                    Ok(Some(Box::new(NormalMode::new())))
                } else {
                    Err(Error::InvalidCommand(format!("no matching buffer for {}", name_query)))
                }
            },
            Some("x") => {
                if let Some((index, _score)) = bufs.get(0) {
                    let mut state = es.write().unwrap();
                    let buf = state.buffers.remove(*index);
                    drop(state);
                    EditorState::make_request_async(es, buf.server_name, protocol::Request::CloseFile(buf.file_id), 
                        |s, res| {
                            match res {
                                protocol::Response::Ack => {},
                                protocol::Response::Error { message } => EditorState::process_usr_msgp(s,
                                                                UserMessage::error(message, None)),
                                _ => panic!("unexpected server response {:?}", res)
                            }
                        }
                    );
                    Ok(Some(Box::new(NormalMode::new())))
                } else {
                    Err(Error::InvalidCommand(format!("no matching buffer for {}", name_query)))
                }
            }
            Some("l") => {
                let mut state = es.write().unwrap();
                let m = UserMessage::info(
                    bufs.iter().fold(String::from("matching buffers ="),
                            |s, b| s + " " + state.buffers[b.0].path.to_str().unwrap_or("")), None);
                state.process_usr_msg(m); 
                Ok(Some(Box::new(NormalMode::new())))
            },
            Some(ukcmd) => Err(Error::InvalidCommand(format!("unknown buffer subcommand: {}", ukcmd)))
        }
    }
}

pub struct SyncFileCommand;

impl CommandFn for SyncFileCommand {
    fn process(&self, es: PEditorState, _: &regex::Captures) -> mode::ModeEventResult {
        let cb = { es.read().unwrap().current_buffer };
        EditorState::sync_buffer(es, cb);
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct ConnectToServerCommand;

impl CommandFn for ConnectToServerCommand {
    fn process(&self, es: PEditorState, args: &regex::Captures) -> mode::ModeEventResult {
        println!("connect {:?}", args);
        EditorState::connect_to_server(es,
               args.name("server_name")
                    .ok_or_else(|| Error::InvalidCommand("expected server name for new connection".into()))?.as_str().into(),
               args.name("server_url")
                    .ok_or_else(|| Error::InvalidCommand("expected server URL for new connection".into()))?.as_str().into());
        Ok(Some(Box::new(NormalMode::new())))
    }
}

