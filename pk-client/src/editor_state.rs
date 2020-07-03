use std::sync::{Arc,RwLock};
use std::collections::{HashMap, BTreeMap};
use futures::prelude::*;
use pk_common::*;
use crate::server::Server;
use pk_common::piece_table::PieceTable;
use crate::buffer::Buffer;
use crate::config::Config;
use super::Error;

pub enum UserMessageType {
    Error, Warning, Info
}

type UserMessageActions = (Vec<String>, Box<dyn Fn(usize, PClientState) + Send + Sync>); 

pub struct UserMessage {
    pub mtype: UserMessageType,
    pub message: String,
    pub actions: Option<UserMessageActions>,
    ttl: f32
}

const USER_MESSAGE_TTL: f32 = 3.0f32;

impl UserMessage {
    pub fn error(message: String, actions: Option<UserMessageActions>) -> UserMessage {
        UserMessage {
            mtype: UserMessageType::Error,
            message, actions,
            ttl: USER_MESSAGE_TTL 
        }
    }

    pub fn warning(message: String, actions: Option<UserMessageActions>) -> UserMessage {
        UserMessage {
            mtype: UserMessageType::Warning,
            message, actions,
            ttl: USER_MESSAGE_TTL 
        }
    }
 
    pub fn info(message: String, actions: Option<UserMessageActions>) -> UserMessage {
        UserMessage {
            mtype: UserMessageType::Info,
            message, actions,
            ttl: USER_MESSAGE_TTL 
        }
    }
}

use std::cell::RefCell;

#[derive(Clone, Debug)]
pub enum PaneContent {
    Empty,
    Buffer {
        buffer_index: usize,
        viewport_start: usize
    }
}

impl PaneContent {
    fn buffer(buffer_index: usize) -> PaneContent {
        PaneContent::Buffer {
            buffer_index, viewport_start: 0
        }
    }
}

use runic::Rect;

#[derive(Debug, Clone)]
pub struct Pane {
    pub content: PaneContent,

    // in units of 0..1 where 0 is the top/left edge of the screen and 1 is the bottom/right edge
    pub bounds: Rect,

    // [ left, right, top, bottom ]
    pub neighbors: [Option<usize>; 4],
}

fn split_rect(rect: Rect, dir: bool, size: f32) -> (Rect, Rect) {
    let inv_size = 1.0 - size;
    if dir {
        (Rect::xywh(rect.x, rect.y, rect.w * inv_size, rect.h),
         Rect::xywh(rect.x + rect.w*inv_size, rect.y, rect.w * size, rect.h))
    } else {
        (Rect::xywh(rect.x, rect.y, rect.w, rect.h*inv_size),
         Rect::xywh(rect.x, rect.y + rect.h*inv_size, rect.w, rect.h*size))
    }
}

fn opposite_neighbor(n: usize) -> usize {
    match n {
        0 => 1,
        1 => 0,
        2 => 3,
        3 => 2,
        _ => panic!()
    }
}

impl Pane {
    pub fn whole_screen(content: PaneContent) -> Pane {
        Pane {
            content,
            bounds: Rect::xywh(0.0, 0.0, 1.0, 1.0),
            neighbors: [None, None, None, None],
        }
    }

    // split always places `new_content to the right and below `index`
    pub fn split(panes: &mut BTreeMap<usize, Pane>, index: usize, direction: bool, size: f32, new_content: PaneContent) -> usize {
        let ix = panes.keys().last().cloned().unwrap_or(0) + 1;
        let mut this = panes.get(&index).cloned().unwrap();
        let (a, b) = split_rect(this.bounds, direction, size);
        this.bounds = a;
        let nb = if direction {
            let n = this.neighbors[1];
            if let Some(nb) = n {
                panes.get_mut(&nb).unwrap().neighbors[0] = Some(ix);
            }
            this.neighbors[1] = Some(ix);
            [Some(index), n, this.neighbors[2], this.neighbors[3]]
        } else {
            let below = this.neighbors[3];
            if let Some(nb) = below {
                panes.get_mut(&nb).unwrap().neighbors[2] = Some(ix);
            }
            this.neighbors[3] = Some(ix);
            [this.neighbors[0], this.neighbors[1], Some(index), below]
        };
        panes.insert(index, this);
        panes.insert(ix, Pane {
            content: new_content,
            bounds: b,
            neighbors: nb,
        });
        ix
    }

