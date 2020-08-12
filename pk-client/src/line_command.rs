use super::*;
 
pub trait CommandFn {
    fn process(&self, cs: PClientState, es: PEditorState, a: &regex::Captures) -> mode::ModeEventResult;
}

pub struct TestCommand;

impl CommandFn for TestCommand {
    fn process(&self, client_state: PClientState, _editor_state: PEditorState, args: &regex::Captures) -> mode::ModeEventResult {
        println!("args = {:?}", args.get(1));
        ClientState::process_usr_msgp(client_state,
            UserMessage::info("This is a test".into(),
                              Some((vec!["option 1".into(), "looong option 2".into(), "option 3".into()],
                              Box::new(move |index, _state| {
                                  println!("selected {}", index);
                              })))));
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct QuitCommand;

impl CommandFn for QuitCommand {
    fn process(&self, cs: PClientState, _: PEditorState, _: &regex::Captures) -> mode::ModeEventResult {
        cs.write().unwrap().should_exit = true;
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct DebugPieceTableCommand;

impl CommandFn for DebugPieceTableCommand {
    fn process(&self, _client_state: PClientState, editor_state: PEditorState, _args: &regex::Captures) -> mode::ModeEventResult {
        println!("{:#?}", editor_state.read().unwrap().current_buffer().map(|b| &b.text));
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct DebugRegistersCommand;

impl CommandFn for DebugRegistersCommand {
    fn process(&self, client_state: PClientState, editor_state: PEditorState, _args: &regex::Captures) -> mode::ModeEventResult {
        ClientState::process_usr_msgp(client_state, UserMessage::info(format!("registers: {:?}", editor_state.read().unwrap().registers), None));
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct EditFileCommand;

impl CommandFn for EditFileCommand {
    fn process(&self, cs: PClientState, es: PEditorState, a: &regex::Captures) -> mode::ModeEventResult {
        use std::path::PathBuf;
        let server_name: String = a.name("server_name").map(|m| m.as_str()).unwrap_or("local").to_owned();
        let path = a.name("path").map(|m| PathBuf::from(m.as_str()))
            .ok_or(Error::InvalidCommand("missing path for editing a file".into()))?;
        ClientState::open_buffer(cs, es, server_name, path, |state, cstate, buffer_index| {
            state.current_pane_mut().content = PaneContent::buffer(buffer_index);
            cstate.write().unwrap().force_redraw = true;
        });
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct BufferCommand;

impl CommandFn for BufferCommand {
    fn process(&self, cs: PClientState, es: PEditorState, a: &regex::Captures) -> mode::ModeEventResult {
        let name_query = a.name("name_query")
            .ok_or_else(|| Error::InvalidCommand("expected buffer name".into()))?.as_str();

        let mut bufs: Vec<(usize, i64)> = {
            use fuzzy_matcher::FuzzyMatcher;
            let mut matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
            es.read().unwrap().buffers.iter().enumerate()
                .flat_map(|(i, b)| b.path.to_str().and_then(|p| matcher.fuzzy_match(p, name_query)).map(|m| (i, m)))
                .collect()
        };

        bufs.sort_by(|a, b| a.1.cmp(&b.1));

        match a.name("subcmd").map(|m| m.as_str()) {
            None => {
                if let Some((buffer_index, _score)) = bufs.get(0) {
                    es.write().unwrap().current_pane_mut().content = PaneContent::buffer(*buffer_index);
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
                    ClientState::make_request_async(cs, buf.server_name, protocol::Request::CloseFile(buf.file_id), 
                        |s, res| {
                            match res {
                                protocol::Response::Ack => {},
                                protocol::Response::Error { message } => 
                                    ClientState::process_usr_msgp(s, UserMessage::error(message, None)),
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
                let mut state = cs.write().unwrap();
                let estate = es.read().unwrap();
                let m = UserMessage::info(
                    bufs.iter().fold(String::from("matching buffers ="),
                            |s, b| s + " " + estate.buffers[b.0].path.to_str().unwrap_or("")), None);
                state.process_usr_msg(m); 
                Ok(Some(Box::new(NormalMode::new())))
            },
            Some(ukcmd) => Err(Error::InvalidCommand(format!("unknown buffer subcommand: {}", ukcmd)))
        }
    }
}

pub struct SyncFileCommand;

impl CommandFn for SyncFileCommand {
    fn process(&self, cs: PClientState, es: PEditorState, _: &regex::Captures) -> mode::ModeEventResult {
        let cb = { es.read().unwrap().current_buffer_index().ok_or_else(|| Error::InvalidCommand("no buffer to sync".into()))? };
        ClientState::sync_buffer(cs, es, cb);
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct ConnectToServerCommand;

impl CommandFn for ConnectToServerCommand {
    fn process(&self, cs: PClientState, _: PEditorState, args: &regex::Captures) -> mode::ModeEventResult {
        println!("connect {:?}", args);
        ClientState::connect_to_server(cs,
               args.name("server_name")
                    .ok_or_else(|| Error::InvalidCommand("expected server name for new connection".into()))?.as_str().into(),
               args.name("server_url")
                    .ok_or_else(|| Error::InvalidCommand("expected server URL for new connection".into()))?.as_str().into());
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct SearchCommand;

impl CommandFn for SearchCommand {
    fn process(&self, cs: PClientState, es: PEditorState, args: &regex::Captures) -> mode::ModeEventResult {
        let mut es = es.write().unwrap();
        let cb = es.current_buffer_mut().unwrap();
        cb.set_query(args.get(2).unwrap().as_str().into());
        match cb.next_query_index(cb.cursor_index, match args.get(1).unwrap().as_str() {
            "/" => Direction::Forward,
            "?" => Direction::Backward,
            _ => panic!()
        }, true) {
            Some(ix) => cb.cursor_index = ix,
            None => ClientState::process_usr_msgp(cs, UserMessage::error(format!("no matches for \"{}\"", args.get(2).unwrap().as_str()), None))
        }
        Ok(Some(Box::new(NormalMode::new())))
    }
}

