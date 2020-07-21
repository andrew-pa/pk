


#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction { Forward, Backward }

pub mod piece_table;

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

    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Debug)]
    pub enum LineEnding {
        LF, CRLF
    }

    impl Default for LineEnding {
        fn default() -> Self {
            if cfg!(windows) {
                LineEnding::CRLF
            } else {
                LineEnding::LF
            }
        }
    }

    impl LineEnding {
        pub fn from_analysis(s: &str) -> LineEnding {
            s.find('\n')
                .and_then(|i| s.chars().nth(i.saturating_sub(1)))
                .map(|c| if c == '\r' { LineEnding::CRLF } else { LineEnding::LF })
                .unwrap_or(LineEnding::default())
        }

        pub fn as_str(&self) -> &'static str {
            match self {
                &LineEnding::LF => "\n",
                &LineEnding::CRLF => "\r\n"
            }
        }
    }
    #[derive(Serialize, Deserialize, PartialEq, Eq, Clone)]
    pub struct FileType {
        data: [u8; 4]
    }

    impl Default for FileType {
        fn default() -> Self {
            FileType {
                data: [b't', b'e', b'x', b't']
            }
        }
    }

    impl From<&str> for FileType {
        fn from(s: &str) -> Self {
            assert!(s.len() >= 4);
            let b = s.as_bytes();
            FileType { data: [b[0], b[1], b[2], b[3]] }
        }
    }

    impl std::fmt::Display for FileType {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            unsafe {
                write!(f, "{}", std::str::from_utf8_unchecked(&self.data))
            }
        }
    }

    impl std::fmt::Debug for FileType {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            unsafe {
                write!(f, "FileType({})", std::str::from_utf8_unchecked(&self.data))
            }
        }
    }

    #[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone)]
    pub struct TextFormat {
        pub line_ending: LineEnding,
        pub stype: FileType
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub enum Request {
        /* files */
        OpenFile { path: std::path::PathBuf },
        SyncFile { id: FileId, new_text: String, version: usize },
        ReloadFile(FileId),
        CloseFile(FileId),
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
        FileInfo {
            id: FileId,
            contents: String,
            version: usize,
            format: TextFormat
        },
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct MsgResponse {
        pub req_id: MessageId,
        pub msg: Response
    }

}

