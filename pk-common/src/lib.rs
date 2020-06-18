


#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction { Forward, Backward }

pub mod piece_table;
use crate::piece_table::PieceTable;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ModeTag {
    Normal, Insert, Command, Visual, UserMessage
}

pub mod protocol {
    use serde::{Serialize, Deserialize};

    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, Debug)]
    pub struct MessageId(pub u64);

    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, Debug)]
    pub struct FileId(pub u64);

    #[derive(Serialize, Deserialize, Debug)]
    pub enum Request {
        /* files */
        NewFile { path: std::path::PathBuf },
        OpenFile { path: std::path::PathBuf },
        SyncFile { id: FileId, new_text: String, version: usize },
        ReloadFile(FileId),
        CloseFile(FileId)
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct MsgRequest {
        pub msg_id: MessageId,
        pub msg: Request
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub enum Response {
        Ack,
        Error { message: String },
        VersionConflict {
            id: FileId,
            client_version_recieved: usize,
            server_version: usize,
            server_text: String
        },
        FileInfo { id: FileId, contents: String, version: usize },
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct MsgResponse {
        pub req_id: MessageId,
        pub msg: Response
    }

}
