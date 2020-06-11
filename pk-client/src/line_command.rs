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
        println!("editing {:?} on {}", path, server_name);
        let ess = es.clone();
        let tp = {es.read().unwrap().thread_pool.clone()};
        let req_fut = {
            es.write().unwrap().servers.get_mut(&server_name)
                .ok_or(Error::InvalidCommand("server name ".to_owned() + &server_name + " is unknown"))?
                .request(protocol::Request::OpenFile { path: path.clone() })
        };
        tp.spawn_ok(req_fut.then(move |resp: protocol::Response| async move
        {
            match resp {
                protocol::Response::FileInfo { id, contents, version } => {
                    let mut state = ess.write().unwrap();
                    state.current_buffer = state.buffers.len();
                    state.buffers.push(Buffer::from_server(String::from(server_name), path, id, contents, version));
                    // need to force a redraw here
                    state.force_redraw = true;
                },
                protocol::Response::Error { message } => {
                    println!("server error {}", message);
                    // we really need some way to display errors here
                },
                _ => panic!("unexpected response to OpenFile {:?}", resp)
            }
        }));
        Ok(Some(Box::new(NormalMode::new())))
    }
}
