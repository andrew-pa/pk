use super::*;
 
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
        let server_name = a.name("server_name").map(|m| m.as_str()).unwrap_or("local");
        let path = a.name("path").map(|m| PathBuf::from(m.as_str()))
            .ok_or(Error::InvalidCommand("missing path for editing a file".into()))?;
        let ess = es.clone();
        let mut state = es.write().unwrap();
        let tp = state.thread_pool.clone();
        let mut server = state.servers.get_mut(server_name)
            .ok_or(Error::InvalidCommand("server name ".to_owned() + server_name + " is unknown"))?;
        tp.spawn_ok(server.request(protocol::Request::OpenFile { path })
                                .then(move |resp: protocol::Response| async move
        {
            match resp {
                protocol::Response::FileInfo { id, contents, version } => {
                    let mut state = ess.write().unwrap();
                    state.current_buffer = state.buffers.len();
                    //es.buffers.push(Buffer::from_server(id, contents, version));
                },
                protocol::Response::Error { message } => {
                },
                _ => panic!("unexpected response to OpenFile {:?}", resp)
            }
        }));
        Ok(Some(Box::new(NormalMode::new())))
    }
}