    pub fn remove(panes: &mut BTreeMap<usize, Pane>, index: usize) -> usize {
        if let Some(p) = panes.remove(&index) {
            for (i, pn) in p.neighbors.iter().enumerate() {
                if let Some(n) = pn.and_then(|ni| panes.get_mut(&ni)) {
                    // can we reasonably resize this neghibor to fill the gap?
                    if i < 2 { //horizontal
                        if p.bounds.h == n.bounds.h {
                            n.bounds.w += p.bounds.w; 
                            n.bounds.x = n.bounds.x.min(p.bounds.x);
                            let o = opposite_neighbor(i);
                            n.neighbors[o] = p.neighbors[o];
                            if let Some(on) = n.neighbors[o] {
                                panes.get_mut(&on).unwrap().neighbors[i] = Some(pn.unwrap());
                            }
                            return pn.unwrap();
                        }
                    } else {
                        if p.bounds.w == n.bounds.w {
                            n.bounds.h += p.bounds.h; 
                            n.bounds.y = n.bounds.y.min(p.bounds.y);
                            let o = opposite_neighbor(i);
                            n.neighbors[o] = p.neighbors[o];
                            if let Some(on) = n.neighbors[o] {
                                panes.get_mut(&on).unwrap().neighbors[i] = Some(pn.unwrap());
                            }
                            return pn.unwrap();
                        }
                    }
                }
            }
        }
        panic!();
    }
}

#[cfg(test)]
mod winman_test {
    use std::collections::BTreeMap;
    use super::{Pane,PaneContent};
    #[test]
    fn split_horiz() {
        let mut panes = BTreeMap::new();
        let ai = 0;
        panes.insert(ai, Pane::whole_screen(PaneContent::Empty));
        let bi = Pane::split(&mut panes, ai, true, 0.5, PaneContent::Empty);
        // [a] | [b]
        assert_eq!(panes[&ai].neighbors, [None, Some(bi), None, None]);
        assert_eq!(panes[&bi].neighbors, [Some(ai), None, None, None]);
        let ci = Pane::split(&mut panes, ai, true, 0.5, PaneContent::Empty);
        // [a] | [c] | [b]
        assert_eq!(panes[&ai].neighbors, [None, Some(ci), None, None], "a");
        assert_eq!(panes[&ci].neighbors, [Some(ai), Some(bi), None, None], "c");
        assert_eq!(panes[&bi].neighbors, [Some(ci), None, None, None], "b");
        let di = Pane::split(&mut panes, ai, true, 0.5, PaneContent::Empty);
        // [a] | [d] | [c] | [b]
        assert_eq!(panes[&ai].neighbors, [None, Some(di), None, None], "a");
        assert_eq!(panes[&di].neighbors, [Some(ai), Some(ci), None, None], "d");
        assert_eq!(panes[&ci].neighbors, [Some(di), Some(bi), None, None], "c");
        assert_eq!(panes[&bi].neighbors, [Some(ci), None, None, None], "b");
    } 

    #[test]
    fn split_vert() {
        let mut panes = BTreeMap::new();
        let ai = 0;
        panes.insert(ai, Pane::whole_screen(PaneContent::Empty));
        let bi = Pane::split(&mut panes, ai, false, 0.5, PaneContent::Empty);
        // [a] | [b]
        assert_eq!(panes[&ai].neighbors, [None, None, None, Some(bi)]);
        assert_eq!(panes[&bi].neighbors, [None, None, Some(ai), None]);
        let ci = Pane::split(&mut panes, ai, false, 0.5, PaneContent::Empty);
        // [a] | [c] | [b]
        assert_eq!(panes[&ai].neighbors, [None, None, None, Some(ci)], "a");
        assert_eq!(panes[&ci].neighbors, [None, None, Some(ai), Some(bi)], "c");
        assert_eq!(panes[&bi].neighbors, [None, None, Some(ci), None], "b");
        let di = Pane::split(&mut panes, ai, false, 0.5, PaneContent::Empty);
        // [a] | [d] | [c] | [b]
        assert_eq!(panes[&ai].neighbors, [None, None, None, Some(di)], "a");
        assert_eq!(panes[&di].neighbors, [None, None, Some(ai), Some(ci)], "d");
        assert_eq!(panes[&ci].neighbors, [None, None, Some(di), Some(bi)], "c");
        assert_eq!(panes[&bi].neighbors, [None, None, Some(ci), None], "b");
    }

