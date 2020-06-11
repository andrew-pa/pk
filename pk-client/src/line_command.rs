use super::*;
use crate::buffer::Buffer;
 
pub trait CommandFn {
    fn process(&self, es: PEditorState, a: &regex::Captures) -> mode::ModeEventResult;
}

pub struct TestCommand;

impl CommandFn for TestCommand {
    fn process(&self, editor_state: PEditorState, args: &regex::Captures) -> mode::ModeEventResult {
        println!("args = {:?}", args.get(1));
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
                protocol::Response::FileInfo { id, contents, version } => {
                    let mut state = ess.write().unwrap();
                    state.current_buffer = state.buffers.len();
                    state.buffers.push(Buffer::from_server(String::from(server_name), path, id, contents, version));
                    state.force_redraw = true;
                },
                _ => panic!() 
            }
        })?;
        Ok(Some(Box::new(NormalMode::new())))
    }
}

pub struct SyncFileCommand;

impl CommandFn for SyncFileCommand {
    fn process(&self, es: PEditorState, a: &regex::Captures) -> mode::ModeEventResult {
        let (curbuf, server_name, id, new_text, version) = {
            let state = es.read().unwrap();
            let cb = state.current_buffer;
            let b = &state.buffers[cb];
            (cb, b.server_name.clone(), b.file_id, b.text.text(), b.version+1)
        };
        EditorState::make_request_async(es, server_name,
            protocol::Request::SyncFile { id, new_text, version },
            move |ess, resp| {
                match resp {
                    protocol::Response::Ack => {
                        let mut state = ess.write().unwrap();
                        state.buffers[curbuf].version = version;
                    },
                    protocol::Response::VersionConflict { id, client_version_recieved,
                        server_version, server_text } =>
                    {
                        // TODO: probably need to show a nice little dialog, ask the user what they
                        // want to do about the conflict. this becomes a tricky situation since
                        // there's no reason to become Git, but it is nice to able to handle this
                        // situation in a nice way
                        todo!("version conflict!");
                    }
                    _ => panic!() 
                }
            }
        )?;
        Ok(Some(Box::new(NormalMode::new())))
    }
}