    #[test]
    fn remove_horiz() {
        let mut panes = BTreeMap::new();
        let ai = 0;
        panes.insert(ai, Pane::whole_screen(PaneContent::Empty));
        let bi = Pane::split(&mut panes, ai, true, 0.5, PaneContent::Empty);
        // [a] | [b]
        assert_eq!(panes[&ai].neighbors, [None, Some(bi), None, None]);
        assert_eq!(panes[&bi].neighbors, [Some(ai), None, None, None]);
        let ci = Pane::split(&mut panes, ai, true, 0.5, PaneContent::Empty);
        // [a] | [c] | [b]
        assert_eq!(panes[&ai].neighbors, [None, Some(ci), None, None], "a");
        assert_eq!(panes[&ci].neighbors, [Some(ai), Some(bi), None, None], "c");
        assert_eq!(panes[&bi].neighbors, [Some(ci), None, None, None], "b");

        Pane::remove(&mut panes, ci);
        assert_eq!(panes[&ai].neighbors, [None, Some(bi), None, None]);
        assert_eq!(panes[&bi].neighbors, [Some(ai), None, None, None]);
    }
}

// an alternative way to deal with managing Panes. The tradeoff: Splits are simpler for resizing/spliting but way more complex to navigate
pub enum Split {
    Intr {
        direction: bool,
        size: f32,
        children: (Box<Split>, Box<Split>)
    },
    Leaf { content: PaneContent, active: bool }
}

impl Split {
    fn split(self, direction: bool, size: f32, new_content: PaneContent) -> Split {
        match self {
            Split::Intr { children, direction, size } => Split::Intr {
                children: (Box::new(children.0.split(direction,size,new_content.clone())),
                            Box::new(children.1.split(direction,size,new_content))),
                direction, size
            },
            Split::Leaf { content, active } if active => Split::Intr {
                direction, size,
                children: (Box::new(Split::Leaf { content, active: false }), Box::new(Split::Leaf { content: new_content, active: true })) 
            },
            Split::Leaf { .. } => self,
        }
    }

    fn active_content(&self) -> &PaneContent {
        fn acr(s: &Split) -> Option<&PaneContent> {
            match s {
                Split::Intr { children, .. } => acr(children.0.as_ref()).or_else(|| acr(children.1.as_ref())),
                Split::Leaf { content, active } => if *active { Some(content) } else { None }
            }
        }
        acr(self).expect("at least one pane must be active!")
    }

    fn active_content_mut(&mut self) -> &PaneContent {
        fn acr(s: &mut Split) -> Option<&mut PaneContent> {
            match s {
                Split::Intr { children, .. } => acr(children.0.as_mut()).or(acr(children.1.as_mut())),
                Split::Leaf { content, active } => if *active { Some(content) } else { None }
            }
        }
        acr(self).expect("at least one pane must be active!")
    }

    /*fn move_right(&mut self) {
        fn mv(s: &mut Split, parent: Option<&mut Split>) -> bool {
            match s {
                Split::Intr { children, .. } => mv(children.0.as_mut(), Some(s)) || mv(children.1.as_mut(), Some(s)),
                Split::Leaf { content, active } => {
                    if active {

                        true
                    } else {
                        false
                    }
                }
            }
        }
        mv(self, None)
    }*/


    // fn left_neighbor(&self, splits: &BTreeMap<usize, Split>) -> &Split {
    // }
}

pub struct EditorState {
    pub buffers: Vec<Buffer>, 
    pub registers: BTreeMap<char, String>,

    pub panes: BTreeMap<usize, Pane>,
    pub current_pane: usize,
}

pub struct ClientState {
    pub thread_pool: futures::executor::ThreadPool,

    pub servers: HashMap<String, Server>,

    pub force_redraw: bool,

    pub usrmsgs: Vec<UserMessage>,
    pub selected_usrmsg: usize,

    pub config: Config
}

impl Default for ClientState{
    fn default() -> ClientState {
        ClientState::with_config(Config::default())
    }
}

impl EditorState {
    pub fn new() -> EditorState {
        EditorState {
            buffers: Vec::new(),
            panes: BTreeMap::new(),
            current_pane: 0,
            registers: BTreeMap::new(),
        }
    }

    pub fn current_pane(&self) -> &Pane {
        &self.panes[&self.current_pane]
    }

    pub fn current_pane_mut(&mut self) -> &mut Pane {
        self.panes.get_mut(&self.current_pane).unwrap()
    }

    pub fn current_buffer_index(&self) -> Option<usize> {
        match self.current_pane().content {
            PaneContent::Buffer { buffer_index: ix, .. } => {
                Some(ix)
            },
            _ => None
        }
    }

    pub fn current_buffer(&self) -> Option<&Buffer> {
        match self.current_pane().content {
            PaneContent::Buffer { buffer_index: ix, .. } => {
                Some(&self.buffers[ix])
            },
            _ => None
        }
    }

    pub fn current_buffer_mut(&mut self) -> Option<&mut Buffer> {
        match self.current_pane().content {
            PaneContent::Buffer { buffer_index: ix, .. } => {
                Some(&mut self.buffers[ix])
            },
            _ => None
        }
    }

}

pub type PEditorState = Arc<RwLock<EditorState>>;
pub type PClientState = Arc<RwLock<ClientState>>;

impl ClientState {
    pub fn with_config(config: Config) -> ClientState {
        use futures::executor::ThreadPoolBuilder;
        ClientState {
            thread_pool: ThreadPoolBuilder::new().create().unwrap(),
            servers: HashMap::new(),
            force_redraw: false,
            usrmsgs: Vec::new(),
            selected_usrmsg: 0,
            config
        }
    }

    pub fn connect_to_server(state: PClientState, name: String, url: &str) {
        let tp = {state.read().unwrap().thread_pool.clone()};
        let stp = tp.clone();
        let url = url.to_owned();
        tp.spawn_ok(async move {
                    let mut state = state.write().unwrap();
            match Server::init(&url, stp) {
                Ok(s) => {
                    println!("c {:?}", std::time::Instant::now());
                    state.servers.insert(name.clone(), s);
                    ClientState::process_usr_msg(&mut state, UserMessage::info(
                            format!("Connected to {} ({})!", name, url),
                            None));
                }
                Err(e) => {
                    ClientState::process_usr_msg(&mut state,
                        UserMessage::error(
                            format!("Connecting to {} ({}) failed (reason: {}), retry?", name, url, e),
                                Some((vec!["Retry".into()], Box::new(move |_, sstate| {
                                    ClientState::connect_to_server(sstate, name.clone(), &url);
                                })))
                            ));
                }
            }
        });
    }

    pub fn make_request_async<F>(state: PClientState, server_name: impl AsRef<str>, request: protocol::Request, f: F)
        where F: FnOnce(PClientState, protocol::Response) + Send + Sync + 'static
    {
        let tp = {state.read().unwrap().thread_pool.clone()};
        let req_fut = match {
            println!("a {:?}", std::time::Instant::now());
            state.write().unwrap().servers.get_mut(server_name.as_ref())
                .ok_or(Error::InvalidCommand(String::from("server name ") + server_name.as_ref() + " is unknown"))
        } {
            Ok(r) => r.request(request),
            Err(e) => {
                state.write().unwrap().process_error(e);
                return;
            }
        };
        let ess = state.clone();
        tp.spawn_ok(req_fut.then(move |resp: protocol::Response| async move
        {
            match resp {
                protocol::Response::Error { message } => {
                    ess.write().unwrap().process_error_str(message);
                },
                _ => f(ess, resp)
            }
        }));
    }

    pub fn open_buffer(state: PClientState, ess: PEditorState, server_name: String, path: std::path::PathBuf,
        f: impl FnOnce(&mut EditorState, PClientState, usize) + Send + Sync + 'static)
    {
        let sstate = state.clone();
        ClientState::make_request_async(state, server_name.clone(), protocol::Request::OpenFile { path: path.clone() }, move |css, resp| {
            match resp {
                protocol::Response::FileInfo { id, contents, version, format } => {
                    let mut estate = ess.write().unwrap();
                    let buffer_index = estate.buffers.len();
                    estate.buffers.push(Buffer::from_server(String::from(server_name),
                        path, id, contents, version, format));
                    f(&mut estate, sstate, buffer_index);
                },
                _ => panic!() 
            }
        });
    }

    pub fn sync_buffer(state: PClientState, ed_state: PEditorState, buffer_index: usize) {
        let (server_name, id, new_text, version) = {
            let state = ed_state.read().unwrap();
            let b = &state.buffers[buffer_index];
            if b.currently_in_conflict { return; }
            (b.server_name.clone(), b.file_id, b.text.text(), b.version+1)
        };
        ClientState::make_request_async(state, server_name,
            protocol::Request::SyncFile { id, new_text, version },
            move |css, resp| {
                match resp {
                    protocol::Response::Ack => {
                        let mut state = ed_state.write().unwrap();
                        state.buffers[buffer_index].version = version;
                    },
                    protocol::Response::VersionConflict { id, client_version_recieved: _,
                        server_version, server_text } =>
                    {
                        // TODO: probably need to show a nice little dialog, ask the user what they
                        // want to do about the conflict. this becomes a tricky situation since
                        // there's no reason to become Git, but it is nice to able to handle this
                        // situation in a nice way
                        let m = {
                            let mut ed_state = ed_state.write().unwrap();
                            let b = &mut ed_state.buffers[buffer_index];
                            b.currently_in_conflict = true;
                            format!("Server version of {}:{} conflicts with local version!",
                                b.server_name, b.path.to_str().unwrap_or(""))
                        };
                        css.write().unwrap().usrmsgs.push(UserMessage::warning(m,
                                Some((vec![
                                        "Keep local version".into(),
                                        "Open server version/Discard local".into(),
                                        "Open server version in new buffer".into()
                                ], Box::new(move |index, state| {
                                    let mut state = ed_state.write().unwrap();
                                    match index {
                                        0 => {
                                            // next time we sync, overwrite server version
                                            state.buffers[buffer_index].version = 
                                                server_version;
                                            state.buffers[buffer_index].currently_in_conflict = false;
                                        },
                                        1 => {
                                            state.buffers[buffer_index].version =
                                                server_version;
                                            state.buffers[buffer_index].text =
                                                PieceTable::with_text(&server_text);
                                            state.buffers[buffer_index].currently_in_conflict = false;
                                        },
                                        2 => {
                                            let cp = state.current_pane;
                                            let nbi = state.buffers.len();
                                            Pane::split(&mut state.panes, cp, true, 0.5,
                                                PaneContent::Buffer { buffer_index: nbi, viewport_start: 0 });
                                            let p = state.buffers[buffer_index].path.clone();
                                            let f = state.buffers[buffer_index].format.clone();
                                            let server_name = state.buffers[buffer_index].server_name.clone();
                                            state.buffers.push(Buffer::from_server(server_name, p,
                                                    id, server_text.clone(), server_version, f));
                                            // don't clear conflict flag on buffer so we don't try
                                            // to sync the conflicting version again. TODO: some
                                            // way to manually clear the flag?
                                        },
                                        _ => {} 
                                    }
                                })))
                        ));
                    }
                    _ => panic!() 
            }
            }
        );
    }

    pub fn process_usr_msg(&mut self, um: UserMessage) {
        self.usrmsgs.push(um);
        self.force_redraw = true;
    }
    
    pub fn process_usr_msgp(state: PClientState, um: UserMessage) {
        state.write().unwrap().process_usr_msg(um);
    }

    pub fn process_error_str(&mut self, e: String) {
        self.process_usr_msg(UserMessage::error(e, None));
    }
    pub fn process_error<E: std::error::Error>(&mut self, e: E) {
        self.process_error_str(format!("{}", e));
    }
}

pub struct AutosyncWorker {
    cstate: PClientState,
    state: PEditorState,
    last_synced_action_ids: HashMap<String, HashMap<protocol::FileId, usize>> 
}

impl AutosyncWorker {
    pub fn new(cstate: PClientState, state: PEditorState) -> AutosyncWorker {
        AutosyncWorker { cstate, state, last_synced_action_ids: HashMap::new() }
    }

    pub fn run(&mut self) {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            // should this function directly manipulate the futures? 
            // it would be possible to join all the request futures together and then poll them
            // with only one task, which would be more efficent.
            let mut need_sync = Vec::new();
            {
            let state = self.state.read().unwrap();
            for (i,b) in state.buffers.iter().enumerate() {
                if let Some(last_synced_action_id) = self.last_synced_action_ids
                    .entry(b.server_name.clone())
                        .or_insert_with(HashMap::new)
                    .insert(b.file_id, b.text.most_recent_action_id())
                {
                    if last_synced_action_id < b.text.most_recent_action_id() {
                        need_sync.push(i);
                    }
                }
            }
            }
            // println!("autosync {:?}", need_sync);
            for i in need_sync {
                ClientState::sync_buffer(self.cstate.clone(), self.state.clone(), i);
            }
        }
    }
}



